//! Races between creating or rotating a refresh token and revoking every
//! token of the user (password reset, reuse detection). After both finish, no
//! token may still be live (P6, P3).
//!
//! The interleavings are forced, not left to chance: a "gate" transaction
//! holds a row lock that the operations queue behind, and each step waits
//! until PostgreSQL reports the backend as waiting on a lock.

use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use datenaow::modules::auth::domain::{
    PasswordResetRepository, RefreshRepoError, RefreshTokenRecord, RefreshTokenRepository,
};
use datenaow::modules::auth::infrastructure::{
    PgPasswordResetRepository, PgRefreshTokenRepository,
};
use datenaow::modules::users::domain::{Email, NewUser, User, UserRepository};
use datenaow::modules::users::infrastructure::PgUserRepository;

use crate::common::default_start;

const OLD_PASSWORD_HASH: &str = "old-password-hash";
const NEW_PASSWORD_HASH: &str = "new-password-hash";

/// Longest time a step may take to reach its lock wait.
const LOCK_WAIT_TIMEOUT: StdDuration = StdDuration::from_secs(5);

fn t0() -> DateTime<Utc> {
    default_start()
}

fn hash_of(byte: u8) -> [u8; 32] {
    [byte; 32]
}

async fn create_user(pool: &PgPool) -> User {
    PgUserRepository::new(pool.clone())
        .create(NewUser {
            email: Email::parse("an@example.com").expect("valid test email"),
            password_hash: OLD_PASSWORD_HASH.to_owned(),
        })
        .await
        .expect("create user")
}

async fn live_tokens(pool: &PgPool, user_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM refresh_tokens WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("count live tokens")
}

/// Waits until at least `n` other backends of this test database wait on a
/// lock. Polls `pg_stat_activity` instead of sleeping blindly.
async fn wait_for_lock_waiters(pool: &PgPool, n: i64) {
    let deadline = tokio::time::Instant::now() + LOCK_WAIT_TIMEOUT;
    loop {
        let waiting = sqlx::query_scalar::<_, i64>(
            r#"SELECT count(*) FROM pg_stat_activity
               WHERE datname = current_database()
                 AND pid <> pg_backend_pid()
                 AND wait_event_type = 'Lock'"#,
        )
        .fetch_one(pool)
        .await
        .expect("read pg_stat_activity");
        if waiting >= n {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "expected {n} backends waiting on a lock, saw {waiting}"
        );
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
}

/// Opens a transaction holding `FOR UPDATE` on the refresh token `id`, so
/// both racing operations queue behind it in a known order.
async fn gate_on_token(pool: &PgPool, id: Uuid) -> Transaction<'static, Postgres> {
    let mut gate = pool.begin().await.expect("begin gate");
    sqlx::query("SELECT 1 FROM refresh_tokens WHERE id = $1 FOR UPDATE")
        .bind(id)
        .execute(&mut *gate)
        .await
        .expect("lock token row");
    gate
}

/// A live token of `user` plus a rotation of it, started behind a gate on its
/// row and already waiting there.
async fn rotation_waiting_behind_gate(
    pool: &PgPool,
    user: &User,
) -> (
    Transaction<'static, Postgres>,
    tokio::task::JoinHandle<Result<RefreshTokenRecord, RefreshRepoError>>,
) {
    let tokens = PgRefreshTokenRepository::new(pool.clone());
    let old = tokens
        .insert(
            user.id,
            OLD_PASSWORD_HASH,
            hash_of(1),
            t0() + Duration::days(30),
        )
        .await
        .expect("insert old token");

    let gate = gate_on_token(pool, old.id).await;
    let user_id = user.id;
    let rotation = tokio::spawn(async move {
        tokens
            .rotate(
                old.id,
                user_id,
                hash_of(2),
                t0() + Duration::days(31),
                t0() + Duration::minutes(1),
            )
            .await
    });
    wait_for_lock_waiters(pool, 1).await;
    (gate, rotation)
}

/// Stores a valid password-reset token for `user` and returns its id.
async fn stored_reset_token(resets: &PgPasswordResetRepository, user: &User) -> Uuid {
    resets
        .replace_for_user(user.id, hash_of(9), t0() + Duration::minutes(30), t0())
        .await
        .expect("store reset token");
    resets
        .find_by_hash(hash_of(9))
        .await
        .expect("query")
        .expect("reset token exists")
        .id
}

#[sqlx::test]
async fn password_reset_racing_a_rotation_leaves_no_live_token(pool: PgPool) {
    let user = create_user(&pool).await;
    let resets = PgPasswordResetRepository::new(pool.clone());
    let reset_id = stored_reset_token(&resets, &user).await;

    let (gate, rotation) = rotation_waiting_behind_gate(&pool, &user).await;

    // The reset queues behind the rotation (on the token row before the fix,
    // on the user row after it).
    let user_id = user.id;
    let reset_task = tokio::spawn(async move {
        resets
            .consume_and_set_password(
                reset_id,
                user_id,
                NEW_PASSWORD_HASH,
                t0() + Duration::minutes(1),
            )
            .await
    });
    wait_for_lock_waiters(&pool, 2).await;

    gate.commit().await.expect("release gate");
    let rotated = rotation.await.expect("rotation task");
    let consumed = reset_task.await.expect("reset task").expect("reset runs");

    assert!(
        rotated.is_ok(),
        "the rotation was first in line: {rotated:?}"
    );
    assert!(consumed, "the reset token was valid");
    assert_eq!(live_tokens(&pool, user.id).await, 0);
}

#[sqlx::test]
async fn revoke_all_racing_a_rotation_leaves_no_live_token(pool: PgPool) {
    let user = create_user(&pool).await;
    let (gate, rotation) = rotation_waiting_behind_gate(&pool, &user).await;

    let tokens = PgRefreshTokenRepository::new(pool.clone());
    let user_id = user.id;
    let revoke_task = tokio::spawn(async move {
        tokens
            .revoke_all_for_user(user_id, t0() + Duration::minutes(1))
            .await
    });
    wait_for_lock_waiters(&pool, 2).await;

    gate.commit().await.expect("release gate");
    let rotated = rotation.await.expect("rotation task");
    revoke_task
        .await
        .expect("revoke task")
        .expect("revoke-all runs");

    assert!(
        rotated.is_ok(),
        "the rotation was first in line: {rotated:?}"
    );
    assert_eq!(live_tokens(&pool, user.id).await, 0);
}

#[sqlx::test]
async fn insert_with_a_replaced_password_hash_stores_nothing(pool: PgPool) {
    let user = create_user(&pool).await;
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(user.id)
        .bind(NEW_PASSWORD_HASH)
        .execute(&pool)
        .await
        .expect("change password hash");
    let tokens = PgRefreshTokenRepository::new(pool.clone());

    let stale = tokens
        .insert(
            user.id,
            OLD_PASSWORD_HASH,
            hash_of(1),
            t0() + Duration::days(30),
        )
        .await;
    let unknown_user = tokens
        .insert(
            Uuid::nil(),
            NEW_PASSWORD_HASH,
            hash_of(2),
            t0() + Duration::days(30),
        )
        .await;

    assert!(
        matches!(stale, Err(RefreshRepoError::CredentialsChanged)),
        "{stale:?}"
    );
    assert!(
        matches!(unknown_user, Err(RefreshRepoError::CredentialsChanged)),
        "{unknown_user:?}"
    );
    assert_eq!(live_tokens(&pool, user.id).await, 0);
    tokens
        .insert(
            user.id,
            NEW_PASSWORD_HASH,
            hash_of(3),
            t0() + Duration::days(30),
        )
        .await
        .expect("current hash is accepted");
}

/// A sign-in whose password was verified against the old hash reaches the
/// token insert while a password reset is in flight (user row updated, not
/// committed yet). The insert waits for the reset, sees the new hash and
/// stores nothing.
#[sqlx::test]
async fn sign_in_racing_a_password_reset_stores_no_token(pool: PgPool) {
    let user = create_user(&pool).await;
    let tokens = PgRefreshTokenRepository::new(pool.clone());
    let existing = tokens
        .insert(
            user.id,
            OLD_PASSWORD_HASH,
            hash_of(1),
            t0() + Duration::days(30),
        )
        .await
        .expect("existing session");
    let resets = PgPasswordResetRepository::new(pool.clone());
    let reset_id = stored_reset_token(&resets, &user).await;

    // The gate pauses the reset at its revocation step, after it updated the
    // user row.
    let gate = gate_on_token(&pool, existing.id).await;
    let user_id = user.id;
    let reset_task = tokio::spawn(async move {
        resets
            .consume_and_set_password(
                reset_id,
                user_id,
                NEW_PASSWORD_HASH,
                t0() + Duration::minutes(1),
            )
            .await
    });
    wait_for_lock_waiters(&pool, 1).await;

    let sign_in = tokio::spawn(async move {
        tokens
            .insert(
                user_id,
                OLD_PASSWORD_HASH,
                hash_of(2),
                t0() + Duration::days(30),
            )
            .await
    });
    wait_for_lock_waiters(&pool, 2).await;

    gate.commit().await.expect("release gate");
    let consumed = reset_task.await.expect("reset task").expect("reset runs");
    let inserted = sign_in.await.expect("sign-in task");

    assert!(consumed, "the reset token was valid");
    assert!(
        matches!(inserted, Err(RefreshRepoError::CredentialsChanged)),
        "{inserted:?}"
    );
    assert_eq!(live_tokens(&pool, user.id).await, 0);
}

//! Refresh tokens: the PostgreSQL repository and `SessionService::start`.
//! The HTTP refresh and logout endpoints are tested here from Task 7 on.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use datenaow::modules::auth::application::services::{JwtService, SessionService, TokenError};
use datenaow::modules::auth::domain::{
    OpaqueToken, RefreshRepoError, RefreshTokenRecord, RefreshTokenRepository,
};
use datenaow::modules::auth::infrastructure::PgRefreshTokenRepository;
use datenaow::modules::users::domain::{Email, NewUser, User, UserRepository};
use datenaow::modules::users::infrastructure::PgUserRepository;
use datenaow::shared::clock::Clock;

use crate::common::{FakeClock, default_start, test_config};

async fn create_user(pool: &PgPool, email: &str) -> User {
    PgUserRepository::new(pool.clone())
        .create(NewUser {
            email: Email::parse(email).expect("valid test email"),
            password_hash: "not-a-real-hash".to_owned(),
        })
        .await
        .expect("create user")
}

fn hash_of(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn t0() -> DateTime<Utc> {
    default_start()
}

async fn find(repo: &PgRefreshTokenRepository, hash: [u8; 32]) -> RefreshTokenRecord {
    repo.find_by_hash(hash)
        .await
        .expect("query runs")
        .expect("token exists")
}

// ---- repository ----

#[sqlx::test]
async fn insert_then_find_by_hash_returns_active_record(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let expires = t0() + Duration::days(30);

    let inserted = repo
        .insert(user.id, hash_of(1), expires)
        .await
        .expect("insert");
    let found = find(&repo, hash_of(1)).await;

    assert_eq!(found, inserted);
    assert_eq!(found.user_id, user.id);
    assert_eq!(found.expires_at, expires);
    assert_eq!(found.revoked_at, None);
    assert_eq!(found.replaced_at, None);
    assert!(
        repo.find_by_hash(hash_of(2))
            .await
            .expect("query")
            .is_none()
    );
}

#[sqlx::test]
async fn rotate_marks_old_revoked_and_inserts_new(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let old = repo
        .insert(user.id, hash_of(1), t0() + Duration::days(30))
        .await
        .expect("insert");
    let now = t0() + Duration::hours(1);
    let new_expires = now + Duration::days(30);

    let new = repo
        .rotate(old.id, user.id, hash_of(2), new_expires, now)
        .await
        .expect("rotate");

    let old_after = find(&repo, hash_of(1)).await;
    assert_eq!(old_after.revoked_at, Some(now));
    assert_eq!(old_after.replaced_at, Some(now));

    let new_found = find(&repo, hash_of(2)).await;
    assert_eq!(new_found, new);
    assert_ne!(new_found.id, old.id);
    assert_eq!(new_found.user_id, user.id);
    assert_eq!(new_found.expires_at, new_expires);
    assert_eq!(new_found.revoked_at, None);
    assert_eq!(new_found.replaced_at, None);
}

#[sqlx::test]
async fn rotate_already_revoked_returns_already_revoked(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let old = repo
        .insert(user.id, hash_of(1), t0() + Duration::days(30))
        .await
        .expect("insert");
    let expires = t0() + Duration::days(31);

    repo.rotate(old.id, user.id, hash_of(2), expires, t0())
        .await
        .expect("first rotate");
    let second = repo
        .rotate(
            old.id,
            user.id,
            hash_of(3),
            expires,
            t0() + Duration::seconds(1),
        )
        .await;

    assert!(matches!(second, Err(RefreshRepoError::AlreadyRevoked)));
    // The failed rotation rolled back: no token was inserted, and the first
    // rotation's timestamps were not overwritten.
    assert!(
        repo.find_by_hash(hash_of(3))
            .await
            .expect("query")
            .is_none()
    );
    let old_after = find(&repo, hash_of(1)).await;
    assert_eq!(old_after.revoked_at, Some(t0()));
    assert_eq!(old_after.replaced_at, Some(t0()));

    // A token revoked by logout cannot be rotated either.
    let logged_out = repo
        .insert(user.id, hash_of(4), expires)
        .await
        .expect("insert");
    repo.revoke(logged_out.id, t0()).await.expect("revoke");
    let res = repo
        .rotate(logged_out.id, user.id, hash_of(5), expires, t0())
        .await;
    assert!(matches!(res, Err(RefreshRepoError::AlreadyRevoked)));
    assert!(
        repo.find_by_hash(hash_of(5))
            .await
            .expect("query")
            .is_none()
    );
}

#[sqlx::test]
async fn revoke_all_for_user_revokes_every_active_token_only_for_that_user(pool: PgPool) {
    let alice = create_user(&pool, "alice@example.com").await;
    let bob = create_user(&pool, "bob@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let expires = t0() + Duration::days(30);
    for hash in [hash_of(1), hash_of(2), hash_of(3)] {
        repo.insert(alice.id, hash, expires).await.expect("insert");
    }
    repo.insert(bob.id, hash_of(9), expires)
        .await
        .expect("insert");
    let earlier = find(&repo, hash_of(3)).await;
    repo.revoke(earlier.id, t0()).await.expect("revoke");
    let now = t0() + Duration::hours(2);

    let revoked = repo
        .revoke_all_for_user(alice.id, now)
        .await
        .expect("revoke all");

    assert_eq!(revoked, 2, "only the two active tokens are revoked");
    for hash in [hash_of(1), hash_of(2)] {
        let record = find(&repo, hash).await;
        assert_eq!(record.revoked_at, Some(now));
        assert_eq!(record.replaced_at, None);
    }
    assert_eq!(
        find(&repo, hash_of(3)).await.revoked_at,
        Some(t0()),
        "an already revoked token keeps its first revocation time"
    );
    let bobs = find(&repo, hash_of(9)).await;
    assert_eq!(bobs.revoked_at, None);
    assert_eq!(bobs.user_id, bob.id);
}

#[sqlx::test]
async fn revoke_sets_revoked_at_but_leaves_replaced_at_null(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool);
    let record = repo
        .insert(user.id, hash_of(1), t0() + Duration::days(30))
        .await
        .expect("insert");
    let now = t0() + Duration::minutes(5);

    repo.revoke(record.id, now).await.expect("revoke");
    // Revoking again keeps the first revocation time.
    repo.revoke(record.id, now + Duration::minutes(1))
        .await
        .expect("revoke again");
    // Unknown ids are ignored.
    repo.revoke(Uuid::nil(), now).await.expect("revoke unknown");

    let after = find(&repo, hash_of(1)).await;
    assert_eq!(after.revoked_at, Some(now));
    assert_eq!(after.replaced_at, None);
}

#[sqlx::test]
async fn deleting_user_deletes_their_refresh_tokens(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let repo = PgRefreshTokenRepository::new(pool.clone());
    repo.insert(user.id, hash_of(1), t0() + Duration::days(30))
        .await
        .expect("insert");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("delete user");

    assert!(
        repo.find_by_hash(hash_of(1))
            .await
            .expect("query")
            .is_none()
    );
}

// ---- SessionService::start ----

struct Fixture {
    service: SessionService<PgUserRepository, PgRefreshTokenRepository>,
    jwt: Arc<JwtService>,
    clock: Arc<FakeClock>,
}

fn session_service(pool: &PgPool) -> Fixture {
    let config = test_config();
    let jwt = Arc::new(JwtService::new(
        config.jwt_secret.as_bytes(),
        Duration::seconds(config.access_token_ttl_secs),
    ));
    let clock = Arc::new(FakeClock::default());
    let service = SessionService::new(
        PgUserRepository::new(pool.clone()),
        PgRefreshTokenRepository::new(pool.clone()),
        jwt.clone(),
        clock.clone(),
        Duration::days(config.refresh_token_ttl_days),
    );
    Fixture {
        service,
        jwt,
        clock,
    }
}

async fn stored_hashes(pool: &PgPool, user_id: Uuid) -> Vec<Vec<u8>> {
    sqlx::query_scalar::<_, Vec<u8>>("SELECT token_hash FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .expect("read stored hashes")
}

#[sqlx::test]
async fn start_returns_jwt_valid_for_900s_and_stores_only_hash(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let f = session_service(&pool);
    let now = f.clock.now();

    let session = f.service.start(&user).await.expect("start session");

    assert_eq!(session.access_expires_in_secs, 900);
    let claims = f
        .jwt
        .verify(&session.access_token, now)
        .expect("access token is valid now");
    assert_eq!(claims.sub, user.id);
    assert_eq!(claims.role, user.role);
    assert_eq!(claims.iat, now.timestamp());
    assert_eq!(claims.exp - claims.iat, 900);
    assert!(
        f.jwt
            .verify(&session.access_token, now + Duration::seconds(899))
            .is_ok()
    );
    assert!(matches!(
        f.jwt
            .verify(&session.access_token, now + Duration::seconds(900)),
        Err(TokenError::Expired)
    ));

    let stored = stored_hashes(&pool, user.id).await;
    assert_eq!(stored.len(), 1);
    assert_ne!(stored[0], session.refresh_token.as_str().as_bytes());
    assert_eq!(stored[0], session.refresh_token.hash().to_vec());
    // The token handed to the client parses back and is found by its hash.
    let presented = OpaqueToken::from_client(session.refresh_token.as_str()).expect("valid");
    let record = PgRefreshTokenRepository::new(pool.clone())
        .find_by_hash(presented.hash())
        .await
        .expect("query")
        .expect("token stored");
    assert_eq!(record.user_id, user.id);
    assert_eq!(record.revoked_at, None);
}

#[sqlx::test]
async fn start_sets_refresh_expiry_30_days_from_now(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let f = session_service(&pool);
    f.clock.advance(Duration::minutes(7));
    let now = f.clock.now();

    let session = f.service.start(&user).await.expect("start session");

    assert_eq!(session.refresh_expires_at, now + Duration::days(30));
    let record = PgRefreshTokenRepository::new(pool.clone())
        .find_by_hash(session.refresh_token.hash())
        .await
        .expect("query")
        .expect("token stored");
    assert_eq!(record.expires_at, now + Duration::days(30));
}

#[sqlx::test]
async fn each_start_issues_a_distinct_refresh_token(pool: PgPool) {
    let user = create_user(&pool, "an@example.com").await;
    let f = session_service(&pool);

    let first = f.service.start(&user).await.expect("first session");
    let second = f.service.start(&user).await.expect("second session");

    assert_ne!(first.refresh_token, second.refresh_token);
    assert_eq!(stored_hashes(&pool, user.id).await.len(), 2);
}

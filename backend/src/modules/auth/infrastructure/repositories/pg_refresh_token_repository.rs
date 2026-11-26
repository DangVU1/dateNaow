//! PostgreSQL implementation of `RefreshTokenRepository`. All SQL for the
//! `refresh_tokens` table lives here, except the revocation step of the
//! password-reset transaction (Task 9), which must share that transaction.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::modules::auth::domain::{RefreshRepoError, RefreshTokenRecord, RefreshTokenRepository};

#[derive(Debug, Clone)]
pub struct PgRefreshTokenRepository {
    pool: PgPool,
}

impl PgRefreshTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Database shape of a `refresh_tokens` row (without the hash); mapped to the
/// domain record explicitly.
struct RefreshTokenRow {
    id: Uuid,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    replaced_at: Option<DateTime<Utc>>,
}

impl From<RefreshTokenRow> for RefreshTokenRecord {
    fn from(row: RefreshTokenRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            expires_at: row.expires_at,
            revoked_at: row.revoked_at,
            replaced_at: row.replaced_at,
        }
    }
}

impl RefreshTokenRepository for PgRefreshTokenRepository {
    async fn insert(
        &self,
        user_id: Uuid,
        password_hash: &str,
        token_hash: [u8; 32],
        expires_at: DateTime<Utc>,
    ) -> Result<RefreshTokenRecord, RefreshRepoError> {
        // One statement: `FOR SHARE` waits for a password reset that has
        // updated the user row but not committed yet, then re-checks the hash
        // against the committed row. So either the reset commits first and
        // nothing is inserted, or this insert commits first and the reset's
        // later revocation sees the new token.
        let row = sqlx::query_as!(
            RefreshTokenRow,
            r#"INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
               SELECT id, $2::bytea, $3::timestamptz FROM users
               WHERE id = $1 AND password_hash = $4
               FOR SHARE
               RETURNING id, user_id, expires_at, revoked_at, replaced_at"#,
            user_id,
            &token_hash[..],
            expires_at,
            password_hash,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or(RefreshRepoError::CredentialsChanged)?;

        Ok(row.into())
    }

    async fn find_by_hash(
        &self,
        token_hash: [u8; 32],
    ) -> Result<Option<RefreshTokenRecord>, RefreshRepoError> {
        let row = sqlx::query_as!(
            RefreshTokenRow,
            r#"SELECT id, user_id, expires_at, revoked_at, replaced_at
               FROM refresh_tokens WHERE token_hash = $1"#,
            &token_hash[..],
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;

        Ok(row.map(Into::into))
    }

    async fn rotate(
        &self,
        old_id: Uuid,
        user_id: Uuid,
        new_hash: [u8; 32],
        new_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<RefreshTokenRecord, RefreshRepoError> {
        let mut tx = self.pool.begin().await.map_err(database)?;

        // Lock order: the user row first, then their tokens, as in
        // `revoke_all_for_user` and the password-reset transaction. `FOR
        // SHARE` lets rotations of different tokens run side by side but
        // waits for (and is waited on by) a revoke-all or password change.
        // Without it, a revoke-all blocked on the old row would skip it once
        // this commits and never see the new row (READ COMMITTED snapshot).
        sqlx::query!(
            "SELECT 1 AS locked FROM users WHERE id = $1 FOR SHARE",
            user_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;

        // The `revoked_at IS NULL` condition makes the row lock decide a race:
        // of two concurrent rotations of the same token, only one updates it.
        // `user_id` is checked too, so a mismatched pair can never hand one
        // user's session to another.
        let revoked = sqlx::query!(
            r#"UPDATE refresh_tokens SET revoked_at = $2, replaced_at = $2
               WHERE id = $1 AND user_id = $3 AND revoked_at IS NULL"#,
            old_id,
            now,
            user_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(database)?;

        if revoked.rows_affected() == 0 {
            tx.rollback().await.map_err(database)?;
            return Err(RefreshRepoError::AlreadyRevoked);
        }

        let row = sqlx::query_as!(
            RefreshTokenRow,
            r#"INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
               VALUES ($1, $2, $3)
               RETURNING id, user_id, expires_at, revoked_at, replaced_at"#,
            user_id,
            &new_hash[..],
            new_expires_at,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(database)?;

        tx.commit().await.map_err(database)?;
        Ok(row.into())
    }

    async fn revoke(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), RefreshRepoError> {
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL",
            id,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(database)?;

        Ok(())
    }

    async fn revoke_all_for_user(
        &self,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<u64, RefreshRepoError> {
        let mut tx = self.pool.begin().await.map_err(database)?;

        // Same lock order as `rotate`: the user row first. This waits for
        // running rotations and sign-ins of the user to commit, so the UPDATE
        // below (a new statement, hence a new snapshot) sees their new tokens.
        sqlx::query!(
            "SELECT 1 AS locked FROM users WHERE id = $1 FOR NO KEY UPDATE",
            user_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;

        let result = sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = $2 WHERE user_id = $1 AND revoked_at IS NULL",
            user_id,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(database)?;

        tx.commit().await.map_err(database)?;
        Ok(result.rows_affected())
    }
}

fn database(error: sqlx::Error) -> RefreshRepoError {
    RefreshRepoError::Database(Box::new(error))
}

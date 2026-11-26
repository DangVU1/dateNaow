//! PostgreSQL implementation of `PasswordResetRepository`. All SQL for the
//! `password_reset_tokens` table lives here, plus the refresh-token revocation
//! that must share the reset transaction (P11).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::modules::auth::domain::{
    PasswordResetRepoError, PasswordResetRepository, ResetTokenRecord,
};
use crate::modules::users::infrastructure::PgUserRepository;

#[derive(Debug, Clone)]
pub struct PgPasswordResetRepository {
    pool: PgPool,
}

impl PgPasswordResetRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Database shape of a `password_reset_tokens` row (without the hash).
struct ResetTokenRow {
    id: Uuid,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    used_at: Option<DateTime<Utc>>,
}

impl From<ResetTokenRow> for ResetTokenRecord {
    fn from(row: ResetTokenRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            expires_at: row.expires_at,
            used_at: row.used_at,
        }
    }
}

impl PasswordResetRepository for PgPasswordResetRepository {
    async fn replace_for_user(
        &self,
        user_id: Uuid,
        token_hash: [u8; 32],
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), PasswordResetRepoError> {
        // One statement, so one round trip and atomic without an explicit
        // transaction: the reset request stays cheap and its response time
        // barely depends on whether the account exists (P6).
        sqlx::query!(
            r#"WITH invalidated AS (
                   UPDATE password_reset_tokens SET used_at = $4
                   WHERE user_id = $1 AND used_at IS NULL
               )
               INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
               VALUES ($1, $2, $3)"#,
            user_id,
            &token_hash[..],
            expires_at,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(database)?;

        Ok(())
    }

    async fn find_by_hash(
        &self,
        token_hash: [u8; 32],
    ) -> Result<Option<ResetTokenRecord>, PasswordResetRepoError> {
        let row = sqlx::query_as!(
            ResetTokenRow,
            r#"SELECT id, user_id, expires_at, used_at
               FROM password_reset_tokens WHERE token_hash = $1"#,
            &token_hash[..],
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?;

        Ok(row.map(Into::into))
    }

    async fn consume_and_set_password(
        &self,
        token_id: Uuid,
        user_id: Uuid,
        new_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, PasswordResetRepoError> {
        let mut tx = self.pool.begin().await.map_err(database)?;

        // The `used_at IS NULL` condition makes the row lock decide a race:
        // of two concurrent confirmations of one token, only one updates it.
        let consumed = sqlx::query!(
            r#"UPDATE password_reset_tokens SET used_at = $3
               WHERE id = $1 AND user_id = $2 AND used_at IS NULL AND expires_at > $3"#,
            token_id,
            user_id,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(database)?;

        if consumed.rows_affected() == 0 {
            tx.rollback().await.map_err(database)?;
            return Ok(false);
        }

        // The user row is updated before the tokens are revoked: this lock
        // waits for running rotations and sign-ins of the user (they hold
        // `FOR SHARE` on it), so the revocation below sees their new tokens,
        // and those that come later find the new hash or a revoked token.
        PgUserRepository::set_password_hash_in_tx(&mut tx, user_id, new_hash)
            .await
            .map_err(|e| PasswordResetRepoError::Database(Box::new(e)))?;

        // `revoked_at` only: an old cookie then gets a plain 401 on refresh,
        // never the reuse detection that revokes every session (P3).
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = $2 WHERE user_id = $1 AND revoked_at IS NULL",
            user_id,
            now,
        )
        .execute(&mut *tx)
        .await
        .map_err(database)?;

        tx.commit().await.map_err(database)?;
        Ok(true)
    }
}

fn database(error: sqlx::Error) -> PasswordResetRepoError {
    PasswordResetRepoError::Database(Box::new(error))
}

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
        token_hash: [u8; 32],
        expires_at: DateTime<Utc>,
    ) -> Result<RefreshTokenRecord, RefreshRepoError> {
        let row = sqlx::query_as!(
            RefreshTokenRow,
            r#"INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
               VALUES ($1, $2, $3)
               RETURNING id, user_id, expires_at, revoked_at, replaced_at"#,
            user_id,
            &token_hash[..],
            expires_at,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(database)?;

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
        let result = sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = $2 WHERE user_id = $1 AND revoked_at IS NULL",
            user_id,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(database)?;

        Ok(result.rows_affected())
    }
}

fn database(error: sqlx::Error) -> RefreshRepoError {
    RefreshRepoError::Database(Box::new(error))
}

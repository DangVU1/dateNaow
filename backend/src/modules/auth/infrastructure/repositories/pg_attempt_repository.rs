//! PostgreSQL implementation of `AttemptRepository`. Each method runs a single
//! auto-committed statement on the pool, which is what makes the limiter's
//! "record first, count second" order safe under concurrency (P5).

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::modules::auth::domain::{AttemptRepoError, AttemptRepository};

#[derive(Debug, Clone)]
pub struct PgAttemptRepository {
    pool: PgPool,
}

impl PgAttemptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl AttemptRepository for PgAttemptRepository {
    async fn record(&self, key_hash: [u8; 32], at: DateTime<Utc>) -> Result<i64, AttemptRepoError> {
        sqlx::query_scalar!(
            "INSERT INTO auth_attempts (key_hash, attempted_at) VALUES ($1, $2) RETURNING id",
            &key_hash[..],
            at,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(database)
    }

    async fn count_since(
        &self,
        key_hash: [u8; 32],
        since: DateTime<Utc>,
    ) -> Result<i64, AttemptRepoError> {
        let count = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_attempts
               WHERE key_hash = $1 AND attempted_at > $2"#,
            &key_hash[..],
            since,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(database)?;

        Ok(count)
    }

    async fn delete_by_id(&self, id: i64) -> Result<(), AttemptRepoError> {
        sqlx::query!("DELETE FROM auth_attempts WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(database)?;
        Ok(())
    }

    async fn clear(&self, key_hash: [u8; 32]) -> Result<(), AttemptRepoError> {
        sqlx::query!(
            "DELETE FROM auth_attempts WHERE key_hash = $1",
            &key_hash[..]
        )
        .execute(&self.pool)
        .await
        .map_err(database)?;
        Ok(())
    }

    async fn delete_older_than(&self, before: DateTime<Utc>) -> Result<(), AttemptRepoError> {
        sqlx::query!("DELETE FROM auth_attempts WHERE attempted_at < $1", before)
            .execute(&self.pool)
            .await
            .map_err(database)?;
        Ok(())
    }
}

fn database(error: sqlx::Error) -> AttemptRepoError {
    AttemptRepoError::Database(Box::new(error))
}

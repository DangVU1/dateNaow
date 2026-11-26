//! PostgreSQL connection pool.

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

const MAX_CONNECTIONS: u32 = 10;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Opens the connection pool and checks that the database is reachable.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect(database_url)
        .await
}

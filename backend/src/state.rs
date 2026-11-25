//! Shared application state handed to every handler through Axum `State`.

use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::shared::clock::Clock;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub clock: Arc<dyn Clock>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self {
            db,
            config: Arc::new(config),
            clock,
        }
    }
}

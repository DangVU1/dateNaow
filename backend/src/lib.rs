//! dateNaow backend: a modular monolith built with Axum, SQLx and PostgreSQL.

pub mod config;
pub mod infrastructure;
pub mod routes;
pub mod shared;
pub mod state;

pub use routes::build_router;
pub use state::AppState;

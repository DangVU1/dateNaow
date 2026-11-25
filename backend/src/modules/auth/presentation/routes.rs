//! Routes of the auth module, nested under `/api/v1/auth` by `crate::routes`.

use axum::Router;
use axum::routing::post;

use crate::modules::auth::presentation::handlers;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(handlers::register))
        .route("/login", post(handlers::login))
}

//! Routes of the users module, merged into `/api/v1` by `crate::routes`.

use axum::Router;
use axum::routing::get;

use crate::modules::users::presentation::handlers;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/me",
        get(handlers::get_current_user).patch(handlers::update_gender),
    )
}

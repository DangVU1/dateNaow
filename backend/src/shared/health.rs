//! `GET /api/v1/health`: reports whether the service and its database are up.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// `ok` or `degraded`.
    pub status: &'static str,
    /// `ok` or `unavailable`.
    pub database: &'static str,
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "health",
    responses(
        (status = 200, description = "Service and database are up", body = HealthResponse),
        (status = 503, description = "Database is unreachable", body = HealthResponse),
    )
)]
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok",
                database: "ok",
            }),
        ),
        Err(error) => {
            tracing::warn!(%error, "health check: database unavailable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "degraded",
                    database: "unavailable",
                }),
            )
        }
    }
}

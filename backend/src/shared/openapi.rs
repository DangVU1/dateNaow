//! OpenAPI document served at `GET /api/v1/openapi.json` (P8).
//!
//! Every task that adds an endpoint adds its handler to `paths(...)` here.

use axum::Json;
use utoipa::OpenApi;

use crate::modules::auth::presentation::dto::{AuthResponse, LoginRequest, RegisterRequest};
use crate::modules::auth::presentation::handlers as auth;
use crate::modules::users::presentation::dto::UserResponse;
use crate::shared::error::{ErrorBody, ErrorResponse};
use crate::shared::health;

#[derive(OpenApi)]
#[openapi(
    info(title = "dateNaow API"),
    paths(health::health, auth::register, auth::login),
    components(schemas(
        ErrorResponse,
        ErrorBody,
        RegisterRequest,
        LoginRequest,
        AuthResponse,
        UserResponse
    ))
)]
pub struct ApiDoc;

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

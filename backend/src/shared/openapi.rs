//! OpenAPI document served at `GET /api/v1/openapi.json` (P8).
//!
//! Every task that adds an endpoint adds its handler to `paths(...)` here.

use axum::Json;
use utoipa::OpenApi;

use crate::shared::error::{ErrorBody, ErrorResponse};
use crate::shared::health;

#[derive(OpenApi)]
#[openapi(
    info(title = "dateNaow API"),
    paths(health::health),
    components(schemas(ErrorResponse, ErrorBody))
)]
pub struct ApiDoc;

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

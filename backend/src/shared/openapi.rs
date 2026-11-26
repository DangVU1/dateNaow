//! OpenAPI document served at `GET /api/v1/openapi.json` (P8).
//!
//! Every task that adds an endpoint adds its handler to `paths(...)` here.

use axum::Json;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::modules::auth::presentation::dto::{
    AuthResponse, LoginRequest, PasswordResetConfirmRequest, PasswordResetRequest, RegisterRequest,
};
use crate::modules::auth::presentation::handlers as auth;
use crate::modules::users::presentation::dto::{UpdateMeRequest, UserResponse};
use crate::modules::users::presentation::handlers as users;
use crate::shared::error::{ErrorBody, ErrorResponse};
use crate::shared::health;

#[derive(OpenApi)]
#[openapi(
    info(title = "dateNaow API"),
    paths(
        health::health,
        auth::register,
        auth::login,
        auth::refresh,
        auth::logout,
        auth::request_password_reset,
        auth::confirm_password_reset,
        users::get_current_user,
        users::update_gender
    ),
    components(schemas(
        ErrorResponse,
        ErrorBody,
        RegisterRequest,
        LoginRequest,
        PasswordResetRequest,
        PasswordResetConfirmRequest,
        AuthResponse,
        UserResponse,
        UpdateMeRequest
    )),
    modifiers(&BearerAuth)
)]
pub struct ApiDoc;

/// Registers the `bearer_auth` scheme (`Authorization: Bearer <access token>`)
/// that protected paths reference with `security(("bearer_auth" = []))`.
struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi
            .components
            .get_or_insert_with(Default::default)
            .add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
    }
}

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

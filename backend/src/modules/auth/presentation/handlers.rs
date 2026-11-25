//! HTTP handlers for `/api/v1/auth`. Thin: extract, delegate, shape the response.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;

use crate::modules::auth::application::services::Session;
use crate::modules::auth::presentation::cookies::refresh_cookie;
use crate::modules::auth::presentation::dto::{AuthResponse, LoginRequest, RegisterRequest};
use crate::modules::users::domain::User;
use crate::shared::client_ip::ClientIp;
use crate::shared::error::{ApiError, ErrorResponse};
use crate::shared::extract::ValidatedJson;
use crate::state::AppState;

const SECS_PER_DAY: i64 = 86_400;

#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created and signed in; sets the `dn_refresh` cookie", body = AuthResponse),
        (status = 400, description = "Malformed JSON or unknown field", body = ErrorResponse),
        (status = 409, description = "`email_taken`", body = ErrorResponse),
        (status = 422, description = "Invalid email or password; see `fields`", body = ErrorResponse),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RegisterRequest>,
) -> Result<(StatusCode, CookieJar, Json<AuthResponse>), ApiError> {
    let (user, session) = state.auth.register.execute(body.into()).await?;
    let (jar, response) = signed_in(&state, &user, &session);
    Ok((StatusCode::CREATED, jar, response))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed in; sets the `dn_refresh` cookie", body = AuthResponse),
        (status = 400, description = "Malformed JSON or unknown field", body = ErrorResponse),
        (status = 401, description = "`invalid_credentials`: wrong password or unknown email", body = ErrorResponse),
        (status = 429, description = "`rate_limited`, with `Retry-After` in seconds", body = ErrorResponse),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ClientIp(client_ip): ClientIp,
    ValidatedJson(body): ValidatedJson<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), ApiError> {
    let (user, session) = state
        .auth
        .login
        .execute(body.into_command(client_ip))
        .await?;
    Ok(signed_in(&state, &user, &session))
}

/// The refresh cookie plus the JSON body for a freshly started session.
fn signed_in(state: &AppState, user: &User, session: &Session) -> (CookieJar, Json<AuthResponse>) {
    let cookie = refresh_cookie(
        &session.refresh_token,
        state.config.refresh_token_ttl_days * SECS_PER_DAY,
        state.config.cookie_secure,
    );
    (
        CookieJar::new().add(cookie),
        Json(AuthResponse::new(user, session)),
    )
}

//! HTTP handlers for `/api/v1/auth`. Thin: extract, delegate, shape the response.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;

use crate::modules::auth::application::AuthError;
use crate::modules::auth::application::services::Session;
use crate::modules::auth::presentation::cookies::{
    REFRESH_COOKIE, clear_refresh_cookie, refresh_cookie,
};
use crate::modules::auth::presentation::dto::{
    AuthResponse, LoginRequest, PasswordResetConfirmRequest, PasswordResetRequest, RegisterRequest,
};
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
        (status = 422, description = "Invalid email or password; see `fields`. Password codes: `too_short`, `too_long`, `too_common`", body = ErrorResponse),
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    params(
        ("dn_refresh" = String, Cookie, description = "The refresh token set by sign-up, sign-in or a previous refresh"),
    ),
    responses(
        (status = 200, description = "New access token; rotates the `dn_refresh` cookie", body = AuthResponse),
        (status = 401, description = "`invalid_refresh_token`: missing, unknown, expired or revoked; clears the cookie", body = ErrorResponse),
        (status = 409, description = "`refresh_conflict`: the token was rotated moments ago (e.g. by another tab); the cookie is kept, retry shortly", body = ErrorResponse),
    )
)]
pub async fn refresh(State(state): State<AppState>, jar: CookieJar) -> Response {
    let result = match jar.get(REFRESH_COOKIE) {
        Some(cookie) => state.auth.sessions.refresh(cookie.value()).await,
        None => Err(AuthError::InvalidRefreshToken),
    };
    match result {
        Ok((user, session)) => signed_in(&state, &user, &session).into_response(),
        Err(error) => refresh_failed(error, state.config.cookie_secure),
    }
}

/// Every 401 also deletes the cookie, so the browser stops sending a dead
/// token; any other error (409 conflict, 500) leaves the cookie alone.
fn refresh_failed(error: AuthError, secure: bool) -> Response {
    let error = ApiError::from(error);
    if error.status() == StatusCode::UNAUTHORIZED {
        (CookieJar::new().add(clear_refresh_cookie(secure)), error).into_response()
    } else {
        error.into_response()
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    params(
        ("dn_refresh" = Option<String>, Cookie, description = "Optional; an unknown or malformed token is ignored"),
    ),
    responses(
        (status = 204, description = "Signed out; clears the `dn_refresh` cookie"),
    )
)]
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Response {
    let clear = CookieJar::new().add(clear_refresh_cookie(state.config.cookie_secure));
    if let Some(cookie) = jar.get(REFRESH_COOKIE)
        && let Err(error) = state.auth.sessions.end(cookie.value()).await
    {
        // Plan: logout always answers 204. The browser drops the cookie
        // either way; the failure is kept in the server log.
        tracing::error!(error = %error, "logout could not revoke the refresh token");
    }
    (StatusCode::NO_CONTENT, clear).into_response()
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/password-reset/request",
    tag = "auth",
    request_body = PasswordResetRequest,
    responses(
        (status = 204, description = "Accepted. Sent whether or not the email has an account; if it has one, a reset link valid for RESET_TOKEN_TTL_MINS (default 30) minutes is emailed and earlier links stop working"),
        (status = 400, description = "Malformed JSON or unknown field", body = ErrorResponse),
        (status = 422, description = "Malformed email; see `fields.email`", body = ErrorResponse),
        (status = 429, description = "`rate_limited` (an hour: 3 per email and IP, 10 per IP, 10 per email), with `Retry-After` in seconds", body = ErrorResponse),
    )
)]
pub async fn request_password_reset(
    State(state): State<AppState>,
    ClientIp(client_ip): ClientIp,
    ValidatedJson(body): ValidatedJson<PasswordResetRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .request_reset
        .execute(body.into_command(client_ip))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/password-reset/confirm",
    tag = "auth",
    request_body = PasswordResetConfirmRequest,
    responses(
        (status = 204, description = "Password changed; every refresh token of the user is revoked"),
        (status = 400, description = "`invalid_or_expired_token`: malformed, unknown, used or expired token; or malformed JSON (`invalid_json`)", body = ErrorResponse),
        (status = 422, description = "Invalid new password (`too_short`, `too_long`, `too_common`); see `fields.new_password`. The token stays usable", body = ErrorResponse),
    )
)]
pub async fn confirm_password_reset(
    State(state): State<AppState>,
    ClientIp(client_ip): ClientIp,
    ValidatedJson(body): ValidatedJson<PasswordResetConfirmRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .confirm_reset
        .execute(body.into_command(client_ip))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

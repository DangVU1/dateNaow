//! Request and response shapes for `/api/v1/auth`.
//!
//! The request fields carry no `#[validate]` rules: the 64 KiB body limit
//! caps their size, and the domain (`Email::parse`, `PlainPassword::parse`)
//! returns the field codes (`too_short`, `too_long`, `invalid_email`, ...).

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::modules::auth::application::confirm_password_reset::ConfirmPasswordResetCommand;
use crate::modules::auth::application::login::LoginCommand;
use crate::modules::auth::application::register::RegisterCommand;
use crate::modules::auth::application::request_password_reset::RequestPasswordResetCommand;
use crate::modules::auth::application::services::Session;
use crate::modules::users::domain::User;
use crate::modules::users::presentation::dto::UserResponse;

#[derive(Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    #[schema(example = "an@example.com")]
    pub email: String,
    /// 8 to 128 characters.
    pub password: String,
}

#[derive(Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    #[schema(example = "an@example.com")]
    pub email: String,
    pub password: String,
}

impl fmt::Debug for RegisterRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterRequest")
            .field("email", &"<redacted>")
            .field("password", &"***")
            .finish()
    }
}

impl fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginRequest")
            .field("email", &"<redacted>")
            .field("password", &"***")
            .finish()
    }
}

impl From<RegisterRequest> for RegisterCommand {
    fn from(request: RegisterRequest) -> Self {
        Self {
            email: request.email,
            password: request.password,
        }
    }
}

impl LoginRequest {
    pub fn into_command(self, client_ip: std::net::IpAddr) -> LoginCommand {
        LoginCommand {
            email: self.email,
            password: self.password,
            client_ip,
        }
    }
}

/// Returned by sign-up, sign-in and refresh. The refresh token travels only
/// in the `dn_refresh` cookie, never in the body.
#[derive(Serialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    /// Always `Bearer`.
    #[schema(example = "Bearer")]
    pub token_type: &'static str,
    /// Access-token lifetime in seconds.
    #[schema(example = 900)]
    pub expires_in: i64,
    pub user: UserResponse,
}

impl AuthResponse {
    pub fn new(user: &User, session: &Session) -> Self {
        Self {
            access_token: session.access_token.clone(),
            token_type: "Bearer",
            expires_in: session.access_expires_in_secs,
            user: UserResponse::from(user),
        }
    }
}

/// Written by hand so the access token never reaches logs.
impl fmt::Debug for AuthResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthResponse")
            .field("access_token", &"***")
            .field("expires_in", &self.expires_in)
            .field("user", &self.user)
            .finish()
    }
}

#[derive(Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRequest {
    #[schema(example = "an@example.com")]
    pub email: String,
}

#[derive(Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetConfirmRequest {
    /// The token from the reset link, after `#token=`.
    pub token: String,
    /// 8 to 128 characters.
    pub new_password: String,
}

impl fmt::Debug for PasswordResetRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasswordResetRequest")
            .field("email", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for PasswordResetConfirmRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasswordResetConfirmRequest")
            .field("token", &"***")
            .field("new_password", &"***")
            .finish()
    }
}

impl PasswordResetRequest {
    pub fn into_command(self, client_ip: std::net::IpAddr) -> RequestPasswordResetCommand {
        RequestPasswordResetCommand {
            email: self.email,
            client_ip,
        }
    }
}

impl From<PasswordResetConfirmRequest> for ConfirmPasswordResetCommand {
    fn from(request: PasswordResetConfirmRequest) -> Self {
        Self {
            token: request.token,
            new_password: request.new_password,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_request_debug_is_redacted() {
        let request = PasswordResetRequest {
            email: "an@example.com".to_owned(),
        };
        assert!(!format!("{request:?}").contains("an@example.com"));

        let confirm = PasswordResetConfirmRequest {
            token: "secret-token".to_owned(),
            new_password: "secret-password".to_owned(),
        };
        let debug = format!("{confirm:?}");
        assert!(!debug.contains("secret-token"), "{debug}");
        assert!(!debug.contains("secret-password"), "{debug}");
    }
}

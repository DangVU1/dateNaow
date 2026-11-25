//! Sessions: a short-lived access token (JWT) plus a long-lived, revocable
//! refresh token of which only the hash is stored (P3).

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::modules::auth::application::error::AuthError;
use crate::modules::auth::application::services::jwt::JwtService;
use crate::modules::auth::domain::{OpaqueToken, RefreshTokenRepository};
use crate::modules::users::domain::{User, UserRepository};
use crate::shared::clock::Clock;

/// What a successful sign-in, sign-up or refresh hands to the client.
pub struct Session {
    pub access_token: String,
    pub access_expires_in_secs: i64,
    /// Sent to the client once, in the refresh cookie; never stored.
    pub refresh_token: OpaqueToken,
    pub refresh_expires_at: DateTime<Utc>,
}

/// Written by hand so the access token never reaches logs.
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("access_token", &"***")
            .field("access_expires_in_secs", &self.access_expires_in_secs)
            .field("refresh_token", &self.refresh_token)
            .field("refresh_expires_at", &self.refresh_expires_at)
            .finish()
    }
}

pub struct SessionService<U: UserRepository, R: RefreshTokenRepository> {
    #[expect(
        dead_code,
        reason = "read by `refresh` (Task 7) to load the user's current role"
    )]
    users: U,
    tokens: R,
    jwt: Arc<JwtService>,
    clock: Arc<dyn Clock>,
    refresh_ttl: chrono::Duration,
}

impl<U: UserRepository, R: RefreshTokenRepository> SessionService<U, R> {
    pub fn new(
        users: U,
        tokens: R,
        jwt: Arc<JwtService>,
        clock: Arc<dyn Clock>,
        refresh_ttl: chrono::Duration,
    ) -> Self {
        Self {
            users,
            tokens,
            jwt,
            clock,
            refresh_ttl,
        }
    }

    /// Starts a new session for `user`: issues an access token and stores the
    /// hash of a new refresh token that expires `refresh_ttl` from now.
    pub async fn start(&self, user: &User) -> Result<Session, AuthError> {
        let now = self.clock.now();
        let access_token = self
            .jwt
            .issue(user.id, user.role, now)
            .map_err(AuthError::internal)?;
        let refresh_token = OpaqueToken::generate().map_err(AuthError::internal)?;
        let refresh_expires_at = self.refresh_expiry(now)?;

        self.tokens
            .insert(user.id, refresh_token.hash(), refresh_expires_at)
            .await
            .map_err(AuthError::internal)?;

        Ok(Session {
            access_token,
            access_expires_in_secs: self.jwt.access_ttl_secs(),
            refresh_token,
            refresh_expires_at,
        })
    }

    fn refresh_expiry(&self, now: DateTime<Utc>) -> Result<DateTime<Utc>, AuthError> {
        now.checked_add_signed(self.refresh_ttl)
            .ok_or_else(|| AuthError::Internal("refresh token expiry out of range".into()))
    }
}

use std::sync::Arc;

use crate::modules::auth::application::error::AuthError;
use crate::modules::auth::application::login::LoginCommand;
use crate::modules::auth::application::services::attempt_limiter::{
    AttemptLimiter, LOGIN_PER_EMAIL, LOGIN_PER_IP, LimitKey,
};
use crate::modules::auth::application::services::{PasswordHashService, Session, SessionService};
use crate::modules::auth::domain::{AttemptRepository, PlainPassword, RefreshTokenRepository};
use crate::modules::users::domain::{Email, User, UserRepository};

/// Action prefix of the sign-in counter keys.
const ACTION: &str = "login";

pub struct LoginHandler<U: UserRepository, R: RefreshTokenRepository, A: AttemptRepository> {
    users: U,
    hasher: PasswordHashService,
    limiter: AttemptLimiter<A>,
    sessions: Arc<SessionService<U, R>>,
}

impl<U, R, A> LoginHandler<U, R, A>
where
    U: UserRepository,
    R: RefreshTokenRepository,
    A: AttemptRepository,
{
    pub fn new(
        users: U,
        hasher: PasswordHashService,
        limiter: AttemptLimiter<A>,
        sessions: Arc<SessionService<U, R>>,
    ) -> Self {
        Self {
            users,
            hasher,
            limiter,
            sessions,
        }
    }

    /// Unknown email and wrong password both give `InvalidCredentials`, after
    /// the same Argon2 cost, so sign-in reveals nothing about which emails
    /// have accounts.
    pub async fn execute(&self, cmd: LoginCommand) -> Result<(User, Session), AuthError> {
        // 1. Malformed input cannot match any account: reject without counting.
        let (Ok(email), Ok(password)) =
            (Email::parse(&cmd.email), PlainPassword::parse(cmd.password))
        else {
            return Err(AuthError::InvalidCredentials);
        };

        // 2. Record first, count second; over the limit means no password check.
        let email_key = LimitKey::email(ACTION, &email, LOGIN_PER_EMAIL);
        let ip_key = LimitKey::ip(ACTION, cmd.client_ip, LOGIN_PER_IP);
        let ticket = self
            .limiter
            .acquire(&[email_key.clone(), ip_key.clone()])
            .await?;

        let credentials = self
            .users
            .find_credentials_by_email(&email)
            .await
            .map_err(AuthError::internal)?;

        // 3. Unknown email: spend the same time as a real check. Still counted.
        let Some(credentials) = credentials else {
            self.hasher.verify_dummy(&password).await;
            return Err(AuthError::InvalidCredentials);
        };

        // 4. Wrong password. Still counted.
        let matches = self
            .hasher
            .verify(&password, &credentials.password_hash)
            .await
            .map_err(AuthError::internal)?;
        if !matches {
            return Err(AuthError::InvalidCredentials);
        }

        // 5. Success: the email's failures are cleared, this attempt is not
        //    counted against the IP.
        self.limiter.clear(&email_key).await;
        self.limiter.forgive(&ticket, &ip_key).await;

        // Loaded by id so the session carries the user as stored right now.
        let user = self
            .users
            .find_by_id(credentials.user_id)
            .await
            .map_err(AuthError::internal)?
            .ok_or(AuthError::InvalidCredentials)?;
        let session = self.sessions.start(&user).await?;
        tracing::info!(user_id = %user.id, "user signed in");
        Ok((user, session))
    }
}

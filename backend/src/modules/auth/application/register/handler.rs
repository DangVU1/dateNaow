use std::sync::Arc;

use crate::modules::auth::application::error::AuthError;
use crate::modules::auth::application::register::RegisterCommand;
use crate::modules::auth::application::services::{PasswordHashService, Session, SessionService};
use crate::modules::auth::domain::{PlainPassword, RefreshTokenRepository};
use crate::modules::users::domain::{Email, NewUser, User, UserRepoError, UserRepository};
use crate::shared::error::FieldErrors;

pub struct RegisterHandler<U: UserRepository, R: RefreshTokenRepository> {
    users: U,
    hasher: PasswordHashService,
    sessions: Arc<SessionService<U, R>>,
}

impl<U: UserRepository, R: RefreshTokenRepository> RegisterHandler<U, R> {
    pub fn new(users: U, hasher: PasswordHashService, sessions: Arc<SessionService<U, R>>) -> Self {
        Self {
            users,
            hasher,
            sessions,
        }
    }

    /// Validates email and password (reporting both at once), hashes the
    /// password, creates the user and starts a session.
    pub async fn execute(&self, cmd: RegisterCommand) -> Result<(User, Session), AuthError> {
        let email = Email::parse(&cmd.email);
        // Counts characters before any hashing, so an oversized password is
        // rejected without running Argon2 (P4).
        let password = PlainPassword::parse(cmd.password);

        let (email, password) = match (email, password) {
            (Ok(email), Ok(password)) => (email, password),
            (email, password) => {
                let mut fields = FieldErrors::new();
                if let Err(e) = email {
                    fields.insert("email".to_owned(), vec![e.code().to_owned()]);
                }
                if let Err(e) = password {
                    fields.insert("password".to_owned(), vec![e.code().to_owned()]);
                }
                return Err(AuthError::Validation(fields));
            }
        };

        let password_hash = self
            .hasher
            .hash(&password)
            .await
            .map_err(AuthError::internal)?;
        let user = self
            .users
            .create(NewUser {
                email,
                password_hash: password_hash.clone(),
            })
            .await
            .map_err(|e| match e {
                UserRepoError::EmailTaken => AuthError::EmailTaken,
                other => AuthError::internal(other),
            })?;

        let session = self.sessions.start(&user, &password_hash).await?;
        tracing::info!(user_id = %user.id, "user registered");
        Ok((user, session))
    }
}

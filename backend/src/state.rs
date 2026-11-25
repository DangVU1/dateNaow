//! Shared application state handed to every handler through Axum `State`.
//! This is the composition root: concrete repositories and services are
//! wired together here.

use std::sync::Arc;

use anyhow::Context;
use sqlx::PgPool;

use crate::config::Config;
use crate::modules::auth::application::login::LoginHandler;
use crate::modules::auth::application::register::RegisterHandler;
use crate::modules::auth::application::services::{
    AttemptLimiter, JwtService, PasswordHashService, SessionService,
};
use crate::modules::auth::infrastructure::{PgAttemptRepository, PgRefreshTokenRepository};
use crate::modules::users::infrastructure::PgUserRepository;
use crate::shared::clock::Clock;

pub type Sessions = SessionService<PgUserRepository, PgRefreshTokenRepository>;
pub type Register = RegisterHandler<PgUserRepository, PgRefreshTokenRepository>;
pub type Login = LoginHandler<PgUserRepository, PgRefreshTokenRepository, PgAttemptRepository>;

/// The auth use cases, each shared behind an `Arc`.
#[derive(Clone)]
pub struct AuthHandlers {
    pub register: Arc<Register>,
    pub login: Arc<Login>,
    pub sessions: Arc<Sessions>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub clock: Arc<dyn Clock>,
    pub jwt: Arc<JwtService>,
    pub users_repo: PgUserRepository,
    pub auth: AuthHandlers,
}

impl AppState {
    /// Builds every dependency. Runs one Argon2 hash (the dummy hash used to
    /// equalise sign-in timing), so call it once at startup.
    pub fn new(config: Config, db: PgPool, clock: Arc<dyn Clock>) -> anyhow::Result<Self> {
        let jwt = Arc::new(JwtService::new(
            config.jwt_secret.as_bytes(),
            chrono::Duration::seconds(config.access_token_ttl_secs),
        ));
        let hasher = PasswordHashService::new(
            config
                .argon2_params()
                .context("invalid Argon2 parameters")?,
        );
        let users_repo = PgUserRepository::new(db.clone());

        let sessions = Arc::new(SessionService::new(
            users_repo.clone(),
            PgRefreshTokenRepository::new(db.clone()),
            jwt.clone(),
            clock.clone(),
            chrono::Duration::days(config.refresh_token_ttl_days),
        ));
        let register = Arc::new(RegisterHandler::new(
            users_repo.clone(),
            hasher.clone(),
            sessions.clone(),
        ));
        let login = Arc::new(LoginHandler::new(
            users_repo.clone(),
            hasher,
            AttemptLimiter::new(PgAttemptRepository::new(db.clone()), clock.clone()),
            sessions.clone(),
        ));

        Ok(Self {
            db,
            config: Arc::new(config),
            clock,
            jwt,
            users_repo,
            auth: AuthHandlers {
                register,
                login,
                sessions,
            },
        })
    }
}

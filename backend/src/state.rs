//! Shared application state handed to every handler through Axum `State`.
//! This is the composition root: concrete repositories and services are
//! wired together here.

use std::sync::Arc;

use anyhow::Context;
use sqlx::PgPool;

use crate::config::Config;
use crate::modules::auth::application::confirm_password_reset::ConfirmPasswordResetHandler;
use crate::modules::auth::application::login::LoginHandler;
use crate::modules::auth::application::ports::EmailSender;
use crate::modules::auth::application::register::RegisterHandler;
use crate::modules::auth::application::request_password_reset::RequestPasswordResetHandler;
use crate::modules::auth::application::services::{
    AttemptLimiter, JwtService, PasswordHashService, SessionService,
};
use crate::modules::auth::infrastructure::{
    PgAttemptRepository, PgPasswordResetRepository, PgRefreshTokenRepository,
};
use crate::modules::users::application::get_current_user::GetCurrentUserHandler;
use crate::modules::users::application::update_gender::UpdateGenderHandler;
use crate::modules::users::infrastructure::PgUserRepository;
use crate::shared::clock::Clock;

pub type Sessions = SessionService<PgUserRepository, PgRefreshTokenRepository>;
pub type Register =
    RegisterHandler<PgUserRepository, PgRefreshTokenRepository, PgAttemptRepository>;
pub type Login = LoginHandler<PgUserRepository, PgRefreshTokenRepository, PgAttemptRepository>;
pub type RequestReset =
    RequestPasswordResetHandler<PgUserRepository, PgPasswordResetRepository, PgAttemptRepository>;
pub type ConfirmReset =
    ConfirmPasswordResetHandler<PgUserRepository, PgPasswordResetRepository, PgAttemptRepository>;
pub type GetCurrentUser = GetCurrentUserHandler<PgUserRepository>;
pub type UpdateGender = UpdateGenderHandler<PgUserRepository>;

/// The auth use cases, each shared behind an `Arc`.
#[derive(Clone)]
pub struct AuthHandlers {
    pub register: Arc<Register>,
    pub login: Arc<Login>,
    pub sessions: Arc<Sessions>,
    pub request_reset: Arc<RequestReset>,
    pub confirm_reset: Arc<ConfirmReset>,
}

/// The users use cases, each shared behind an `Arc`.
#[derive(Clone)]
pub struct UsersHandlers {
    pub get_current_user: Arc<GetCurrentUser>,
    pub update_gender: Arc<UpdateGender>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub clock: Arc<dyn Clock>,
    pub jwt: Arc<JwtService>,
    pub users_repo: PgUserRepository,
    pub auth: AuthHandlers,
    pub users: UsersHandlers,
}

impl AppState {
    /// Builds every dependency. Runs one Argon2 hash (the dummy hash used to
    /// equalise sign-in timing), so call it once at startup.
    pub fn new(
        config: Config,
        db: PgPool,
        clock: Arc<dyn Clock>,
        mailer: Arc<dyn EmailSender>,
    ) -> anyhow::Result<Self> {
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
        let limiter = || AttemptLimiter::new(PgAttemptRepository::new(db.clone()), clock.clone());
        let register = Arc::new(RegisterHandler::new(
            users_repo.clone(),
            hasher.clone(),
            limiter(),
            sessions.clone(),
        ));
        let login = Arc::new(LoginHandler::new(
            users_repo.clone(),
            hasher.clone(),
            limiter(),
            sessions.clone(),
        ));
        let request_reset = Arc::new(RequestPasswordResetHandler::new(
            users_repo.clone(),
            PgPasswordResetRepository::new(db.clone()),
            limiter(),
            mailer,
            clock.clone(),
            config.app_base_url.clone(),
            chrono::Duration::minutes(config.reset_token_ttl_mins),
        ));
        let confirm_reset = Arc::new(ConfirmPasswordResetHandler::new(
            users_repo.clone(),
            PgPasswordResetRepository::new(db.clone()),
            hasher,
            limiter(),
            clock.clone(),
        ));

        Ok(Self {
            db,
            config: Arc::new(config),
            clock,
            jwt,
            auth: AuthHandlers {
                register,
                login,
                sessions,
                request_reset,
                confirm_reset,
            },
            users: UsersHandlers {
                get_current_user: Arc::new(GetCurrentUserHandler::new(users_repo.clone())),
                update_gender: Arc::new(UpdateGenderHandler::new(users_repo.clone())),
            },
            users_repo,
        })
    }
}

//! Auth domain: the password policy, opaque tokens and the repository
//! contracts for refresh tokens, password-reset tokens and failed-attempt
//! counters. No dependency on Axum, SQLx or HTTP.

pub mod repositories {
    pub mod attempt_repository;
    pub mod password_reset_repository;
    pub mod refresh_token_repository;
}
pub mod value_objects {
    pub mod opaque_token;
    pub mod password;
}

pub use repositories::attempt_repository::{AttemptRepoError, AttemptRepository};
pub use repositories::password_reset_repository::{
    PasswordResetRepoError, PasswordResetRepository, ResetTokenRecord,
};
pub use repositories::refresh_token_repository::{
    RefreshRepoError, RefreshTokenRecord, RefreshTokenRepository,
};
pub use value_objects::opaque_token::{InvalidToken, OpaqueToken, TokenGenerationError};
pub use value_objects::password::{PasswordError, PlainPassword};

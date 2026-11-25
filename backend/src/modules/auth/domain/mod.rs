//! Auth domain: the password policy, opaque tokens and the refresh-token
//! repository contract. No dependency on Axum, SQLx or HTTP.

pub mod repositories {
    pub mod refresh_token_repository;
}
pub mod value_objects {
    pub mod opaque_token;
    pub mod password;
}

pub use repositories::refresh_token_repository::{
    RefreshRepoError, RefreshTokenRecord, RefreshTokenRepository,
};
pub use value_objects::opaque_token::{InvalidToken, OpaqueToken, TokenGenerationError};
pub use value_objects::password::{PasswordError, PlainPassword};

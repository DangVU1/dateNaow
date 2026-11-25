//! Auth application layer: the module's error type and the services shared
//! by the auth use cases.

pub mod error;

pub mod services {
    pub mod jwt;
    pub mod password_hasher;
    pub mod session;

    pub use jwt::{AccessClaims, JwtService, TokenError};
    pub use password_hasher::{HashError, PasswordHashService};
    pub use session::{Session, SessionService};
}

pub use error::AuthError;

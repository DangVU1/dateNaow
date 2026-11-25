//! Auth application layer: the module's error type, its ports, the services
//! shared by the auth use cases, and the use cases themselves.

pub mod confirm_password_reset;
pub mod error;
pub mod login;
pub mod register;
pub mod request_password_reset;

pub mod ports {
    pub mod email_sender;

    pub use email_sender::{EmailSender, OutgoingEmail, SendEmailError};
}

pub mod services {
    pub mod attempt_limiter;
    pub mod jwt;
    pub mod password_hasher;
    pub mod session;

    pub use attempt_limiter::{AttemptLimiter, AttemptTicket, LimitKey, LimitPolicy};
    pub use jwt::{AccessClaims, JwtService, TokenError};
    pub use password_hasher::{HashError, PasswordHashService};
    pub use session::{Session, SessionService};
}

pub use error::AuthError;

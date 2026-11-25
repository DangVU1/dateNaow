//! Outgoing email port (P11). The only email the app sends is the
//! password-reset link (spec §2, §6.1). The SMTP implementation lives in
//! `crate::infrastructure::email`; tests use an in-memory fake.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::modules::users::domain::Email;

/// A plain-text email. `Debug` hides the recipient and the body, which holds
/// the reset token.
#[derive(Clone, PartialEq, Eq)]
pub struct OutgoingEmail {
    pub to: Email,
    pub subject: String,
    pub text_body: String,
}

impl fmt::Debug for OutgoingEmail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutgoingEmail")
            .field("to", &"<redacted>")
            .field("subject", &self.subject)
            .field("text_body", &"***")
            .finish()
    }
}

/// Why an email could not be sent. Deliberately carries no source error: SMTP
/// replies can echo the recipient address, and this error is logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SendEmailError {
    #[error("sender or recipient address is not accepted")]
    InvalidAddress,
    #[error("email message could not be built")]
    Build,
    /// The transport failed; the text names the kind of failure, e.g. `timeout`.
    #[error("email delivery failed: {0}")]
    Delivery(&'static str),
}

/// Sends one email. Object-safe so `AppState` can hold `Arc<dyn EmailSender>`.
pub trait EmailSender: Send + Sync {
    fn send(
        &self,
        email: OutgoingEmail,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendEmailError>> + Send + '_>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_recipient_and_body() {
        let email = OutgoingEmail {
            to: Email::parse("an@example.com").unwrap(),
            subject: "Subject".to_owned(),
            text_body: "token=abc".to_owned(),
        };
        let debug = format!("{email:?}");
        assert!(!debug.contains("an@example.com"), "{debug}");
        assert!(!debug.contains("token=abc"), "{debug}");
    }
}

//! SMTP implementation of `EmailSender`, built on `lettre`.
//!
//! Plan 1 only sends to Mailpit in development, over plain SMTP without TLS
//! or authentication (P6). TLS for a real SMTP provider comes with the
//! deployment plan.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::Context;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::modules::auth::application::ports::{EmailSender, OutgoingEmail, SendEmailError};

/// Upper bound for one SMTP exchange, so a stuck server cannot pile up tasks.
const SMTP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct SmtpEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpEmailSender {
    /// `from` is a mailbox such as `dateNaow <no-reply@datenaow.local>`.
    pub fn new(host: &str, port: u16, from: &str) -> anyhow::Result<Self> {
        let from = from
            .parse::<Mailbox>()
            .context("SMTP_FROM must be a mailbox such as `Name <user@example.com>`")?;
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .timeout(Some(SMTP_TIMEOUT))
            .build();
        Ok(Self { transport, from })
    }

    fn message(&self, email: OutgoingEmail) -> Result<Message, SendEmailError> {
        let to = email
            .to
            .as_str()
            .parse::<Mailbox>()
            .map_err(|_| SendEmailError::InvalidAddress)?;
        Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(email.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(email.text_body)
            .map_err(|_| SendEmailError::Build)
    }
}

impl EmailSender for SmtpEmailSender {
    fn send(
        &self,
        email: OutgoingEmail,
    ) -> Pin<Box<dyn Future<Output = Result<(), SendEmailError>> + Send + '_>> {
        Box::pin(async move {
            let message = self.message(email)?;
            self.transport
                .send(message)
                .await
                .map(|_| ())
                .map_err(|e| SendEmailError::Delivery(failure_kind(&e)))
        })
    }
}

/// A coarse label for a transport error. The error itself is not kept: its
/// text can include the SMTP server's reply, which may echo the address.
fn failure_kind(error: &lettre::transport::smtp::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_transient() {
        "transient SMTP reply"
    } else if error.is_permanent() {
        "permanent SMTP reply"
    } else {
        "connection or protocol error"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::users::domain::Email;

    fn email() -> OutgoingEmail {
        OutgoingEmail {
            to: Email::parse("an@example.com").unwrap(),
            subject: "Đặt lại mật khẩu dateNaow".to_owned(),
            text_body: "Xin chào".to_owned(),
        }
    }

    #[test]
    fn new_rejects_invalid_from() {
        assert!(SmtpEmailSender::new("localhost", 1025, "not a mailbox").is_err());
        assert!(
            SmtpEmailSender::new("localhost", 1025, "dateNaow <no-reply@datenaow.local>").is_ok()
        );
    }

    #[test]
    fn message_has_sender_recipient_subject_and_plain_text_body() {
        let sender =
            SmtpEmailSender::new("localhost", 1025, "dateNaow <no-reply@datenaow.local>").unwrap();
        let formatted = String::from_utf8(sender.message(email()).unwrap().formatted()).unwrap();
        assert!(
            formatted.contains("From: dateNaow <no-reply@datenaow.local>"),
            "{formatted}"
        );
        assert!(formatted.contains("To: an@example.com"), "{formatted}");
        assert!(
            formatted.contains("Content-Type: text/plain; charset=utf-8"),
            "{formatted}"
        );
        // The Vietnamese subject is RFC 2047 encoded.
        assert!(formatted.contains("Subject: =?utf-8?"), "{formatted}");
    }
}

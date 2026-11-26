use std::sync::Arc;

use chrono::Duration;
use tracing::Instrument;

use crate::modules::auth::application::error::AuthError;
use crate::modules::auth::application::ports::{EmailSender, OutgoingEmail};
use crate::modules::auth::application::request_password_reset::RequestPasswordResetCommand;
use crate::modules::auth::application::services::attempt_limiter::{
    AttemptLimiter, LimitKey, RESET_ACTION, RESET_PER_EMAIL, RESET_PER_IP, RESET_PER_PAIR,
};
use crate::modules::auth::domain::{AttemptRepository, OpaqueToken, PasswordResetRepository};
use crate::modules::users::domain::{Email, UserRepository};
use crate::shared::clock::Clock;
use crate::shared::error::FieldErrors;

pub const RESET_EMAIL_SUBJECT: &str = "Đặt lại mật khẩu dateNaow";

pub struct RequestPasswordResetHandler<U, R, A>
where
    U: UserRepository,
    R: PasswordResetRepository,
    A: AttemptRepository,
{
    users: U,
    resets: R,
    limiter: AttemptLimiter<A>,
    mailer: Arc<dyn EmailSender>,
    clock: Arc<dyn Clock>,
    app_base_url: String,
    token_ttl: Duration,
}

impl<U, R, A> RequestPasswordResetHandler<U, R, A>
where
    U: UserRepository,
    R: PasswordResetRepository,
    A: AttemptRepository,
{
    pub fn new(
        users: U,
        resets: R,
        limiter: AttemptLimiter<A>,
        mailer: Arc<dyn EmailSender>,
        clock: Arc<dyn Clock>,
        app_base_url: String,
        token_ttl: Duration,
    ) -> Self {
        Self {
            users,
            resets,
            limiter,
            mailer,
            clock,
            app_base_url,
            token_ttl,
        }
    }

    /// Answers `Ok(())` whether or not the email has an account, so the
    /// endpoint reveals nothing about which emails are registered (P6). The
    /// email itself is sent from a background task.
    pub async fn execute(&self, cmd: RequestPasswordResetCommand) -> Result<(), AuthError> {
        // 1. Malformed email: a 422, like any other invalid input.
        let email = Email::parse(&cmd.email).map_err(|e| {
            let mut fields = FieldErrors::new();
            fields.insert("email".to_owned(), vec![e.code().to_owned()]);
            AuthError::Validation(fields)
        })?;

        // 2. Record first, count second (P5, #13). Unknown emails count too.
        //    The email's global counter is reached only through the local
        //    tier, so one address cannot use it up and block recovery.
        self.limiter
            .acquire(&[
                &[
                    LimitKey::pair(RESET_ACTION, &email, cmd.client_ip, RESET_PER_PAIR),
                    LimitKey::ip(RESET_ACTION, cmd.client_ip, RESET_PER_IP),
                ],
                &[LimitKey::email(RESET_ACTION, &email, RESET_PER_EMAIL)],
            ])
            .await?;

        // 3. No account: nothing to send.
        let Some(credentials) = self
            .users
            .find_credentials_by_email(&email)
            .await
            .map_err(AuthError::internal)?
        else {
            return Ok(());
        };

        // 4. A new token replaces every earlier one of this user.
        let now = self.clock.now();
        let token = OpaqueToken::generate().map_err(AuthError::internal)?;
        self.resets
            .replace_for_user(credentials.user_id, token.hash(), now + self.token_ttl, now)
            .await
            .map_err(AuthError::internal)?;

        // 5. Fire and forget, so the response does not wait for SMTP.
        let message = reset_email(
            email,
            &self.app_base_url,
            &token,
            self.token_ttl.num_minutes(),
        );
        let mailer = self.mailer.clone();
        // Instrumented with the request span so a failure keeps its request id.
        tokio::spawn(
            async move {
                if let Err(e) = mailer.send(message).await {
                    // Neither the token nor the address is logged.
                    tracing::error!(error = %e, "password reset email failed");
                }
            }
            .instrument(tracing::Span::current()),
        );
        tracing::info!(user_id = %credentials.user_id, "password reset requested");
        Ok(())
    }
}

/// The reset email for `to`. The token sits after `#`, so browsers never send
/// it to the server and it stays out of access logs (P6).
pub fn reset_email(
    to: Email,
    app_base_url: &str,
    token: &OpaqueToken,
    ttl_mins: i64,
) -> OutgoingEmail {
    let base = app_base_url.trim_end_matches('/');
    let text_body = format!(
        "Bạn (hoặc ai đó) vừa yêu cầu đặt lại mật khẩu.\n\
         Mở link này trong {ttl_mins} phút để đặt mật khẩu mới:\n\
         {base}/reset-password#token={token}\n\
         Nếu không phải bạn, cứ bỏ qua email này.",
        token = token.as_str(),
    );
    OutgoingEmail {
        to,
        subject: RESET_EMAIL_SUBJECT.to_owned(),
        text_body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> OpaqueToken {
        OpaqueToken::from_client(&"ab".repeat(32)).unwrap()
    }

    fn email() -> Email {
        Email::parse("an@example.com").unwrap()
    }

    #[test]
    fn reset_email_matches_the_planned_text() {
        let message = reset_email(email(), "http://localhost:5173", &token(), 30);
        assert_eq!(message.to, email());
        assert_eq!(message.subject, "Đặt lại mật khẩu dateNaow");
        assert_eq!(
            message.text_body,
            format!(
                "Bạn (hoặc ai đó) vừa yêu cầu đặt lại mật khẩu.\n\
                 Mở link này trong 30 phút để đặt mật khẩu mới:\n\
                 http://localhost:5173/reset-password#token={}\n\
                 Nếu không phải bạn, cứ bỏ qua email này.",
                "ab".repeat(32)
            )
        );
    }

    #[test]
    fn reset_email_drops_trailing_slash_of_base_url() {
        let message = reset_email(email(), "https://datenaow.vn/", &token(), 30);
        assert!(
            message
                .text_body
                .contains("https://datenaow.vn/reset-password#token="),
            "{}",
            message.text_body
        );
        assert!(!message.text_body.contains("vn//reset"));
    }

    #[test]
    fn reset_email_states_the_configured_lifetime() {
        let message = reset_email(email(), "http://localhost:5173", &token(), 45);
        assert!(message.text_body.contains("trong 45 phút"));
    }
}

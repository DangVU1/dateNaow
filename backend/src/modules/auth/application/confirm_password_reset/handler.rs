use std::sync::Arc;

use crate::modules::auth::application::confirm_password_reset::ConfirmPasswordResetCommand;
use crate::modules::auth::application::error::AuthError;
use crate::modules::auth::application::services::PasswordHashService;
use crate::modules::auth::application::services::attempt_limiter::{
    AttemptLimiter, LOGIN_ACTION, LOGIN_PER_EMAIL, LOGIN_PER_PAIR, LimitKey,
};
use crate::modules::auth::domain::{
    AttemptRepository, NewPassword, OpaqueToken, PasswordResetRepository,
};
use crate::modules::users::domain::UserRepository;
use crate::shared::clock::Clock;
use crate::shared::error::{FieldErrors, display_chain};

pub struct ConfirmPasswordResetHandler<U, R, A>
where
    U: UserRepository,
    R: PasswordResetRepository,
    A: AttemptRepository,
{
    users: U,
    resets: R,
    hasher: PasswordHashService,
    limiter: AttemptLimiter<A>,
    clock: Arc<dyn Clock>,
}

impl<U, R, A> ConfirmPasswordResetHandler<U, R, A>
where
    U: UserRepository,
    R: PasswordResetRepository,
    A: AttemptRepository,
{
    pub fn new(
        users: U,
        resets: R,
        hasher: PasswordHashService,
        limiter: AttemptLimiter<A>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            users,
            resets,
            hasher,
            limiter,
            clock,
        }
    }

    /// Sets the new password, consumes the token and revokes every refresh
    /// token, all in one transaction. A malformed, unknown, used or expired
    /// token gives `InvalidOrExpiredResetToken`.
    pub async fn execute(&self, cmd: ConfirmPasswordResetCommand) -> Result<(), AuthError> {
        // 1. The password first, so a typo or a common password (#16) does
        //    not burn the token.
        let password = NewPassword::parse(cmd.new_password).map_err(|e| {
            let mut fields = FieldErrors::new();
            fields.insert("new_password".to_owned(), vec![e.code().to_owned()]);
            AuthError::Validation(fields)
        })?;

        // 2. The token: well-formed, known, unused and unexpired.
        let token = OpaqueToken::from_client(&cmd.token)
            .map_err(|_| AuthError::InvalidOrExpiredResetToken)?;
        let record = self
            .resets
            .find_by_hash(token.hash())
            .await
            .map_err(AuthError::internal)?
            .ok_or(AuthError::InvalidOrExpiredResetToken)?;
        let now = self.clock.now();
        if record.used_at.is_some() || now >= record.expires_at {
            return Err(AuthError::InvalidOrExpiredResetToken);
        }

        // 3. Hash, then consume + set + revoke atomically. `false` means a
        //    concurrent request used the token in the meantime.
        let new_hash = self
            .hasher
            .hash(&password)
            .await
            .map_err(AuthError::internal)?;
        let consumed = self
            .resets
            .consume_and_set_password(record.id, record.user_id, &new_hash, now)
            .await
            .map_err(AuthError::internal)?;
        if !consumed {
            return Err(AuthError::InvalidOrExpiredResetToken);
        }
        tracing::info!(user_id = %record.user_id, "password reset completed");

        // 4. Lift the sign-in limits that could stop the new password from
        //    working at once: the email's global counter and the pair of the
        //    IP that reset. Other IPs' pairs stay (#13).
        match self.users.find_by_id(record.user_id).await {
            Ok(Some(user)) => {
                let email_key = LimitKey::email(LOGIN_ACTION, &user.email, LOGIN_PER_EMAIL);
                let pair_key =
                    LimitKey::pair(LOGIN_ACTION, &user.email, cmd.client_ip, LOGIN_PER_PAIR);
                self.limiter.clear(&email_key).await;
                self.limiter.clear(&pair_key).await;
            }
            Ok(None) => {}
            // The password is already changed; failing the request now would
            // only confuse the user, so the lockout simply runs out.
            Err(e) => {
                tracing::error!(
                    error = %display_chain(&e),
                    "could not clear sign-in failures after password reset"
                );
            }
        }
        Ok(())
    }
}

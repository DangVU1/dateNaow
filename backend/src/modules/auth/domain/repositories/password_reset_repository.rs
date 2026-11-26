use std::future::Future;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A stored password-reset token (P6). Only the hash is stored, and it is not
/// carried here because no caller needs it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetTokenRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    /// Set when the token was used, or replaced by a newer request.
    pub used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PasswordResetRepoError {
    /// Any storage failure. The source is kept for logs, never sent to clients.
    #[error("password reset storage error")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait PasswordResetRepository: Send + Sync + 'static {
    /// Atomically marks every unused token of `user_id` as used at `now` and
    /// stores the new one, so only the latest link works. Concurrent calls
    /// for one user are serialised: afterwards exactly one of their tokens is
    /// unused.
    fn replace_for_user(
        &self,
        user_id: Uuid,
        token_hash: [u8; 32],
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), PasswordResetRepoError>> + Send;

    fn find_by_hash(
        &self,
        token_hash: [u8; 32],
    ) -> impl Future<Output = Result<Option<ResetTokenRecord>, PasswordResetRepoError>> + Send;

    /// In one transaction: marks `token_id` used (only if it is still unused
    /// and unexpired at `now`), sets the user's password hash, and revokes
    /// every active refresh token of the user (`revoked_at` only, never
    /// `replaced_at`). Returns `false`, changing nothing, when the token was
    /// no longer usable, e.g. because a concurrent request consumed it first.
    fn consume_and_set_password(
        &self,
        token_id: Uuid,
        user_id: Uuid,
        new_hash: &str,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<bool, PasswordResetRepoError>> + Send;
}

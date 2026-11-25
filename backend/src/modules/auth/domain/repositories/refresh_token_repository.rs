use std::future::Future;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A stored refresh token. The token itself is never stored, only its hash,
/// and the hash is not carried here because no caller needs it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshTokenRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    /// Set when the token stops being usable: rotation, logout, password reset
    /// or reuse detection.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Set only by rotation, at the same time as `revoked_at` (P3).
    pub replaced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error)]
pub enum RefreshRepoError {
    /// `rotate` found the old token already revoked; nothing was changed.
    #[error("refresh token already revoked")]
    AlreadyRevoked,
    /// `insert` found the user gone or their password hash no longer equal to
    /// the one the caller checked; nothing was stored.
    #[error("user credentials changed")]
    CredentialsChanged,
    /// Any storage failure. The source is kept for logs, never sent to clients.
    #[error("refresh token storage error")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Every method that creates, rotates or revokes all tokens of a user first
/// locks that user's row, so it is ordered against a password change and
/// against the others: a token is never created alongside a concurrent
/// "revoke everything" that then misses it (P3, P6).
pub trait RefreshTokenRepository: Send + Sync + 'static {
    /// Stores a new token for `user_id`, but only while the user's stored
    /// password hash still equals `password_hash` (the one just verified at
    /// sign-in, or just set at sign-up). Fails with `CredentialsChanged`,
    /// storing nothing, otherwise, e.g. when a password reset committed after
    /// the password was checked.
    fn insert(
        &self,
        user_id: Uuid,
        password_hash: &str,
        token_hash: [u8; 32],
        expires_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<RefreshTokenRecord, RefreshRepoError>> + Send;

    fn find_by_hash(
        &self,
        token_hash: [u8; 32],
    ) -> impl Future<Output = Result<Option<RefreshTokenRecord>, RefreshRepoError>> + Send;

    /// In one transaction: marks `old_id` revoked and replaced at `now`, but
    /// only if it is not revoked yet, then inserts the new token. Fails with
    /// `AlreadyRevoked`, changing nothing, when `old_id` was already revoked
    /// (or does not exist), e.g. because a concurrent request rotated it first.
    fn rotate(
        &self,
        old_id: Uuid,
        user_id: Uuid,
        new_hash: [u8; 32],
        new_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<RefreshTokenRecord, RefreshRepoError>> + Send;

    /// Sets `revoked_at` (never `replaced_at`) if the token is still active.
    /// An unknown or already revoked id is not an error.
    fn revoke(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), RefreshRepoError>> + Send;

    /// Revokes every active token of `user_id` (sets `revoked_at` only) and
    /// returns how many were revoked.
    fn revoke_all_for_user(
        &self,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, RefreshRepoError>> + Send;
}

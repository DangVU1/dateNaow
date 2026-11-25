use std::future::Future;

use chrono::{DateTime, Utc};

#[derive(Debug, thiserror::Error)]
pub enum AttemptRepoError {
    /// Any storage failure. The source is kept for logs, never sent to clients.
    #[error("attempt counter storage error")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Storage for failed-attempt counters (P5). Keys arrive already hashed; this
/// trait never sees an email or IP address. Every method commits on its own,
/// so a recorded attempt is visible to concurrent counts at once.
pub trait AttemptRepository: Send + Sync + 'static {
    /// Records one attempt for `key_hash` and returns the new row's id.
    fn record(
        &self,
        key_hash: [u8; 32],
        at: DateTime<Utc>,
    ) -> impl Future<Output = Result<i64, AttemptRepoError>> + Send;

    /// Number of attempts for `key_hash` strictly after `since`.
    fn count_since(
        &self,
        key_hash: [u8; 32],
        since: DateTime<Utc>,
    ) -> impl Future<Output = Result<i64, AttemptRepoError>> + Send;

    /// Deletes one attempt. An unknown id is not an error.
    fn delete_by_id(&self, id: i64) -> impl Future<Output = Result<(), AttemptRepoError>> + Send;

    /// Deletes every attempt for `key_hash`.
    fn clear(
        &self,
        key_hash: [u8; 32],
    ) -> impl Future<Output = Result<(), AttemptRepoError>> + Send;

    /// Deletes every attempt recorded before `before`.
    fn delete_older_than(
        &self,
        before: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), AttemptRepoError>> + Send;
}

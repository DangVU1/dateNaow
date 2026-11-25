//! Failed-attempt limits for sign-in and password-reset requests (P5).
//!
//! An attempt is **recorded first and counted second**: each recorded row is
//! committed before its count runs, so the i-th of many concurrent requests
//! counts at least i rows and only the first `max` of them get through.
//!
//! Keys are labels such as `login:email:an@example.com` or
//! `login:ip:2001:db8:1:2::/64`. This module is the only place that hashes a
//! label (SHA-256) before it reaches storage, so the counter table never holds
//! a readable email or IP address.

use std::fmt;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::modules::auth::application::error::AuthError;
use crate::modules::auth::domain::{AttemptRepoError, AttemptRepository};
use crate::modules::users::domain::Email;
use crate::shared::clock::Clock;

/// At most `max` attempts per key within any `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitPolicy {
    pub max: i64,
    pub window: Duration,
}

/// Sign-in: 5 attempts per email in 15 minutes.
pub const LOGIN_PER_EMAIL: LimitPolicy = LimitPolicy {
    max: 5,
    window: Duration::minutes(15),
};
/// Sign-in: 20 attempts per IP address (IPv6: per /64) in 15 minutes.
pub const LOGIN_PER_IP: LimitPolicy = LimitPolicy {
    max: 20,
    window: Duration::minutes(15),
};
/// Password-reset requests: 3 per email in an hour.
pub const RESET_PER_EMAIL: LimitPolicy = LimitPolicy {
    max: 3,
    window: Duration::minutes(60),
};
/// Password-reset requests: 10 per IP address (IPv6: per /64) in an hour.
pub const RESET_PER_IP: LimitPolicy = LimitPolicy {
    max: 10,
    window: Duration::minutes(60),
};

/// Rows older than this are deleted on every `acquire`. Must be at least the
/// longest policy window.
const PRUNE_AFTER: Duration = Duration::minutes(60);

/// A counter key: a plain-text label plus the policy that applies to it.
/// `Debug` hides the email or address in the label.
#[derive(Clone, PartialEq, Eq)]
pub struct LimitKey {
    pub label: String,
    pub policy: LimitPolicy,
}

impl LimitKey {
    /// `"{action}:email:{email}"`, e.g. `login:email:an@example.com`.
    pub fn email(action: &str, email: &Email, policy: LimitPolicy) -> Self {
        Self {
            label: format!("{action}:email:{}", email.as_str()),
            policy,
        }
    }

    /// `"{action}:ip:{scope}"`: the full address for IPv4, the /64 network for
    /// IPv6 (one subscriber usually holds a whole /64). IPv4-mapped IPv6
    /// addresses count as their IPv4 address.
    pub fn ip(action: &str, ip: IpAddr, policy: LimitPolicy) -> Self {
        Self {
            label: format!("{action}:ip:{}", ip_scope(ip)),
            policy,
        }
    }

    fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.label.as_bytes()).into()
    }
}

impl fmt::Debug for LimitKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep "action:kind", drop the email or address.
        let kind: Vec<&str> = self.label.splitn(3, ':').take(2).collect();
        f.debug_struct("LimitKey")
            .field("label", &format!("{}:***", kind.join(":")))
            .field("policy", &self.policy)
            .finish()
    }
}

fn ip_scope(ip: IpAddr) -> String {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let network = Ipv6Addr::from(u128::from(v6) & !u128::from(u64::MAX));
            format!("{network}/64")
        }
    }
}

/// The rows recorded by one successful `acquire`, so a successful attempt can
/// be taken back with `forgive`.
#[derive(Debug)]
pub struct AttemptTicket {
    ids: Vec<(LimitKey, i64)>,
}

pub struct AttemptLimiter<A: AttemptRepository> {
    repo: A,
    clock: Arc<dyn Clock>,
}

impl<A: AttemptRepository> AttemptLimiter<A> {
    pub fn new(repo: A, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    /// Records one attempt for every key, then counts each key's attempts in
    /// its window (the new one included). If any key is over its `max`,
    /// returns `RateLimited` with the longest window of the exceeded keys; the
    /// recorded rows stay, because a blocked attempt is still an attempt.
    pub async fn acquire(&self, keys: &[LimitKey]) -> Result<AttemptTicket, AuthError> {
        let now = self.clock.now();
        self.prune(now).await;

        let mut ids = Vec::with_capacity(keys.len());
        for key in keys {
            let id = self
                .repo
                .record(key.hash(), now)
                .await
                .map_err(AuthError::internal)?;
            ids.push((key.clone(), id));
        }

        let mut retry_after: Option<u64> = None;
        for key in keys {
            let count = self
                .repo
                .count_since(key.hash(), now - key.policy.window)
                .await
                .map_err(AuthError::internal)?;
            if count > key.policy.max {
                let secs = u64::try_from(key.policy.window.num_seconds()).unwrap_or(0);
                retry_after = Some(retry_after.map_or(secs, |current| current.max(secs)));
            }
        }

        match retry_after {
            Some(retry_after_secs) => Err(AuthError::RateLimited { retry_after_secs }),
            None => Ok(AttemptTicket { ids }),
        }
    }

    /// Deletes the attempt `ticket` recorded for `key`, e.g. because the
    /// sign-in succeeded. Best effort: a storage failure is only logged.
    pub async fn forgive(&self, ticket: &AttemptTicket, key: &LimitKey) {
        let Some((_, id)) = ticket.ids.iter().find(|(k, _)| k.label == key.label) else {
            return;
        };
        if let Err(e) = self.repo.delete_by_id(*id).await {
            log_storage_error("failed to forgive attempt", &e);
        }
    }

    /// Deletes every attempt for `key`. Best effort: a storage failure is only
    /// logged.
    pub async fn clear(&self, key: &LimitKey) {
        if let Err(e) = self.repo.clear(key.hash()).await {
            log_storage_error("failed to clear attempts", &e);
        }
    }

    async fn prune(&self, now: DateTime<Utc>) {
        if let Err(e) = self.repo.delete_older_than(now - PRUNE_AFTER).await {
            log_storage_error("failed to prune old attempts", &e);
        }
    }
}

/// Logs with `Display` only: the `Debug` of a database error can hold row data.
fn log_storage_error(message: &str, error: &AttemptRepoError) {
    let AttemptRepoError::Database(cause) = error;
    tracing::error!(error = %error, cause = %cause, "{message}");
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// `(id, key_hash, attempted_at)`.
    type Row = (i64, [u8; 32], DateTime<Utc>);

    #[derive(Default)]
    struct MemoryRepo {
        rows: Mutex<Vec<Row>>,
        /// Monotonic, like the BIGINT identity column: ids are never reused.
        next_id: std::sync::atomic::AtomicI64,
    }

    impl AttemptRepository for Arc<MemoryRepo> {
        async fn record(
            &self,
            key_hash: [u8; 32],
            at: DateTime<Utc>,
        ) -> Result<i64, AttemptRepoError> {
            let mut rows = self.rows.lock().unwrap();
            let id = self
                .next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            rows.push((id, key_hash, at));
            Ok(id)
        }

        async fn count_since(
            &self,
            key_hash: [u8; 32],
            since: DateTime<Utc>,
        ) -> Result<i64, AttemptRepoError> {
            let rows = self.rows.lock().unwrap();
            let n = rows
                .iter()
                .filter(|(_, k, at)| *k == key_hash && *at > since)
                .count();
            Ok(i64::try_from(n).unwrap())
        }

        async fn delete_by_id(&self, id: i64) -> Result<(), AttemptRepoError> {
            self.rows.lock().unwrap().retain(|(i, _, _)| *i != id);
            Ok(())
        }

        async fn clear(&self, key_hash: [u8; 32]) -> Result<(), AttemptRepoError> {
            self.rows.lock().unwrap().retain(|(_, k, _)| *k != key_hash);
            Ok(())
        }

        async fn delete_older_than(&self, before: DateTime<Utc>) -> Result<(), AttemptRepoError> {
            self.rows.lock().unwrap().retain(|(_, _, at)| *at >= before);
            Ok(())
        }
    }

    struct FixedClock(Mutex<DateTime<Utc>>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }
    }

    fn t0() -> DateTime<Utc> {
        "2026-11-25T09:00:00Z".parse().unwrap()
    }

    fn limiter() -> (
        AttemptLimiter<Arc<MemoryRepo>>,
        Arc<MemoryRepo>,
        Arc<FixedClock>,
    ) {
        let repo = Arc::new(MemoryRepo::default());
        let clock = Arc::new(FixedClock(Mutex::new(t0())));
        (
            AttemptLimiter::new(repo.clone(), clock.clone()),
            repo,
            clock,
        )
    }

    fn email_key() -> LimitKey {
        LimitKey::email(
            "login",
            &Email::parse("an@example.com").unwrap(),
            LOGIN_PER_EMAIL,
        )
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn ipv4_key_uses_full_address() {
        let key = LimitKey::ip("login", ip("203.0.113.7"), LOGIN_PER_IP);
        assert_eq!(key.label, "login:ip:203.0.113.7");
        assert_ne!(
            key.hash(),
            LimitKey::ip("login", ip("203.0.113.8"), LOGIN_PER_IP).hash()
        );
    }

    #[test]
    fn ipv6_key_uses_the_64_network() {
        let a = LimitKey::ip(
            "login",
            ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd"),
            LOGIN_PER_IP,
        );
        let b = LimitKey::ip("login", ip("2001:db8:1:2::1"), LOGIN_PER_IP);
        let other = LimitKey::ip("login", ip("2001:db8:1:3::1"), LOGIN_PER_IP);
        assert_eq!(a.label, "login:ip:2001:db8:1:2::/64");
        assert_eq!(a, b);
        assert_ne!(a.hash(), other.hash());
    }

    #[test]
    fn ipv4_mapped_ipv6_counts_as_ipv4() {
        // A dual-stack listener reports IPv4 clients as ::ffff:a.b.c.d; without
        // canonicalising, every IPv4 client would share one /64.
        let key = LimitKey::ip("login", ip("::ffff:203.0.113.7"), LOGIN_PER_IP);
        assert_eq!(key.label, "login:ip:203.0.113.7");
    }

    #[test]
    fn email_key_uses_normalised_email_and_action() {
        let key = LimitKey::email(
            "reset",
            &Email::parse(" An@Example.com ").unwrap(),
            RESET_PER_EMAIL,
        );
        assert_eq!(key.label, "reset:email:an@example.com");
        assert_ne!(key.hash(), email_key().hash(), "actions are counted apart");
    }

    #[test]
    fn hash_is_sha256_of_label() {
        let key = LimitKey::ip("login", ip("203.0.113.7"), LOGIN_PER_IP);
        let expected: [u8; 32] = Sha256::digest(b"login:ip:203.0.113.7").into();
        assert_eq!(key.hash(), expected);
    }

    #[test]
    fn debug_hides_email_and_address() {
        let debug = format!("{:?}", email_key());
        assert!(!debug.contains("an@example.com"), "{debug}");
        assert!(debug.contains("login:email:***"), "{debug}");
    }

    #[test]
    fn policies_match_p5_and_fit_the_prune_window() {
        assert_eq!(
            (LOGIN_PER_EMAIL.max, LOGIN_PER_EMAIL.window.num_minutes()),
            (5, 15)
        );
        assert_eq!(
            (LOGIN_PER_IP.max, LOGIN_PER_IP.window.num_minutes()),
            (20, 15)
        );
        assert_eq!(
            (RESET_PER_EMAIL.max, RESET_PER_EMAIL.window.num_minutes()),
            (3, 60)
        );
        assert_eq!(
            (RESET_PER_IP.max, RESET_PER_IP.window.num_minutes()),
            (10, 60)
        );
        for policy in [LOGIN_PER_EMAIL, LOGIN_PER_IP, RESET_PER_EMAIL, RESET_PER_IP] {
            assert!(policy.window <= PRUNE_AFTER);
        }
    }

    #[tokio::test]
    async fn allows_max_attempts_then_limits_and_keeps_the_row() {
        let (limiter, repo, _) = limiter();
        let keys = [email_key()];
        for _ in 0..5 {
            assert!(limiter.acquire(&keys).await.is_ok());
        }
        let err = limiter.acquire(&keys).await.unwrap_err();
        assert!(
            matches!(
                err,
                AuthError::RateLimited {
                    retry_after_secs: 900
                }
            ),
            "{err:?}"
        );
        assert_eq!(repo.rows.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn retry_after_is_the_longest_exceeded_window() {
        let (limiter, _, _) = limiter();
        let short = LimitKey {
            label: "a".into(),
            policy: LimitPolicy {
                max: 0,
                window: Duration::minutes(15),
            },
        };
        let long = LimitKey {
            label: "b".into(),
            policy: LimitPolicy {
                max: 0,
                window: Duration::minutes(60),
            },
        };
        let err = limiter.acquire(&[short, long]).await.unwrap_err();
        assert!(
            matches!(
                err,
                AuthError::RateLimited {
                    retry_after_secs: 3600
                }
            ),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn forgive_removes_only_that_keys_row() {
        let (limiter, repo, _) = limiter();
        let ip_key = LimitKey::ip("login", ip("203.0.113.7"), LOGIN_PER_IP);
        let ticket = limiter
            .acquire(&[email_key(), ip_key.clone()])
            .await
            .unwrap();

        limiter.forgive(&ticket, &ip_key).await;

        let rows = repo.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, email_key().hash());
    }

    #[tokio::test]
    async fn clear_removes_every_row_of_the_key() {
        let (limiter, repo, _) = limiter();
        for _ in 0..3 {
            limiter.acquire(&[email_key()]).await.unwrap();
        }
        limiter.clear(&email_key()).await;
        assert!(repo.rows.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn attempts_outside_the_window_do_not_count_and_are_pruned() {
        let (limiter, repo, clock) = limiter();
        for _ in 0..5 {
            limiter.acquire(&[email_key()]).await.unwrap();
        }
        *clock.0.lock().unwrap() = t0() + Duration::minutes(15) + Duration::seconds(1);
        assert!(limiter.acquire(&[email_key()]).await.is_ok());

        *clock.0.lock().unwrap() = t0() + Duration::minutes(61);
        limiter.acquire(&[email_key()]).await.unwrap();
        // The five rows at t0 are older than 60 minutes and were deleted.
        assert_eq!(repo.rows.lock().unwrap().len(), 2);
    }
}

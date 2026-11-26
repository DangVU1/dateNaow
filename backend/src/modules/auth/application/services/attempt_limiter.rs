//! Attempt limits for sign-in, password-reset requests (P5, as amended by
//! DECISIONS #13) and sign-up (#17).
//!
//! Sign-up has a single counter per IP. Sign-in and reset requests have three
//! counters: one per (email, IP) pair, one per IP and one global counter per
//! email. They are acquired in **tiers**: the local
//! tier (pair + IP) first, and the global tier (email) only if the local tier
//! let the attempt through. An attempt blocked locally therefore never reaches
//! the email's global counter, so one IP (IPv6: one /64) can add at most
//! `*_PER_PAIR.max` rows per window to it, and a stranger hammering from one
//! address cannot lock the owner out.
//!
//! Within a tier an attempt is **recorded first and counted second**: each
//! recorded row is committed before its count runs, so the i-th of many
//! concurrent requests counts at least i rows and only the first `max` of
//! them get through.
//!
//! Keys are labels such as `login:email:an@example.com`,
//! `login:ip:2001:db8:1:2::/64` or `login:pair:an@example.com|203.0.113.7`.
//! This module is the only place that hashes a label (SHA-256) before it
//! reaches storage, so the counter table never holds a readable email or IP
//! address.

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

/// Action prefix of the sign-in counter keys (`login:pair:...`, `login:ip:...`,
/// `login:email:...`).
pub const LOGIN_ACTION: &str = "login";
/// Action prefix of the password-reset request counter keys.
pub const RESET_ACTION: &str = "reset";
/// Action prefix of the sign-up counter keys (`register:ip:...`).
pub const REGISTER_ACTION: &str = "register";

/// Sign-in: 5 attempts per (email, IP) pair in 15 minutes.
pub const LOGIN_PER_PAIR: LimitPolicy = LimitPolicy {
    max: 5,
    window: Duration::minutes(15),
};
/// Sign-in: 20 attempts per IP address (IPv6: per /64) in 15 minutes.
pub const LOGIN_PER_IP: LimitPolicy = LimitPolicy {
    max: 20,
    window: Duration::minutes(15),
};
/// Sign-in: 50 attempts per email from all IPs together in 15 minutes. Bounds
/// guessing spread over many addresses.
pub const LOGIN_PER_EMAIL: LimitPolicy = LimitPolicy {
    max: 50,
    window: Duration::minutes(15),
};
/// Password-reset requests: 3 per (email, IP) pair in an hour.
pub const RESET_PER_PAIR: LimitPolicy = LimitPolicy {
    max: 3,
    window: Duration::minutes(60),
};
/// Password-reset requests: 10 per IP address (IPv6: per /64) in an hour.
pub const RESET_PER_IP: LimitPolicy = LimitPolicy {
    max: 10,
    window: Duration::minutes(60),
};
/// Password-reset requests: 10 per email from all IPs together in an hour.
/// Bounds the reset emails one inbox can receive.
pub const RESET_PER_EMAIL: LimitPolicy = LimitPolicy {
    max: 10,
    window: Duration::minutes(60),
};

/// Sign-up: 10 attempts per IP address (IPv6: per /64) in an hour (#17).
/// Successful sign-ups count too, so the limit also bounds how many accounts
/// one address can create.
pub const REGISTER_PER_IP: LimitPolicy = LimitPolicy {
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

    /// `"{action}:pair:{email}|{scope}"`, e.g.
    /// `login:pair:an@example.com|203.0.113.7`, with the IP scope of
    /// [`LimitKey::ip`]. The scope never contains `|`, so the last `|` always
    /// splits the label unambiguously, and the `pair` kind keeps it apart from
    /// `email` and `ip` labels.
    pub fn pair(action: &str, email: &Email, ip: IpAddr, policy: LimitPolicy) -> Self {
        Self {
            label: format!("{action}:pair:{}|{}", email.as_str(), ip_scope(ip)),
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

    /// Acquires `tiers` in order. For each tier, records one attempt for every
    /// key, then counts each key's attempts in its window (the new one
    /// included). If any key of the tier is over its `max`, returns
    /// `RateLimited` with the longest window of that tier's exceeded keys and
    /// **does not touch the later tiers**; the rows already recorded stay,
    /// because a blocked attempt is still an attempt.
    pub async fn acquire(&self, tiers: &[&[LimitKey]]) -> Result<AttemptTicket, AuthError> {
        let now = self.clock.now();
        self.prune(now).await;

        let mut ids = Vec::with_capacity(tiers.iter().map(|tier| tier.len()).sum());
        for tier in tiers {
            self.acquire_tier(tier, now, &mut ids).await?;
        }
        Ok(AttemptTicket { ids })
    }

    async fn acquire_tier(
        &self,
        keys: &[LimitKey],
        now: DateTime<Utc>,
        ids: &mut Vec<(LimitKey, i64)>,
    ) -> Result<(), AuthError> {
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
            None => Ok(()),
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
            LimitPolicy {
                max: 5,
                window: Duration::minutes(15),
            },
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
    fn register_key_is_per_ip_scope_and_apart_from_other_actions() {
        let key = LimitKey::ip(REGISTER_ACTION, ip("2001:db8:1:2::9"), REGISTER_PER_IP);
        assert_eq!(key.label, "register:ip:2001:db8:1:2::/64");
        let addr = ip("203.0.113.7");
        let register = LimitKey::ip(REGISTER_ACTION, addr, REGISTER_PER_IP);
        for other in [
            LimitKey::ip(LOGIN_ACTION, addr, LOGIN_PER_IP),
            LimitKey::ip(RESET_ACTION, addr, RESET_PER_IP),
        ] {
            assert_ne!(register.hash(), other.hash(), "{}", other.label);
        }
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
    fn policies_match_decisions_13_and_17_and_fit_the_prune_window() {
        let minutes = |p: LimitPolicy| (p.max, p.window.num_minutes());
        assert_eq!(minutes(LOGIN_PER_PAIR), (5, 15));
        assert_eq!(minutes(LOGIN_PER_IP), (20, 15));
        assert_eq!(minutes(LOGIN_PER_EMAIL), (50, 15));
        assert_eq!(minutes(RESET_PER_PAIR), (3, 60));
        assert_eq!(minutes(RESET_PER_IP), (10, 60));
        assert_eq!(minutes(RESET_PER_EMAIL), (10, 60));
        assert_eq!(minutes(REGISTER_PER_IP), (10, 60));
        for policy in [
            LOGIN_PER_PAIR,
            LOGIN_PER_IP,
            LOGIN_PER_EMAIL,
            RESET_PER_PAIR,
            RESET_PER_IP,
            RESET_PER_EMAIL,
            REGISTER_PER_IP,
        ] {
            assert!(policy.window <= PRUNE_AFTER);
        }
    }

    #[test]
    fn pair_key_joins_normalised_email_and_ip_scope() {
        let email = Email::parse(" An@Example.com ").unwrap();
        let v4 = LimitKey::pair("login", &email, ip("::ffff:203.0.113.7"), LOGIN_PER_PAIR);
        assert_eq!(v4.label, "login:pair:an@example.com|203.0.113.7");
        let v6 = LimitKey::pair("reset", &email, ip("2001:db8:1:2::9"), RESET_PER_PAIR);
        assert_eq!(v6.label, "reset:pair:an@example.com|2001:db8:1:2::/64");
        assert_eq!(
            v6,
            LimitKey::pair("reset", &email, ip("2001:db8:1:2::1"), RESET_PER_PAIR),
            "one /64 is one pair"
        );
    }

    #[test]
    fn pair_key_is_counted_apart_from_other_kinds_and_pairs() {
        let email = Email::parse("an@example.com").unwrap();
        let other = Email::parse("bo@example.com").unwrap();
        let addr = ip("203.0.113.7");
        let pair = LimitKey::pair("login", &email, addr, LOGIN_PER_PAIR);
        for different in [
            LimitKey::pair("reset", &email, addr, RESET_PER_PAIR),
            LimitKey::pair("login", &other, addr, LOGIN_PER_PAIR),
            LimitKey::pair("login", &email, ip("203.0.113.8"), LOGIN_PER_PAIR),
            LimitKey::email("login", &email, LOGIN_PER_EMAIL),
            LimitKey::ip("login", addr, LOGIN_PER_IP),
        ] {
            assert_ne!(pair.hash(), different.hash(), "{}", different.label);
        }
    }

    #[test]
    fn debug_hides_email_and_address_of_a_pair() {
        let key = LimitKey::pair(
            "login",
            &Email::parse("an@example.com").unwrap(),
            ip("203.0.113.7"),
            LOGIN_PER_PAIR,
        );
        let debug = format!("{key:?}");
        assert!(!debug.contains("an@example.com"), "{debug}");
        assert!(!debug.contains("203.0.113.7"), "{debug}");
        assert!(debug.contains("login:pair:***"), "{debug}");
    }

    #[tokio::test]
    async fn allows_max_attempts_then_limits_and_keeps_the_row() {
        let (limiter, repo, _) = limiter();
        let keys = [email_key()];
        for _ in 0..5 {
            assert!(limiter.acquire(&[&keys]).await.is_ok());
        }
        let err = limiter.acquire(&[&keys]).await.unwrap_err();
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
        let err = limiter.acquire(&[&[short, long]]).await.unwrap_err();
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
            .acquire(&[&[email_key(), ip_key.clone()]])
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
            limiter.acquire(&[&[email_key()]]).await.unwrap();
        }
        limiter.clear(&email_key()).await;
        assert!(repo.rows.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn attempts_outside_the_window_do_not_count_and_are_pruned() {
        let (limiter, repo, clock) = limiter();
        for _ in 0..5 {
            limiter.acquire(&[&[email_key()]]).await.unwrap();
        }
        *clock.0.lock().unwrap() = t0() + Duration::minutes(15) + Duration::seconds(1);
        assert!(limiter.acquire(&[&[email_key()]]).await.is_ok());

        *clock.0.lock().unwrap() = t0() + Duration::minutes(61);
        limiter.acquire(&[&[email_key()]]).await.unwrap();
        // The five rows at t0 are older than 60 minutes and were deleted.
        assert_eq!(repo.rows.lock().unwrap().len(), 2);
    }

    fn key(label: &str, max: i64) -> LimitKey {
        LimitKey {
            label: label.into(),
            policy: LimitPolicy {
                max,
                window: Duration::minutes(15),
            },
        }
    }

    fn rows_of(repo: &MemoryRepo, key: &LimitKey) -> usize {
        let hash = key.hash();
        repo.rows
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, k, _)| *k == hash)
            .count()
    }

    #[tokio::test]
    async fn blocked_local_tier_records_nothing_on_the_global_tier() {
        let (limiter, repo, _) = limiter();
        let local = key("pair", 2);
        let global = key("email", 100);
        for _ in 0..2 {
            limiter
                .acquire(&[std::slice::from_ref(&local), std::slice::from_ref(&global)])
                .await
                .unwrap();
        }
        for _ in 0..10 {
            let err = limiter
                .acquire(&[std::slice::from_ref(&local), std::slice::from_ref(&global)])
                .await
                .unwrap_err();
            assert!(matches!(err, AuthError::RateLimited { .. }), "{err:?}");
        }
        // Every local attempt is recorded; only the two that passed reached
        // the global counter.
        assert_eq!(rows_of(&repo, &local), 12);
        assert_eq!(rows_of(&repo, &global), 2);
    }

    #[tokio::test]
    async fn global_tier_limits_after_the_local_tier_passes() {
        let (limiter, repo, _) = limiter();
        let global = key("email", 3);
        for i in 0..3 {
            let local = key(&format!("pair{i}"), 5);
            limiter
                .acquire(&[&[local], std::slice::from_ref(&global)])
                .await
                .unwrap();
        }
        let fresh_local = key("pair-fresh", 5);
        let err = limiter
            .acquire(&[
                std::slice::from_ref(&fresh_local),
                std::slice::from_ref(&global),
            ])
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                AuthError::RateLimited {
                    retry_after_secs: 900
                }
            ),
            "{err:?}"
        );
        // A blocked attempt is still recorded on both tiers it reached.
        assert_eq!(rows_of(&repo, &fresh_local), 1);
        assert_eq!(rows_of(&repo, &global), 4);
    }

    #[tokio::test]
    async fn forgive_reaches_rows_of_every_tier() {
        let (limiter, repo, _) = limiter();
        let local = key("pair", 5);
        let global = key("email", 5);
        let ticket = limiter
            .acquire(&[std::slice::from_ref(&local), std::slice::from_ref(&global)])
            .await
            .unwrap();

        limiter.forgive(&ticket, &global).await;
        assert_eq!(rows_of(&repo, &global), 0);
        assert_eq!(rows_of(&repo, &local), 1);

        limiter.forgive(&ticket, &local).await;
        assert!(repo.rows.lock().unwrap().is_empty());
    }
}

//! Application configuration read from environment variables.

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, anyhow};

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
/// Default request timeout (P9).
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 15;
/// Default access-token lifetime: 15 minutes (P3).
const DEFAULT_ACCESS_TOKEN_TTL_SECS: i64 = 900;
/// Shortest accepted `JWT_SECRET`, in bytes (the HS256 key size).
use crate::modules::auth::application::services::jwt::MIN_SECRET_BYTES as MIN_JWT_SECRET_BYTES;
/// Upper bound for the access token lifetime (one day), so `iat + ttl` cannot overflow.
const MAX_ACCESS_TOKEN_TTL_SECS: i64 = 86_400;
/// Default refresh-token lifetime: 30 days (P3).
const DEFAULT_REFRESH_TOKEN_TTL_DAYS: i64 = 30;
/// Upper bound for the refresh-token lifetime (one year), so
/// `chrono::Duration::days` and `now + ttl` cannot overflow.
const MAX_REFRESH_TOKEN_TTL_DAYS: i64 = 365;
/// Default SMTP server: Mailpit on the same machine (P6).
const DEFAULT_SMTP_HOST: &str = "localhost";
const DEFAULT_SMTP_PORT: u16 = 1025;
const DEFAULT_SMTP_FROM: &str = "dateNaow <no-reply@datenaow.local>";
/// Default password-reset link lifetime: 30 minutes (P6).
const DEFAULT_RESET_TOKEN_TTL_MINS: i64 = 30;
/// Upper bound for the reset link lifetime (one day).
const MAX_RESET_TOKEN_TTL_MINS: i64 = 1_440;

/// Runtime configuration. `Debug` is written by hand so secrets never reach logs.
#[derive(Clone)]
pub struct Config {
    /// PostgreSQL connection string. Contains the database password.
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Public origin of the web app, used to build links in emails.
    pub app_base_url: String,
    /// Requests running longer than this are answered with 408 (P9).
    pub request_timeout: Duration,
    /// HS256 signing secret for access tokens, at least 32 bytes.
    pub jwt_secret: String,
    /// Access-token lifetime in seconds (P3).
    pub access_token_ttl_secs: i64,
    /// Refresh-token lifetime in days, counted from each sign-in or refresh (P3).
    pub refresh_token_ttl_days: i64,
    /// Argon2id memory cost in KiB (P4).
    pub argon2_memory_kib: u32,
    /// Argon2id number of passes (P4).
    pub argon2_iterations: u32,
    /// Argon2id degree of parallelism (P4).
    pub argon2_parallelism: u32,
    /// Adds `Secure` to the refresh cookie (P3). Default `true`.
    pub cookie_secure: bool,
    /// Read the client address from `X-Forwarded-For` (P5). Turn on only
    /// behind a reverse proxy that sets it. Default `false`.
    pub trust_proxy: bool,
    /// SMTP server for the password-reset email. Default `localhost`.
    pub smtp_host: String,
    /// Default 1025 (Mailpit).
    pub smtp_port: u16,
    /// Sender mailbox. Default `dateNaow <no-reply@datenaow.local>`.
    pub smtp_from: String,
    /// Password-reset link lifetime in minutes (P6). Default 30.
    pub reset_token_ttl_mins: i64,
}

impl Config {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Reads the configuration through `lookup`, so it can be tested without
    /// touching the process environment.
    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let required = |key: &str| {
            lookup(key)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow!("environment variable {key} must be set"))
        };

        let database_url = required("DATABASE_URL")?;
        let app_base_url = required("APP_BASE_URL")?;
        let bind_addr = lookup("BIND_ADDR")
            .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_owned())
            .parse::<SocketAddr>()
            .context("BIND_ADDR must be a socket address such as 0.0.0.0:8080")?;
        let positive = |key: &str, default: u64| {
            optional_number(&lookup, key, default, |n: &u64| *n > 0)
                .ok_or_else(|| anyhow!("{key} must be a positive whole number"))
        };
        let request_timeout_secs = positive("REQUEST_TIMEOUT_SECS", DEFAULT_REQUEST_TIMEOUT_SECS)?;

        let jwt_secret = required("JWT_SECRET")?;
        if jwt_secret.len() < MIN_JWT_SECRET_BYTES {
            return Err(anyhow!(
                "JWT_SECRET must be at least {MIN_JWT_SECRET_BYTES} bytes long"
            ));
        }
        let access_token_ttl_secs = optional_number(
            &lookup,
            "ACCESS_TOKEN_TTL_SECS",
            DEFAULT_ACCESS_TOKEN_TTL_SECS,
            |n: &i64| (1..=MAX_ACCESS_TOKEN_TTL_SECS).contains(n),
        )
        .ok_or_else(|| {
            anyhow!(
                "ACCESS_TOKEN_TTL_SECS must be a whole number from 1 to {MAX_ACCESS_TOKEN_TTL_SECS}"
            )
        })?;
        let refresh_token_ttl_days = optional_number(
            &lookup,
            "REFRESH_TOKEN_TTL_DAYS",
            DEFAULT_REFRESH_TOKEN_TTL_DAYS,
            |n: &i64| (1..=MAX_REFRESH_TOKEN_TTL_DAYS).contains(n),
        )
        .ok_or_else(|| {
            anyhow!(
                "REFRESH_TOKEN_TTL_DAYS must be a whole number from 1 to {MAX_REFRESH_TOKEN_TTL_DAYS}"
            )
        })?;
        let argon2_number = |key: &str, default: u32| {
            optional_number(&lookup, key, default, |_: &u32| true)
                .ok_or_else(|| anyhow!("{key} must be a whole number"))
        };
        let flag = |key: &str, default: bool| {
            optional_bool(&lookup, key, default)
                .ok_or_else(|| anyhow!("{key} must be true or false"))
        };

        let config = Self {
            database_url,
            bind_addr,
            app_base_url,
            request_timeout: Duration::from_secs(request_timeout_secs),
            jwt_secret,
            access_token_ttl_secs,
            refresh_token_ttl_days,
            argon2_memory_kib: argon2_number("ARGON2_MEMORY_KIB", argon2::Params::DEFAULT_M_COST)?,
            argon2_iterations: argon2_number("ARGON2_ITERATIONS", argon2::Params::DEFAULT_T_COST)?,
            argon2_parallelism: argon2_number(
                "ARGON2_PARALLELISM",
                argon2::Params::DEFAULT_P_COST,
            )?,
            cookie_secure: flag("COOKIE_SECURE", true)?,
            trust_proxy: flag("TRUST_PROXY", false)?,
            smtp_host: optional_text(&lookup, "SMTP_HOST", DEFAULT_SMTP_HOST),
            smtp_port: optional_number(&lookup, "SMTP_PORT", DEFAULT_SMTP_PORT, |n: &u16| *n > 0)
                .ok_or_else(|| anyhow!("SMTP_PORT must be a port number from 1 to 65535"))?,
            smtp_from: optional_text(&lookup, "SMTP_FROM", DEFAULT_SMTP_FROM),
            reset_token_ttl_mins: optional_number(
                &lookup,
                "RESET_TOKEN_TTL_MINS",
                DEFAULT_RESET_TOKEN_TTL_MINS,
                |n: &i64| (1..=MAX_RESET_TOKEN_TTL_MINS).contains(n),
            )
            .ok_or_else(|| {
                anyhow!(
                    "RESET_TOKEN_TTL_MINS must be a whole number from 1 to {MAX_RESET_TOKEN_TTL_MINS}"
                )
            })?,
        };
        // Reject parameters Argon2 would refuse now, not at the first sign-up.
        config.argon2_params()?;
        Ok(config)
    }

    /// The Argon2id parameters built from `argon2_*`.
    pub fn argon2_params(&self) -> anyhow::Result<argon2::Params> {
        argon2::Params::new(
            self.argon2_memory_kib,
            self.argon2_iterations,
            self.argon2_parallelism,
            None,
        )
        .map_err(|e| {
            anyhow!("ARGON2_MEMORY_KIB, ARGON2_ITERATIONS and ARGON2_PARALLELISM are invalid: {e}")
        })
    }
}

/// Parses an optional numeric variable. A missing or blank value gives
/// `default`; `None` means the value is present but unparsable or not `valid`.
fn optional_number<T: std::str::FromStr>(
    lookup: &impl Fn(&str) -> Option<String>,
    key: &str,
    default: T,
    valid: impl Fn(&T) -> bool,
) -> Option<T> {
    match lookup(key).filter(|value| !value.trim().is_empty()) {
        Some(value) => value.trim().parse::<T>().ok().filter(|n| valid(n)),
        None => Some(default),
    }
}

/// An optional text variable, trimmed; a missing or blank value gives `default`.
fn optional_text(lookup: &impl Fn(&str) -> Option<String>, key: &str, default: &str) -> String {
    lookup(key)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Parses an optional boolean variable (`true`/`false`, `1`/`0`, any case).
/// A missing or blank value gives `default`; `None` means it is unparsable.
fn optional_bool(
    lookup: &impl Fn(&str) -> Option<String>,
    key: &str,
    default: bool,
) -> Option<bool> {
    match lookup(key).filter(|value| !value.trim().is_empty()) {
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        None => Some(default),
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"***")
            .field("bind_addr", &self.bind_addr)
            .field("app_base_url", &self.app_base_url)
            .field("request_timeout", &self.request_timeout)
            .field("jwt_secret", &"***")
            .field("access_token_ttl_secs", &self.access_token_ttl_secs)
            .field("refresh_token_ttl_days", &self.refresh_token_ttl_days)
            .field("argon2_memory_kib", &self.argon2_memory_kib)
            .field("argon2_iterations", &self.argon2_iterations)
            .field("argon2_parallelism", &self.argon2_parallelism)
            .field("cookie_secure", &self.cookie_secure)
            .field("trust_proxy", &self.trust_proxy)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("smtp_from", &self.smtp_from)
            .field("reset_token_ttl_mins", &self.reset_token_ttl_mins)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const TEST_SECRET: &str = "test-jwt-secret-that-is-at-least-32-bytes";

    /// A lookup over `pairs`, with `JWT_SECRET` set unless `pairs` sets it.
    fn lookup_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let mut map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        map.entry("JWT_SECRET".to_owned())
            .or_insert_with(|| TEST_SECRET.to_owned());
        move |key| map.get(key).cloned()
    }

    /// The two required variables besides `JWT_SECRET`, plus `extra`.
    fn minimal_with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
        let mut pairs = vec![
            ("DATABASE_URL", "postgres://localhost/db"),
            ("APP_BASE_URL", "http://localhost:5173"),
        ];
        pairs.extend_from_slice(extra);
        pairs
    }

    #[test]
    fn auth_settings_default() {
        let config = Config::from_lookup(lookup_from(&minimal_with(&[]))).unwrap();
        assert_eq!(config.jwt_secret, TEST_SECRET);
        assert_eq!(config.access_token_ttl_secs, 900);
        assert_eq!(config.refresh_token_ttl_days, 30);
        assert_eq!(config.argon2_memory_kib, argon2::Params::DEFAULT_M_COST);
        assert_eq!(config.argon2_iterations, argon2::Params::DEFAULT_T_COST);
        assert_eq!(config.argon2_parallelism, argon2::Params::DEFAULT_P_COST);
        let params = config.argon2_params().unwrap();
        assert_eq!(params.m_cost(), argon2::Params::DEFAULT_M_COST);
    }

    #[test]
    fn reads_auth_settings() {
        let config = Config::from_lookup(lookup_from(&minimal_with(&[
            ("ACCESS_TOKEN_TTL_SECS", "600"),
            ("REFRESH_TOKEN_TTL_DAYS", "14"),
            ("ARGON2_MEMORY_KIB", "65536"),
            ("ARGON2_ITERATIONS", "3"),
            ("ARGON2_PARALLELISM", "2"),
        ])))
        .unwrap();
        assert_eq!(config.access_token_ttl_secs, 600);
        assert_eq!(config.refresh_token_ttl_days, 14);
        assert_eq!(config.argon2_memory_kib, 65536);
        assert_eq!(config.argon2_iterations, 3);
        assert_eq!(config.argon2_parallelism, 2);
    }

    #[test]
    fn cookie_secure_and_trust_proxy_default_to_safe_values() {
        let config = Config::from_lookup(lookup_from(&minimal_with(&[]))).unwrap();
        assert!(config.cookie_secure);
        assert!(!config.trust_proxy);
    }

    #[test]
    fn reads_cookie_secure_and_trust_proxy() {
        let config = Config::from_lookup(lookup_from(&minimal_with(&[
            ("COOKIE_SECURE", "false"),
            ("TRUST_PROXY", " TRUE "),
        ])))
        .unwrap();
        assert!(!config.cookie_secure);
        assert!(config.trust_proxy);

        let config = Config::from_lookup(lookup_from(&minimal_with(&[
            ("COOKIE_SECURE", "0"),
            ("TRUST_PROXY", "1"),
        ])))
        .unwrap();
        assert!(!config.cookie_secure);
        assert!(config.trust_proxy);
    }

    #[test]
    fn email_settings_default() {
        let config = Config::from_lookup(lookup_from(&minimal_with(&[]))).unwrap();
        assert_eq!(config.smtp_host, "localhost");
        assert_eq!(config.smtp_port, 1025);
        assert_eq!(config.smtp_from, "dateNaow <no-reply@datenaow.local>");
        assert_eq!(config.reset_token_ttl_mins, 30);
    }

    #[test]
    fn reads_email_settings() {
        let config = Config::from_lookup(lookup_from(&minimal_with(&[
            ("SMTP_HOST", " mailpit "),
            ("SMTP_PORT", "2525"),
            ("SMTP_FROM", "Test <t@example.com>"),
            ("RESET_TOKEN_TTL_MINS", "60"),
        ])))
        .unwrap();
        assert_eq!(config.smtp_host, "mailpit");
        assert_eq!(config.smtp_port, 2525);
        assert_eq!(config.smtp_from, "Test <t@example.com>");
        assert_eq!(config.reset_token_ttl_mins, 60);
    }

    #[test]
    fn invalid_email_settings_are_an_error() {
        for (key, value) in [
            ("SMTP_PORT", "0"),
            ("SMTP_PORT", "65536"),
            ("SMTP_PORT", "abc"),
            ("RESET_TOKEN_TTL_MINS", "0"),
            ("RESET_TOKEN_TTL_MINS", "-1"),
            ("RESET_TOKEN_TTL_MINS", "1441"),
        ] {
            let result = Config::from_lookup(lookup_from(&minimal_with(&[(key, value)])));
            assert!(result.is_err(), "{key}={value} should be rejected");
        }
    }

    #[test]
    fn invalid_flags_are_an_error() {
        for key in ["COOKIE_SECURE", "TRUST_PROXY"] {
            for value in ["yes", "2", "on"] {
                let result = Config::from_lookup(lookup_from(&minimal_with(&[(key, value)])));
                assert!(result.is_err(), "{key}={value} should be rejected");
            }
        }
    }

    #[test]
    fn missing_jwt_secret_is_an_error() {
        let err =
            Config::from_lookup(lookup_from(&minimal_with(&[("JWT_SECRET", "  ")]))).unwrap_err();
        assert!(err.to_string().contains("JWT_SECRET"));
    }

    #[test]
    fn jwt_secret_shorter_than_32_bytes_is_an_error() {
        let short = "a".repeat(31);
        let pairs = [
            ("DATABASE_URL", "postgres://localhost/db"),
            ("APP_BASE_URL", "http://localhost:5173"),
            ("JWT_SECRET", short.as_str()),
        ];
        let err = Config::from_lookup(lookup_from(&pairs)).unwrap_err();
        assert!(err.to_string().contains("JWT_SECRET"));
        assert!(
            !err.to_string().contains(&short),
            "error must not echo the secret"
        );

        let exact = "a".repeat(32);
        let pairs = [
            ("DATABASE_URL", "postgres://localhost/db"),
            ("APP_BASE_URL", "http://localhost:5173"),
            ("JWT_SECRET", exact.as_str()),
        ];
        assert!(Config::from_lookup(lookup_from(&pairs)).is_ok());
    }

    #[test]
    fn invalid_access_token_ttl_is_an_error() {
        for value in ["0", "-5", "abc", "86401", "9223372036854775807"] {
            let result = Config::from_lookup(lookup_from(&minimal_with(&[(
                "ACCESS_TOKEN_TTL_SECS",
                value,
            )])));
            assert!(result.is_err(), "{value} should be rejected");
        }
    }

    #[test]
    fn invalid_refresh_token_ttl_is_an_error() {
        for value in ["0", "-1", "abc", "1.5", "366", "9223372036854775807"] {
            let result = Config::from_lookup(lookup_from(&minimal_with(&[(
                "REFRESH_TOKEN_TTL_DAYS",
                value,
            )])));
            assert!(result.is_err(), "{value} should be rejected");
        }
        let max = Config::from_lookup(lookup_from(&minimal_with(&[(
            "REFRESH_TOKEN_TTL_DAYS",
            "365",
        )])))
        .unwrap();
        assert_eq!(max.refresh_token_ttl_days, 365);
    }

    #[test]
    fn invalid_argon2_params_are_an_error() {
        for (key, value) in [
            ("ARGON2_MEMORY_KIB", "abc"),
            ("ARGON2_MEMORY_KIB", "4"),
            ("ARGON2_ITERATIONS", "0"),
            ("ARGON2_PARALLELISM", "0"),
            ("ARGON2_PARALLELISM", "-1"),
        ] {
            let result = Config::from_lookup(lookup_from(&minimal_with(&[(key, value)])));
            assert!(result.is_err(), "{key}={value} should be rejected");
        }
    }

    #[test]
    fn debug_redacts_jwt_secret() {
        let config = Config::from_lookup(lookup_from(&minimal_with(&[]))).unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains(TEST_SECRET));
        assert!(debug.contains("jwt_secret: \"***\""));
    }

    #[test]
    fn reads_all_fields() {
        let config = Config::from_lookup(lookup_from(&[
            ("DATABASE_URL", "postgres://u:p@localhost/db"),
            ("BIND_ADDR", "127.0.0.1:9000"),
            ("APP_BASE_URL", "http://localhost:5173"),
        ]))
        .unwrap();
        assert_eq!(config.database_url, "postgres://u:p@localhost/db");
        assert_eq!(config.bind_addr, SocketAddr::from(([127, 0, 0, 1], 9000)));
        assert_eq!(config.app_base_url, "http://localhost:5173");
    }

    #[test]
    fn bind_addr_defaults_when_missing() {
        let config = Config::from_lookup(lookup_from(&[
            ("DATABASE_URL", "postgres://localhost/db"),
            ("APP_BASE_URL", "http://localhost:5173"),
        ]))
        .unwrap();
        assert_eq!(config.bind_addr, SocketAddr::from(([0, 0, 0, 0], 8080)));
        assert_eq!(config.request_timeout, Duration::from_secs(15));
    }

    #[test]
    fn reads_request_timeout() {
        let config = Config::from_lookup(lookup_from(&[
            ("DATABASE_URL", "postgres://localhost/db"),
            ("APP_BASE_URL", "http://localhost:5173"),
            ("REQUEST_TIMEOUT_SECS", "30"),
        ]))
        .unwrap();
        assert_eq!(config.request_timeout, Duration::from_secs(30));
    }

    #[test]
    fn invalid_request_timeout_is_an_error() {
        for value in ["0", "-1", "abc", "1.5"] {
            let result = Config::from_lookup(lookup_from(&[
                ("DATABASE_URL", "postgres://localhost/db"),
                ("APP_BASE_URL", "http://localhost:5173"),
                ("REQUEST_TIMEOUT_SECS", value),
            ]));
            assert!(result.is_err(), "{value} should be rejected");
        }
    }

    #[test]
    fn missing_database_url_is_an_error() {
        let err = Config::from_lookup(lookup_from(&[("APP_BASE_URL", "http://x")])).unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn invalid_bind_addr_is_an_error() {
        let result = Config::from_lookup(lookup_from(&[
            ("DATABASE_URL", "postgres://localhost/db"),
            ("APP_BASE_URL", "http://localhost:5173"),
            ("BIND_ADDR", "not-an-address"),
        ]));
        assert!(result.is_err());
    }

    #[test]
    fn debug_redacts_database_url() {
        let config = Config::from_lookup(lookup_from(&[
            ("DATABASE_URL", "postgres://user:s3cret@localhost/db"),
            ("APP_BASE_URL", "http://localhost:5173"),
        ]))
        .unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains("s3cret"));
        assert!(debug.contains("database_url: \"***\""));
    }
}

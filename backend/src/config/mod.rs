//! Application configuration read from environment variables.

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, anyhow};

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
/// Default request timeout (P9).
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 15;

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
        let request_timeout_secs = match lookup("REQUEST_TIMEOUT_SECS")
            .filter(|value| !value.trim().is_empty())
        {
            Some(value) => value
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|secs| *secs > 0)
                .ok_or_else(|| anyhow!("REQUEST_TIMEOUT_SECS must be a positive whole number"))?,
            None => DEFAULT_REQUEST_TIMEOUT_SECS,
        };

        Ok(Self {
            database_url,
            bind_addr,
            app_base_url,
            request_timeout: Duration::from_secs(request_timeout_secs),
        })
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"***")
            .field("bind_addr", &self.bind_addr)
            .field("app_base_url", &self.app_base_url)
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| map.get(key).cloned()
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

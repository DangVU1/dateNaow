//! The client's IP address, for the failed-attempt limits (P5).
//!
//! Taken from the TCP peer address. Only when `TRUST_PROXY=true` (the app runs
//! behind a reverse proxy that appends the real client address) is
//! `X-Forwarded-For` read, and then only its rightmost entry: the last entry
//! of the last header line, the one the proxy appended (DECISIONS #17). If
//! that entry is not a valid address, the peer address is used. Entries to
//! the left of it are supplied by the client and can be forged, so they are
//! never used, not even as a fallback.
//!
//! A request that carries `X-Forwarded-For` while `TRUST_PROXY` is off logs a
//! warning once per process (without the header value): it usually means the
//! app runs behind a proxy and every client shares the proxy's address in the
//! attempt limits. Only routes that read the client IP (sign-up, sign-in,
//! password reset) notice the header.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;

use crate::shared::error::ApiError;
use crate::state::AppState;

const X_FORWARDED_FOR: &str = "x-forwarded-for";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let ConnectInfo(peer) = ConnectInfo::<SocketAddr>::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                ApiError::Internal(anyhow::anyhow!(
                    "no ConnectInfo: serve with into_make_service_with_connect_info"
                ))
            })?;
        let trust_proxy = state.config.trust_proxy;
        if should_warn_untrusted(&parts.headers, trust_proxy, &WARNED_UNTRUSTED_FORWARDED_FOR) {
            tracing::warn!(
                "request has X-Forwarded-For but TRUST_PROXY is off; using the socket address \
                 (set TRUST_PROXY=true only behind a proxy that appends the client address)"
            );
        }
        Ok(Self(resolve(peer.ip(), &parts.headers, trust_proxy)))
    }
}

/// Set once the "untrusted X-Forwarded-For" warning has been logged.
static WARNED_UNTRUSTED_FORWARDED_FOR: AtomicBool = AtomicBool::new(false);

/// `true` exactly once per `warned` flag: the first time a request carries
/// `X-Forwarded-For` while `trust_proxy` is off.
fn should_warn_untrusted(headers: &HeaderMap, trust_proxy: bool, warned: &AtomicBool) -> bool {
    !trust_proxy && headers.contains_key(X_FORWARDED_FOR) && !warned.swap(true, Ordering::Relaxed)
}

/// The client address: the rightmost `X-Forwarded-For` entry when
/// `trust_proxy` is on and that entry is a valid address, otherwise `peer`.
/// IPv4-mapped IPv6 addresses are turned back into IPv4.
pub fn resolve(peer: IpAddr, headers: &HeaderMap, trust_proxy: bool) -> IpAddr {
    let forwarded = if trust_proxy {
        rightmost_forwarded(headers)
    } else {
        None
    };
    forwarded.unwrap_or(peer).to_canonical()
}

/// The last entry of the last `X-Forwarded-For` line, if it parses. Nothing
/// to its left is ever looked at.
fn rightmost_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    let last_line = headers.get_all(X_FORWARDED_FOR).iter().next_back()?;
    let last_entry = last_line.to_str().ok()?.rsplit(',').next()?;
    parse_entry(last_entry)
}

/// Accepts `1.2.3.4`, `2001:db8::1`, and the forms with a port
/// (`1.2.3.4:5678`, `[2001:db8::1]:5678`).
fn parse_entry(entry: &str) -> Option<IpAddr> {
    let entry = entry.trim();
    entry
        .parse::<IpAddr>()
        .ok()
        .or_else(|| entry.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn headers(values: &[&str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for value in values {
            map.append(X_FORWARDED_FOR, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    const PEER: &str = "10.0.0.2";

    #[test]
    fn ignores_forwarded_for_without_trust_proxy() {
        let h = headers(&["198.51.100.1"]);
        assert_eq!(resolve(ip(PEER), &h, false), ip(PEER));
    }

    #[test]
    fn takes_rightmost_entry_with_trust_proxy() {
        let h = headers(&["1.1.1.1, 198.51.100.1", "198.51.100.9"]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("198.51.100.9"));
        let h = headers(&["1.1.1.1,  198.51.100.1 "]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("198.51.100.1"));
        // The last entry of the last header line, however many lines.
        let h = headers(&["198.51.100.1", "198.51.100.2", "1.1.1.1, 198.51.100.3"]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("198.51.100.3"));
    }

    #[test]
    fn accepts_ports_on_the_rightmost_entry() {
        let h = headers(&["[2001:db8::7]:443"]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("2001:db8::7"));
        let h = headers(&["1.1.1.1, 198.51.100.1:8080"]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("198.51.100.1"));
    }

    #[test]
    fn invalid_rightmost_entry_falls_back_to_peer_never_to_the_left() {
        // #17: entries left of the rightmost come from the client and are
        // never used, even when the rightmost one is unusable.
        for value in [
            &["198.51.100.1, garbage"][..],
            &["198.51.100.1, "],
            &["198.51.100.1,"],
            &["198.51.100.1", ""],
            &["198.51.100.1", "unknown"],
            &["198.51.100.1", "garbage, "],
        ] {
            assert_eq!(
                resolve(ip(PEER), &headers(value), true),
                ip(PEER),
                "{value:?}"
            );
        }
    }

    #[test]
    fn non_utf8_last_line_falls_back_to_peer() {
        let mut h = headers(&["198.51.100.1"]);
        h.append(
            X_FORWARDED_FOR,
            HeaderValue::from_bytes(&[0xff, b'1', b'.', b'2']).unwrap(),
        );
        assert_eq!(resolve(ip(PEER), &h, true), ip(PEER));
    }

    #[test]
    fn falls_back_to_peer_when_header_is_missing_or_unusable() {
        assert_eq!(resolve(ip(PEER), &HeaderMap::new(), true), ip(PEER));
        assert_eq!(resolve(ip(PEER), &headers(&["unknown"]), true), ip(PEER));
    }

    #[test]
    fn untrusted_forwarded_for_warns_only_the_first_time() {
        let flag = AtomicBool::new(false);
        let h = headers(&["198.51.100.1"]);
        assert!(should_warn_untrusted(&h, false, &flag));
        assert!(!should_warn_untrusted(&h, false, &flag));
        assert!(!should_warn_untrusted(&h, false, &flag));
    }

    #[test]
    fn no_warning_without_the_header_or_with_trust_proxy() {
        let flag = AtomicBool::new(false);
        assert!(!should_warn_untrusted(&HeaderMap::new(), false, &flag));
        assert!(!should_warn_untrusted(
            &headers(&["198.51.100.1"]),
            true,
            &flag
        ));
        // Neither case used up the one warning.
        assert!(should_warn_untrusted(
            &headers(&["198.51.100.1"]),
            false,
            &flag
        ));
    }

    #[test]
    fn canonicalises_ipv4_mapped_addresses() {
        assert_eq!(
            resolve(ip("::ffff:203.0.113.7"), &HeaderMap::new(), false),
            ip("203.0.113.7")
        );
    }
}

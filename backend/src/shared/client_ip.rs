//! The client's IP address, for the failed-attempt limits (P5).
//!
//! Taken from the TCP peer address. Only when `TRUST_PROXY=true` (the app runs
//! behind a reverse proxy that appends the real client address) is
//! `X-Forwarded-For` read, and then only its rightmost valid entry: entries to
//! the left of it are supplied by the client and can be forged.

use std::net::{IpAddr, SocketAddr};

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
        Ok(Self(resolve(
            peer.ip(),
            &parts.headers,
            state.config.trust_proxy,
        )))
    }
}

/// The client address: the rightmost valid `X-Forwarded-For` entry when
/// `trust_proxy` is on and one exists, otherwise `peer`. IPv4-mapped IPv6
/// addresses are turned back into IPv4.
pub fn resolve(peer: IpAddr, headers: &HeaderMap, trust_proxy: bool) -> IpAddr {
    let forwarded = if trust_proxy {
        rightmost_forwarded(headers)
    } else {
        None
    };
    forwarded.unwrap_or(peer).to_canonical()
}

fn rightmost_forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get_all(X_FORWARDED_FOR)
        .iter()
        .rev()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.rsplit(','))
        .find_map(parse_entry)
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
    }

    #[test]
    fn skips_invalid_entries_and_accepts_ports() {
        let h = headers(&["198.51.100.1, garbage"]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("198.51.100.1"));
        let h = headers(&["[2001:db8::7]:443"]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("2001:db8::7"));
        let h = headers(&["198.51.100.1:8080"]);
        assert_eq!(resolve(ip(PEER), &h, true), ip("198.51.100.1"));
    }

    #[test]
    fn falls_back_to_peer_when_header_is_missing_or_unusable() {
        assert_eq!(resolve(ip(PEER), &HeaderMap::new(), true), ip(PEER));
        assert_eq!(resolve(ip(PEER), &headers(&["unknown"]), true), ip(PEER));
    }

    #[test]
    fn canonicalises_ipv4_mapped_addresses() {
        assert_eq!(
            resolve(ip("::ffff:203.0.113.7"), &HeaderMap::new(), false),
            ip("203.0.113.7")
        );
    }
}

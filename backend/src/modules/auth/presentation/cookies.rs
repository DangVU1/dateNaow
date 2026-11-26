//! The refresh-token cookie (P3): `dn_refresh`, `HttpOnly`,
//! `SameSite=Strict`, `Path=/api/v1/auth`, `Secure` when configured.

use axum_extra::extract::cookie::{Cookie, SameSite};

use crate::modules::auth::domain::OpaqueToken;

pub const REFRESH_COOKIE: &str = "dn_refresh";
/// Only the auth endpoints (refresh, logout) ever receive the cookie.
const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";

/// The cookie carrying `token`, kept by the browser for `max_age_secs`.
pub fn refresh_cookie(token: &OpaqueToken, max_age_secs: i64, secure: bool) -> Cookie<'static> {
    build(token.as_str().to_owned(), max_age_secs, secure)
}

/// A cookie that makes the browser delete `dn_refresh` (`Max-Age=0`).
pub fn clear_refresh_cookie(secure: bool) -> Cookie<'static> {
    build(String::new(), 0, secure)
}

fn build(value: String, max_age_secs: i64, secure: bool) -> Cookie<'static> {
    let max_age = std::time::Duration::from_secs(u64::try_from(max_age_secs).unwrap_or(0));
    Cookie::build((REFRESH_COOKIE, value))
        .http_only(true)
        .same_site(SameSite::Strict)
        .path(REFRESH_COOKIE_PATH)
        .secure(secure)
        // A u64 second count always fits `time::Duration`; zero is the fallback.
        .max_age(max_age.try_into().unwrap_or_default())
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> OpaqueToken {
        OpaqueToken::from_client(&"ab".repeat(32)).expect("valid token")
    }

    #[test]
    fn refresh_cookie_has_all_attributes() {
        let c = refresh_cookie(&token(), 2_592_000, true).to_string();
        assert!(
            c.starts_with(&format!("dn_refresh={}", "ab".repeat(32))),
            "{c}"
        );
        for attr in [
            "HttpOnly",
            "SameSite=Strict",
            "Path=/api/v1/auth",
            "Max-Age=2592000",
            "Secure",
        ] {
            assert!(c.contains(attr), "{attr} missing from {c}");
        }
    }

    #[test]
    fn secure_is_omitted_when_disabled() {
        let c = refresh_cookie(&token(), 60, false).to_string();
        assert!(!c.contains("Secure"), "{c}");
    }

    #[test]
    fn clear_cookie_expires_immediately() {
        let c = clear_refresh_cookie(false).to_string();
        assert!(c.starts_with("dn_refresh=;"), "{c}");
        assert!(
            c.contains("Max-Age=0") && c.contains("Path=/api/v1/auth"),
            "{c}"
        );
    }

    #[test]
    fn negative_max_age_becomes_zero() {
        let c = refresh_cookie(&token(), -5, false).to_string();
        assert!(c.contains("Max-Age=0"), "{c}");
    }
}

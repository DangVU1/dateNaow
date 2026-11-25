//! Access tokens: short-lived JWTs signed with HS256 (P3).
//!
//! Expiry is checked against the `now` passed in by the caller, not against
//! the system clock, so tests can drive it with `FakeClock`.

use std::fmt;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::actor::Role;

/// Shortest accepted signing secret, in bytes (the HS256 key size).
pub const MIN_SECRET_BYTES: usize = 32;

/// The claims carried by an access token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessClaims {
    pub sub: Uuid,
    pub role: Role,
    /// Issued at, in seconds since the Unix epoch.
    pub iat: i64,
    /// Expires at, in seconds since the Unix epoch. Invalid from this second on.
    pub exp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// Malformed, wrongly signed, signed with another algorithm, or with bad claims.
    #[error("invalid access token")]
    Invalid,
    /// Correctly signed but `now >= exp`.
    #[error("access token expired")]
    Expired,
    /// The token could not be signed.
    #[error("access token could not be encoded")]
    Encode,
}

/// Issues and verifies access tokens. `Debug` never prints the keys.
pub struct JwtService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    access_ttl: chrono::Duration,
}

impl JwtService {
    /// # Panics
    ///
    /// If `secret` is shorter than [`MIN_SECRET_BYTES`]. `Config` already
    /// rejects such a secret at startup, so this guards against wiring mistakes.
    pub fn new(secret: &[u8], access_ttl: chrono::Duration) -> Self {
        assert!(
            secret.len() >= MIN_SECRET_BYTES,
            "JWT secret must be at least 32 bytes"
        );
        // `from_secret` only fails for an empty secret, excluded above.
        let encoding = EncodingKey::from_secret(secret).expect("non-empty HMAC secret");
        let decoding = DecodingKey::from_secret(secret).expect("non-empty HMAC secret");

        // Only HS256: a token whose header names any other algorithm is
        // rejected before its signature is checked.
        let mut validation = Validation::new(Algorithm::HS256);
        // Expiry is checked in `verify` against the injected `now`.
        validation.validate_exp = false;
        validation.validate_nbf = false;
        // Tokens carry no `aud` and there is a single audience (this API).
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["exp", "sub"]);

        Self {
            encoding,
            decoding,
            validation,
            access_ttl,
        }
    }

    pub fn issue(
        &self,
        user_id: Uuid,
        role: Role,
        now: DateTime<Utc>,
    ) -> Result<String, TokenError> {
        let iat = now.timestamp();
        let claims = AccessClaims {
            sub: user_id,
            role,
            iat,
            exp: iat + self.access_ttl_secs(),
        };
        jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(|_| TokenError::Encode)
    }

    /// Checks algorithm, signature and claim shapes first, then expiry, so a
    /// forged token is reported as `Invalid` rather than `Expired`.
    pub fn verify(&self, token: &str, now: DateTime<Utc>) -> Result<AccessClaims, TokenError> {
        let claims = jsonwebtoken::decode::<AccessClaims>(token, &self.decoding, &self.validation)
            .map_err(|_| TokenError::Invalid)?
            .claims;
        if now.timestamp() >= claims.exp {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }

    pub fn access_ttl_secs(&self) -> i64 {
        self.access_ttl.num_seconds()
    }
}

impl fmt::Debug for JwtService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JwtService")
            .field("keys", &"***")
            .field("access_ttl", &self.access_ttl)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    const SECRET: &[u8] = b"test-secret-that-is-at-least-32-bytes!!";

    fn svc() -> JwtService {
        JwtService::new(SECRET, Duration::seconds(900))
    }

    fn now() -> DateTime<Utc> {
        "2026-11-25T09:00:00Z".parse().expect("valid timestamp")
    }

    /// Replaces the payload segment of `token` with `payload_json`, keeping the
    /// original header and signature.
    fn with_payload(token: &str, payload_json: &str) -> String {
        use base64_url::encode;
        let parts: Vec<&str> = token.split('.').collect();
        format!("{}.{}.{}", parts[0], encode(payload_json), parts[2])
    }

    /// Minimal unpadded base64url encoder so the test needs no extra crate.
    mod base64_url {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

        pub fn encode(input: &str) -> String {
            let bytes = input.as_bytes();
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let b = [
                    chunk[0],
                    chunk.get(1).copied().unwrap_or(0),
                    chunk.get(2).copied().unwrap_or(0),
                ];
                let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
                let symbols = chunk.len() + 1;
                for i in 0..symbols {
                    let index = (n >> (18 - 6 * i)) & 0x3f;
                    out.push(char::from(ALPHABET[index as usize]));
                }
            }
            out
        }
    }

    #[test]
    fn issue_then_verify_returns_same_claims() {
        let svc = svc();
        let id = Uuid::from_u128(0x0193_6f3a_1b2c_7d4e_8f00_1122_3344_5566);
        let t = svc.issue(id, Role::User, now()).expect("issue");
        let c = svc.verify(&t, now()).expect("verify");
        assert_eq!(c.sub, id);
        assert_eq!(c.role, Role::User);
        assert_eq!(c.iat, now().timestamp());
        assert_eq!(c.exp - c.iat, 900);
        assert_eq!(svc.access_ttl_secs(), 900);
    }

    #[test]
    fn admin_role_round_trips() {
        let svc = svc();
        let t = svc.issue(Uuid::nil(), Role::Admin, now()).expect("issue");
        assert_eq!(svc.verify(&t, now()).expect("verify").role, Role::Admin);
    }

    #[test]
    fn token_is_valid_one_second_before_exp() {
        let svc = svc();
        let t = svc.issue(Uuid::nil(), Role::User, now()).expect("issue");
        assert!(svc.verify(&t, now() + Duration::seconds(899)).is_ok());
    }

    #[test]
    fn token_is_expired_at_exp() {
        let svc = svc();
        let t = svc.issue(Uuid::nil(), Role::User, now()).expect("issue");
        assert!(matches!(
            svc.verify(&t, now() + Duration::seconds(900)),
            Err(TokenError::Expired)
        ));
    }

    #[test]
    fn token_issued_in_the_past_is_expired_even_if_system_clock_disagrees() {
        // Issued and checked long ago relative to the wall clock: only `now` matters.
        let svc = svc();
        let long_ago: DateTime<Utc> = "2001-01-01T00:00:00Z".parse().expect("valid timestamp");
        let t = svc.issue(Uuid::nil(), Role::User, long_ago).expect("issue");
        assert!(svc.verify(&t, long_ago + Duration::seconds(10)).is_ok());
        assert!(matches!(
            svc.verify(&t, long_ago + Duration::seconds(900)),
            Err(TokenError::Expired)
        ));
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let svc = svc();
        let t = svc.issue(Uuid::nil(), Role::User, now()).expect("issue");
        let exp = now().timestamp() + 900;
        let forged = with_payload(
            &t,
            &format!(
                r#"{{"sub":"{}","role":"admin","iat":{},"exp":{exp}}}"#,
                Uuid::nil(),
                now().timestamp()
            ),
        );
        assert!(matches!(
            svc.verify(&forged, now()),
            Err(TokenError::Invalid)
        ));
    }

    #[test]
    fn token_signed_with_other_secret_is_rejected() {
        let other = JwtService::new(
            b"another-secret-that-is-32-bytes-long!!",
            Duration::seconds(900),
        );
        let t = other.issue(Uuid::nil(), Role::User, now()).expect("issue");
        assert!(matches!(svc().verify(&t, now()), Err(TokenError::Invalid)));
    }

    #[test]
    fn hs512_token_is_rejected() {
        let claims = AccessClaims {
            sub: Uuid::nil(),
            role: Role::User,
            iat: now().timestamp(),
            exp: now().timestamp() + 900,
        };
        let t = jsonwebtoken::encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(SECRET).expect("key"),
        )
        .expect("encode HS512");
        assert!(matches!(svc().verify(&t, now()), Err(TokenError::Invalid)));
    }

    #[test]
    fn garbage_and_unsigned_tokens_are_rejected() {
        let svc = svc();
        for token in ["", "abc", "a.b.c", "eyJhbGciOiJub25lIn0.e30."] {
            assert!(
                matches!(svc.verify(token, now()), Err(TokenError::Invalid)),
                "{token}"
            );
        }
    }

    #[test]
    fn token_with_missing_claim_is_rejected() {
        #[derive(Serialize)]
        struct NoRole {
            sub: Uuid,
            iat: i64,
            exp: i64,
        }
        let t = jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            &NoRole {
                sub: Uuid::nil(),
                iat: now().timestamp(),
                exp: now().timestamp() + 900,
            },
            &EncodingKey::from_secret(SECRET).expect("key"),
        )
        .expect("encode");
        assert!(matches!(svc().verify(&t, now()), Err(TokenError::Invalid)));
    }

    #[test]
    fn debug_hides_keys() {
        let debug = format!("{:?}", svc());
        assert!(!debug.contains("test-secret"));
    }

    #[test]
    #[should_panic(expected = "at least 32 bytes")]
    fn short_secret_panics() {
        let _ = JwtService::new(b"too-short", Duration::seconds(900));
    }
}

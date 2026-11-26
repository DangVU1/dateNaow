//! Opaque random tokens (refresh tokens, password-reset tokens).
//!
//! A token is 32 bytes from the operating system's random number generator,
//! written as 64 lowercase hex characters. Only its SHA-256 hash is stored
//! (P3), so a leaked database row cannot be replayed as a token.

use std::fmt;

use rand::TryRng;
use rand::rngs::SysRng;
use sha2::{Digest, Sha256};

/// Number of random bytes in a token.
const TOKEN_BYTES: usize = 32;
/// Length of the hex form sent to clients.
const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// A token in its client-facing hex form. Has no `Display`, and `Debug`
/// prints `OpaqueToken(***)`, so it cannot end up in logs by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct OpaqueToken(String);

/// The presented string is not exactly 64 lowercase hex characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid token format")]
pub struct InvalidToken;

/// The operating system's random number generator failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("operating system random number generator failed")]
pub struct TokenGenerationError;

impl OpaqueToken {
    /// A new random token. Fails only if the OS random number generator does,
    /// which callers report as an internal error instead of panicking.
    pub fn generate() -> Result<Self, TokenGenerationError> {
        let mut bytes = [0u8; TOKEN_BYTES];
        SysRng
            .try_fill_bytes(&mut bytes)
            .map_err(|_| TokenGenerationError)?;
        Ok(Self(to_lower_hex(&bytes)))
    }

    /// Accepts only the exact form `generate` produces: 64 lowercase hex characters.
    pub fn from_client(s: &str) -> Result<Self, InvalidToken> {
        let well_formed =
            s.len() == TOKEN_HEX_LEN && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if well_formed {
            Ok(Self(s.to_owned()))
        } else {
            Err(InvalidToken)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// SHA-256 of the hex string: the value stored and looked up in the database.
    pub fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.0.as_bytes()).into()
    }
}

impl fmt::Debug for OpaqueToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpaqueToken(***)")
    }
}

fn to_lower_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate() -> OpaqueToken {
        OpaqueToken::generate().expect("OS RNG available")
    }

    #[test]
    fn generate_is_64_lowercase_hex() {
        let token = generate();
        assert_eq!(token.as_str().len(), 64);
        assert!(
            token
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(OpaqueToken::from_client(token.as_str()), Ok(token));
    }

    #[test]
    fn two_generated_tokens_differ() {
        assert_ne!(generate(), generate());
    }

    #[test]
    fn from_client_rejects_63_chars_and_uppercase() {
        let valid = "0123456789abcdef".repeat(4);
        assert!(OpaqueToken::from_client(&valid).is_ok());

        let too_short = &valid[..63];
        let too_long = format!("{valid}0");
        let upper = valid.to_uppercase();
        let non_hex = format!("{}g", &valid[..63]);
        let multibyte = format!("{}é", &valid[..62]); // 64 bytes, not hex
        for bad in [
            too_short,
            too_long.as_str(),
            upper.as_str(),
            non_hex.as_str(),
            multibyte.as_str(),
            "",
        ] {
            assert_eq!(OpaqueToken::from_client(bad), Err(InvalidToken), "{bad:?}");
        }
    }

    #[test]
    fn hash_is_stable_for_same_token() {
        let token = generate();
        let again = OpaqueToken::from_client(token.as_str()).expect("valid");
        assert_eq!(token.hash(), again.hash());
        assert_ne!(token.hash(), generate().hash());
    }

    #[test]
    fn hash_is_sha256_of_hex_string() {
        // Expected value from `printf '0%.0s' $(seq 64) | sha256sum`.
        let zeros = OpaqueToken::from_client(&"0".repeat(64)).expect("valid");
        assert_eq!(
            to_lower_hex(&zeros.hash()),
            "60e05bd1b195af2f94112fa7197a5c88289058840ce7c6df9693756bc6250f55"
        );
    }

    #[test]
    fn hex_encoder_matches_known_value() {
        assert_eq!(to_lower_hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }

    #[test]
    fn debug_is_redacted() {
        let token = generate();
        let debug = format!("{token:?}");
        assert_eq!(debug, "OpaqueToken(***)");
        assert!(!debug.contains(token.as_str()));
    }
}

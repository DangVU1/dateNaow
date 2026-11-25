//! A plaintext password that satisfies the password policy (P4): 8-128
//! characters, counted as Unicode scalar values rather than bytes, with no
//! composition rules.

use std::fmt;

/// Shortest accepted password, in characters.
pub const MIN_PASSWORD_CHARS: usize = 8;
/// Longest accepted password, in characters. Together with the request body
/// limit this keeps Argon2 from being fed huge inputs.
pub const MAX_PASSWORD_CHARS: usize = 128;

/// A password that passed the policy. `Debug` never prints the value, so it
/// cannot leak into logs by accident.
pub struct PlainPassword(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    #[error("password is shorter than {MIN_PASSWORD_CHARS} characters")]
    TooShort,
    #[error("password is longer than {MAX_PASSWORD_CHARS} characters")]
    TooLong,
}

impl PasswordError {
    /// The field error code sent to clients.
    pub fn code(self) -> &'static str {
        match self {
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
        }
    }
}

impl PlainPassword {
    pub fn parse(raw: String) -> Result<Self, PasswordError> {
        // `take` stops counting early, so a huge input is not walked in full.
        let chars = raw.chars().take(MAX_PASSWORD_CHARS + 1).count();
        if chars < MIN_PASSWORD_CHARS {
            return Err(PasswordError::TooShort);
        }
        if chars > MAX_PASSWORD_CHARS {
            return Err(PasswordError::TooLong);
        }
        Ok(Self(raw))
    }

    /// The raw password, for hashing only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PlainPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlainPassword(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_7_chars() {
        assert_eq!(
            PlainPassword::parse("a".repeat(7)).unwrap_err(),
            PasswordError::TooShort
        );
    }

    #[test]
    fn accepts_8_chars() {
        let p = PlainPassword::parse("a".repeat(8)).expect("8 chars is valid");
        assert_eq!(p.expose(), "aaaaaaaa");
    }

    #[test]
    fn accepts_128_chars() {
        assert!(PlainPassword::parse("a".repeat(128)).is_ok());
    }

    #[test]
    fn rejects_129_chars() {
        assert_eq!(
            PlainPassword::parse("a".repeat(129)).unwrap_err(),
            PasswordError::TooLong
        );
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(
            PlainPassword::parse(String::new()).unwrap_err(),
            PasswordError::TooShort
        );
    }

    #[test]
    fn counts_unicode_chars_not_bytes() {
        let raw = "mậtkhẩu😺";
        assert_eq!(raw.chars().count(), 8);
        assert!(raw.len() > 8);
        assert!(PlainPassword::parse(raw.into()).is_ok());
        // 128 emoji are 512 bytes but still 128 characters.
        assert!(PlainPassword::parse("😺".repeat(128)).is_ok());
        assert_eq!(
            PlainPassword::parse("😺".repeat(129)).unwrap_err(),
            PasswordError::TooLong
        );
    }

    #[test]
    fn debug_never_prints_password() {
        let p = PlainPassword::parse("hunter42-secret".into()).expect("valid password");
        assert_eq!(format!("{p:?}"), "PlainPassword(***)");
    }

    #[test]
    fn error_codes_match_api_contract() {
        assert_eq!(PasswordError::TooShort.code(), "too_short");
        assert_eq!(PasswordError::TooLong.code(), "too_long");
    }
}

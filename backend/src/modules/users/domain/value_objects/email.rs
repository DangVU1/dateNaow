//! A normalized email address: trimmed and lowercased, so `" An@Mail.com "`
//! and `"an@mail.com"` are the same account (plan 1, Review Focus #2).

use std::fmt;

use validator::ValidateEmail;

/// Longest accepted address, in characters (matches the `users.email` CHECK).
pub const MAX_EMAIL_CHARS: usize = 254;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EmailError {
    #[error("email is empty")]
    Empty,
    #[error("email is longer than {MAX_EMAIL_CHARS} characters")]
    TooLong,
    #[error("email is not a valid address")]
    Invalid,
}

impl EmailError {
    /// The field error code sent to clients.
    pub fn code(self) -> &'static str {
        match self {
            Self::Empty => "required",
            Self::TooLong => "too_long",
            Self::Invalid => "invalid_email",
        }
    }
}

impl Email {
    pub fn parse(raw: &str) -> Result<Self, EmailError> {
        let normalized = raw.trim().to_lowercase();
        if normalized.is_empty() {
            return Err(EmailError::Empty);
        }
        if normalized.chars().count() > MAX_EMAIL_CHARS {
            return Err(EmailError::TooLong);
        }
        if !normalized.validate_email() {
            return Err(EmailError::Invalid);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trims_and_lowercases() {
        let email = Email::parse("  An@Example.COM ").expect("valid email");
        assert_eq!(email.as_str(), "an@example.com");
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Email::parse(""), Err(EmailError::Empty));
        assert_eq!(Email::parse("   "), Err(EmailError::Empty));
    }

    #[test]
    fn rejects_missing_at() {
        assert_eq!(Email::parse("an.example.com"), Err(EmailError::Invalid));
    }

    #[test]
    fn rejects_longer_than_254_chars() {
        // 64 + 1 + 189 = 254 characters is accepted; 255 is too long.
        // DNS labels are kept at <= 63 characters so the address is valid.
        let label = "d".repeat(61);
        let domain = format!("{label}.{label}.{label}.com");
        let at_limit = format!("{}@{}", "a".repeat(64), domain);
        assert_eq!(at_limit.chars().count(), 254);
        assert!(Email::parse(&at_limit).is_ok());

        let too_long = format!("{}@{}", "a".repeat(65), domain);
        assert_eq!(Email::parse(&too_long), Err(EmailError::TooLong));
    }

    #[test]
    fn error_codes_match_api_contract() {
        assert_eq!(EmailError::Empty.code(), "required");
        assert_eq!(EmailError::TooLong.code(), "too_long");
        assert_eq!(EmailError::Invalid.code(), "invalid_email");
    }
}

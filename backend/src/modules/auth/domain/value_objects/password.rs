//! A plaintext password that satisfies the password policy (P4): 8-128
//! characters, counted as Unicode scalar values rather than bytes, with no
//! composition rules. A new password (sign-up, password reset) must also not
//! be in the common-password list (DECISIONS #16).

use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

/// Shortest accepted password, in characters.
pub const MIN_PASSWORD_CHARS: usize = 8;
/// Longest accepted password, in characters. Together with the request body
/// limit this keeps Argon2 from being fed huge inputs.
pub const MAX_PASSWORD_CHARS: usize = 128;

/// SecLists `Passwords/Common-Credentials/10k-most-common.txt`, pinned at
/// commit e458f754946ed0accb41744db3f71f3a3bbcdbc8 and kept byte-for-byte.
/// MIT-licensed; the license is in `common_passwords/SECLISTS_LICENSE`.
const SECLISTS_10K: &str = include_str!("common_passwords/seclists_10k_most_common.txt");
/// Common Vietnamese and project-specific passwords, written for dateNaow.
const VIETNAMESE: &str = include_str!("common_passwords/vietnamese.txt");

/// Every listed password that the length rules would otherwise accept,
/// lowercased. Built once, on first use (about 2,100 entries).
static COMMON_PASSWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    add_entries(&mut set, SECLISTS_10K);
    add_entries(&mut set, VIETNAMESE);
    set
});

fn common_passwords() -> &'static HashSet<String> {
    &COMMON_PASSWORDS
}

/// Adds one entry per line of `list`. Lines are trimmed (a checkout with CRLF
/// line endings must not change the result) and lowercased; blank lines and
/// entries outside the length limits are skipped, since `TooShort` and
/// `TooLong` already reject those.
fn add_entries(set: &mut HashSet<String>, list: &str) {
    for line in list.lines() {
        let entry = line.trim();
        let chars = entry.chars().count();
        if (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&chars) {
            set.insert(entry.to_lowercase());
        }
    }
}

/// A password that passed the policy. `Debug` never prints the value, so it
/// cannot leak into logs by accident.
pub struct PlainPassword(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    #[error("password is shorter than {MIN_PASSWORD_CHARS} characters")]
    TooShort,
    #[error("password is longer than {MAX_PASSWORD_CHARS} characters")]
    TooLong,
    #[error("password is in the common-password list")]
    TooCommon,
}

impl PasswordError {
    /// The field error code sent to clients.
    pub fn code(self) -> &'static str {
        match self {
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
            Self::TooCommon => "too_common",
        }
    }
}

impl PlainPassword {
    /// Length rules only. Use it for a password that is checked against an
    /// existing account (sign-in), never for one that is being set.
    ///
    /// Sign-in must not reject common passwords: it treats a parse failure as
    /// wrong credentials *without* counting an attempt, so rejecting them here
    /// would let common-password guesses skip the attempt limits (P5), and an
    /// account created before #16 with such a password could not sign in.
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

    /// A password being set (sign-up, password reset): the length rules, then
    /// the common-password list, compared case-insensitively (#16).
    pub fn parse_new(raw: String) -> Result<Self, PasswordError> {
        let password = Self::parse(raw)?;
        // At most 128 characters here, so lowercasing is cheap.
        if common_passwords().contains(&password.0.to_lowercase()) {
            return Err(PasswordError::TooCommon);
        }
        Ok(password)
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
        assert_eq!(PasswordError::TooCommon.code(), "too_common");
    }

    // ---- common passwords (DECISIONS #16) ----

    #[test]
    fn parse_new_rejects_common_passwords() {
        for raw in ["password", "12345678", "iloveyou", "matkhau123", "anhyeuem"] {
            assert_eq!(
                PlainPassword::parse_new(raw.into()).unwrap_err(),
                PasswordError::TooCommon,
                "{raw}"
            );
        }
    }

    #[test]
    fn parse_new_ignores_case() {
        for raw in ["PassWord", "PASSWORD", "MatKhau123"] {
            assert_eq!(
                PlainPassword::parse_new(raw.into()).unwrap_err(),
                PasswordError::TooCommon,
                "{raw}"
            );
        }
    }

    #[test]
    fn parse_new_accepts_an_uncommon_password() {
        let p = PlainPassword::parse_new("correct horse battery".into()).expect("uncommon");
        assert_eq!(p.expose(), "correct horse battery");
    }

    #[test]
    fn parse_new_reports_length_before_commonness() {
        // "123456" is the most common password of all, but too short first.
        assert_eq!(
            PlainPassword::parse_new("123456".into()).unwrap_err(),
            PasswordError::TooShort
        );
        assert_eq!(
            PlainPassword::parse_new("a".repeat(129)).unwrap_err(),
            PasswordError::TooLong
        );
    }

    #[test]
    fn parse_for_sign_in_accepts_common_passwords() {
        // Login must not apply the list: see `PlainPassword::parse`.
        for raw in ["password", "password1", "12345678", "matkhau123"] {
            assert!(PlainPassword::parse(raw.into()).is_ok(), "{raw}");
        }
    }

    #[test]
    fn common_set_holds_only_entries_that_length_allows() {
        let set = common_passwords();
        // About 2,100 SecLists entries have 8+ characters, plus the
        // Vietnamese list.
        assert!(set.len() > 2_000, "{}", set.len());
        for entry in set {
            let chars = entry.chars().count();
            assert!(
                (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&chars),
                "{entry:?}"
            );
            assert_eq!(entry, &entry.to_lowercase(), "{entry:?}");
            assert_eq!(entry.trim(), entry, "{entry:?}");
        }
    }

    #[test]
    fn list_parsing_tolerates_crlf_and_blank_lines() {
        let mut set = HashSet::new();
        add_entries(&mut set, "Password\r\n\r\n  \r\nshort\r\nmatkhau123\n");
        assert_eq!(set.len(), 2);
        assert!(set.contains("password"));
        assert!(set.contains("matkhau123"));
    }
}

//! A plaintext password that satisfies the password policy (P4): 8-128
//! characters, counted as Unicode scalar values rather than bytes, with no
//! composition rules. A new password (sign-up, password reset) must also not
//! be in the common-password list (DECISIONS #16).
//!
//! Every password is normalised to Unicode NFC first (#17), so that "mật
//! khẩu" typed with precomposed letters on one device and with combining
//! marks on another is the same password. The length rules and the
//! common-password list apply to the NFC form, and only the NFC form is ever
//! hashed or verified.
//!
//! Two types keep sign-in and password setting apart: [`PlainPassword`]
//! (length rules only) is what sign-in verifies, and [`NewPassword`] (also
//! not a common password) is the only thing that can be hashed into a stored
//! password.

use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

use unicode_normalization::UnicodeNormalization;

/// Shortest accepted password, in characters.
pub const MIN_PASSWORD_CHARS: usize = 8;
/// Longest accepted password, in characters. Together with the request body
/// limit this keeps Argon2 from being fed huge inputs.
pub const MAX_PASSWORD_CHARS: usize = 128;

/// Longest raw input, in bytes, that could still be at most
/// `MAX_PASSWORD_CHARS` characters after NFC. NFC never drops characters and
/// composes each output character from at most 4 input code points (the
/// longest canonical decomposition in Unicode), each at most 4 bytes in
/// UTF-8. So more bytes than this always means more than 128 NFC
/// characters, and `TooLong` is the right answer without normalising.
const MAX_PASSWORD_BYTES: usize = MAX_PASSWORD_CHARS * 4 * 4;

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
/// `TooLong` already reject those. Entries are put in NFC like the passwords
/// they are compared with (#17); the bundled lists are ASCII today.
fn add_entries(set: &mut HashSet<String>, list: &str) {
    for line in list.lines() {
        let entry: String = line.trim().nfc().collect();
        let chars = entry.chars().count();
        if (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&chars) {
            set.insert(entry.to_lowercase());
        }
    }
}

/// A password that passed the length rules, in NFC. `Debug` never prints the
/// value, so it cannot leak into logs by accident.
pub struct PlainPassword(String);

/// A password that may be set on an account (sign-up, password reset): a
/// [`PlainPassword`] that is also not a common password (#16). Only
/// [`NewPassword::parse`] builds one, so code that stores a password hash
/// cannot be handed an unchecked password. `Debug` never prints the value.
pub struct NewPassword(PlainPassword);

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
    /// NFC, then the length rules only. Use it for a password that is checked
    /// against an existing account (sign-in); a password being set goes
    /// through [`NewPassword::parse`].
    ///
    /// Sign-in must not reject common passwords: it treats a parse failure as
    /// wrong credentials *without* counting an attempt, so rejecting them here
    /// would let common-password guesses skip the attempt limits (P5), and an
    /// account created before #16 with such a password could not sign in.
    pub fn parse(raw: String) -> Result<Self, PasswordError> {
        // Anything longer than this many bytes is over the limit in every
        // normalisation form, so it is rejected before NFC runs at all.
        if raw.len() > MAX_PASSWORD_BYTES {
            return Err(PasswordError::TooLong);
        }
        // Counted on the lazy NFC iterator with `take`. That stops early for
        // ordinary text, but NFC buffers a run of combining marks in full, so
        // the work is bounded by the byte guard above (and the request body
        // limit), not by `take`.
        let chars = raw.nfc().take(MAX_PASSWORD_CHARS + 1).count();
        if chars < MIN_PASSWORD_CHARS {
            return Err(PasswordError::TooShort);
        }
        if chars > MAX_PASSWORD_CHARS {
            return Err(PasswordError::TooLong);
        }
        Ok(Self(raw.nfc().collect()))
    }

    /// The NFC password, for hashing and verification only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl NewPassword {
    /// A password being set: NFC and the length rules (as
    /// [`PlainPassword::parse`]), then the common-password list, compared
    /// case-insensitively on the NFC form (#16, #17).
    pub fn parse(raw: String) -> Result<Self, PasswordError> {
        let password = PlainPassword::parse(raw)?;
        // At most 128 characters here, so lowercasing is cheap.
        if common_passwords().contains(&password.0.to_lowercase()) {
            return Err(PasswordError::TooCommon);
        }
        Ok(Self(password))
    }

    /// The NFC password, for hashing only.
    pub fn expose(&self) -> &str {
        self.0.expose()
    }
}

impl fmt::Debug for PlainPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlainPassword(***)")
    }
}

impl fmt::Debug for NewPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NewPassword(***)")
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
    fn new_password_rejects_common_passwords() {
        for raw in ["password", "12345678", "iloveyou", "matkhau123", "anhyeuem"] {
            assert_eq!(
                NewPassword::parse(raw.into()).unwrap_err(),
                PasswordError::TooCommon,
                "{raw}"
            );
        }
    }

    #[test]
    fn new_password_ignores_case() {
        for raw in ["PassWord", "PASSWORD", "MatKhau123"] {
            assert_eq!(
                NewPassword::parse(raw.into()).unwrap_err(),
                PasswordError::TooCommon,
                "{raw}"
            );
        }
    }

    #[test]
    fn new_password_accepts_an_uncommon_password() {
        let p = NewPassword::parse("correct horse battery".into()).expect("uncommon");
        assert_eq!(p.expose(), "correct horse battery");
    }

    #[test]
    fn new_password_reports_length_before_commonness() {
        // "123456" is the most common password of all, but too short first.
        assert_eq!(
            NewPassword::parse("123456".into()).unwrap_err(),
            PasswordError::TooShort
        );
        assert_eq!(
            NewPassword::parse("a".repeat(129)).unwrap_err(),
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

    // ---- NFC (DECISIONS #17) ----

    /// "mật khẩu đẹp lắm" with precomposed letters (NFC) and with base
    /// letters plus combining marks (NFD), as different keyboards send it.
    const VI_NFC: &str = "m\u{1EAD}t kh\u{1EA9}u \u{0111}\u{1EB9}p l\u{1EAF}m";
    const VI_NFD: &str =
        "ma\u{0323}\u{0302}t kha\u{0302}\u{0309}u \u{0111}e\u{0323}p la\u{0306}\u{0301}m";

    #[test]
    fn test_strings_are_the_nfc_and_nfd_forms_of_one_password() {
        assert_ne!(VI_NFC, VI_NFD);
        assert_eq!(VI_NFC.nfd().collect::<String>(), VI_NFD);
        assert_eq!(VI_NFD.nfc().collect::<String>(), VI_NFC);
    }

    #[test]
    fn parse_normalises_to_nfc() {
        let composed = PlainPassword::parse(VI_NFC.into()).expect("valid");
        let decomposed = PlainPassword::parse(VI_NFD.into()).expect("valid");
        assert_eq!(composed.expose(), VI_NFC);
        assert_eq!(decomposed.expose(), VI_NFC);
    }

    #[test]
    fn new_password_normalises_to_nfc() {
        let p = NewPassword::parse(VI_NFD.into()).expect("uncommon");
        assert_eq!(p.expose(), VI_NFC);
    }

    #[test]
    fn length_counts_characters_of_the_nfc_form() {
        // "ậ" is one character in NFC and three in NFD.
        let nfd_a = "a\u{0323}\u{0302}";
        // 7 NFC characters (21 in NFD): still too short.
        assert_eq!(
            PlainPassword::parse(nfd_a.repeat(7)).unwrap_err(),
            PasswordError::TooShort
        );
        // 128 NFC characters (384 in NFD): accepted, and stored composed.
        let p = PlainPassword::parse(nfd_a.repeat(128)).expect("128 NFC chars");
        assert_eq!(p.expose(), "\u{1EAD}".repeat(128));
        assert_eq!(
            PlainPassword::parse(nfd_a.repeat(129)).unwrap_err(),
            PasswordError::TooLong
        );
    }

    #[test]
    fn byte_guard_rejects_oversized_input_before_normalising() {
        assert_eq!(MAX_PASSWORD_BYTES, 2048);
        // One base letter and a long run of combining marks: NFC would
        // buffer the whole run; the guard answers first.
        let marks = format!("a{}", "\u{0301}".repeat(10_000));
        assert!(marks.len() > MAX_PASSWORD_BYTES);
        assert_eq!(
            PlainPassword::parse(marks).unwrap_err(),
            PasswordError::TooLong
        );
        assert_eq!(
            PlainPassword::parse("a".repeat(MAX_PASSWORD_BYTES + 1)).unwrap_err(),
            PasswordError::TooLong
        );
        assert_eq!(
            NewPassword::parse("a".repeat(MAX_PASSWORD_BYTES + 1)).unwrap_err(),
            PasswordError::TooLong
        );
    }

    #[test]
    fn byte_guard_does_not_reject_valid_long_decomposed_input() {
        // "ᾂ" decomposes into 4 code points (8 bytes): 128 of them are 1,024
        // bytes, under the guard, and 128 NFC characters.
        let nfd = "\u{03B1}\u{0313}\u{0300}\u{0345}";
        assert_eq!(nfd.nfc().collect::<String>(), "\u{1F82}");
        let p = PlainPassword::parse(nfd.repeat(128)).expect("128 NFC chars");
        assert_eq!(p.expose().chars().count(), 128);
        // 128 four-byte characters (512 bytes) are fine too.
        assert!(PlainPassword::parse("😺".repeat(128)).is_ok());
    }

    #[test]
    fn common_password_check_uses_the_nfc_form() {
        let mut set = HashSet::new();
        add_entries(&mut set, VI_NFD);
        // The list entry is stored in NFC, so the NFC form of either input
        // matches it.
        assert!(set.contains(VI_NFC));
        assert!(!set.contains(VI_NFD));
    }

    #[test]
    fn new_password_debug_never_prints_password() {
        let p = NewPassword::parse("hunter42-secret".into()).expect("valid password");
        assert_eq!(format!("{p:?}"), "NewPassword(***)");
    }
}

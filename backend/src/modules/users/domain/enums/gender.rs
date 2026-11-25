//! Gender: optional, changeable at any time, and never shown to the partner
//! (spec §6.1).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Male,
    Female,
    Other,
    PreferNotToSay,
}

impl Gender {
    pub const ALL: [Self; 4] = [Self::Male, Self::Female, Self::Other, Self::PreferNotToSay];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Male => "male",
            Self::Female => "female",
            Self::Other => "other",
            Self::PreferNotToSay => "prefer_not_to_say",
        }
    }
}

impl fmt::Display for Gender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returned by `Gender::from_str` for a value outside the four choices.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown gender")]
pub struct ParseGenderError;

impl FromStr for Gender {
    type Err = ParseGenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|gender| gender.as_str() == s)
            .ok_or(ParseGenderError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_four_values() {
        let cases = [
            ("male", Gender::Male),
            ("female", Gender::Female),
            ("other", Gender::Other),
            ("prefer_not_to_say", Gender::PreferNotToSay),
        ];
        for (raw, expected) in cases {
            assert_eq!(raw.parse::<Gender>(), Ok(expected));
            assert_eq!(expected.as_str(), raw);
            let json = serde_json::to_string(&expected).expect("serialize gender");
            assert_eq!(json, format!("\"{raw}\""));
        }
    }

    #[test]
    fn rejects_unknown_value() {
        assert_eq!("unknown".parse::<Gender>(), Err(ParseGenderError));
        assert_eq!("Male".parse::<Gender>(), Err(ParseGenderError));
        assert_eq!("".parse::<Gender>(), Err(ParseGenderError));
    }
}

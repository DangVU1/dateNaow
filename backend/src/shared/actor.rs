//! Who is making a request: user id and role. Shared by every module because
//! every module checks permissions.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::error::ApiError;

/// A user's role. Serialized in snake_case because it is part of the JWT claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returned by `Role::from_str` for a value that is not a known role.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown role")]
pub struct ParseRoleError;

impl FromStr for Role {
    type Err = ParseRoleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "admin" => Ok(Self::Admin),
            _ => Err(ParseRoleError),
        }
    }
}

/// The authenticated user behind a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub user_id: Uuid,
    pub role: Role,
}

impl Actor {
    pub fn require_admin(&self) -> Result<(), Forbidden> {
        match self.role {
            Role::Admin => Ok(()),
            Role::User => Err(Forbidden),
        }
    }
}

/// The actor lacks the permission an operation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("forbidden")]
pub struct Forbidden;

impl From<Forbidden> for ApiError {
    fn from(_: Forbidden) -> Self {
        Self::Forbidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: Role) -> Actor {
        Actor {
            user_id: Uuid::nil(),
            role,
        }
    }

    #[test]
    fn require_admin_ok_for_admin() {
        assert_eq!(actor(Role::Admin).require_admin(), Ok(()));
    }

    #[test]
    fn require_admin_forbidden_for_user() {
        assert_eq!(actor(Role::User).require_admin(), Err(Forbidden));
    }

    #[test]
    fn forbidden_maps_to_api_forbidden() {
        assert!(matches!(ApiError::from(Forbidden), ApiError::Forbidden));
    }

    #[test]
    fn role_round_trips_through_str_and_serde() {
        for role in [Role::User, Role::Admin] {
            assert_eq!(role.as_str().parse::<Role>(), Ok(role));
            let json = serde_json::to_string(&role).expect("serialize role");
            assert_eq!(json, format!("\"{}\"", role.as_str()));
            assert_eq!(
                serde_json::from_str::<Role>(&json).expect("deserialize role"),
                role
            );
        }
        assert_eq!("Admin".parse::<Role>(), Err(ParseRoleError));
    }
}

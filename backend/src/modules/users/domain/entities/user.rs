use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::modules::users::domain::enums::gender::Gender;
use crate::modules::users::domain::value_objects::email::Email;
use crate::shared::actor::Role;

/// A user account as the rest of the app sees it. Deliberately carries no
/// password hash: only `UserCredentials` does, and only for sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub email: Email,
    pub role: Role,
    pub gender: Option<Gender>,
    pub created_at: DateTime<Utc>,
}

/// What sign-in needs to check a password and issue a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserCredentials {
    pub user_id: Uuid,
    pub password_hash: String,
    pub role: Role,
}

/// Input for creating a user. The password is already hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUser {
    pub email: Email,
    pub password_hash: String,
}

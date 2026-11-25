//! Response shapes for users. Never carries the password hash.

use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::users::domain::User;

#[derive(Debug, Serialize, ToSchema)]
pub struct UserResponse {
    /// A UUID.
    #[schema(value_type = String, example = "01936f3a-1b2c-7d4e-8f00-112233445566")]
    pub id: Uuid,
    /// Normalized: trimmed and lowercased.
    pub email: String,
    /// `user` or `admin`.
    #[schema(example = "user")]
    pub role: &'static str,
    /// `male`, `female`, `other`, `prefer_not_to_say`, or `null` when not set.
    pub gender: Option<&'static str>,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            email: user.email.as_str().to_owned(),
            role: user.role.as_str(),
            gender: user.gender.map(|g| g.as_str()),
        }
    }
}

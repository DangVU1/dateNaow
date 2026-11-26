//! Request and response shapes for users. Never carries the password hash.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::modules::users::application::update_gender::UpdateGenderCommand;
use crate::modules::users::domain::User;
use crate::shared::error::{ApiError, FieldErrors};
use crate::shared::extract::double_option;

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

/// Body of `PATCH /api/v1/me`. Unknown fields (e.g. `role`) are rejected.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMeRequest {
    /// Required, but may be `null` to clear the choice. One of `male`,
    /// `female`, `other`, `prefer_not_to_say`.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, required = true, example = "female")]
    pub gender: Option<Option<String>>,
}

impl UpdateMeRequest {
    /// A missing `gender` gives 422 `{"gender": ["required"]}`.
    pub fn into_command(self, actor_id: Uuid) -> Result<UpdateGenderCommand, ApiError> {
        let Some(gender) = self.gender else {
            let mut fields = FieldErrors::new();
            fields.insert("gender".to_owned(), vec!["required".to_owned()]);
            return Err(ApiError::Validation(fields));
        };
        Ok(UpdateGenderCommand { actor_id, gender })
    }
}

//! Errors returned by the users use cases, and their mapping to HTTP (P2).

use std::fmt;

use crate::modules::users::domain::UserRepoError;
use crate::shared::error::{ApiError, FieldErrors, display_chain};

#[derive(thiserror::Error)]
pub enum UsersError {
    #[error("validation failed")]
    Validation(FieldErrors),
    /// The account behind a still-valid token no longer exists.
    #[error("user not found")]
    NotFound,
    /// Any unexpected failure. Only its `Display` chain is ever logged.
    #[error("internal users error: {0}")]
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

/// Written by hand so `{:?}` never prints the `Debug` of an internal source.
impl fmt::Debug for UsersError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(fields) => f.debug_tuple("Validation").field(fields).finish(),
            Self::NotFound => f.write_str("NotFound"),
            Self::Internal(error) => f
                .debug_tuple("Internal")
                .field(&display_chain(error.as_ref()))
                .finish(),
        }
    }
}

impl From<UserRepoError> for UsersError {
    fn from(error: UserRepoError) -> Self {
        match error {
            UserRepoError::NotFound => Self::NotFound,
            other => Self::Internal(Box::new(other)),
        }
    }
}

impl From<UsersError> for ApiError {
    fn from(error: UsersError) -> Self {
        match error {
            UsersError::Validation(fields) => Self::Validation(fields),
            // The token is valid but the account is gone: sign in again.
            UsersError::NotFound => Self::Unauthorized("unauthorized"),
            UsersError::Internal(source) => {
                Self::Internal(anyhow::anyhow!(display_chain(source.as_ref())))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn maps_each_variant_to_status_and_code() {
        let mut fields = FieldErrors::new();
        fields.insert("gender".to_owned(), vec!["invalid_choice".to_owned()]);
        let cases = [
            (UsersError::Validation(fields), 422, "validation_error"),
            (UsersError::NotFound, 401, "unauthorized"),
            (
                UsersError::from(UserRepoError::Database("boom".into())),
                500,
                "internal_error",
            ),
        ];
        for (error, status, code) in cases {
            let label = format!("{error:?}");
            let api = ApiError::from(error);
            assert_eq!(
                api.status(),
                StatusCode::from_u16(status).unwrap(),
                "{label}"
            );
            assert_eq!(api.code(), code, "{label}");
        }
    }

    #[test]
    fn repo_not_found_becomes_not_found() {
        assert!(matches!(
            UsersError::from(UserRepoError::NotFound),
            UsersError::NotFound
        ));
    }
}

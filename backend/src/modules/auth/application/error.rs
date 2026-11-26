//! Errors returned by the auth use cases, and their mapping to HTTP (P2, P3).

use std::fmt;

use crate::shared::error::{ApiError, FieldErrors, display_chain};

#[derive(thiserror::Error)]
pub enum AuthError {
    #[error("validation failed")]
    Validation(FieldErrors),
    #[error("email is already taken")]
    EmailTaken,
    /// Wrong password or unknown email: deliberately indistinguishable.
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("invalid refresh token")]
    InvalidRefreshToken,
    /// The refresh token was rotated moments ago, typically by another tab.
    #[error("refresh conflict")]
    RefreshConflict,
    #[error("invalid or expired password-reset token")]
    InvalidOrExpiredResetToken,
    /// Any unexpected failure. Only its `Display` chain is ever logged: the
    /// `Debug` of some sources (e.g. sqlx database errors) can contain row data.
    #[error("internal auth error: {0}")]
    Internal(Box<dyn std::error::Error + Send + Sync>),
}

impl AuthError {
    pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(error))
    }
}

/// Written by hand so `{:?}` never prints the `Debug` of an internal source.
impl fmt::Debug for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(fields) => f.debug_tuple("Validation").field(fields).finish(),
            Self::RateLimited { retry_after_secs } => f
                .debug_struct("RateLimited")
                .field("retry_after_secs", retry_after_secs)
                .finish(),
            Self::Internal(error) => f
                .debug_tuple("Internal")
                .field(&display_chain(error.as_ref()))
                .finish(),
            Self::EmailTaken => f.write_str("EmailTaken"),
            Self::InvalidCredentials => f.write_str("InvalidCredentials"),
            Self::InvalidRefreshToken => f.write_str("InvalidRefreshToken"),
            Self::RefreshConflict => f.write_str("RefreshConflict"),
            Self::InvalidOrExpiredResetToken => f.write_str("InvalidOrExpiredResetToken"),
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::Validation(fields) => Self::Validation(fields),
            AuthError::EmailTaken => Self::Conflict("email_taken"),
            AuthError::InvalidCredentials => Self::Unauthorized("invalid_credentials"),
            AuthError::RateLimited { retry_after_secs } => Self::RateLimited { retry_after_secs },
            AuthError::InvalidRefreshToken => Self::Unauthorized("invalid_refresh_token"),
            AuthError::RefreshConflict => Self::Conflict("refresh_conflict"),
            AuthError::InvalidOrExpiredResetToken => Self::BadRequest("invalid_or_expired_token"),
            // Rebuilt from `Display` only, so the 500 log line carries no `Debug` detail.
            AuthError::Internal(source) => {
                Self::Internal(anyhow::anyhow!(display_chain(source.as_ref())))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("outer")]
    struct Outer(#[source] Inner);

    /// `Debug` holds "secret row data"; `Display` does not.
    #[derive(thiserror::Error)]
    #[error("inner")]
    struct Inner;

    impl fmt::Debug for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("secret row data")
        }
    }

    #[test]
    fn maps_each_variant_to_status_and_code() {
        let mut fields = FieldErrors::new();
        fields.insert("email".to_owned(), vec!["invalid".to_owned()]);
        let cases = [
            (AuthError::Validation(fields), 422, "validation_error"),
            (AuthError::EmailTaken, 409, "email_taken"),
            (AuthError::InvalidCredentials, 401, "invalid_credentials"),
            (
                AuthError::RateLimited {
                    retry_after_secs: 900,
                },
                429,
                "rate_limited",
            ),
            (AuthError::InvalidRefreshToken, 401, "invalid_refresh_token"),
            (AuthError::RefreshConflict, 409, "refresh_conflict"),
            (
                AuthError::InvalidOrExpiredResetToken,
                400,
                "invalid_or_expired_token",
            ),
            (AuthError::internal(Outer(Inner)), 500, "internal_error"),
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
    fn rate_limited_keeps_retry_after() {
        let api = ApiError::from(AuthError::RateLimited {
            retry_after_secs: 900,
        });
        assert!(matches!(
            api,
            ApiError::RateLimited {
                retry_after_secs: 900
            }
        ));
    }

    #[test]
    fn internal_keeps_display_chain_but_never_debug() {
        let error = AuthError::internal(Outer(Inner));
        assert!(!format!("{error:?}").contains("secret row data"));

        let ApiError::Internal(inner) = ApiError::from(error) else {
            panic!("expected ApiError::Internal");
        };
        let logged = format!("{inner:?}");
        assert!(logged.contains("outer: inner"), "{logged}");
        assert!(!logged.contains("secret row data"), "{logged}");
    }
}

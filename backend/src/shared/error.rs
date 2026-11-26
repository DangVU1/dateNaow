//! The API error type and its JSON envelope (P2):
//! `{"error":{"code":"...","message":"...","fields":{...}}}`.
//!
//! Clients branch on `code` (and, for 422, on the field names in `fields`);
//! `message` is a generic Vietnamese fallback for display.

use std::collections::BTreeMap;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;
use validator::{ValidationErrors, ValidationErrorsKind};

/// Field name to the list of error codes for that field,
/// e.g. `"password" -> ["too_short"]`.
pub type FieldErrors = BTreeMap<String, Vec<String>>;

/// Message returned for every 500. The cause is only logged.
pub const INTERNAL_MESSAGE: &str = "Đã có lỗi, bạn thử lại sau nhé.";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// HTTP 400: the code says what was wrong, e.g. `"invalid_json"`.
    #[error("bad request: {0}")]
    BadRequest(&'static str),
    /// HTTP 422 with per-field error codes.
    #[error("validation failed: {0:?}")]
    Validation(FieldErrors),
    /// HTTP 401: the code is usually `"unauthorized"` or a more specific one such
    /// as `"invalid_credentials"`.
    #[error("unauthorized: {0}")]
    Unauthorized(&'static str),
    /// HTTP 403.
    #[error("forbidden")]
    Forbidden,
    /// HTTP 404.
    #[error("not found")]
    NotFound,
    /// HTTP 409: the code names the conflict, e.g. `"email_taken"`.
    #[error("conflict: {0}")]
    Conflict(&'static str),
    /// HTTP 413.
    #[error("payload too large")]
    PayloadTooLarge,
    /// HTTP 408.
    #[error("request timed out")]
    Timeout,
    /// HTTP 429 with a `Retry-After` header.
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    /// HTTP 500: the cause is logged, never sent to the client.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// The JSON error envelope, as documented in OpenAPI.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// Machine-readable error code, e.g. `invalid_json` or `email_taken`.
    pub code: &'static str,
    /// Generic Vietnamese message; clients should prefer their own text per `code`.
    pub message: &'static str,
    /// Only for 422: field name to the list of error codes for that field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<FieldErrors>,
}

impl ApiError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(code) | Self::Unauthorized(code) | Self::Conflict(code) => code,
            Self::Validation(_) => "validation_error",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::PayloadTooLarge => "payload_too_large",
            Self::Timeout => "request_timeout",
            Self::RateLimited { .. } => "rate_limited",
            Self::Internal(_) => "internal_error",
        }
    }

    fn message(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "Dữ liệu gửi lên không hợp lệ.",
            Self::Validation(_) => "Một số thông tin chưa hợp lệ.",
            Self::Unauthorized(_) => "Bạn cần đăng nhập lại.",
            Self::Forbidden => "Bạn không có quyền làm việc này.",
            Self::NotFound => "Không tìm thấy nội dung này.",
            Self::Conflict(_) => "Dữ liệu bị trùng hoặc xung đột.",
            Self::PayloadTooLarge => "Dữ liệu gửi lên quá lớn.",
            Self::Timeout => "Máy chủ phản hồi quá lâu, bạn thử lại nhé.",
            Self::RateLimited { .. } => "Bạn thử quá nhiều lần, chờ một lát rồi thử lại nhé.",
            Self::Internal(_) => INTERNAL_MESSAGE,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorBody {
            code: self.code(),
            message: self.message(),
            fields: None,
        };

        match self {
            Self::Validation(fields) => (
                status,
                Json(ErrorResponse {
                    error: ErrorBody {
                        fields: Some(fields),
                        ..body
                    },
                }),
            )
                .into_response(),
            Self::RateLimited { retry_after_secs } => {
                let mut response = (status, Json(ErrorResponse { error: body })).into_response();
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from(retry_after_secs));
                response
            }
            Self::Internal(error) => {
                tracing::error!(error = ?error, "internal error");
                (status, Json(ErrorResponse { error: body })).into_response()
            }
            _ => (status, Json(ErrorResponse { error: body })).into_response(),
        }
    }
}

impl From<ValidationErrors> for ApiError {
    fn from(errors: ValidationErrors) -> Self {
        Self::Validation(field_errors(&errors))
    }
}

/// `error: source: source-of-source`, using `Display` at every level. Used to
/// log internal errors whose `Debug` (e.g. sqlx database errors) can contain
/// row data.
pub fn display_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        out.push_str(": ");
        out.push_str(&inner.to_string());
        source = inner.source();
    }
    out
}

/// Flattens `validator` errors into `FieldErrors`, using each error's `code`.
/// Nested structs become `parent.child`, list items `parent[0].child`.
pub fn field_errors(errors: &ValidationErrors) -> FieldErrors {
    let mut out = FieldErrors::new();
    collect_field_errors(errors, None, &mut out);
    out
}

fn collect_field_errors(errors: &ValidationErrors, prefix: Option<&str>, out: &mut FieldErrors) {
    for (field, kind) in errors.errors() {
        let path = match prefix {
            Some(prefix) => format!("{prefix}.{field}"),
            None => field.to_string(),
        };
        match kind {
            ValidationErrorsKind::Field(list) => {
                out.entry(path)
                    .or_default()
                    .extend(list.iter().map(|error| error.code.to_string()));
            }
            ValidationErrorsKind::Struct(inner) => collect_field_errors(inner, Some(&path), out),
            ValidationErrorsKind::List(items) => {
                for (index, inner) in items {
                    collect_field_errors(inner, Some(&format!("{path}[{index}]")), out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use serde_json::{Value, json};
    use validator::ValidationError;

    use super::*;

    async fn parts(error: ApiError) -> (StatusCode, axum::http::HeaderMap, Value) {
        let response = error.into_response();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        let body = serde_json::from_slice(&bytes).expect("JSON body");
        (status, headers, body)
    }

    #[tokio::test]
    async fn validation_maps_to_422_with_fields() {
        let mut fields = FieldErrors::new();
        fields.insert("password".to_owned(), vec!["too_short".to_owned()]);

        let (status, _, body) = parts(ApiError::Validation(fields)).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "validation_error");
        assert_eq!(body["error"]["fields"], json!({"password": ["too_short"]}));
        assert!(body["error"]["message"].is_string());
    }

    #[tokio::test]
    async fn rate_limited_sets_retry_after_header() {
        let (status, headers, body) = parts(ApiError::RateLimited {
            retry_after_secs: 120,
        })
        .await;

        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(headers[header::RETRY_AFTER], "120");
        assert_eq!(body["error"]["code"], "rate_limited");
    }

    #[tokio::test]
    async fn internal_hides_details() {
        let (status, _, body) = parts(ApiError::Internal(anyhow::anyhow!(
            "connection to db-secret-host refused"
        )))
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            body,
            json!({"error": {"code": "internal_error", "message": INTERNAL_MESSAGE}})
        );
        assert!(!body.to_string().contains("db-secret-host"));
    }

    #[tokio::test]
    async fn unauthorized_uses_given_code() {
        let (status, _, body) = parts(ApiError::Unauthorized("invalid_credentials")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_credentials");
    }

    #[tokio::test]
    async fn non_validation_errors_omit_fields() {
        let (_, _, body) = parts(ApiError::NotFound).await;
        assert_eq!(body["error"]["code"], "not_found");
        assert!(body["error"].get("fields").is_none());
    }

    #[test]
    fn default_codes_and_statuses() {
        let cases = [
            (ApiError::BadRequest("invalid_json"), 400, "invalid_json"),
            (ApiError::Forbidden, 403, "forbidden"),
            (ApiError::NotFound, 404, "not_found"),
            (ApiError::Conflict("email_taken"), 409, "email_taken"),
            (ApiError::PayloadTooLarge, 413, "payload_too_large"),
            (ApiError::Timeout, 408, "request_timeout"),
        ];
        for (error, status, code) in cases {
            assert_eq!(error.status().as_u16(), status, "{error:?}");
            assert_eq!(error.code(), code, "{error:?}");
        }
    }

    #[test]
    fn field_errors_use_codes_and_flatten_nested_paths() {
        let mut inner = ValidationErrors::new();
        inner.add("city", ValidationError::new("required"));

        let mut errors = ValidationErrors::new();
        errors.add("name", ValidationError::new("length"));
        errors.add("name", ValidationError::new("blank"));
        errors.errors_mut().insert(
            "address".into(),
            ValidationErrorsKind::Struct(Box::new(inner)),
        );

        let fields = field_errors(&errors);

        assert_eq!(fields["name"], vec!["length", "blank"]);
        assert_eq!(fields["address.city"], vec!["required"]);
    }
}

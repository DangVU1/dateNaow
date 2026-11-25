//! Request extractors that reject with the JSON error envelope.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::shared::error::ApiError;

/// A JSON body that is deserialized and then validated with `validator`.
///
/// - Malformed JSON, wrong types, missing fields and a missing
///   `Content-Type: application/json` all give 400 `invalid_json`. (Axum's
///   own `Json` answers 422 for type errors; 422 is kept for rule violations.)
/// - A body over the `DefaultBodyLimit` gives 413 `payload_too_large`.
/// - Failed validation gives 422 `validation_error` with field codes.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(json_rejection_to_api_error)?;
        value.validate()?;
        Ok(Self(value))
    }
}

fn json_rejection_to_api_error(rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::JsonSyntaxError(_)
        | JsonRejection::JsonDataError(_)
        | JsonRejection::MissingJsonContentType(_) => ApiError::BadRequest("invalid_json"),
        // In axum 0.8 the body limit surfaces here as
        // `BytesRejection::FailedToBufferBody(LengthLimitError)` with status 413.
        JsonRejection::BytesRejection(inner) if inner.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            ApiError::PayloadTooLarge
        }
        // Other buffering failures (e.g. the client aborted the upload).
        JsonRejection::BytesRejection(_) => ApiError::BadRequest("bad_request"),
        // `JsonRejection` is non-exhaustive.
        _ => ApiError::BadRequest("bad_request"),
    }
}

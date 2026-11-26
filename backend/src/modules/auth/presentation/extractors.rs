//! `CurrentUser`: the actor behind a request, from `Authorization: Bearer <jwt>`.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};

use crate::shared::actor::Actor;
use crate::shared::error::ApiError;
use crate::state::AppState;

/// The authenticated user. A missing or malformed header, a bad signature and
/// an expired token are all rejected with 401 `unauthorized`. The role is the
/// one in the token; admin checks use `AdminActor`, which re-reads it (P7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentUser(pub Actor);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = bearer_token(&parts.headers).ok_or(UNAUTHORIZED)?;
        let claims = state
            .jwt
            .verify(token, state.clock.now())
            .map_err(|_| UNAUTHORIZED)?;
        Ok(Self(Actor {
            user_id: claims.sub,
            role: claims.role,
        }))
    }
}

const UNAUTHORIZED: ApiError = ApiError::Unauthorized("unauthorized");

/// The token from `Authorization: Bearer <token>`. The scheme is matched
/// case-insensitively (RFC 9110 §11.1); an empty token counts as missing.
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(value).expect("valid header value"),
        );
        headers
    }

    #[test]
    fn reads_bearer_token() {
        assert_eq!(bearer_token(&headers("Bearer abc.def")), Some("abc.def"));
        assert_eq!(bearer_token(&headers("bearer abc")), Some("abc"));
    }

    #[test]
    fn rejects_missing_or_malformed_header() {
        assert_eq!(bearer_token(&HeaderMap::new()), None);
        for value in ["Bearer", "Bearer ", "Basic abc", "abc", "Bearerabc"] {
            assert_eq!(bearer_token(&headers(value)), None, "{value}");
        }
    }
}

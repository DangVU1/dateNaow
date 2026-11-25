//! `AdminActor`: a signed-in user whose **current** role in the database is
//! `admin` (P7). The role in the token is ignored, so a demoted admin loses
//! access at once, even with a token issued before the demotion.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::modules::auth::presentation::extractors::CurrentUser;
use crate::modules::users::application::UsersError;
use crate::modules::users::domain::UserRepository;
use crate::shared::actor::Actor;
use crate::shared::error::ApiError;
use crate::state::AppState;

/// Rejections: no valid token, or the account no longer exists: 401
/// `unauthorized`; the account is not an admin: 403 `forbidden`.
/// From plan 2 on, admin use cases also call `actor.require_admin()` themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdminActor(pub Actor);

impl FromRequestParts<AppState> for AdminActor {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let CurrentUser(from_token) = CurrentUser::from_request_parts(parts, state).await?;
        let user = state
            .users_repo
            .find_by_id(from_token.user_id)
            .await
            .map_err(UsersError::from)?
            .ok_or(UsersError::NotFound)?;
        let actor = Actor {
            user_id: user.id,
            role: user.role,
        };
        actor.require_admin()?;
        Ok(Self(actor))
    }
}

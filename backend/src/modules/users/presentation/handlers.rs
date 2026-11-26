//! HTTP handlers for `/api/v1/me`. Thin: extract, delegate, shape the response.
//! Both read the user id from the access token only (`CurrentUser`).

use axum::Json;
use axum::extract::State;

use crate::modules::auth::presentation::extractors::CurrentUser;
use crate::modules::users::application::get_current_user::GetCurrentUserQuery;
use crate::modules::users::presentation::dto::{UpdateMeRequest, UserResponse};
use crate::shared::error::{ApiError, ErrorResponse};
use crate::shared::extract::ValidatedJson;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "users",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "The signed-in user's own account", body = UserResponse),
        (status = 401, description = "`unauthorized`: missing, malformed or expired access token, or the account no longer exists", body = ErrorResponse),
    )
)]
pub async fn get_current_user(
    State(state): State<AppState>,
    CurrentUser(actor): CurrentUser,
) -> Result<Json<UserResponse>, ApiError> {
    let user = state
        .users
        .get_current_user
        .execute(GetCurrentUserQuery {
            actor_id: actor.user_id,
        })
        .await?;
    Ok(Json(UserResponse::from(&user)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/me",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = UpdateMeRequest,
    responses(
        (status = 200, description = "Gender updated (or cleared with `null`)", body = UserResponse),
        (status = 400, description = "`invalid_json`: malformed JSON, wrong type or unknown field", body = ErrorResponse),
        (status = 401, description = "`unauthorized`", body = ErrorResponse),
        (status = 422, description = "`gender`: `required` when the field is missing, `invalid_choice` for an unknown value", body = ErrorResponse),
    )
)]
pub async fn update_gender(
    State(state): State<AppState>,
    CurrentUser(actor): CurrentUser,
    ValidatedJson(body): ValidatedJson<UpdateMeRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let command = body.into_command(actor.user_id)?;
    let user = state.users.update_gender.execute(command).await?;
    Ok(Json(UserResponse::from(&user)))
}

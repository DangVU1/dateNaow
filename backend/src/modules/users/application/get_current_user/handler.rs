use crate::modules::users::application::UsersError;
use crate::modules::users::application::get_current_user::GetCurrentUserQuery;
use crate::modules::users::domain::{User, UserRepository};

pub struct GetCurrentUserHandler<R> {
    users: R,
}

impl<R: UserRepository> GetCurrentUserHandler<R> {
    pub fn new(users: R) -> Self {
        Self { users }
    }

    /// `NotFound` when the account behind the token no longer exists.
    pub async fn execute(&self, query: GetCurrentUserQuery) -> Result<User, UsersError> {
        self.users
            .find_by_id(query.actor_id)
            .await?
            .ok_or(UsersError::NotFound)
    }
}

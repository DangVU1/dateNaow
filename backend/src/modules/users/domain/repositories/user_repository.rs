use std::future::Future;

use uuid::Uuid;

use crate::modules::users::domain::entities::user::{NewUser, User, UserCredentials};
use crate::modules::users::domain::enums::gender::Gender;
use crate::modules::users::domain::value_objects::email::Email;
use crate::shared::actor::Role;

#[derive(Debug, thiserror::Error)]
pub enum UserRepoError {
    #[error("email is already taken")]
    EmailTaken,
    #[error("user not found")]
    NotFound,
    /// Any storage failure. The source is kept for logs, never sent to clients.
    #[error("user storage error")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait UserRepository: Send + Sync + 'static {
    /// Fails with `EmailTaken` when another user already has this email.
    fn create(&self, new_user: NewUser)
    -> impl Future<Output = Result<User, UserRepoError>> + Send;

    fn find_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<User>, UserRepoError>> + Send;

    fn find_credentials_by_email(
        &self,
        email: &Email,
    ) -> impl Future<Output = Result<Option<UserCredentials>, UserRepoError>> + Send;

    /// Fails with `NotFound` when the user does not exist.
    fn update_gender(
        &self,
        id: Uuid,
        gender: Option<Gender>,
    ) -> impl Future<Output = Result<User, UserRepoError>> + Send;

    /// Returns `false` when no user has this email.
    fn set_role(
        &self,
        email: &Email,
        role: Role,
    ) -> impl Future<Output = Result<bool, UserRepoError>> + Send;
}

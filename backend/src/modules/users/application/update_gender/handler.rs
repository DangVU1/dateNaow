use crate::modules::users::application::UsersError;
use crate::modules::users::application::update_gender::UpdateGenderCommand;
use crate::modules::users::domain::{Gender, User, UserRepository};
use crate::shared::error::FieldErrors;

pub struct UpdateGenderHandler<R> {
    users: R,
}

impl<R: UserRepository> UpdateGenderHandler<R> {
    pub fn new(users: R) -> Self {
        Self { users }
    }

    /// A value outside the four choices gives `Validation{"gender": ["invalid_choice"]}`
    /// without touching storage. `NotFound` when the account no longer exists.
    pub async fn execute(&self, command: UpdateGenderCommand) -> Result<User, UsersError> {
        let gender = command
            .gender
            .as_deref()
            .map(str::parse::<Gender>)
            .transpose()
            .map_err(|_| invalid_choice())?;
        Ok(self.users.update_gender(command.actor_id, gender).await?)
    }
}

fn invalid_choice() -> UsersError {
    let mut fields = FieldErrors::new();
    fields.insert("gender".to_owned(), vec!["invalid_choice".to_owned()]);
    UsersError::Validation(fields)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::modules::users::domain::{Email, NewUser, UserCredentials, UserRepoError};
    use crate::shared::actor::Role;

    /// One stored user (or none), and a log of `update_gender` calls.
    struct FakeRepo {
        user: Option<User>,
        updates: Mutex<Vec<Option<Gender>>>,
    }

    impl FakeRepo {
        fn with_user(id: Uuid) -> Self {
            Self {
                user: Some(User {
                    id,
                    email: Email::parse("an@example.com").expect("valid email"),
                    role: Role::User,
                    gender: None,
                    created_at: Utc::now(),
                }),
                updates: Mutex::new(Vec::new()),
            }
        }

        fn empty() -> Self {
            Self {
                user: None,
                updates: Mutex::new(Vec::new()),
            }
        }

        fn updates(&self) -> Vec<Option<Gender>> {
            self.updates.lock().expect("updates mutex").clone()
        }
    }

    impl UserRepository for Arc<FakeRepo> {
        async fn create(&self, _: NewUser) -> Result<User, UserRepoError> {
            unimplemented!("not used by UpdateGenderHandler")
        }

        async fn find_by_id(&self, _: Uuid) -> Result<Option<User>, UserRepoError> {
            unimplemented!("not used by UpdateGenderHandler")
        }

        async fn find_credentials_by_email(
            &self,
            _: &Email,
        ) -> Result<Option<UserCredentials>, UserRepoError> {
            unimplemented!("not used by UpdateGenderHandler")
        }

        async fn update_gender(
            &self,
            id: Uuid,
            gender: Option<Gender>,
        ) -> Result<User, UserRepoError> {
            self.updates.lock().expect("updates mutex").push(gender);
            match &self.user {
                Some(user) if user.id == id => Ok(User {
                    gender,
                    ..user.clone()
                }),
                _ => Err(UserRepoError::NotFound),
            }
        }

        async fn set_role(&self, _: &Email, _: Role) -> Result<bool, UserRepoError> {
            unimplemented!("not used by UpdateGenderHandler")
        }
    }

    fn command(actor_id: Uuid, gender: Option<&str>) -> UpdateGenderCommand {
        UpdateGenderCommand {
            actor_id,
            gender: gender.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn sets_a_known_gender() {
        let id = Uuid::from_u128(1);
        let repo = Arc::new(FakeRepo::with_user(id));

        let user = UpdateGenderHandler::new(repo.clone())
            .execute(command(id, Some("prefer_not_to_say")))
            .await
            .expect("update succeeds");

        assert_eq!(user.gender, Some(Gender::PreferNotToSay));
        assert_eq!(repo.updates(), vec![Some(Gender::PreferNotToSay)]);
    }

    #[tokio::test]
    async fn none_clears_the_gender() {
        let id = Uuid::from_u128(1);
        let repo = Arc::new(FakeRepo::with_user(id));

        let user = UpdateGenderHandler::new(repo.clone())
            .execute(command(id, None))
            .await
            .expect("update succeeds");

        assert_eq!(user.gender, None);
        assert_eq!(repo.updates(), vec![None]);
    }

    #[tokio::test]
    async fn unknown_value_is_invalid_choice_and_not_stored() {
        let id = Uuid::from_u128(1);
        let repo = Arc::new(FakeRepo::with_user(id));

        let error = UpdateGenderHandler::new(repo.clone())
            .execute(command(id, Some("Female")))
            .await
            .expect_err("unknown value is rejected");

        let UsersError::Validation(fields) = error else {
            panic!("expected Validation, got {error:?}");
        };
        assert_eq!(fields["gender"], vec!["invalid_choice"]);
        assert!(repo.updates().is_empty());
    }

    #[tokio::test]
    async fn missing_account_is_not_found() {
        let repo = Arc::new(FakeRepo::empty());

        let error = UpdateGenderHandler::new(repo.clone())
            .execute(command(Uuid::from_u128(9), Some("male")))
            .await
            .expect_err("no such user");

        assert!(matches!(error, UsersError::NotFound), "{error:?}");
    }
}

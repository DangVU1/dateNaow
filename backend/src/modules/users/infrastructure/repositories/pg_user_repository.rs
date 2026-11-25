//! PostgreSQL implementation of `UserRepository`. All SQL for the `users`
//! table lives here (P11).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::modules::users::domain::{
    Email, Gender, NewUser, User, UserCredentials, UserRepoError, UserRepository,
};
use crate::shared::actor::Role;

/// Name of the unique index on `users.email` (migration 20261125000001).
const UQ_USERS_EMAIL: &str = "uq_users_email";

#[derive(Debug, Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Replaces a user's password hash on a connection the caller controls,
    /// typically inside the `auth` module's password-reset transaction (P11).
    /// Fails with `NotFound` when the user does not exist.
    pub async fn set_password_hash_in_tx(
        conn: &mut PgConnection,
        user_id: Uuid,
        hash: &str,
    ) -> Result<(), UserRepoError> {
        let result = sqlx::query!(
            "UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1",
            user_id,
            hash,
        )
        .execute(&mut *conn)
        .await
        .map_err(database)?;

        if result.rows_affected() == 0 {
            return Err(UserRepoError::NotFound);
        }
        Ok(())
    }
}

/// Database shape of a user row; mapped to the domain `User` explicitly.
struct UserRow {
    id: Uuid,
    email: String,
    role: String,
    gender: Option<String>,
    created_at: DateTime<Utc>,
}

impl TryFrom<UserRow> for User {
    type Error = UserRepoError;

    fn try_from(row: UserRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            email: Email::parse(&row.email).map_err(corrupt)?,
            role: row.role.parse::<Role>().map_err(corrupt)?,
            gender: row
                .gender
                .as_deref()
                .map(str::parse::<Gender>)
                .transpose()
                .map_err(corrupt)?,
            created_at: row.created_at,
        })
    }
}

struct CredentialsRow {
    id: Uuid,
    password_hash: String,
    role: String,
}

impl TryFrom<CredentialsRow> for UserCredentials {
    type Error = UserRepoError;

    fn try_from(row: CredentialsRow) -> Result<Self, Self::Error> {
        Ok(Self {
            user_id: row.id,
            password_hash: row.password_hash,
            role: row.role.parse::<Role>().map_err(corrupt)?,
        })
    }
}

impl UserRepository for PgUserRepository {
    async fn create(&self, new_user: NewUser) -> Result<User, UserRepoError> {
        let row = sqlx::query_as!(
            UserRow,
            r#"INSERT INTO users (email, password_hash)
               VALUES ($1, $2)
               RETURNING id, email, role, gender, created_at"#,
            new_user.email.as_str(),
            new_user.password_hash,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| {
            let is_email_taken = error
                .as_database_error()
                .is_some_and(|db| db.constraint() == Some(UQ_USERS_EMAIL));
            if is_email_taken {
                UserRepoError::EmailTaken
            } else {
                database(error)
            }
        })?;

        row.try_into()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, UserRepoError> {
        sqlx::query_as!(
            UserRow,
            "SELECT id, email, role, gender, created_at FROM users WHERE id = $1",
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .map(User::try_from)
        .transpose()
    }

    async fn find_credentials_by_email(
        &self,
        email: &Email,
    ) -> Result<Option<UserCredentials>, UserRepoError> {
        sqlx::query_as!(
            CredentialsRow,
            "SELECT id, password_hash, role FROM users WHERE email = $1",
            email.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .map(UserCredentials::try_from)
        .transpose()
    }

    async fn update_gender(&self, id: Uuid, gender: Option<Gender>) -> Result<User, UserRepoError> {
        sqlx::query_as!(
            UserRow,
            r#"UPDATE users SET gender = $2, updated_at = now()
               WHERE id = $1
               RETURNING id, email, role, gender, created_at"#,
            id,
            gender.map(Gender::as_str),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or(UserRepoError::NotFound)?
        .try_into()
    }

    async fn set_role(&self, email: &Email, role: Role) -> Result<bool, UserRepoError> {
        let result = sqlx::query!(
            "UPDATE users SET role = $2, updated_at = now() WHERE email = $1",
            email.as_str(),
            role.as_str(),
        )
        .execute(&self.pool)
        .await
        .map_err(database)?;

        Ok(result.rows_affected() > 0)
    }
}

fn database(error: sqlx::Error) -> UserRepoError {
    UserRepoError::Database(Box::new(error))
}

/// A stored value that the domain rejects, e.g. an unknown role. Only possible
/// if the table was changed outside the migrations; reported as a storage error.
fn corrupt(error: impl std::error::Error + Send + Sync + 'static) -> UserRepoError {
    UserRepoError::Database(Box::new(error))
}

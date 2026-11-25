//! Users domain: entities, value objects and the repository contract.
//! No dependency on Axum, SQLx or HTTP.

pub mod entities {
    pub mod user;
}
pub mod enums {
    pub mod gender;
}
pub mod repositories {
    pub mod user_repository;
}
pub mod value_objects {
    pub mod email;
}

pub use entities::user::{NewUser, User, UserCredentials};
pub use enums::gender::{Gender, ParseGenderError};
pub use repositories::user_repository::{UserRepoError, UserRepository};
pub use value_objects::email::{Email, EmailError};

//! Auth domain: the password policy. No dependency on Axum, SQLx or HTTP.

pub mod value_objects {
    pub mod password;
}

pub use value_objects::password::{PasswordError, PlainPassword};

//! Users application layer: the module's error type and its use cases.

pub mod error;
pub mod get_current_user;
pub mod update_gender;

pub use error::UsersError;

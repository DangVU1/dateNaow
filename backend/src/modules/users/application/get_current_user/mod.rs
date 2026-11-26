//! Use case: read the signed-in user's own account.

mod handler;
mod query;

pub use handler::GetCurrentUserHandler;
pub use query::GetCurrentUserQuery;

//! Sign-in with email and password, under the failed-attempt limits (P5).

mod command;
mod handler;

pub use command::LoginCommand;
pub use handler::LoginHandler;

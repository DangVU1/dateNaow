//! Setting a new password with an emailed reset token (P6).

mod command;
mod handler;

pub use command::ConfirmPasswordResetCommand;
pub use handler::ConfirmPasswordResetHandler;

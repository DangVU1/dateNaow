//! Use case: set or clear the signed-in user's own gender (spec §6.1).

mod command;
mod handler;

pub use command::UpdateGenderCommand;
pub use handler::UpdateGenderHandler;

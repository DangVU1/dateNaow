//! HTTP layer of the users module: `GET`/`PATCH /api/v1/me`.

pub mod dto;
pub mod handlers;
pub mod routes;

pub use routes::routes;

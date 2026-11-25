//! HTTP layer of the auth module: routes under `/api/v1/auth`.

pub mod cookies;
pub mod dto;
pub mod handlers;
pub mod routes;

pub use routes::routes;

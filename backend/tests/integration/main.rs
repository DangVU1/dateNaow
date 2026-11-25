//! Integration tests: one test binary for all API and repository tests.
//! Each `#[sqlx::test]` gets its own fresh PostgreSQL database.

#[path = "../common/mod.rs"]
mod common;

mod auth_refresh_logout;
mod errors;
mod health;
mod openapi;
mod users_repository;

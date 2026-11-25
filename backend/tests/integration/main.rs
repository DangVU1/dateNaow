//! Integration tests: one test binary for all API and repository tests.
//! Each `#[sqlx::test]` gets its own fresh PostgreSQL database.

#[path = "../common/mod.rs"]
mod common;

mod auth_refresh_logout;
mod auth_register_login;
mod auth_session_races;
mod errors;
mod health;
mod me_and_admin;
mod openapi;
mod password_reset;
mod users_repository;

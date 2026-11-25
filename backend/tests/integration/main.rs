//! Integration tests: one test binary for all API and repository tests.
//! Each `#[sqlx::test]` gets its own fresh PostgreSQL database.

#[path = "../common/mod.rs"]
mod common;

mod health;

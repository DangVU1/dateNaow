//! Grants the `admin` role to an existing account (P7):
//! `cargo run --bin make_admin -- <email>`.
//!
//! There is no admin sign-up page, so this command is the only way to create
//! an admin. Exits with 0 on success, 1 on any error (including no account).

use std::process::ExitCode;

use anyhow::Context;
use datenaow::config::Config;
use datenaow::infrastructure::db;
use datenaow::modules::users::domain::{Email, UserRepository};
use datenaow::modules::users::infrastructure::PgUserRepository;
use datenaow::shared::actor::Role;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(email) => {
            println!("Đã cấp quyền admin cho {email}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            // `{:#}` prints the context chain with `Display` only.
            eprintln!("Lỗi: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<Email> {
    let mut args = std::env::args().skip(1);
    let (Some(raw), None) = (args.next(), args.next()) else {
        anyhow::bail!("cách dùng: make_admin <email>");
    };
    let email =
        Email::parse(&raw).map_err(|error| anyhow::anyhow!("email không hợp lệ: {error}"))?;

    dotenvy::dotenv().ok();
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url)
        .await
        .context("không kết nối được PostgreSQL")?;

    let found = PgUserRepository::new(pool)
        .set_role(&email, Role::Admin)
        .await
        .context("không cập nhật được vai trò")?;
    if !found {
        anyhow::bail!("không có tài khoản nào với email {email}");
    }
    Ok(email)
}

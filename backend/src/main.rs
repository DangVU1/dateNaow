use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use datenaow::config::Config;
use datenaow::infrastructure::{db, telemetry};
use datenaow::shared::clock::SystemClock;
use datenaow::{AppState, build_router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    telemetry::init()?;

    let config = Config::from_env()?;
    tracing::debug!(?config, "configuration loaded");

    let db = db::connect(&config.database_url)
        .await
        .context("failed to connect to PostgreSQL")?;

    sqlx::migrate!()
        .run(&db)
        .await
        .context("failed to run database migrations")?;
    tracing::info!("database migrations applied");

    let bind_addr = config.bind_addr;
    let router = build_router(AppState::new(config, db, Arc::new(SystemClock)));

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind {bind_addr}"))?;
    tracing::info!(%bind_addr, "listening");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")?;

    tracing::info!("server stopped");
    Ok(())
}

/// Resolves on Ctrl+C, or on SIGTERM on Unix (sent by `docker stop`).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "failed to listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections");
}

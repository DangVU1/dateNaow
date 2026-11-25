//! Structured logging with `tracing`.

use anyhow::anyhow;
use tracing_subscriber::EnvFilter;

/// Filter used when `RUST_LOG` is not set.
const DEFAULT_FILTER: &str = "info,datenaow=debug";

/// Installs the global `tracing` subscriber. The filter comes from `RUST_LOG`.
pub fn init() -> anyhow::Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(|error| anyhow!("failed to install tracing subscriber: {error}"))
}

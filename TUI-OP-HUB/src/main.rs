//! Entry point for TUI-OP-HUB.
//!
//! US-NF-02: async runtime (tokio)
//! US-NF-04: structured logging (tracing)
//! US-NF-08: graceful error handling

use tui_op_hub::config::AppConfig;
use tui_op_hub::db;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // US-NF-04: initialise structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("TUI-OP-HUB starting up");

    // US-APP-06: load TOML configuration
    let config_path = AppConfig::default_path();
    let config = AppConfig::load(&config_path)?;

    tracing::info!(?config, "configuration loaded");

    // US-NF-06: initialise SQLite with WAL + foreign keys
    let pool = db::init_pool(&config.database).await?;

    // US-NF-07: run versioned schema migrations
    db::run_migrations(&pool).await?;

    tracing::info!("TUI-OP-HUB ready — press Ctrl+C to shut down");

    // Keep the process alive until interrupted
    tokio::signal::ctrl_c().await?;

    tracing::info!("shutdown signal received, goodbye");
    Ok(())
}

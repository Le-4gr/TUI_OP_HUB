//! Entry point for TUI-OP-HUB.
use std::sync::Arc;
use tui_op_hub::api;
use tui_op_hub::config::AppConfig;
use tui_op_hub::db;
use tui_op_hub::tui::modern_app::ModernApp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("TUI-OP-HUB starting up");

    let config_path = AppConfig::default_path();
    let config = AppConfig::load(&config_path)?;
    tracing::info!(?config, "configuration loaded");

    let pool = Arc::new(db::init_pool(&config.database).await?);
    db::run_migrations(&pool).await?;

    // Workflow scheduler daemon (US-WF-07): executes cron-scheduled workflows
    let scheduler = tui_op_hub::scheduler::WorkflowScheduler::new(pool.clone());
    tokio::spawn(async move {
        if let Err(e) = scheduler.start().await {
            tracing::error!(error = %e, "workflow scheduler failed to start");
        }
    });

    let pool_clone = pool.clone();
    let bind_addr = config.api.bind_addr.clone();
    tokio::spawn(async move {
        let router = api::router(pool_clone);
        let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
        tracing::info!(addr = %bind_addr, "API server listening");
        axum::serve(listener, router).await.unwrap();
    });

    if config.tui.enabled {
        // Use modern UI with login/signup
        let mut app = ModernApp::new(pool, config);
        app.run().await?;
    } else {
        tracing::info!("TUI disabled, running headless. Press Ctrl+C to shut down.");
        tokio::signal::ctrl_c().await?;
    }

    tracing::info!("shutdown signal received, goodbye");
    Ok(())
}

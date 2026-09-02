//! Entry point for TUI-OP-HUB.
use std::sync::Arc;
use tui_op_hub::api;
use tui_op_hub::config::AppConfig;
use tui_op_hub::db;
use tui_op_hub::tui::modern_app::ModernApp;

/// Minimal CLI flags (no clap dependency): `--help`, `--headless`,
/// `--print-unit`, `--install-service`, `--uninstall-service` (US-DEP-04).
fn handle_cli_flags() -> Option<bool> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return None;
    }
    match args[0].as_str() {
        "--help" | "-h" => {
            println!(
                "tui-op-hub \u{2014} terminal operations hub\n\n\
                 Usage: tui-op-hub [FLAG]\n\n\
                 Flags:\n\
                   (none)              Launch the interactive TUI\n\
                   --headless          Run without the TUI (scheduler + API only)\n\
                   --print-unit        Print the systemd user unit file\n\
                   --install-service   Install + enable the systemd user service\n\
                   --uninstall-service Disable + remove the systemd user service\n\
                   --help              Show this help"
            );
            Some(true) // handled: exit successfully
        }
        "--print-unit" => {
            println!("{}", tui_op_hub::service::systemd_user_unit());
            Some(true)
        }
        "--install-service" => match tui_op_hub::service::install_service() {
            Ok(path) => {
                println!("✓ Service installed and enabled: {}", path.display());
                println!(
                    "  Env file:        {}",
                    tui_op_hub::service::env_file_path().display()
                );
                println!(
                    "  Manage:          systemctl --user start|stop|status {0}",
                    tui_op_hub::service::SERVICE_NAME
                );
                println!(
                    "  Logs:            journalctl --user -u {0} -f",
                    tui_op_hub::service::SERVICE_NAME
                );
                println!("  The background service runs the scheduler + API headless.");
                println!("  Launch the TUI anytime with: tui-op-hub");
                Some(true)
            }
            Err(e) => {
                eprintln!("\u{2717} Install failed: {e}");
                Some(false)
            }
        },
        "--uninstall-service" => match tui_op_hub::service::uninstall_service() {
            Ok(()) => {
                println!("\u{2713} Service uninstalled");
                Some(true)
            }
            Err(e) => {
                eprintln!("\u{2717} Uninstall failed: {e}");
                Some(false)
            }
        },
        other => {
            eprintln!("Unknown flag: {other} (try --help)");
            Some(false)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(true) = handle_cli_flags() {
        return Ok(());
    }

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

    // Prepopulate the knowledge base with common commands + options and known
    // tools (idempotent; US-CMD-01, US-PROC).
    tui_op_hub::seed::seed_builtin_commands(&pool).await?;

    // Workflow scheduler daemon (US-WF-07): executes cron-scheduled workflows
    let scheduler = tui_op_hub::scheduler::WorkflowScheduler::new(pool.clone());
    tokio::spawn(async move {
        if let Err(e) = scheduler.start().await {
            tracing::error!(error = %e, "workflow scheduler failed to start");
        }
    });

    // When the systemd service is already running, its API owns the port and
    // both processes share the SQLite database (WAL) — the TUI simply skips
    // starting a second API server.
    {
        let pool_clone = pool.clone();
        let bind_addr = config.api.bind_addr.clone();
        tokio::spawn(async move {
            let router = api::router(pool_clone);
            match tokio::net::TcpListener::bind(&bind_addr).await {
                Ok(listener) => {
                    tracing::info!(addr = %bind_addr, "API server listening");
                    if let Err(e) = axum::serve(listener, router).await {
                        tracing::error!(error = %e, "API server failed");
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    tracing::warn!(
                        addr = %bind_addr,
                        "API port already in use; another instance (the service?) is serving it, skipping local API"
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to bind API address");
                }
            }
        });
    }

    let headless = std::env::args().nth(1).is_some_and(|a| a == "--headless");

    if config.tui.enabled && !headless {
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

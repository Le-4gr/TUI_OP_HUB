//! Entry point for TUI-OP-HUB.
use std::sync::Arc;
use tui_op_hub::api;
use tui_op_hub::config::AppConfig;
use tui_op_hub::db;
use tui_op_hub::tui::modern_app::ModernApp;

/// Minimal CLI flags (no clap dependency): `--help`, `--headless`,
/// `--print-unit`, `--install-service`, `--uninstall-service` (US-DEP-04).

/// D180: `--section <id>` / `--section=<id>` — open the TUI directly on a
/// section. Ids match the tab ids (dash|kb|proj|wf|sec|cfg|plug|pkg|set);
/// friendly aliases included. Used by the MyDesk shell's deep links.
fn parse_section_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for (i, a) in args.iter().enumerate() {
        let val = if a == "--section" {
            args.get(i + 1).cloned()
        } else {
            a.strip_prefix("--section=").map(|s| s.to_string())
        };
        if let Some(v) = val {
            return Some(match v.trim().to_lowercase().as_str() {
                "packages" | "pkg" => "pkg".to_string(),
                "secrets" | "keys" | "sec" => "sec".to_string(),
                "plugins" | "plug" => "plug".to_string(),
                "knowledge" | "kb" | "cmds" => "kb".to_string(),
                "workflows" | "wf" => "wf".to_string(),
                "configs" | "cfg" => "cfg".to_string(),
                "projects" | "proj" => "proj".to_string(),
                "settings" | "set" => "set".to_string(),
                "dashboard" | "dash" => "dash".to_string(),
                other => other.to_string(),
            });
        }
    }
    None
}

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
                   --section <id>      Open the TUI directly on a section\n\
                                       (dash|kb|proj|wf|sec|cfg|plug|pkg|set)\n\
                   --export-keybinds   Print all keybinds as JSON and exit\n\
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
        "--export-keybinds" => {
            // D195: machine-readable keybind dump for the docs CI (docs.yml
            // renders it as static HTML). No DB, no terminal app — safe to
            // run in CI right after the test suite.
            use tui_op_hub::tui::modern_ui::AppState;
            let global: Vec<(&str, &str)> = vec![
                ("Tab", "Switch tabs (configurable order, Settings last)"),
                (
                    "1-9,0",
                    "1 Dashboard · 2 Knowledge · 3 Projects · 4 Workflows · 5 Secrets · 6 Configs · 7 Plugins · 8 Packages · 0 Settings",
                ),
                ("f", "Knowledge tab: cycle the type filter (All/Commands/Apps/Scripts)"),
                ("`", "Open a new terminal window"),
                ("/", "Fuzzy search in the current list"),
                ("?", "Toggle the keybind helper"),
                ("q", "Quit"),
            ];
            let screens: Vec<(&str, AppState)> = vec![
                ("dashboard", AppState::Dashboard),
                ("knowledge", AppState::Knowledge),
                ("projects", AppState::Projects),
                ("workflows", AppState::Workflows),
                ("secrets", AppState::Secrets),
                ("configs", AppState::Configs),
                ("plugins", AppState::Plugins),
                ("packages", AppState::Packages),
                ("settings", AppState::Settings),
            ];
            let mut screens_json = serde_json::Map::new();
            for (name, state) in &screens {
                let hints: Vec<serde_json::Value> = ModernApp::keybind_hints_for(state)
                    .into_iter()
                    .map(|(k, l)| serde_json::json!([k, l]))
                    .collect();
                screens_json.insert((*name).to_string(), serde_json::Value::Array(hints));
            }
            let payload = serde_json::json!({
                "global": global
                    .iter()
                    .map(|(k, l)| serde_json::json!([k, l]))
                    .collect::<Vec<_>>(),
                "screens": serde_json::Value::Object(screens_json),
            });
            println!("{payload}");
            Some(true)
        }
        "mcp" => {
            // US-MCP-01: serve the hub as an MCP stdio server for AI agents.
            let config_path = tui_op_hub::config::AppConfig::default_path();
            match tui_op_hub::config::AppConfig::load(&config_path) {
                Ok(config) => {
                    // main is #[tokio::main] — reuse the ambient runtime (nested
                    // Runtime::new().block_on panics).
                    let handle = tokio::runtime::Handle::current();
                    tokio::task::block_in_place(|| handle.block_on(async move {
                        match tui_op_hub::db::init_pool(&config.database).await {
                            Ok(pool) => {
                                let pool = std::sync::Arc::new(pool);
                                if let Err(e) = tui_op_hub::db::run_migrations(&pool).await {
                                    eprintln!("tui-op-hub mcp: migrations failed: {e}");
                                    return Some(false);
                                }
                                match tui_op_hub::mcp::run(pool).await {
                                    Ok(()) => Some(true),
                                    Err(e) => {
                                        eprintln!("tui-op-hub mcp: {e}");
                                        Some(false)
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("tui-op-hub mcp: db pool failed: {e}");
                                Some(false)
                            }
                        }
                    }))
                }
                Err(e) => {
                    eprintln!("tui-op-hub mcp: config load failed: {e}");
                    Some(false)
                }
            }
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
        // D105: recognized — startup continues; headless is resolved below
        // from the full arg list (this flag used to fall into "Unknown")
        "--headless" => None,
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
        // D264: tracing must NEVER write to stdout — the TUI paints on the
        // alternate screen and INFO lines land in the visible pane as
        // "debug codes" (the user-visible Secrets-tab symptom).
        .with_writer(std::io::stderr)
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

    // D105: scan .desktop files → `app` entities (every installed application
    // shows up in the Knowledge launcher; GUI apps launch detached). Cheap
    // upsert + prune — safe on every start.
    tui_op_hub::seed::seed_desktop_apps(&pool).await?;

    // D161 (US-MDSK-03): auto-import knowledge bundles from the seed dir
    // (default ~/.config/mydesk/bundles — MyDesk ships its actions there).
    // Missing/empty dir = no-op, so standalone installs are unaffected.
    let bundles_dir = {
        let raw = config.seed.bundles_dir.clone();
        if raw.starts_with('~') {
            if let Ok(home) = std::env::var("HOME") {
                home + &raw[1..]
            } else {
                raw
            }
        } else {
            raw
        }
    };
    let seeded = tui_op_hub::seed::seed_bundles_from_dir(&pool, std::path::Path::new(&bundles_dir)).await?;
    if seeded > 0 {
        tracing::info!(seeded, "knowledge bundles auto-seeded");
    }

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

    // D105: any position; used to be nth(1) only, so a lone `--headless`
    // flag never actually enabled headless mode
    let headless = std::env::args().skip(1).any(|a| a == "--headless");

    if config.tui.enabled && !headless {
        // Use modern UI with login/signup
        let mut app = ModernApp::new(pool, config);
        // D180: `--section <id>` deep link — the MyDesk shell's package/distro
        // cards jump straight into the hub's Packages section instead of
        // reimplementing the install flow in QML.
        if let Some(sec) = parse_section_arg() {
            app.set_initial_section(&sec);
        }
        app.run().await?;
    } else {
        // Headless service: load plugins. Admin-trusted plugins
        // (trusted_headless) auto-approve here — the background service has
        // no interactive consent flow (US-PLG leftover).
        let mut pm = tui_op_hub::plugin::PluginManager::new(
            pool.clone(),
            tui_op_hub::plugin::PluginManager::default_dir(),
        );
        pm.set_headless(true);
        pm.load_all_plugins().await?;
        tracing::info!("TUI disabled, running headless. Press Ctrl+C to shut down.");

        // D121: background session — link the OS login to the hub account and
        // offer stored SSH keys to the user's ssh-agent (Hyprland session
        // starts the service, so SSH_AUTH_SOCK is the session agent).
        // D145: passphrase-locked keys with a STORED passphrase (typed at
        // import) unlock non-interactively here; locked keys without one are
        // skipped (headless has no prompt — the TUI unlocks those with `S`).
        {
            let uid = match tui_op_hub::auth::first_user_id(&pool).await {
                Ok(Some(id)) => id,
                _ => config.current_user.clone(),
            };
            let out =
                tui_op_hub::secrets::ssh_agent::offer_user_keys(&pool, &uid).await;
            tracing::info!(
                offered = out.offered,
                unlocked = out.unlocked,
                skipped = out.skipped,
                "headless ssh-agent offer complete"
            );
        }
        tokio::signal::ctrl_c().await?;
    }

    tracing::info!("shutdown signal received, goodbye");
    Ok(())
}

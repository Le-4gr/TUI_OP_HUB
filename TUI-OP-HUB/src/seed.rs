//! Idempotent prepopulation of the knowledge base (US-CMD-01, US-SRCH, US-PROC).
//!
//! Seeds **command families** (`cmd` entities) with their **options** as child
//! entities (`opt` type, linked via `entities.parent_id`) — each option carries
//! its own description. Also seeds known desktop/TUI tools as `app` entities
//! (file browsers like yazi/ranger/lf, editors, fetch tools, process viewers)
//! so the hub is useful out of the box. Covers both systemd (`systemctl`,
//! `journalctl`) and OpenRC (`rc-service`, `rc-update`) init systems.
//!
//! Seeding is idempotent: families/options/tools are checked by name before
//! insert, and a `seed_meta` marker short-circuits repeated runs.

use crate::error::AppResult;

use sqlx::SqlitePool;

/// Seed data version — bump to re-run seeds with new content.
pub const SEED_VERSION: &str = "commands_v1";

/// Detect the running init system (systemd / OpenRC / unknown). Shown in the
/// fetch panel and used to pick sensible service commands.
pub fn detect_init_system() -> &'static str {
    if std::path::Path::new("/run/systemd/system").exists() || crate::keygen::which("systemctl") {
        "systemd"
    } else if crate::keygen::which("rc-service") || std::path::Path::new("/sbin/openrc").exists() {
        "openrc"
    } else {
        "unknown"
    }
}

/// Seed command families, options and known tools into the database.
/// Idempotent — safe to call on every startup.
pub async fn seed_builtin_commands(pool: &SqlitePool) -> AppResult<()> {
    // Fast path: already seeded with this version
    let seeded: Option<(String,)> =
        sqlx::query_as("SELECT value FROM seed_meta WHERE key = 'commands'")
            .fetch_optional(pool)
            .await?;
    if seeded.map(|(v,)| v == SEED_VERSION).unwrap_or(false) {
        return Ok(());
    }

    for family in seed_commands() {
        // Family entity (skip when it already exists — user may have edited it)
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM entities WHERE name = ? AND type_id = 'cmd' AND parent_id IS NULL",
        )
        .bind(family.name)
        .fetch_optional(pool)
        .await?;
        let family_id = match existing {
            Some((id,)) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO entities (id, name, description, content, type_id) VALUES (?, ?, ?, ?, 'cmd')",
                )
                .bind(&id)
                .bind(family.name)
                .bind(family.description)
                .bind(family.name) // content defaults to the command itself
                .execute(pool)
                .await?;
                id
            }
        };

        // Options as child entities (skip existing ones)
        for option in family.options {
            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM entities WHERE name = ? AND type_id = 'opt' AND parent_id = ?",
            )
            .bind(option.flag)
            .bind(&family_id)
            .fetch_optional(pool)
            .await?;
            if exists.is_none() {
                sqlx::query(
                    "INSERT INTO entities (id, name, description, content, type_id, parent_id) VALUES (?, ?, ?, ?, 'opt', ?)",
                )
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(option.flag)
                .bind(option.description)
                .bind(format!("{} {}", family.name, option.flag))
                .bind(&family_id)
                .execute(pool)
                .await?;
            }
        }
    }

    // Known tools as `app` entities, tagged for filtering
    for (name, description, tag) in SEED_TOOLS {
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM entities WHERE name = ? AND type_id = 'app'")
                .bind(name)
                .fetch_optional(pool)
                .await?;
        if exists.is_none() {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO entities (id, name, description, content, type_id) VALUES (?, ?, ?, ?, 'app')",
            )
            .bind(&id)
            .bind(name)
            .bind(description)
            .bind(name) // launching an app runs the binary itself
            .execute(pool)
            .await?;
            sqlx::query("INSERT OR IGNORE INTO tags (id, name) VALUES (?, ?)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(tag)
                .execute(pool)
                .await?;
            sqlx::query(
                "INSERT OR IGNORE INTO entity_tags (entity_id, tag_id) SELECT ?, id FROM tags WHERE name = ?",
            )
            .bind(&id)
            .bind(tag)
            .execute(pool)
            .await?;
        }
    }

    // Mark the seed version
    sqlx::query(
        "INSERT INTO seed_meta (key, value) VALUES ('commands', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(SEED_VERSION)
    .execute(pool)
    .await?;

    tracing::info!("knowledge base seeded (version {})", SEED_VERSION);
    Ok(())
}

// ---------------------------------------------------------------------------
// Seed data (was src/seed_data.rs): command families, options, known tools.
// ---------------------------------------------------------------------------

/// One command option/flag with its description.
#[derive(Clone, Copy)]
pub struct SeedOption {
    pub flag: &'static str,
    pub description: &'static str,
}

/// A command family: the command itself plus its structured options.
#[derive(Clone, Copy)]
pub struct SeedCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub options: &'static [SeedOption],
}

/// Known tools worth having as `app` entities (desktop use, terminal only).
pub const SEED_TOOLS: &[(&str, &str, &str)] = &[
    // name, description, tag
    ("yazi", "Blazing fast terminal file browser", "file-manager"),
    (
        "ranger",
        "Console file manager with VI key bindings",
        "file-manager",
    ),
    (
        "lf",
        "Terminal file manager (Go, ranger-like)",
        "file-manager",
    ),
    (
        "btop",
        "Resource monitor with CPU/memory/network graphs",
        "process-viewer",
    ),
    ("htop", "Interactive process viewer", "process-viewer"),
    (
        "fastfetch",
        "System information tool (neofetch successor)",
        "fetch",
    ),
    (
        "neofetch",
        "System information tool with ASCII logo",
        "fetch",
    ),
    (
        "nvim",
        "Neovim — hyperextensible Vim-based text editor",
        "editor",
    ),
    ("vim", "Vi IMproved — terminal text editor", "editor"),
    ("nano", "Small, friendly terminal text editor", "editor"),
    ("lazygit", "Simple terminal UI for git commands", "git"),
    (
        "lazydocker",
        "The lazier way to manage everything docker",
        "docker",
    ),
    (
        "k9s",
        "Terminal UI to interact with your Kubernetes clusters",
        "kubernetes",
    ),
    ("ctop", "Top-like interface for container metrics", "docker"),
    ("fzf", "Command-line fuzzy finder", "fuzzy-finder"),
    (
        "kubectl",
        "Command-line tool for controlling Kubernetes clusters",
        "kubernetes",
    ),
    (
        "docker compose",
        "Define and run multi-container applications",
        "docker",
    ),
];

/// Command families seeded into the knowledge base (part 1).
pub const SEED_COMMANDS_A: &[SeedCommand] = &[
    SeedCommand {
        name: "git",
        description: "Distributed version control system",
        options: &[
            SeedOption {
                flag: "status",
                description: "Show the working tree status",
            },
            SeedOption {
                flag: "add",
                description: "Add file contents to the staging area",
            },
            SeedOption {
                flag: "commit",
                description: "Record changes to the repository",
            },
            SeedOption {
                flag: "push",
                description: "Update remote refs with local commits",
            },
            SeedOption {
                flag: "pull",
                description: "Fetch from and integrate with a remote",
            },
            SeedOption {
                flag: "log",
                description: "Show commit history",
            },
            SeedOption {
                flag: "diff",
                description: "Show changes between commits/working tree",
            },
            SeedOption {
                flag: "branch",
                description: "List, create or delete branches",
            },
        ],
    },
    SeedCommand {
        name: "docker",
        description: "Container engine — build, ship and run containers",
        options: &[
            SeedOption {
                flag: "ps",
                description: "List containers",
            },
            SeedOption {
                flag: "images",
                description: "List images",
            },
            SeedOption {
                flag: "run",
                description: "Run a command in a new container",
            },
            SeedOption {
                flag: "build",
                description: "Build an image from a Dockerfile",
            },
            SeedOption {
                flag: "pull",
                description: "Pull an image from a registry",
            },
            SeedOption {
                flag: "logs",
                description: "Fetch container logs",
            },
            SeedOption {
                flag: "exec",
                description: "Run a command inside a running container",
            },
        ],
    },
    SeedCommand {
        name: "systemctl",
        description: "Control the systemd system and service manager",
        options: &[
            SeedOption {
                flag: "status",
                description: "Show unit status",
            },
            SeedOption {
                flag: "start",
                description: "Start a unit",
            },
            SeedOption {
                flag: "stop",
                description: "Stop a unit",
            },
            SeedOption {
                flag: "restart",
                description: "Restart a unit",
            },
            SeedOption {
                flag: "enable",
                description: "Enable a unit to start at boot",
            },
            SeedOption {
                flag: "disable",
                description: "Disable a unit from starting at boot",
            },
            SeedOption {
                flag: "list-units",
                description: "List loaded units",
            },
        ],
    },
    SeedCommand {
        name: "rc-service",
        description: "OpenRC service manager — start/stop services",
        options: &[
            SeedOption {
                flag: "status",
                description: "Show service status",
            },
            SeedOption {
                flag: "start",
                description: "Start a service",
            },
            SeedOption {
                flag: "stop",
                description: "Stop a service",
            },
            SeedOption {
                flag: "restart",
                description: "Restart a service",
            },
        ],
    },
    SeedCommand {
        name: "rc-update",
        description: "OpenRC — manage services that run at boot",
        options: &[
            SeedOption {
                flag: "add",
                description: "Add a service to a runlevel",
            },
            SeedOption {
                flag: "delete",
                description: "Remove a service from a runlevel",
            },
            SeedOption {
                flag: "show",
                description: "Show services in runlevels",
            },
        ],
    },
    SeedCommand {
        name: "journalctl",
        description: "Query the systemd journal",
        options: &[
            SeedOption {
                flag: "-f",
                description: "Follow new journal messages",
            },
            SeedOption {
                flag: "-u",
                description: "Show messages of a unit",
            },
            SeedOption {
                flag: "--since",
                description: "Show entries since a date/time",
            },
            SeedOption {
                flag: "-b",
                description: "Show messages from a boot",
            },
        ],
    },
    SeedCommand {
        name: "ssh",
        description: "OpenSSH remote login client",
        options: &[
            SeedOption {
                flag: "-p",
                description: "Connect to a specific port",
            },
            SeedOption {
                flag: "-i",
                description: "Select the identity (private key) file",
            },
            SeedOption {
                flag: "-v",
                description: "Verbose mode (debug output)",
            },
            SeedOption {
                flag: "-L",
                description: "Local port forwarding",
            },
        ],
    },
];

/// Command families seeded into the knowledge base (part 2).
pub const SEED_COMMANDS_B: &[SeedCommand] = &[
    SeedCommand {
        name: "curl",
        description: "Transfer data to/from URLs",
        options: &[
            SeedOption {
                flag: "-X",
                description: "HTTP request method",
            },
            SeedOption {
                flag: "-H",
                description: "Add a request header",
            },
            SeedOption {
                flag: "-d",
                description: "Send request body data",
            },
            SeedOption {
                flag: "-o",
                description: "Write output to a file",
            },
            SeedOption {
                flag: "-L",
                description: "Follow redirects",
            },
            SeedOption {
                flag: "-I",
                description: "Fetch headers only",
            },
        ],
    },
    SeedCommand {
        name: "grep",
        description: "Print lines matching a pattern",
        options: &[
            SeedOption {
                flag: "-r",
                description: "Search directories recursively",
            },
            SeedOption {
                flag: "-i",
                description: "Ignore case",
            },
            SeedOption {
                flag: "-n",
                description: "Show line numbers",
            },
            SeedOption {
                flag: "-v",
                description: "Invert match",
            },
            SeedOption {
                flag: "-E",
                description: "Extended regular expressions",
            },
        ],
    },
    SeedCommand {
        name: "find",
        description: "Search for files in a directory hierarchy",
        options: &[
            SeedOption {
                flag: "-name",
                description: "Match file name (glob)",
            },
            SeedOption {
                flag: "-type",
                description: "Match file type (f=file, d=dir)",
            },
            SeedOption {
                flag: "-size",
                description: "Match file size",
            },
            SeedOption {
                flag: "-exec",
                description: "Run a command on each match",
            },
        ],
    },
    SeedCommand {
        name: "tar",
        description: "Archive files (tarball utility)",
        options: &[
            SeedOption {
                flag: "-c",
                description: "Create an archive",
            },
            SeedOption {
                flag: "-x",
                description: "Extract an archive",
            },
            SeedOption {
                flag: "-z",
                description: "Filter through gzip",
            },
            SeedOption {
                flag: "-f",
                description: "Use the given archive file",
            },
            SeedOption {
                flag: "-v",
                description: "Verbose output",
            },
        ],
    },
    SeedCommand {
        name: "python3",
        description: "Python interpreter",
        options: &[
            SeedOption {
                flag: "-m",
                description: "Run a module as a script",
            },
            SeedOption {
                flag: "-V",
                description: "Print the version",
            },
            SeedOption {
                flag: "-c",
                description: "Execute the given program text",
            },
            SeedOption {
                flag: "-i",
                description: "Interactive REPL after running script",
            },
        ],
    },
    SeedCommand {
        name: "cargo",
        description: "Rust package manager and build tool",
        options: &[
            SeedOption {
                flag: "build",
                description: "Compile the current package",
            },
            SeedOption {
                flag: "run",
                description: "Build and run the binary target",
            },
            SeedOption {
                flag: "test",
                description: "Run tests",
            },
            SeedOption {
                flag: "check",
                description: "Fast type-check without codegen",
            },
            SeedOption {
                flag: "add",
                description: "Add a dependency",
            },
            SeedOption {
                flag: "fmt",
                description: "Format code with rustfmt",
            },
        ],
    },
];

/// All command families (parts combined).
pub fn seed_commands() -> impl Iterator<Item = &'static SeedCommand> {
    SEED_COMMANDS_A.iter().chain(SEED_COMMANDS_B.iter())
}

// ---------------------------------------------------------------------------
// Desktop application scan (D105)
// ---------------------------------------------------------------------------

/// One parsed `.desktop` application.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopApp {
    /// `.desktop` file id (e.g. `firefox.desktop`) — the upsert key, stored in
    /// `metadata_json` so entries can be updated/pruned across rescans.
    pub desktop_id: String,
    /// `Name=` — shown in the launcher.
    pub name: String,
    /// `Exec=` with field codes stripped — the launch command.
    pub exec: String,
    /// `Icon=` — icon theme name, exposed for frontends.
    pub icon: String,
    /// `Terminal=true` — TUI/CLI apps; false = GUI (launch detached).
    pub terminal: bool,
}

/// XDG data dirs in precedence order (`XDG_DATA_HOME`, `$HOME/.local/share`,
/// `XDG_DATA_DIRS` or `/usr/local/share:/usr/share`).
fn desktop_data_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        if !x.is_empty() {
            dirs.push(std::path::PathBuf::from(x));
        }
    }
    if let Ok(h) = std::env::var("HOME") {
        dirs.push(std::path::PathBuf::from(h).join(".local/share"));
    }
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in data_dirs.split(':').filter(|s| !s.is_empty()) {
        dirs.push(std::path::PathBuf::from(d));
    }
    dirs
}

/// Parse a `.desktop` file — minimal `[Desktop Entry]` INI (no deps).
/// Skips `NoDisplay`/`Hidden` entries (menu-invisible → not launchable).
fn parse_desktop_file(path: &std::path::Path, desktop_id: &str) -> Option<DesktopApp> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut name: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut icon = String::new();
    let mut nodisplay = false;
    let mut hidden = false;
    let mut terminal = false;
    let mut in_entry = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_entry = t == "[Desktop Entry]";
            continue;
        }
        if !in_entry || t.starts_with('#') {
            continue;
        }
        let Some((key, value)) = t.split_once('=') else {
            continue;
        };
        match key {
            "Name" => name = Some(value.to_string()),
            "Exec" => exec = Some(value.to_string()),
            "Icon" => icon = value.to_string(),
            "NoDisplay" => nodisplay = value.trim().eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            "Terminal" => terminal = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    if nodisplay || hidden {
        return None;
    }
    let exec = clean_exec(exec.as_deref().unwrap_or(""))?;
    if exec.is_empty() {
        return None;
    }
    let name = name.unwrap_or_else(|| desktop_id.trim_end_matches(".desktop").to_string());
    Some(DesktopApp {
        desktop_id: desktop_id.to_string(),
        name,
        exec,
        icon,
        terminal,
    })
}

/// Clean an `Exec=` line per the desktop-entry spec: strip `%` field codes
/// (`%f %F %u %U …`, `%%` → literal `%`) and unescape `\s` (space),
/// `\n`/`\t` (whitespace), `\\` (backslash).
fn clean_exec(exec: &str) -> Option<String> {
    if exec.trim().is_empty() {
        return None;
    }
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '%' => match chars.next() {
                Some('%') => out.push('%'),
                Some(_) => {} // field code — dropped (no file/URL args)
                None => {}
            },
            '\\' => match chars.next() {
                Some('s') | Some(' ') => out.push(' '), // \s per spec; "\ " common in the wild
                Some('n') | Some('t') => out.push(' '),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    let trimmed = out.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Extract the `desktop_id` marker from an app entity's metadata (None when
/// the entity is not a desktop-scan entry — e.g. seeded TUI tools).
fn desktop_id_of_metadata(metadata_json: &Option<String>) -> Option<String> {
    let raw = metadata_json.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value
        .get("desktop_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Scan `.desktop` files and upsert them as `app` entities so the Knowledge
/// launcher offers every installed application. Upsert key = the `.desktop`
/// id in `metadata_json` (entities without the marker are never touched);
/// entries for uninstalled apps are pruned. Cheap — safe on every startup.
pub async fn seed_desktop_apps(pool: &SqlitePool) -> AppResult<()> {
    let mut apps: Vec<DesktopApp> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for dir in desktop_data_dirs() {
        let Ok(entries) = std::fs::read_dir(dir.join("applications")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Some(id) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                continue; // earlier XDG dir already provided this id
            }
            if let Some(app) = parse_desktop_file(&path, id) {
                apps.push(app);
            }
        }
    }

    for app in &apps {
        let gui = !app.terminal;
        let metadata = serde_json::json!({
            "desktop_id": app.desktop_id,
            "icon": app.icon,
            "gui": gui,
        })
        .to_string();
        let existing: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT id, content FROM entities WHERE type_id = 'app' AND metadata_json LIKE ?",
        )
        .bind(format!("%\"desktop_id\":\"{}\"%", app.desktop_id))
        .fetch_optional(pool)
        .await?;
        match existing {
            Some((id, _)) => {
                sqlx::query(
                    "UPDATE entities SET name = ?, description = ?, content = ?, metadata_json = ? WHERE id = ?",
                )
                .bind(&app.name)
                .bind(&app.name)
                .bind(&app.exec)
                .bind(&metadata)
                .bind(&id)
                .execute(pool)
                .await?;
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO entities (id, name, description, content, type_id, metadata_json) VALUES (?, ?, ?, ?, 'app', ?)",
                )
                .bind(&id)
                .bind(&app.name)
                .bind(&app.name)
                .bind(&app.exec)
                .bind(&metadata)
                .execute(pool)
                .await?;
            }
        }
    }

    // Prune entities whose .desktop file disappeared (app uninstalled).
    let tracked: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, metadata_json FROM entities WHERE type_id = 'app' AND metadata_json LIKE '%\"desktop_id\":%'",
    )
    .fetch_all(pool)
    .await?;
    let current: std::collections::HashSet<String> =
        apps.iter().map(|a| a.desktop_id.clone()).collect();
    for (id, metadata) in tracked {
        let known = desktop_id_of_metadata(&metadata)
            .map(|d| current.contains(&d))
            .unwrap_or(true); // unparseable → keep (never delete blindly)
        if !known {
            sqlx::query("DELETE FROM entities WHERE id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod desktop_tests {
    use super::*;

    #[test]
    fn exec_field_codes_are_stripped() {
        assert_eq!(clean_exec("firefox %u").as_deref(), Some("firefox"));
        assert_eq!(clean_exec("code %F").as_deref(), Some("code"));
        assert_eq!(clean_exec("app %%").as_deref(), Some("app %"));
        assert_eq!(clean_exec("app %f %F").as_deref(), Some("app"));
    }

    #[test]
    fn exec_escapes_are_unescaped() {
        assert_eq!(
            clean_exec(r"my\ app --flag").as_deref(),
            Some("my app --flag")
        );
        assert_eq!(clean_exec(r"my\sapp").as_deref(), Some("my app"));
        assert_eq!(clean_exec(r"a\\b").as_deref(), Some(r"a\b"));
    }

    #[test]
    fn empty_exec_is_rejected() {
        assert_eq!(clean_exec(""), None);
        assert_eq!(clean_exec("   %f"), None);
    }

    #[test]
    fn desktop_id_roundtrips_through_metadata() {
        let meta = serde_json::json!({"desktop_id": "firefox.desktop", "gui": true}).to_string();
        assert_eq!(
            desktop_id_of_metadata(&Some(meta)).as_deref(),
            Some("firefox.desktop")
        );
        assert_eq!(desktop_id_of_metadata(&None), None);
        assert_eq!(desktop_id_of_metadata(&Some("not json".into())), None);
    }
}

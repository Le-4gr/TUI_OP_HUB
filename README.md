# TUI-OP-HUB

<p align="center">
<b>A terminal-based operations hub — commands, workflows, secrets, configs,<br>
SSH hosts and plugins in one keyboard-driven, local-first app.</b>
</p>

---

TUI-OP-HUB centralizes a power user's daily tooling into one Rust binary with a
ratatui TUI and a local REST API. Everything is stored in a local SQLite
database — no cloud, no daemons required (the optional background service is
just your own systemd user unit).

```text
┌────────────────────────────────────────────────────────────────────┐
│  1 Dashboard   2 Knowledge   3 Projects   4 Workflows   5 Secrets  │
│  6 Configs     7 Plugins     0 Settings                            │
└────────────────────────────────────────────────────────────────────┘
   ▲ after login you land on `2 Knowledge` — the launcher: search is
     auto-focused, type to filter across all entities, Enter runs.
```

## Highlights

| Area | What you get |
|:---|:---|
| **Knowledge base** | Commands, scripts and apps in one searchable (FTS5) tab with a type filter, runnable **command chains** (`cat /proc/meminfo \| grep Dirty`), per-command **options** with big descriptions, a read-only detail view, copy-to-clipboard and one-key run |
| **Run modes** | Every command can run in a **new terminal window** (interactive tools like nvim/htop work), **foreground** (captured output), **background** (tracked, stoppable) or via **nohup** (survives the hub, logged to `/tmp/tui-op-hub-logs/`) — plus a **jobs panel** (`j`) to stop/force-kill anything running |
| **Workflows** | Visual builder (compose from saved commands + **logic nodes** AND/OR/NOT/XOR/compare/if-else with `run_when` gates), Lua 5.4 engine with host functions, cron scheduling, run history, live cancel |
| **Secrets** | XChaCha20Poly1305 AEAD, per-user keys, groups, password-manager fields, optional passphrase layer, ssh-agent integration, SSH/GPG keygen |
| **Configs** | Register config files (built-in file browser), deploy to multiple targets as symlink/hard-link/copy, drift detection + fix, git-versioned store |
| **Projects** | Workspaces with kind scaffolding (Python/Rust/Node/…), plugin **templates** with preview + customize, editor/shell launch, optional folder deletion on delete, merge-into-existing-folder |
| **SSH hosts** | CRUD, tags + live filter, **connection test** (non-interactive probe), quick-connect in a new terminal |
| **Plugins** | Lua mods with capability approval, event hooks, **UI actions** (buttons on the Projects tab), project scaffold templates, headless trust workflow |
| **Theming** | 8 presets (dark, light, nord, dracula, gruvbox, solarized, **catppuccin**, catppuccin-latte), custom palettes, live color editor |

## Quick start

### Install

```bash
./install.sh        # build + install binary + enable background service
```

or build manually:

```bash
cd TUI-OP-HUB
cargo build --release
```

### Run

```bash
# Secrets need a master key (generate once per machine):
export TUI_OP_HUB_SECRETS_KEY="$(openssl rand -base64 32)"
export TUI_OP_HUB_USER="default"          # optional

./target/release/tui-op-hub               # TUI
./target/release/tui-op-hub --headless    # scheduler + API only
./target/release/tui-op-hub --install-service
```

First launch shows a signup screen — the first user becomes the admin.

## Keybindings (a taste)

| Key | Action |
|:---|:---|
| `1-9`/`0` | Switch tabs from any screen (0 = Settings) |
| `Enter` | Read-only detail view (info + options) / project detail |
| `r` | Run — choose terminal / foreground / background / nohup |
| `j` | Jobs panel — stop or force-kill running things |
| `v` | Visual workflow builder (logic nodes, `run_when` gates) |
| `i` | Options of a command family / chain segments |
| `/` | Search with a visible search bar |
| `?` | Full keybind helper |

The complete table lives in the [docs](docs/INDEX.md).

## Configuration

Hyprland-style config at `~/.config/tui-op-hub/config.conf` — themes
(including custom palettes), keybindings, page size, API bind address,
database path. Everything is also editable live in Settings (`0`) and
persisted with `Ctrl+S`.

```ini
theme {
    name = catppuccin
}

keybindings {
    run = r
    copy = c
}

api {
    bind_addr = 127.0.0.1:3001
}
```

## REST API

The TUI and the background service expose a local API on `127.0.0.1:3001`
(configurable) — entities, projects, workflows, schedules, secrets
(user-scoped), export/import, health. Run something with
`curl -X POST localhost:3001/entities/<id>/run` or schedule a workflow with
`curl -X POST localhost:3001/workflows/<id>/schedule -d '{"cron_expr": "30 2 * * *"}'`.
The full endpoint list is in the docs.

## Security model

- Secrets are **never stored in plaintext**: XChaCha20Poly1305 AEAD with a
  per-user key derived at signup (`TUI_OP_HUB_SECRETS_KEY` wraps user keys)
- Passphrase-protected secrets add a second Argon2+XChaCha layer and re-ask
  on every use
- The systemd unit never embeds the secrets key
- Plugins run in a fresh Lua sandbox per invocation; capabilities require
  explicit user approval, and headless runs only auto-load plugins an admin
  explicitly marked as trusted

## Testing & quality

```bash
cd TUI-OP-HUB
cargo test            # 269 tests: unit + BDD scenarios
cargo clippy --all-targets
```

## Documentation

| Document | What it covers |
|:---|:---|
| [docs/INDEX.md](docs/INDEX.md) | **Start here** — the documentation index |
| [docs/GUIDE.md](docs/GUIDE.md) | **User guide** — install, service/TUI model, every feature and how to use it |
| [docs/IMPORT_EXPORT.md](docs/IMPORT_EXPORT.md) | Bundle schema + LLM prompt for generating entries (cmds, scripts, apps, chains) |
| [docs/PLUGINS.md](docs/PLUGINS.md) | Plugin API: actions, templates, headless trust |
| [docs/IMPORT_EXPORT.md](docs/IMPORT_EXPORT.md) | Bundle schema + LLM prompt for generating entries |
| [docs/reference/ARCHITECTURE.md](docs/reference/ARCHITECTURE.md) | Module responsibilities, data flow |
| [docs/reference/USER_STORIES.md](docs/reference/USER_STORIES.md) | The full spec (`US-XXX-NN`) |
| [docs/development/AGENTS.md](docs/development/AGENTS.md) | Guide for AI coding agents |
| [docs/development/WORK.md](docs/development/WORK.md) | Live work log / changelog |
| [docs/business/ROADMAP.md](docs/business/ROADMAP.md) | Phase plan and roadmap |
| [docs/design/DB_LOGICAL.drawio](docs/design/DB_LOGICAL.drawio) | Database diagram |

## Project layout

```text
TUI-OP-HUB/
├── README.md                  # This file
├── docs/GUIDE.md               # User guide (install + all features)
├── install.sh                  # Release installer
├── docs/                      # All documentation (see docs/INDEX.md)
│   ├── INDEX.md               # Documentation index — start here
│   ├── GUIDE.md                # User guide (install + features)
│   ├── IMPORT_EXPORT.md       # Import/export guide
│   ├── PLUGINS.md             # Plugin/mod API
│   ├── reference/             # ARCHITECTURE, USER_STORIES, TUI guide, stack rationale
│   ├── development/           # AGENTS.md (agent guide), WORK.md (live work log)
│   ├── business/              # bp.md (plan), ROADMAP, VISION, GOVERNANCE
│   └── design/                # DB diagram (draw.io) + Figma sketches
└── TUI-OP-HUB/                # Rust application (cargo crate root)
    ├── Cargo.toml
    ├── tests/bdd_scenarios.rs # BDD integration tests
    └── src/
        ├── main.rs            # Entry point (CLI flags, headless wiring)
        ├── lib.rs             # Module declarations
        ├── api.rs             # REST API (axum)
        ├── auth.rs            # Argon2 auth, per-user keys
        ├── config.rs          # Configuration (themes, keybindings)
        ├── config_manager.rs  # Managed-config store/registry
        ├── db/                # SQLite pool + migrations (0001..0010)
        ├── models.rs          # Data models
        ├── plugin.rs          # Plugin system (sandbox, actions, templates)
        ├── project_workspace.rs # Workspace creation + editors
        ├── repository.rs      # Data access layer (all SQL)
        ├── scheduler.rs       # Cron scheduler daemon
        ├── secrets/           # Encryption + ssh-agent
        ├── share/             # Import/export (+ crypto)
        ├── workflow.rs        # Lua engine, run plans, chains
        └── tui/               # modern_app, modern_ui, list_state, helpers
```

**Layout rule**: a folder exists only when a module has real submodules
(`db`, `secrets`, `share`, `tui`); every other module is a flat `<name>.rs`.

## Tech stack

| Component | Technology |
|:----------|:-----------|
| Language | Rust (edition 2021) |
| Async runtime | Tokio |
| Database | SQLite via sqlx (WAL mode, FTS5) |
| API | Axum |
| TUI | Ratatui + Crossterm |
| Scripting | mlua (Lua 5.4, sandboxed) |
| Crypto | chacha20poly1305 (XChaCha20Poly1305), argon2 |
| Serialization | serde, serde_json, toml |
| Clipboard | arboard |
| Config | Hyprland-style `config.conf` (custom parser) |
| Errors | thiserror + anyhow |

## Status

**Version**: 0.2.0 — Phases 1–3 complete (see
[docs/reference/USER_STORIES.md](docs/reference/USER_STORIES.md) and
[docs/development/WORK.md](docs/development/WORK.md)).
Deliberately parked: process/package managers (prefer btop and friends),
web UI, cross-machine sync.

## License

MIT OR Apache-2.0

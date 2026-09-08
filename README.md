# TUI-OP-HUB

A terminal-based operations hub for command management, workflow automation, and
secure secret storage — local-first, single binary, keyboard-driven.

## Features

### Phase 1 ✅ Complete

- **Entity Management**: Create, read, update, delete typed entities (commands, scripts, apps, workflows, environment variables, configs, secrets)
- **Project Organization**: Group entities by project with filtering and search
- **Tag System**: Organize entities with flexible tagging
- **Full-Text Search**: Fast FTS5-powered search across all entities
- **Workflow Automation**: Lua-based workflow engine with Host functions (run_command, query_entity, emit_event, log)
- **Workflow Execution**: Execute workflows from TUI with run history tracking
- **Encrypted Secrets**: XChaCha20Poly1305 AEAD cipher with per-user key management
- **User Profiles**: Multi-user support with individual encryption keys
- **REST API**: Axum-based HTTP API for full CRUD and workflow execution
- **TUI Dashboard**: Interactive terminal UI with 10 screens (Dashboard, **Knowledge Base** parent page, Commands, Apps, Scripts, Projects, Workflows, Secrets, Plugins, Settings last) — keys 1-9 + 0; the **Knowledge Base** is ONE tab (2) holding commands, apps and scripts in a single list with type icons, a **type filter** (`f`: All/Commands/Apps/Scripts) and `n` to create an item of the filtered type; Enter on a project opens its detail view
- ✅ Knowledge **import/export**: portable JSON bundles (see [`docs/IMPORT_EXPORT.md`](docs/IMPORT_EXPORT.md)) — AI-generatable
- ✅ **Cron scheduling** (US-WF-07): run workflows on cron expressions via the built-in scheduler daemon
- ✅ **systemd / cron compatibility** (US-DEP-04): `tui-op-hub --install-service` runs the hub headless — `./install.sh` sets everything up; see [`docs/INSTALL.md`](docs/INSTALL.md)

### Phase 2 ✅ Complete

- **Scheduler daemon** (US-WF-07): cron-scheduled workflows execute automatically
- **Secrets as workflow variables**: `secrets.<name>` / `get_secret("<name>")` in Lua workflows
- **Admin users** (US-SEC): first registered user is admin; admins can delete users
  (their secrets are removed with them) and reset forgotten passwords
- **Secret classification & access control**: kinds (`password`, `ssh_key`, `gpg_key`,
  `api_key`) + `requires_reauth` flag for secrets that need more than just being logged in
- **Config management (US-CFG)**: register existing config files, deploy them as symlink / hard link / copy to multiple targets, detect + fix drift with one `u`, and version the whole store with git
- **SSH & GPG keygen**: `k` on the Secrets tab generates keys via the known tools
  (`ssh-keygen`, `gpg`); private key location and passphrase stored encrypted
- **File-backed scripts & workflows**: metadata `{"file": "…"}` runs Lua, JSON, Python,
  Node, shell — interpreter chosen by extension or shebang
- **Type-aware execution** (US-CMD-09): `cmd` runs in shell, `script` via its interpreter,
  `app` launched detached
- **Projects as environments** (US-ENV): attach an activation command (venv/pyenv/conda)
  to a project, `E` opens a shell inside it
- **Processes via known tools** (US-PROC): `p` launches btop/htop/top
- **Sudo compat** (US-CMD-09): `R` on a command runs it with elevated privileges

### Phase 2 extras (this iteration)

- **Dashboard mini-btop** (US-PROC-01): CPU (overall + per-core bars), RAM/swap, physical
  network interfaces with live RX/TX (docker/veth/VPN filtered out), temperatures, GPU
  stats via nvidia-smi — auto-refreshing every 2s
- **Quick launches**: `g` lazygit / `d` lazydocker / `k` k9s / `n` lazynpm open in a new
  terminal window when installed
- **Plugin/mod system** (US-PLG): Lua mods with `plugin.toml` manifests, capability
  approval, event hooks (`project_created` — e.g. automatic git init + first commit);
  Plugins tab (`9`) to approve/enable; see [`docs/PLUGINS.md`](docs/PLUGINS.md)
- **Project workspaces** (US-PROJ): `N` creates dir + git + env scaffold (path stored),
  `n` registers an existing directory, `O` opens the workspace in your editor
- **Secrets v2** (US-SEC): groups, username/URL/email fields, optional passphrase layer
  (`[locked]`), ssh-agent loading (`S`, auto after login), SSH terminals (`t`)
- **Lenient import + duplicate strategies**: full bundle, bare AI entity array, or
  `entities`-only JSON; per-import skip / overwrite / rename; file-picker integration
  (yazi → nnn → ranger → lf → zenity → kdialog)
- **New terminal window**: `` ` `` spawns your terminal emulator running the shell
  (`$TERMINAL` override, 11-emulator fallback)
### Phase 3 ✅ Complete

- **Navigation restructure** (US-TUI-11/12): digits `1-9`/`0` switch tabs from any
  screen (0 = Settings); `Tab` cycles in a configurable order (`[tui].tab_order`);
  the **Knowledge Base** (2) is ONE tab with a type filter
- **Config management page** (US-CFG-09..12): the Configs tab (6) — register existing
  config files with a full form (path with **built-in file browser** `Ctrl+O`, name,
  description, tags, deploy targets, mode) or `Ctrl+P` for the external picker;
  deploy targets can live anywhere (even in not-yet-existing folders); deploy as
  symlink / hard link / copy (`l`), update + drift fix (`u`), git commit (`g`)
- **Plugin UI actions** (US-PLG-13): plugins declare labeled `[[actions]]` in their
  manifest; `a` on the Projects tab runs them against the selected project —
  capability-gated twice (declaration subset + loaded/approved plugin)
- **Project scaffold templates** (US-PLG-14/15, US-PROJ-08): plugins ship starters
  as inert data in `<plugin>/templates/*.toml` (files with `{{project_name}}`
  placeholders, git init, post-create commands); optional template picker in the
  New Project Workspace form (default none) with a preview popup — include/exclude
  files, edit contents — before anything is written
- **Logic gates for visual scripting** (US-FUT-07): AND / OR / NOT / XOR /
  comparison / if-else nodes with typed boolean ports; the engine captures every
  step's return into `results["<step>"]` and `run_when` expressions gate any step
- **SSH host tags + connection test** (US-SSH-04/05): tag hosts in the SSH panel
  (`H` on Secrets), filter live with `f`, verify reachability with `t`
  (non-interactive probe with bounded timeout)
- **Project folder options** (US-PROJ): deleting a project can also remove its
  workspace folder (explicit toggle, default off); creating a workspace into an
  existing folder merges instead of failing (`Ctrl+O`)
- **Visible search bar** and **selection cursor markers** (`❯`) on every list

## Quick Start

### Install (recommended)

```bash
./install.sh          # build + install binary + enable background service
```

See [`INSTALL.md`](INSTALL.md) and [`docs/INSTALL.md`](docs/INSTALL.md).

### Build manually

```bash
cd TUI-OP-HUB
cargo build --release
```

### Run

```bash
# Set encryption key (generate with: openssl rand -base64 32)
export TUI_OP_HUB_SECRETS_KEY="<base64-32-byte-key>"
export TUI_OP_HUB_USER="default"  # Optional

./target/release/tui-op-hub
```

CLI flags: `--headless` (scheduler + API, no TUI), `--install-service`,
`--uninstall-service`, `--print-unit`, `--help`.

## Configuration

Configuration lives in a **Hyprland-style** config file — `section { key = value }`
blocks, `#` comments, quoted or bare values — at
`~/.config/tui-op-hub/config.conf`. Missing keys fall back to defaults and unknown
keys are ignored, so a minimal file is fine. Everything is also editable live in the
**Settings screen** (`0`), then `Ctrl+S` to persist:

```ini
# ~/.config/tui-op-hub/config.conf
# Format: section { key = value } — like Hyprland/sway configs

general {
    # External editor for the "open in editor" action (o on the Commands tab).
    editor = nano
}

tui {
    page_size = 20
    # Tab-cycle order for the Tab key (US-TUI-12). Valid ids:
    # dash, kb, cmd, app, script, proj, wf, sec, plug, set
    # tab_order = dash,kb,cmd,app,script,proj,wf,sec,plug,set
}

theme {
    name = dark
}

keybindings {
    copy = c
    run = r
}

api {
    bind_addr = 127.0.0.1:3001
}

database {
    path = tuihub.db
    busy_timeout_ms = 5000
}
```

### Create your own theme

Themes are plain color lists in the config. Copy a preset (`dark`, `light`, `gruvbox`,
`nord`, `dracula`, `solarized`) under `themes.<name>`, tweak, then set `theme.name`.
The Settings screen (`8`) previews live and `a` opens the visual color editor.

## Security

- Secrets are encrypted with **XChaCha20-Poly1305**; the key comes from the per-user
  entry in `user_keys` (Argon2-derived from the login password) or the
  `TUI_OP_HUB_SECRETS_KEY` env fallback (written by `--install-service` to
  `~/.config/tui-op-hub/env`, mode 600)
- Passphrase-protected secrets add a second Argon2+XChaCha layer; every use re-asks
- SSH keys flagged for the agent are offered to ssh-agent on login (agent auto-spawned);
  key files written for ssh/ssh-add are mode 600 and removed after use
- The systemd unit never embeds the secrets key

## TUI Keybindings

| Key | Action |
|:----|:-------|
| 1-9, 0 | Switch tab (from any screen) — 1 Dashboard, 2 Knowledge Base, 3 Projects, 4 Workflows, 5 Secrets, 6 Configs, 7 Plugins, 0 Settings |
| Tab | Cycle tabs (order from `[tui].tab_order` in config.conf; Settings last by default) |
| f | Knowledge tab: cycle the type filter (All, Commands, Apps, Scripts) |
| n / t / m / l / u / g | Configs tab: register file (form), add deploy target, cycle mode (symlink/hard link/copy), deploy, update (drift fix), git commit |
| Ctrl+O / Ctrl+P | Configs register form & target input: built-in file browser (create files/folders inline) / external picker |
| Up/Down | Navigate items |
| Enter | Select / open project detail |
| n | Create new |
| N | Create project workspace (Projects tab) — optional plugin template with preview |
| e | Edit selected |
| d | Delete (with confirm; projects offer an `f` toggle to also delete the folder) |
| / | Search (visible bar) |
| c | Copy content |
| o | Open in external editor |
| r | Run (commands/workflows) |
| R | Run with sudo/doas/su |
| s | Schedule workflow (Workflows tab) |
| X | Cancel the running background workflow (Workflows tab) |
| v | Visual workflow builder (`l` adds a logic node, `o` edits it — AND/OR/NOT/XOR/compare/if-else with `run_when` gates) |
| a | Projects tab: run plugin UI actions · Plugins tab: approve plugin |
| S | Load SSH keys into ssh-agent (Secrets tab) |
| t | Open SSH terminal (Secrets tab) · SSH panel: test connection |
| f | SSH panel: live tag/name filter |
| k | Keygen (Secrets tab) |
| H | SSH host manager — CRUD, tags, connection test, quick connect (Secrets tab) |
| u | Admin: manage users (Settings tab) |
| x / I | Export / Import knowledge base |
| g/d/k/n | Quick launch lazygit/lazydocker/k9s/lazynpm (Dashboard) |
| ` | Open a new terminal window |
| ? | Keybind helper |
| q / Esc | Quit / cancel |

## API Endpoints

Base: `http://127.0.0.1:3001` (configurable via `api.bind_addr`)

### Entities
- `GET /entities` - List entities
- `POST /entities` - Create entity
- `GET /entities/{id}` - Get entity
- `PUT /entities/{id}` - Update entity
- `DELETE /entities/{id}` - Delete entity
- `GET /entities/search?q=…` - FTS5 search
- `GET /entities/filter-by-tags?tags=a,b` - Tag filter
- `POST /entities/{id}/run` - Run entity

### Projects
- `GET /projects` - List projects
- `POST /projects` - Create project
- `GET /projects/{id}` - Get project
- `DELETE /projects/{id}` - Delete project
- `GET /projects/{id}/entities` - Project entities
- `GET /projects/{id}/dashboard` - Project dashboard

### Workflows & schedules
- `POST /workflows/{id}/execute` - Execute workflow
- `POST /workflows/{id}/schedule` - Schedule on cron (`{"cron_expr": "30 2 * * *"}`)
- `GET /schedules` - List schedules
- `DELETE /schedules/{id}` - Remove schedule

### Secrets (user-specific)
- `GET /secrets` - List user secrets
- `POST /secrets` - Create encrypted secret
- `GET /secrets/{id}` - Get secret (decrypted)
- `PUT /secrets/{id}` - Update secret
- `DELETE /secrets/{id}` - Delete secret

### Knowledge base
- `GET /export` - Export bundle (secrets excluded)
- `POST /import?duplicates=skip|overwrite|rename` - Import (lenient JSON shapes)

### Other
- `GET /health` - Health + version
- `GET /tags`, `GET /types` - Tag/type lists
- `GET /users`, `DELETE /users` - Dev mode only

## Testing

```bash
cd TUI-OP-HUB
cargo test          # unit + BDD scenarios (257 tests)
```

**Developer mode** — active automatically in debug builds (`cargo run`, `cargo test`)
or with `TUI_OP_HUB_DEV=1`:
- Settings screen title shows `[DEV]` and the footer offers `d: DEV wipe users`
- Pressing `d` **twice** deletes **all users** (secrets + keys cascade) **without
  logging in** — auth state resets instantly while testing, and the next signup
  becomes admin again
- REST: `GET /users` (list) and `DELETE /users` (wipe) are available in dev mode only
- Dev DB ops: `Shift+D` wipes the entire DB, `Shift+A` deletes all entities in the
  current tab — both dev-mode only, work on any screen
- Login dev manager (`u` on login screen): lists users, `x` deletes user + secrets,
  `r` resets password to `reset-me`

## Project Layout

```
TUI-OP-HUB/
├── README.md                  # This file
├── INSTALL.md                 # Quick install instructions
├── install.sh                 # Release installer
├── AGENTS.md                  # AI agent guide
├── WORK.md                    # AI agent work log
├── bp.md                      # Business plan
├── STACK_AND_TOOLS_GUIDE.md   # Tech stack reference
├── USER_STORIES.md            # User stories and requirements
├── docs/                      # Deep-dive guides (see docs/INDEX.md)
├── DB/                        # Database design (draw.io)
├── SKETCHES/                  # UI/UX sketches
└── TUI-OP-HUB/                # Rust application
    ├── Cargo.toml
    ├── tests/bdd_scenarios.rs # BDD integration tests
    └── src/
        ├── main.rs            # Entry point (CLI flags, wiring)
        ├── lib.rs             # Module declarations
        ├── error.rs           # AppError / AppResult
        ├── api.rs             # REST API (axum)
        ├── auth.rs            # Argon2 auth, per-user keys
        ├── config.rs          # Configuration (themes, keybindings)
        ├── config_manager.rs  # Managed-config store/registry (Configs tab)
        ├── db/                # SQLite pool + migrations/ (0001..0008)
        ├── environment.rs     # Env var management (Phase 3 stub)
        ├── filepicker.rs      # yazi/nnn/ranger/lf/zenity/kdialog chain
        ├── fuzzy.rs           # Fuzzy matcher
        ├── keygen.rs          # SSH/GPG key generation
        ├── models.rs          # Data models
        ├── monitor.rs         # Dashboard mini-btop snapshot
        ├── plugin.rs          # Plugin/mod system (Lua sandbox, events)
        ├── privilege.rs       # sudo/doas/su detection
        ├── process.rs         # Process/resource monitoring (sysinfo)
        ├── project_workspace.rs # Workspace creation + editors
        ├── repository.rs      # Data access layer (all SQL)
        ├── scheduler.rs       # Cron scheduler daemon
        ├── seed.rs            # Idempotent seed data
        ├── secrets/           # Encryption + ssh_agent
        ├── service.rs         # systemd/init integration
        ├── share/             # Import/export (+ crypto submodule)
        └── tui/               # modern_app, modern_ui, list_state, helpers
```

**Layout rule**: a folder exists only when a module has real submodules
(`db`, `secrets`, `share`, `tui`); every other module is a flat `<name>.rs`.

## Tech Stack

| Component | Technology |
|:----------|:-----------|
| Language | Rust (edition 2021) |
| Async runtime | Tokio |
| Database | SQLite (via sqlx, WAL mode) |
| API | Axum |
| TUI | Ratatui + Crossterm |
| Scripting | mlua (Lua 5.4) |
| Crypto | chacha20poly1305, argon2, getrandom |
| Serialization | serde, serde_json, serde_yaml, toml |
| Clipboard | arboard |
| Config | Hyprland-style `config.conf` (custom parser) |
| Logging | tracing + tracing-subscriber |
| Errors | thiserror + anyhow |

## Status

**Version**: 0.2.0
**Phase 1, 2 & 3**: ✅ Complete
**Remaining backlog** (see [`WORK.md`](WORK.md)): plugin approval workflow for
headless service runs; P3 parked areas (process manager via known tools, package
management, environments, backups, multi-DB) — see [`USER_STORIES.md`](USER_STORIES.md).

## License

MIT OR Apache-2.0

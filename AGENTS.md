# AGENTS.md — AI Agent Guide for TUI-OP-HUB

> This file is the entry point for AI coding agents (Cline, Cursor, Copilot, Claude, etc.).
> Read this before making any changes. It explains what the project is, how it is organized,
> what conventions to follow, what work remains, and the workflow to use.

---

## 1. What This Project Is About

**TUI-OP-HUB** is a **terminal-based operations hub** written in **Rust**. It centralizes a
developer's/power-user's daily tooling into one keyboard-driven app:

- **Command & knowledge base** — store, tag, search (FTS5), copy, and run commands/scripts/apps
- **Project organization** — group entities by project
- **Workflow automation** — Lua 5.4 (mlua) workflow engine with host functions
  (`run_command`, `query_entity`, `emit_event`, `log`) and run-history tracking
- **Secrets management** — XChaCha20Poly1305 AEAD encryption, per-user keys, multi-user profiles
- **REST API** — Axum HTTP server on localhost (default `127.0.0.1:3001`) mirroring the core features
- **Knowledge import/export** — portable JSON bundles (full bundle, bare AI-generated
  entity array, or `entities`-only object — all import leniently); see `docs/IMPORT_EXPORT.md`
- **Cron scheduling** — workflows run on cron expressions via the built-in scheduler daemon
- **Background service** — systemd user service (or cron/any init) runs the hub headless
  (`--headless`: scheduler + API); the TUI runs on demand and coexists with the service
- **Modern TUI** — ratatui + crossterm UI with login screen and 8 main tabs
  (Dashboard, Knowledge Base — the ONE tab for commands/apps/scripts with a type
  filter — Projects, Workflows, Secrets, Configs, Plugins, Settings last; tab order
  configurable via `[tui].tab_order`); digits 1-9/0 switch tabs from any screen,
  `Enter` on a project opens its detail view, `` ` `` opens a new terminal window,
  `/` search with a visible search bar, `I`/`x` import/export, selected rows show
  a `❯` cursor marker
- **Configs management** — register existing config files (form with file browser,
  metadata, multiple deploy targets), deploy as symlink/hard link/copy, update +
  drift fix, git-versioned store
- **Plugin system** — Lua mods with capability approval, event hooks, **UI
  actions** (`[[actions]]` → `a` on Projects) and **project scaffold templates**
  (`templates/*.toml` + preview/customize in the workspace form)
- **Visual workflow builder** — compose workflows from saved commands plus logic
  nodes (AND/OR/NOT/XOR/compare/if-else) with `results` flow and `run_when` gates
- **SSH host manager** — tags + live filter, connection test, quick-connect in a
  new terminal window

**Long-term vision** (see `bp.md`): a unified "system control center" that will also cover
process monitoring, package management, environment/config management, SSH host management,
plugin system, scheduled tasks, and cross-machine sync.

**Local-first philosophy**: SQLite database, single ~6 MB static binary, no external runtime
dependencies, Hyprland-style config (`~/.config/tui-op-hub/config.conf`).

---

## 2. Repository Layout

```
TUI-OP-HUB/                       # repo root (docs live here)
├── AGENTS.md                     # ← this file
├── WORK.md                       # live work log for AI agents (ongoing tasks, what/how)
├── README.md                     # user-facing readme (features, config, API)
├── INSTALL.md                    # quick install instructions (repo root)
├── install.sh                    # release installer: build + install binary + enable service
├── docs/                         # documentation (start at docs/INDEX.md)
│   ├── INDEX.md                  # map of all documentation
│   ├── INSTALL.md                # service/TUI coexistence, secrets key, non-systemd inits
│   ├── IMPORT_EXPORT.md          # knowledge bundle schema, AI prompt template, cron workflows
│   ├── PLUGINS.md                # Lua mod API: manifest, capabilities, event hooks
│   └── business/                 # business structure
│       ├── VISION.md             # product vision & personas
│       ├── ROADMAP.md            # phase plan
│       └── GOVERNANCE.md         # roles, branching, definition of done
├── bp.md                         # business plan / product vision (source of truth for scope)
├── ARCHITECTURE.md               # system architecture, module responsibilities, data flow
├── USER_STORIES.md               # user stories with IDs (US-XXX-NN), priorities, phase status
├── STACK_AND_TOOLS_GUIDE.md      # rationale for Rust/Lua/Python layering and DB choices
├── MODERN_TUI_GUIDE.md           # TUI design guide (colors, layout, login/dashboard)
├── DB/LOGICAL.drawio             # database design diagram (draw.io)
├── SKETCHES/                     # UI/UX sketches (Figma SVG exports)
└── TUI-OP-HUB/                   # the Rust application (cargo crate root)
    ├── Cargo.toml                # dependencies & release profile
    ├── tests/
    │   └── bdd_scenarios.rs      # BDD integration scenarios (public API, in-memory DB)
    └── src/
        ├── main.rs               # entry point: CLI flags, config, DB pool, plugins, scheduler,
        │                         # API spawn (port-conflict safe), TUI loop
        ├── lib.rs                # 23 module declarations (single flat list)
        ├── error.rs              # AppError / AppResult
        │
        ├── api.rs                # axum routes/handlers (entities, projects, workflows,
        │                         # schedules, export/import)
        ├── auth.rs               # Argon2 password auth, per-user EncryptionKey (zeroized)
        ├── config.rs             # AppConfig — Hyprland-style config.conf (themes, keybindings)
        ├── config_manager.rs     # Managed-config store/registry (Configs tab, US-CFG)
        ├── db/                   # SQLite pool (WAL) + migrations/ (0001..0008)
        ├── environment.rs        # env var management (Phase 3 stub)
        ├── filepicker.rs         # yazi/nnn/ranger/lf/zenity/kdialog chooser chain
        ├── fuzzy.rs              # fuzzy matcher used by list search
        ├── keygen.rs             # SSH/GPG key generation
        ├── models.rs             # data models (entities, projects, secrets, plugins, ...)
        ├── monitor.rs            # dashboard mini-btop: cpu/ram/net/temps/gpu snapshot
        ├── plugin.rs             # plugin system (Capability model, Lua sandbox, events)
        ├── privilege.rs          # sudo/doas/su detection for privileged runs
        ├── process.rs            # sysinfo-based process/resource monitoring
        ├── project_workspace.rs  # project workspace editors (open project in IDE)
        ├── repository.rs         # data access layer (all SQL lives here)
        ├── scheduler.rs          # cron-based WorkflowScheduler daemon (cron crate)
        ├── service.rs            # systemd user-unit generation/install, cron watchdog line
        ├── share/                # knowledge export/import (US-CMD-01)
        │   ├── mod.rs            # KnowledgeBundle, lenient parsing, duplicate modes
        │   └── crypto.rs         # passphrase-based portable encryption
        ├── seed.rs               # idempotent seed data (commands, options, apps)
        ├── secrets/              # secret encryption + ssh-agent
        │   ├── mod.rs            # XChaCha20Poly1305 helpers, passphrase wrap
        │   └── ssh_agent.rs      # ssh-agent spawn + key loading, ssh terminals
        └── tui/                  # ratatui UI layer
            ├── modern_app.rs     # the application (login, 8 main tabs, forms, popups)
            ├── modern_ui.rs      # theme + login/dashboard rendering
            ├── list_state.rs     # reusable list/form state + entity types
            └── helpers.rs        # free helpers (paths, formatting, terminal spawn)
```

**Layout rule**: a folder exists ONLY when a module has real submodules
(`share`, `secrets`, `tui`, `db`); every other module is a flat `<name>.rs`.

⚠️ **Syncthing artifacts**: files like `Cargo.sync-conflict-*.toml` and
`main.sync-conflict-*.rs` exist from file syncing. **Never edit or import them**; they are
stale duplicates. (Cleaning them up is a valid housekeeping task — see §6.)


⚠️ **Syncthing artifacts**: files like `Cargo.sync-conflict-*.toml` and
`main.sync-conflict-*.rs` exist from file syncing. **Never edit or import them**; they are
stale duplicates. (Cleaning them up is a valid housekeeping task — see §6.)

---

## 3. Tech Stack (do not introduce new frameworks without strong justification)

| Concern        | Technology |
|:---------------|:-----------|
| Language       | Rust, edition 2021 |
| Async runtime  | tokio (`#[tokio::main]`, full features) |
| Database       | SQLite via sqlx 0.8, WAL mode, FTS5 search, embedded migrations |
| HTTP API       | axum 0.8 + tower-http (trace, cors) |
| TUI            | ratatui 0.29 + crossterm 0.28 |
| Scripting      | mlua 0.10 (Lua 5.4, vendored, send) |
| Crypto         | chacha20poly1305 (XChaCha20Poly1305), argon2, zeroize, rand, base64 |
| Scheduling     | cron 0.12 |
| Plugins        | libloading 0.8 |
| Process info   | sysinfo 0.32 |
| Serialization  | serde, serde_json, serde_yaml, toml |
| Errors         | thiserror (AppError) + anyhow (main/workflow/config) |
| Logging        | tracing + tracing-subscriber (env-filter) |
| Clipboard      | arboard |

Release profile: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`.

---

## 4. Build, Run, Test Commands

All cargo commands run from the `TUI-OP-HUB/TUI-OP-HUB/` directory (the crate root):

```bash
cd TUI-OP-HUB          # from repo root
cargo build            # debug build
cargo build --release  # optimized build (recommended for running)
cargo check            # fast type-check without codegen
cargo clippy           # lints — run before declaring a task done
cargo fmt              # formatting (rustfmt defaults)

# Tests:
cargo test                            # everything: unit + BDD scenarios
cargo test --test bdd_scenarios       # BDD integration scenarios only
cargo test repository::               # one module's tests only

# Run the TUI (secrets key required for secret functionality):
export TUI_OP_HUB_SECRETS_KEY="$(openssl rand -base64 32)"
export TUI_OP_HUB_USER="default"        # optional
./target/release/tui-op-hub

# CLI flags (parsed in main.rs, no clap):
./target/release/tui-op-hub --headless            # scheduler + API, no TUI
./target/release/tui-op-hub --print-unit          # show systemd user unit
./target/release/tui-op-hub --install-service     # idempotent full setup
./target/release/tui-op-hub --uninstall-service

# Release install (build + binary + enabled background service):
./install.sh                                      # from the repo root
```

Note: the TUI and the `--headless` service can run **simultaneously** — they share the
SQLite DB (WAL); if the API port is taken, the second instance logs a warning and skips
starting its own API (never panic on AddrInUse).

Environment variables:
- `TUI_OP_HUB_SECRETS_KEY` — base64 32-byte master key for secret encryption
- `TUI_OP_HUB_USER` — current user profile name (default: `default`)
- `RUST_LOG` — tracing filter (default `info`)

---

## 5. Architecture Rules (follow these when adding code)

1. **Layered separation**: `tui/` and `api/` are *presentation* layers. All business logic and
   SQL belong in `repository/`, `models/`, and domain modules. Never write SQL inside TUI or
   API handlers.
2. **Database access** goes through the shared `Arc<SqlitePool>`. Migrations are numbered SQL
   files in `src/db/migrations/` (`0001_...`, `0002_...`, ...). Add a new migration rather than
   altering old ones.
3. **Secrets are never stored in plaintext.** Encrypt with the per-user key via the
   `secrets`/`auth` modules before insert. Random 24-byte nonce per encryption. Keys come from
   the `TUI_OP_HUB_SECRETS_KEY` env var or the `user_keys` table. New keys should use
   `auth::EncryptionKey` (zeroized on drop).
4. **Error handling**: domain/repository code returns `AppResult<T>` (`error.rs`, thiserror).
   `main.rs` and workflow/config use `anyhow::Result`. API handlers return
   `Result<Json<T>, String>` for JSON error responses.
5. **Async model**: TUI event loop is synchronous crossterm; long operations use
   `tokio::task::spawn_blocking`. The Lua engine context is created per execution.
6. **User stories are the spec.** Each feature maps to `US-<AREA>-<NN>` IDs in
   `USER_STORIES.md`. When implementing a feature, reference its story ID in code comments
   (existing code does this, e.g. `// US-PROC-01`) and update the story status.
7. **Docs must stay truthful**: after meaningful changes, update `README.md`,
   `ARCHITECTURE.md`, and the phase status in `USER_STORIES.md`.
8. **Tests are part of the feature**: every behavior change ships with unit tests in the
   touched module and a BDD scenario in `tests/bdd_scenarios.rs` (see §7 Step 4).
9. **Service/TUI coexistence**: the background service (`--headless`) and the TUI share the
   SQLite DB (WAL). Never panic on API `AddrInUse` — log a warning and skip the local API
   (see `main.rs`). Anything that must never break the TUI loop (spawn processes, edit files)
   belongs in `run()` or a dedicated method, not inline key handlers.
10. **Filesystem side effects must be injectable**: functions that write under `$HOME`
   (see `src/service/mod.rs`) have `*_in(home)` variants so tests use temp dirs and never
   touch the real home directory. Idempotency matters: setup routines must be safe to re-run.

---

## 6. Current Status & Roadmap

### ✅ Phase 1 — Complete (v0.2.0)
Entity CRUD, projects, tags, FTS5 search, Lua workflows + run history, encrypted secrets,
user profiles, REST API, modern TUI (login + tabs incl. the single Knowledge Base tab
with type filter, Settings last), Argon2-based auth (migration 0002),
config/theming/keybindings.

### ✅ Phase 2 — Complete
Scheduler (US-WF-07), systemd/cron integration (US-DEP-04), knowledge import/export
(US-CMD-01), project detail view (US-PROJ-07), plugin manifest/approval/loading/events
(US-PLG-05/06/07/10), SSH host manager (US-SSH-01..03/06), workflow stop/cancel
(US-WF-09), secrets v2, admin users, config-management foundation (US-CFG-01..08).

### ✅ Phase 3 — Complete (TUI expansion)
- [x] **Navigation restructure** (US-TUI-11/12): Knowledge Base parent tab (2),
      digits 1-9/0 switch tabs from any screen, `[tui].tab_order` cycle
- [x] **Configs management page** (US-CFG-09..12): register form (path + name +
      description + tags + deploy targets + mode) with `Ctrl+O` built-in file
      browser / `Ctrl+P` external picker, deploy `l`, update+drift `u`, git `g`
- [x] **Plugin UI actions** (US-PLG-13): manifest `[[actions]]`, `a` on Projects,
      capability-gated at discovery + run time
- [x] **Project scaffold templates** (US-PLG-14/15, US-PROJ-08):
      `<plugin>/templates/*.toml`, optional picker + preview/customize, applied
      off-thread, never overwrites
- [x] **Logic gates** (US-FUT-07): AND/OR/NOT/XOR/Compare/IfElse nodes, `results`
      capture in the engine, `run_when` gates
- [x] **SSH tags + connection test** (US-SSH-04/05): migration 0008, live filter,
      non-interactive probe
- [x] Project folder options (delete-with-folder toggle, merge-into-existing),
      selection cursor markers, consistent digit behavior

### 🟢 Phase 4+ — Remaining (future)
- Plugin approval workflow for headless service runs (leftover)
- Process manager (prefer btop/htop) — US-PROC-01..07 (parked)
- Package manager integration — US-PKG-01..09
- Environment/venv management — US-ENV-01..08
- Multi-database / multi-context support — US-DB-01..06
- Web UI — US-WEB-01..04
- Cross-machine sync — US-SYNC-01..04
- Backup/export — US-BAK-01..04

### 🧹 Housekeeping
- Remove stale `*.sync-conflict-*` files (Syncthing duplicates)
- `Cargo.toml` has empty `repository` field
- Expand test coverage further (repository, tui, share, scheduler, service now have
  solid coverage; plugin/, process/, environment/ are still thin)

---

## 7. AI Agent Workflow

When working on this repo, follow this loop:

### Step 1 — Understand
1. Read this file fully.
2. **Read `WORK.md`** — the live work log. It lists what is currently being changed, how,
   and what is next. Append your session to it when you finish (what you did, what broke,
   what's next).
3. Identify the relevant user story ID(s) in `USER_STORIES.md` and read the story.
4. Read `ARCHITECTURE.md` sections covering the modules you will touch. For a map of all
   documentation (including the business docs in `docs/business/`), see `docs/INDEX.md`.
5. Trace the existing pattern: for a feature, find how a *similar* feature flows through
   `models/` → `repository/` → `api/` + `tui/` and mirror it.

### Step 2 — Plan
5. State the plan before editing: which modules change, which story IDs are covered, what
   migrations (if any) are needed, and how it will be tested.
6. Database changes → new numbered migration file; never edit existing migrations.

### Step 3 — Implement
7. Follow the conventions in §5 (layering, error types, encryption rules, story-ID comments).
8. Keep changes minimal and focused; don't refactor unrelated code in the same change.
9. Match existing code style: rustfmt defaults, module-per-directory with `mod.rs`,
   doc comments (`//!`) on modules, `///` on public items.

### Step 4 — Test & Validate
10. **Add or update tests for every behavior change.** Two kinds live in this repo:
    - **Unit tests**: `#[cfg(test)] mod tests` inside the module you touched.
      Pure logic (state machines, parsing, helpers) needs no setup; DB-backed tests use an
      in-memory SQLite pool with a **single connection** (so all queries share one schema):
      `SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:")` + `db::run_migrations`.
      See `repository::tests` and `tui::modern_app::tests` for patterns.
    - **BDD scenarios**: `TUI-OP-HUB/tests/bdd_scenarios.rs` — Gherkin-style integration
      tests over the public API, named `given_<state>_when_<action>_then_<result>` with a
      `Feature:`/`Scenario:` doc comment. Add a new scenario for each user-visible behavior
      you implement (see the existing Features: command base, projects, secrets, visual
      workflows, TUI state).
    - Behavioral TUI tests: build a `ModernApp` on an in-memory pool and drive
      `handle_key(KeyEvent::new(...))` — no terminal needed (see `modern_app::tests`).
      For DB-backed TUI tests use the `test_app_db()` helper (pool + migrations);
      remember the app starts in the `Login` state — set `app.ui.state` before
      driving keys, and note service/TUI coexistence rules (§5 rule 9).
11. Run from `TUI-OP-HUB/TUI-OP-HUB/` — all four must pass before you are done:
    ```bash
    cargo fmt
    cargo clippy --all-targets 2>&1 | grep -c '^error'   # must be 0
    cargo test                                           # unit + BDD, 0 failures
    cargo build
    ```
12. If the change touches TUI, also do a manual smoke run (launch the binary, exercise the
    affected tab/keys). If it touches the API, smoke-test the endpoint with `curl` on the
    configured bind address (default `127.0.0.1:3001`).

### Step 5 — Commit with git
13. Check what actually changed before committing:
    ```bash
    git status
    git diff
    ```
14. Commit after every validated unit of work — never leave finished work uncommitted and
    never commit with a red build or failing tests. Use concise conventional commits and
    reference the story IDs:
    ```bash
    git add <specific files>
    git commit -m "feat(tui): working create/edit/delete forms (US-CMD-01, US-PROJ-01)"
    ```
    Types: `feat`, `fix`, `test`, `docs`, `refactor`, `chore`. Scope: the module area
    (`tui`, `repo`, `workflow`, `secrets`, `api`, `docs`, ...).
15. Never commit secrets, keys, `*.db` database files, `target/`, or `*.sync-conflict-*`
    files. If a `.gitignore` entry is missing for one of those, add it.

### Step 6 — Update docs & finish
16. Update `USER_STORIES.md` phase status (✅/🔄) for completed stories.
17. Update `README.md` / `ARCHITECTURE.md` if behavior, endpoints, keybindings, or schema changed.
18. Append your session to `WORK.md` (what you did, what broke, what's next).
19. Summarize: what changed, which story IDs were completed, how it was validated
    (tests added, commands run).

### Do NOT
- ❌ Store plaintext secrets or log decrypted values
- ❌ Write SQL outside `repository/` or domain modules
- ❌ Edit or reference `*.sync-conflict-*` files
- ❌ Add a new dependency for something the existing stack already covers (§3)
- ❌ Break the single-binary/local-first design (no required network services)
- ❌ Edit already-applied migration files
- ❌ Ship a behavior change without tests (unit + BDD scenario)
- ❌ Leave work uncommitted or commit with failing tests
- ❌ Trust a scripted edit without verifying it: after any scripted patch, `grep` the file
  for the expected markers (and re-check after `cargo fmt`). Several edits have been lost
  this way — verify, then compile, then test
- ❌ Use `\u{...}` escapes inside Python heredoc patch scripts: Python decodes `\u` as a
  unicode escape and silently mangles or rejects the script. Write real UTF-8 characters
  (✓, —, 💻) or raw strings instead


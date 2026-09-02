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
- **Modern TUI** — ratatui + crossterm UI with login screen, dashboard, and 7 tabs
  (Dashboard, Commands, Projects, Tags, Search, Workflows, Secrets)

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
├── docs/                         # documentation (start at docs/INDEX.md)
│   ├── INDEX.md                  # map of all documentation
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
        ├── main.rs               # entry point: config → DB pool → API spawn → TUI loop
        ├── lib.rs                # module declarations
        ├── error.rs              # AppError / AppResult
        ├── config/               # AppConfig — Hyprland-style config.conf (themes, keybindings)
        ├── db/                   # SQLite pool (WAL) + migrations/
        │   └── migrations/       # 0001_init.sql, 0002_user_auth.sql
        ├── models/               # data models (entities, projects, secrets, plugins, ...)
        ├── repository/           # data access layer (all SQL lives here)
        ├── api/                  # axum routes/handlers
        ├── tui/                  # ratatui UI (modern_app, modern_ui, login_view, list_state)
        ├── secrets/              # encryption helpers
        ├── auth/                 # Argon2 password auth, per-user EncryptionKey (zeroized)
        ├── workflow/             # Lua workflow engine
        ├── scheduler/            # cron-based WorkflowScheduler daemon (cron crate)
        ├── plugin/               # plugin system (Capability model, loading)
        ├── process/              # sysinfo-based process/resource monitoring
        ├── environment/          # environment variable management
        └── config_manager/       # config file management
```

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

# Run (secrets key required for secret functionality):
export TUI_OP_HUB_SECRETS_KEY="$(openssl rand -base64 32)"
export TUI_OP_HUB_USER="default"        # optional
./target/release/tui-op-hub
```

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

---

## 6. Current Status & Roadmap

### ✅ Phase 1 — Complete (v0.2.0)
Entity CRUD, projects, tags, FTS5 search, Lua workflows + run history, encrypted secrets,
user profiles, REST API, modern TUI (login + 7 tabs), Argon2-based auth (migration 0002),
config/theming/keybindings.

### 🔄 Phase 2 — Foundation ready, integration pending (the active TODO list)
Models, schema, DB functions, and module skeletons exist; **TUI/API integration is missing**:

- [ ] **Scheduler** (`src/scheduler/`): wire `WorkflowScheduler` daemon into `main.rs`;
      cron parsing works; needs TUI management + start/stop from API (US-WF-07)
- [ ] **Plugins** (`src/plugin/`): implement approval workflow UI + enforcement (US-PLG-05/06),
      plugin command registration + TUI listing (US-PLG-07/10)
- [ ] **SSH host manager**: TUI + quick-connect over existing models/DB functions
      (US-SSH-01..06); SSH key storage + ssh-agent integration (US-SEC-01/05)
- [ ] **Workflow stop/cancel** for running workflows (US-WF-09)
- [ ] **Systemd integration** (US-DEP-04)

### 🟢 Phase 3+ — Not started (future)
- Process management TUI (`src/process/` backend exists via sysinfo) — US-PROC-01..07
- Package manager integration (apt/pacman/nix) — US-PKG-01..09
- Environment/venv management — US-ENV-01..08
- Config file management — US-CFG-01..08 (module stub exists in `config_manager/`)
- Multi-database / multi-context support — US-DB-01..06
- Web UI — US-WEB-01..04
- Cross-machine sync — US-SYNC-01..04
- Backup/export — US-BAK-01..04

### 🧹 Housekeeping
- Remove stale `*.sync-conflict-*` files (Syncthing duplicates)
- `Cargo.toml` has empty `repository` field
- Expand test coverage (tests currently exist in only a few modules:
  shell_exports, environment_parsing, simple_lua_script areas)

---

## 7. AI Agent Workflow

When working on this repo, follow this loop:

### Step 1 — Understand
1. Read this file fully.
2. **Read `WORK.md`** — the live work log. It lists what is currently being changed, how,
   and what is next. Append your session to it when you finish (what you did, what broke,
   what's next).
3. Identify the relevant user story ID(s) in `USER_STORIES.md` and read the story.
3. Read `ARCHITECTURE.md` sections covering the modules you will touch. For a map of all
   documentation (including the business docs in `docs/business/`), see `docs/INDEX.md`.
4. Trace the existing pattern: for a feature, find how a *similar* feature flows through
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


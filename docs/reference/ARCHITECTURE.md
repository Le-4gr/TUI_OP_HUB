# TUI-OP-HUB Architecture

## System Overview

TUI-OP-HUB is a terminal-based operations hub built with Rust, designed to provide a unified interface for command management, workflow automation, and secure secret storage. The system follows a layered architecture with clear separation of concerns.

```
┌─────────────────────────────────┐
│   Terminal UI (ratatui)         │
│   - 8 Tabs (Dashboard,          │
│     Knowledge*, Projects,       │
│     Workflows, Secrets,         │
│     Configs, Plugins, Settings) │
│   - * Knowledge = default       │
│     screen after login (launcher│
│     with auto-focused search)   │
│   - Forms, Lists, Detail views  │
└──────────────┬──────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│   Application State (App)       │
│   - Selected items              │
│   - Forms (Entity, Project,     │
│     Secret)                     │
│   - Loaded data (entities,      │
│     projects, secrets, etc)     │
└──────────────┬──────────────────┘
               │
        ┌──────┴──────┐
        ▼             ▼
┌──────────────┐  ┌──────────────────────┐
│ Repository   │  │ API Layer (axum)     │
│ (Data Access)│  │ - HTTP routes        │
│ - Entity CRUD│  │ - Request handlers   │
│ - Project    │  │ - Error responses    │
│ - Secret CRUD│  │ - JSON serialization │
│ - Workflow   │  │                      │
│ - User/Plugin│  │ Port: 3001           │
└──────┬───────┘  └──────────┬───────────┘
       │                     │
       └──────────┬──────────┘
                  ▼
        ┌─────────────────────┐
        │   Secrets Module    │
        │ XChaCha20Poly1305   │
        │ - Encryption        │
        │ - Decryption        │
        │ - User key mgmt     │
        └──────────┬──────────┘
                   │
        ┌──────────┴──────────┐
        ▼                     ▼
   ┌────────────┐      ┌─────────────────┐
   │ Workflow   │      │ Database Layer  │
   │ Engine     │      │ (sqlx, SQLite)  │
   │ - Lua 5.4  │      │ - 12+ tables    │
   │ - Host fns │      │ - FTS search    │
   │ - Execution│      │ - Migrations    │
   └────────────┘      │ - WAL mode      │
                       └────────────────┘
```

## Core Modules

### 1. Config Module (`src/config.rs`)

**Responsibility**: Load and manage application configuration

**Key Structures**:
- `AppConfig`: Root config with database, API, TUI, theme, keybindings, current_user
- `DatabaseConfig`: DB path and busy timeout
- `ApiConfig`: HTTP server bind address
- `TuiConfig`: Page size and enabled flag
- `ThemeConfig`: Colors for UI rendering
- `KeybindingsConfig`: Customizable key mappings

**Behavior**:
- Reads from `~/.config/tui-op-hub/config.toml` (XDG standard)
- Falls back to sensible defaults if file missing
- Environment variables override config (e.g., `TUI_OP_HUB_USER`)

### 2. Database Module (`src/db/mod.rs`)

**Responsibility**: Initialize database pool and run migrations

**Key Functions**:
- `init_pool(database_path)`: Create SQLite pool with WAL mode
- `run_migrations(pool)`: Execute the embedded migrations (`0001`–`0008`: core
  schema, user auth, phase-2 tables, tooling, project paths, plugin tables,
  secret groups, SSH host tags)

**Schema Highlights**:
- **entities**: Core CRUD entities with FTS indexing
- **projects**: Grouping container for entities (+ stored workspace path)
- **tags**: Flexible categorization
- **types**: Entity type definitions
- **workflow_runs**: Execution history with duration/output/error
- **secrets**: Encrypted values with user_id (+ groups, metadata, passphrase layer, ssh-agent flag)
- **user_profiles**: Multi-user support
- **user_keys**: Per-user encryption keys
- **plugins**: Plugin definitions with manifest
- **ssh_hosts**: SSH host configurations (+ `tags` for grouping, US-SSH-04)
- **scheduled_tasks**: Cron-based task scheduling

### 3. Models Module (`src/models.rs`)

**Responsibility**: Data structure definitions with serialization

**Key Structs** (all with `sqlx::FromRow` and `serde` derives):
- `Entity`: Command, script, workflow, config, etc.
- `Project`: Grouping container
- `Tag`: Categorization
- `WorkflowRun`: Execution record (run_id, success, output, error, duration_ms)
- `Secret`: Encrypted secret with user_id and value_enc
- `UserProfile`: User with username, email
- `UserKey`: Per-user encryption key (user_id, key_b64)
- `Plugin`: Plugin definition (name, version, manifest, enabled)
- `SshHost`: SSH connection configuration
- `ScheduledTask`: Cron task definition

### 4. Repository Module (`src/repository.rs`)

**Responsibility**: Data access layer with async CRUD operations

**Key Operations**:
- Entity CRUD: `create_entity`, `get_entity`, `list_entities`, `update_entity`, `delete_entity`
- Project CRUD: Full CRUD operations
- Secret CRUD with user_id: `create_secret(pool, user_id, name, value_enc)`
- Search: `search_entities`, `filter_by_tags`
- Workflow: `insert_workflow_run`, `list_workflow_runs_by_workflow_id`
- User: `get_or_create_user`, `list_user_profiles`, `set_user_key`
- Plugin: `create_plugin`, `list_plugins`, `enable_plugin`, `disable_plugin`
- SSH: `create_ssh_host`, `list_ssh_hosts`, `delete_ssh_host`
- Scheduler: `create_scheduled_task`, `list_scheduled_tasks`

**Implementation Pattern**:
```rust
pub async fn create_entity(pool: &SqlitePool, req: &CreateEntity) -> AppResult<Entity> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO entities (...) VALUES (...)")
        .bind(...).bind(...)
        .execute(pool).await?;
    get_entity(pool, &id).await
}
```

### 5. API Module (`src/api.rs`)

**Responsibility**: HTTP endpoints and request/response handling

**Architecture**:
- **Router**: Routes organized by resource (entities, projects, secrets, workflows)
- **State**: `AppState { pool: Arc<SqlitePool> }`
- **Handlers**: Async functions returning `Result<Json<T>, String>`
- **Error Handling**: Errors converted to JSON error responses

**Key Routes**:
- `GET /secrets` → `list_secrets_handler` → calls `repository::list_secrets(pool, "default")`
- `POST /secrets` → `create_secret_handler` → encrypts with `secrets::encrypt_for_user`, stores
- `GET /secrets/{id}` → `get_secret_handler` → decrypts with `secrets::decrypt_for_user`
- `POST /workflows/{id}/execute` → spawns blocking task for engine, persists run

**Request/Response Pattern**:
```rust
async fn handler(State(state): State<AppState>, Json(req): Json<Req>) -> Result<Json<Res>, String> {
    let result = repository::operation(&state.pool, req.field).await?;
    Ok(Json(result))
}
```

### 6. TUI Module (`src/tui/`)

**Responsibility**: Terminal user interface with interactive navigation

**Structure** (`modern_app.rs` ~10k lines + `modern_ui.rs` theme/login/dashboard,
`list_state.rs` reusable list/form/node state, `helpers.rs`):
- `ModernApp` struct: tab state (`AppState` — Knowledge **(default/launcher)**, Dashboard,
  Projects, Workflows, Secrets, Configs, Plugins, Settings), all popup/panel states,
  selections and loaded data; single async `handle_key` dispatcher routes
  keypresses through the overlay stack (sudo popup → file browser → panels →
  forms → tab handlers)
- Overlays render on top of the base screen in `render()`; popups include the
  visual workflow builder (+ logic-node editor, command picker, template
  preview), config register/target forms, file browser, SSH panel, plugin
  actions, keybinds overlay and delete confirmations
- Reusable state types live in `list_state.rs` (pagination, forms,
  `VisualStep`/`VisualNodeKind` + logic-node compilation)
- **Knowledge tab** holds all entity kinds in one list — `cmd`, `script`,
  `app` and **`chain`** (pipe/semicolon one-liners, parsed quote-aware; `i`
  shows per-segment notes, `r` opens the run-mode chooser)
- Jobs panel (`j` on the Dashboard) tracks running workflows + background /
  nohup processes (`s` stop, `K` force-kill, `r` refresh)

**Selected rows** show an explicit `❯` cursor marker in addition to the
highlight background, so selection is theme-independent (US-APP-01).

**Tab features** (highlights):
- **Knowledge (2)**: one list for commands/scripts/apps/**chains** with type
  filter (`f`); run modes via `r` (new terminal / foreground / background /
  nohup), `i` for options panel (cmds) or chain segment info
- **Projects (3)**: workspace creation with plugin-template picker + preview
  (`N`), register existing dir (`n`), delete with optional folder removal
  (`d` + `f` toggle), plugin UI actions (`a`), editor/shell open (`O`/`E`)
- **Configs (6)**: register/target forms with `Ctrl+O` built-in file browser
  (create files/folders inline) and `Ctrl+P` external picker, deploy/update/git
  actions, tag-less list navigation — digits always switch tabs
- **Secrets (5)** → `H`: SSH host panel with tags + live filter (`f`) and
  connection test (`t`)
- Digits `1-9`/`0` switch tabs from any screen; `Tab` follows
  `[tui].tab_order`

### 7. Secrets Module (`src/secrets/mod.rs`)

**Responsibility**: Encryption/decryption with per-user key management

**Algorithm**: XChaCha20Poly1305 AEAD (authenticated encryption with associated data)

**Key Management**:
- Per-user 32-byte keys stored in `user_keys` table
- Fallback to `TUI_OP_HUB_SECRETS_KEY` environment variable
- Key lookup: `get_user_key(pool, user_id)` → DB or env

**Encryption Pattern**:
```rust
pub async fn encrypt_for_user(pool: &SqlitePool, user_id: &str, plaintext: &str) -> Result<String> {
    let key_b64 = get_user_key(pool, user_id).await?;  // DB lookup or env fallback
    let key_bytes = base64::decode(&key_b64)?;  // 32 bytes
    let cipher = XChaCha20Poly1305::new_from_slice(&key_bytes)?;
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce)?;  // Random per message
    let ciphertext = cipher.encrypt(XNonce::from_slice(&nonce), plaintext.as_bytes())?;
    Ok(base64::encode([&nonce[..], &ciphertext[..]].concat()))  // nonce + ciphertext
}
```

**Decryption Pattern**:
```rust
pub async fn decrypt_for_user(pool: &SqlitePool, user_id: &str, value_enc_b64: &str) -> Result<String> {
    let key_b64 = get_user_key(pool, user_id).await?;
    let encrypted = base64::decode(&value_enc_b64)?;
    let (nonce, ciphertext) = encrypted.split_at(24);
    let cipher = XChaCha20Poly1305::new_from_slice(&key_bytes)?;
    let plaintext = cipher.decrypt(XNonce::from_slice(nonce), ciphertext)?;
    Ok(String::from_utf8(plaintext)?)
}
```

### 8. Workflow Module (`src/workflow.rs`)

**Responsibility**: Lua-based workflow engine with Host functions, plus the
visual-builder compilation target (US-WF, US-FUT-07)

**Engine**: mlua (Lua 5.4, vendored, Send-compatible)

**Host Functions** (callable from Lua):
- `run_command(cmd)`: Execute shell command, return output
- `query_entity(id)`: Fetch entity from DB by ID
- `emit_event(event_type, data)`: Log event
- `log(message)`: Print log message

**Execution Model**:
- Steps run in definition order; the Lua engine is shared across steps
- Every step's **return value is captured** into the global `results` table
  (`results["<step name>"]`; `nil`/`false` → `false`) so later steps can
  reference earlier outputs (US-FUT-07)
- `WorkflowStep.run_when` (optional Lua expression) gates a step: skipped
  (reported as "◌ skipped") when falsy; a broken condition fails the run
- The cooperative cancel flag is checked before every step (US-WF-09)

**Logic nodes** (visual builder, US-FUT-07): AND/OR/NOT/XOR/Compare/IfElse
nodes compile to Lua expressions over `results[...]` in
`list_state::build_workflow_definition`; inputs must reference earlier steps
(linear DAG order) and arity is enforced (XOR=2, NOT=1).

**Result Structure**:
```rust
pub struct WorkflowResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub steps_completed: usize,
}
```

### 9. Service Module (`src/service.rs`) — US-DEP-04

**Responsibility**: init-system integration without external dependencies.

- `systemd_user_unit()`: generates a systemd **user** unit running
  `tui-op-hub --headless` (scheduler + REST API, no TUI), `Restart=on-failure`.
  The unit never embeds `TUI_OP_HUB_SECRETS_KEY`; it is provided via
  `systemctl --user set-environment` or `EnvironmentFile=`.
- `install_service()` / `uninstall_service()`: write/remove
  `~/.config/systemd/user/tui-op-hub.service` and best-effort
  `systemctl --user daemon-reload` + `enable --now`.
- `cron_line()`: a cron watchdog line for cron-only systems.

**CLI flags** (parsed in `main.rs`, no clap): `--help`, `--headless`,
`--print-unit`, `--install-service`, `--uninstall-service`.

### 10. Scheduler (`src/scheduler.rs`) — US-WF-07

**Responsibility**: execute cron-scheduled workflows.

- `validate_cron(expr) -> (normalized, Schedule)`: accepts classic 5-field
  crontab syntax (normalized by prepending a `0` seconds field), 6/7-field
  `cron`-crate syntax and `@daily`-style aliases.
- `WorkflowScheduler`: daemon started from `main.rs`; loads enabled tasks from
  the `scheduled_tasks` table, executes due workflows and records run history.
- API: `POST /workflows/{id}/schedule`, `GET /schedules`, `DELETE /schedules/{id}`.
- TUI: Workflows tab → `s` opens a cron input popup on the selected workflow.

## Data Flow Examples

### Create Secret (TUI)
1. User presses `n` on Secrets tab → Mode::CreateSecret
2. User fills in form (name, value)
3. User presses Enter → `handle_secret_form`
4. `secrets::encrypt_for_user(pool, "default", value)` → encrypted string
5. `repository::create_secret(pool, "default", name, encrypted)` → DB insert
6. Refresh Secrets tab → `list_secrets(pool, "default")` → load all user secrets
7. TUI renders updated list

### Execute Workflow (TUI)
1. User navigates to Workflows tab, selects workflow, presses `r`
2. `workflow::execute_workflow_by_id(arc_pool, workflow_id, None)` spawned in blocking task
3. Engine fetches workflow entity, loads manifest, creates Lua context
4. Lua script calls Host functions (run_command, query_entity, etc.)
5. Engine returns WorkflowResult
6. `repository::insert_workflow_run(pool, &run)` persists execution record
7. `list_workflow_runs_by_workflow_id(pool, workflow_id)` reloaded for detail view
8. TUI shows "✓ Workflow finished" with run history updated

### Create Secret (API)
1. POST /secrets with `{"name": "api_key", "value": "secret123"}`
2. `create_secret_handler` receives request
3. `secrets::encrypt_for_user(pool, "default", "secret123")`
4. `repository::create_secret(pool, "default", "api_key", encrypted)`
5. Return `Secret { id, user_id, name, value_enc, created_at, updated_at }`

## Threading & Async Model

- **Async Runtime**: tokio with `#[tokio::main]` on main.rs
- **API Server**: axum on separate thread (async), spawned before TUI
- **TUI**: crossterm event loop (sync), uses `tokio::task::spawn_blocking` for long operations
- **Lua Engine**: mlua context created per execution (not Send), wrapped in Arc for blocking task
- **Database**: SqlitePool with concurrent connections, WAL mode for write concurrency

## Error Handling

**Error Types**:
- `AppError`: Custom error enum with variants (Database, NotFound, Conflict, etc.)
- `anyhow::Result`: Used in workflow and config modules
- HTTP handlers return `Result<Json<T>, String>` for JSON error responses

**Pattern**:
```rust
pub fn database_operation() -> AppResult<T> {
    sqlx::query(...).execute(pool).await?  // ? converts sqlx::Error to AppError
    Ok(result)
}
```

## Security Considerations

1. **Secrets Storage**: Never stored in plaintext, always encrypted before DB insert
2. **User Keys**: Stored securely in user_keys table or environment variable
3. **Nonce Management**: Random 24-byte nonce generated per encryption (semantic security)
4. **AEAD Properties**: XChaCha20Poly1305 provides both confidentiality and authenticity
5. **SQL Injection**: Prevented by parameterized queries with sqlx
6. **No Hardcoded Keys**: Keys loaded from environment or database

## Performance Optimizations

1. **SQLite WAL Mode**: Concurrent reads while write is happening
2. **Full-Text Search**: FTS5 virtual table for fast text search
3. **Connection Pool**: Arc<SqlitePool> reused across handlers
4. **Lazy Loading**: Data refreshed on tab switch, not continuously
5. **LTO & Optimization**: Release build with opt-level=3, LTO enabled
6. **Reduced Binary Size**: Strip symbols in release build

## Future Extensibility

### Phase 2 (Plugins)
- Plugin manifests stored in database
- Approval workflow in plugin_approvals table
- Enable/disable plugins via repository functions

### Phase 2 (SSH)
- SSH host configurations stored in database
- Integration with ssh-agent for key management
- SSH command execution via Host function in workflow

### Phase 2 (Scheduler)
- Cron expressions parsed and stored
- Background task executor checking next_run times
- Workflow execution on schedule
- last_run and next_run timestamp tracking

## Testing Strategy

**Unit Tests**: Located in each module (shell_exports, environment_parsing, simple_lua_script)

**Integration Points**:
- Database migrations validate schema
- Workflow execution tests Lua integration
- Repository functions tested with in-memory pool

**Manual Testing**: TUI interactive testing with sample data

## Deployment Considerations

- **Single Binary**: No external dependencies after build
- **Database**: SQLite file-based, portable across systems
- **Configuration**: Supports XDG standard locations
- **API Security**: Bind to localhost by default, requires reverse proxy for production
- **Key Management**: Environment variable or database for key storage (consider KMS in production)

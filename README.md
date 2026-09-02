# TUI-OP-HUB

A terminal-based operations hub for command management, workflow automation, and secure secret storage with multi-user encryption support.

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
- **TUI Dashboard**: Interactive terminal UI with 7 tabs (Dashboard, Commands, Projects, Tags, Search, Workflows, Secrets)

### Phase 2 🔄 Foundation Ready

- Plugin architecture (models, schema, DB functions, approval workflow structure)
- SSH host manager (models, schema, DB functions)
- Task scheduler with cron support (models, schema, DB functions)

## Quick Start

### Build
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

## Configuration

Configuration lives in a **Hyprland-style** config file — `section { key = value }`
blocks, `#` comments, quoted or bare values — at
`~/.config/tui-op-hub/config.conf`. Missing keys fall back to defaults and unknown
keys are ignored, so a minimal file is fine. Everything is also editable live in the
**Settings screen** (`6`), then `Ctrl+S` to persist:

```ini
# ~/.config/tui-op-hub/config.conf
# Format: section { key = value } — like Hyprland/sway configs

general {
    # External editor for the "open in editor" action (o on the Commands tab).
    # Empty = use $EDITOR, falling back to "vi".
    editor = ""
    user = default
}

database {
    path = tuihub.db
    busy_timeout_ms = 5000
}

api {
    bind_addr = 127.0.0.1:3001
}

tui {
    enabled = true
    page_size = 15
}

theme {
    # Preset: dark | light | nord | dracula | gruvbox
    # Any other name = your own custom theme: set the colors below.
    name = dark
    fg = white
    bg = black
    accent = yellow
    status_bg = blue
}

keybindings {
    quit = q
    help = ?
    search = /
    filter = f
    create = n
    edit = e
    delete = d
    copy = c
    run = r
}
```

### Create your own theme

Give `name` any value that is not a preset and set the colors you want (named colors
or hex `#rrggbb`). Unset colors fall back to the default palette, and you can also
override individual colors *on top of* a preset:

```ini
theme {
    name = mynight   # custom — anything not in the preset list
    fg = #e6edf3
    bg = #0d1117
    accent = #f78166
    primary = #58a6ff      # optional overrides
    success = #3fb950
    warning = #d29922
    error = #f85149
    border = #30363d
    highlight = #58a6ff
}
```

You can even override a single color of a preset — keep `name = nord` and add
`primary = #89dceb` to tint just the primary color.

### Settings screen (key `6`)
| Key | Action |
|:----|:-------|
| ↑/↓ | Navigate setting rows |
| Enter | Edit value (editor / page size) or start key rebind capture |
| ←/→ | Cycle theme preset (applied live) |
| a | **Advanced mode** — visual theme color editor + system options |
| Ctrl+S | Save settings to `config.conf` |
| Esc | Back to dashboard |

### Advanced mode (press `a` in Settings)
A visual config editor with live preview:

| Section | Rows |
|:---|:---|
| Theme colors | `fg`, `bg`, `accent`, `status_bg`, `primary`, `secondary`, `success`, `warning`, `error`, `border`, `highlight` — each row shows a **live color swatch** |
| System | Database path, API bind address, DB busy timeout |

| Key | Action |
|:----|:-------|
| ←/→ | **Cycle the row's color** through the palette (applied live) |
| Enter | Type an exact value (named color or hex `#rrggbb`, paths, numbers) |
| Backspace | Reset: optional colors back to `(preset)`, others to defaults |
| Ctrl+S | Save everything to `config.conf` |
| Esc | Back to Settings |

## Security

**Encryption Model**:
- Secrets encrypted with XChaCha20Poly1305 AEAD cipher
- Per-user 32-byte keys stored in `user_keys` table
- Random 24-byte nonce per message (semantic security)
- Plaintext never stored in database

**Key Management**:
- Generate key: `openssl rand -base64 32` (outputs 44-char base64)
- Set via: `TUI_OP_HUB_SECRETS_KEY` environment variable
- Or store in database via `user_keys` table with `user_id`

## TUI Keybindings

| Key | Action |
|:----|:-------|
| Tab/Left/Right | Switch tabs |
| Up/Down | Navigate items |
| Enter | Select/Detail view |
| ? | Help |
| q | Quit |
| n | Create new |
| e | Edit (detail view) |
| d | Delete |
| / | Search |
| f | Filter by tags |
| c | Copy content |
| o | Open command content in the configured external editor (Commands tab) |
| r | Run (commands/workflows) |
| v | View secret (Secrets tab) |
| Esc | Cancel |

## API Endpoints

### Secrets (User-Specific)
- `GET /secrets` - List user secrets
- `POST /secrets` - Create encrypted secret
- `GET /secrets/{id}` - Get secret (decrypted)
- `PUT /secrets/{id}` - Update secret
- `DELETE /secrets/{id}` - Delete secret

### Workflows
- `GET /workflows` - List workflows
- `POST /workflows/{id}/execute` - Execute workflow

### Entities
- `GET /entities` - List entities
- `POST /entities` - Create entity  
- `GET /entities/{id}` - Get entity
- `PUT /entities/{id}` - Update entity
- `DELETE /entities/{id}` - Delete entity

### Other
- `GET /projects` - List projects
- `GET /tags` - List tags
- `GET /types` - List entity types

## Architecture

**Core Modules**:
- **config**: Configuration management (`config.conf`, Hyprland-style)
- **db**: SQLite schema with migrations, WAL mode
- **models**: Data structures with sqlx derives
- **repository**: Data access layer with async operations
- **api**: Axum HTTP routes and handlers
- **tui**: Ratatui terminal UI with 7 tabs
- **secrets**: XChaCha20Poly1305 encryption/decryption
- **workflow**: Lua-based engine with Host functions

**Database**:
- SQLite with WAL mode for concurrent access
- 12+ tables: entities, projects, tags, types, workflow_runs, secrets, user_profiles, user_keys, plugins, ssh_hosts, scheduled_tasks
- Full-text search with FTS5 virtual table
- Foreign key constraints with ON CASCADE DELETE

## Development

### Tests
```bash
cargo test
```

### Format
```bash
cargo fmt --all
```

### Clippy
```bash
cargo clippy --all-targets -- -D warnings
```

## Build Profile

**Release Build** (recommended):
```bash
cargo build --release
```

Optimizations applied:
- opt-level = 3 (maximum optimization)
- lto = true (link-time optimization)
- codegen-units = 1 (better optimization)
- strip = true (reduce binary size)

Result: ~6.1 MB single executable with full feature set

## Project Structure

```
TUI-OP-HUB/
├── src/
│   ├── main.rs              # Entry point
│   ├── lib.rs               # Library exports
│   ├── error.rs             # Error types
│   ├── config/              # Configuration
│   ├── db/                  # Database
│   ├── models/              # Data models  
│   ├── repository/          # Data access
│   ├── api/                 # HTTP API
│   ├── tui/                 # Terminal UI
│   ├── secrets/             # Encryption
│   ├── workflow/            # Lua engine
│   └── environment.rs       # Env vars
├── target/                  # Build output
└── Cargo.toml               # Dependencies
```

## Dependencies

Key stack:
- **Runtime**: tokio (async)
- **Web**: axum, tower-http
- **Database**: sqlx (SQLite)
- **TUI**: ratatui, crossterm
- **Scripting**: mlua (Lua 5.4)
- **Crypto**: chacha20poly1305, getrandom
- **Serialization**: serde, serde_json, serde_yaml

## Status

**Version**: 0.2.0  
**Phase 1**: ✅ Complete (workflows, secrets, user profiles)  
**Phase 2**: 🔄 Ready (plugins, SSH, scheduler foundation)

## License

See project documentation for license terms.

# Search
curl "http://127.0.0.1:3000/entities/search?q=list"

# Run a command via API
curl -X POST http://127.0.0.1:3000/entities/\{id\}/run

# Get project dashboard
curl http://127.0.0.1:3000/projects/\{id\}/dashboard
```

## Testing

```bash
cd TUI-OP-HUB
cargo test
```

## Project Layout

```
TUI-OP-HUB/
├── README.md                  # This file
├── bp.md                      # Business plan
├── STACK_AND_TOOLS_GUIDE.md   # Tech stack reference
├── USER_STORIES.md            # User stories and requirements
├── DB/                        # Database design (draw.io)
├── SKETCHES/                  # UI/UX sketches
└── TUI-OP-HUB/                # Rust application
    ├── Cargo.toml
    ├── RUST_COMMANDS.md       # Cargo cheat sheet
    └── src/
        ├── main.rs            # Entry point
        ├── lib.rs             # Module declarations
        ├── config/mod.rs      # Configuration (themes, keybindings)
        ├── db/mod.rs          # SQLite pool + migrations
        ├── db/migrations/     # SQL migrations
        ├── models/mod.rs      # Data models
        ├── repository/mod.rs  # Data access layer
        ├── api/mod.rs         # REST API (axum)
        ├── tui/mod.rs         # Terminal UI (ratatui)
        └── error.rs           # Error types
```

## Tech Stack

| Component | Technology |
|:----------|:-----------|
| Language | Rust (edition 2021) |
| Async rsleep 3 && wc -l /mnt/data/Fabian/Development/Projects/TUI-OP-HUB/README.md && head -3 /mnt/data/Fabian/Development/Projects/TUI-OP-HUB/README.mduntime | Tokio |
| Database | SQLite (via sqlx, WAL mode) |
| API | Axum |
| TUI | Ratatui + Crossterm |
| Clipboard | arboard |
| Config | Hyprland-style `config.conf` (custom parser) |
| Logging | tracing + tracing-subscriber |
| Errors | thiserror + anyhow |

## License

MIT OR Apache-2.0

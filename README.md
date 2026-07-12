# TUI-OP-HUB

A terminal user interface (TUI) operations hub, built in Rust.

## Overview

TUI-OP-HUB is a keyboard-driven terminal application that centralizes operational
workflows — commands, scripts, projects, tags, search — into a single, ergonomic
interface. It is designed for operators, developers, and power users who live in
the terminal and want a unified cockpit for their day-to-day tasks.

## Features

- **Dashboard** — At-a-glance overview of commands, projects, tags, and types
- **Commands** — Store, edit, delete, search, filter, run, and copy commands/scripts
- **Projects** — Group related resources, switch between projects, project dashboards
- **Tags** — Organize and filter entities by tags
- **Full-text search** — FTS5-powered search with relevance ranking
- **Configurable** — Themes and keybindings via TOML config
- **REST API** — Full CRUD API with workflow execution endpoint
- **Headless mode** — Run backend-only without TUI

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain, 1.70+)
- Cargo (ships with Rust)
- A Unix-like terminal (Linux, macOS, BSD)

## Quick Start

```bash
# Clone the repository
git clone <repo-url>
cd TUI-OP-HUB

# Enter the Rust project directory
cd TUI-OP-HUB

# Build and run (debug mode)
cargo run

# Or build first, then run
cargo build
./target/debug/tui-op-hub
```

## Build

### Debug build

```bash
cd TUI-OP-HUB
cargo build
```

### Release build (optimized)

```bash
cd TUI-OP-HUB
cargo build --release
# Binary at: ./target/release/tui-op-hub
```

The release build is a single optimized binary with LTO, stripped symbols,
and opt-level=3.

## Run

### Interactive TUI mode (default)

```bash
cd TUI-OP-HUB
cargo run
# or after building:
./target/debug/tui-op-hub
```

### Headless mode (backend + API only, no TUI)

Set `tui.enabled = false` in your config file (see Configuration below).

In headless mode, the API server runs and the process waits for Ctrl+C.

## Configuration

TUI-OP-HUB reads its configuration from a TOML file. The default location is:

```
$XDG_CONFIG_HOME/tui-op-hub/config.toml
# or
~/.config/tui-op-hub/config.toml
```

If no config file exists, sensible defaults are used.

### Example config.toml

```toml
[database]
path = "tuihub.db"
busy_timeout_ms = 5000

[api]
bind_addr = "127.0.0.1:3000"

[tui]
enabled = true

[theme]
name = "dark"
fg = "white"
bg = "black"
accent = "yellow"
status_bg = "blue"

[keybindings]
quit = "q"
help = "?"
search = "/"
filter = "f"
create = "n"
edit = "e"
delete = "d"
copy = "c"
run = "r"
```

### Theme colors

Supported color names: white, black, red, green, yellow, blue,
magenta, cyan, gray, darkgray.

### Keybinding keys

Single characters (e.g. "q", "n") or special key names: tab, enter,
esc, up, down, left, right, backspace, space.

## TUI Keybindings

| Key | Action |
|:----|:-------|
| Tab / Left / Right | Switch tabs |
| Up / Down | Navigate list items |
| Enter | Select item / Switch project |
| ? | Toggle help overlay |
| q | Quit (in Normal mode) |
| n | Create new entity/project |
| e | Edit entity (in detail view) |
| d | Delete entity/project (with confirmation) |
| / | Start search (on Search tab) |
| f | Filter by tags |
| c | Copy content to clipboard (in detail view) |
| r | Run command/script (in detail view) |
| Esc | Cancel / Back |

## Tabs

1. **Dashboard** — Stats overview and quick actions
2. **Commands** — List, create, filter, and manage commands/scripts
3. **Projects** — List, create, switch, and delete projects
4. **Tags** — Browse all tags
5. **Search** — Full-text search across all entities

## API

The backend exposes a REST API (default: 127.0.0.1:0 = random port, check logs).

### Endpoints

| Method | Path | Description |
|:-------|:-----|:------------|
| GET | /health | Health check |
| POST | /entities | Create entity |
| GET | /entities | List entities (query: type_id, project_id) |
| GET | /entities/search?q=... | Full-text search |
| GET | /entities/filter-by-tags?tags=a,b | Filter by tags |
| GET | /entities/{id} | Get entity |
| PUT | /entities/{id} | Update entity |
| DELETE | /entities/{id} | Delete entity |
| GET | /entities/{id}/tags | Get entity's tags |
| POST | /entities/{id}/run | Execute entity content |
| POST | /projects | Create project |
| GET | /projects | List projects |
| GET | /projects/{id} | Get project |
| DELETE | /projects/{id} | Delete project |
| GET | /projects/{id}/entities | List project's entities |
| GET | /projects/{id}/dashboard | Project dashboard |
| GET | /tags | List all tags |
| GET | /types | List all entity types |

### Example API calls

```bash
# Health check
curl http://127.0.0.1:3000/health

# Create a command
curl -X POST http://127.0.0.1:3000/entities \
  -H "Content-Type: application/json" \
  -d '{"name":"list files","description":"ls -la","content":"ls -la","type_id":"cmd"}'

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
| Config | TOML (serde + toml) |
| Logging | tracing + tracing-subscriber |
| Errors | thiserror + anyhow |

## License

MIT OR Apache-2.0

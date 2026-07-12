# TUI-OP-HUB

A terminal user interface (TUI) operations hub, built in Rust.

## Overview

TUI-OP-HUB is a keyboard-driven terminal application that centralizes operational
workflows (databases, services, scripts, monitoring) into a single, ergonomic
interface. It is designed for operators, developers, and power users who live in
the terminal and want a unified cockpit for their day-to-day tasks.

## Project Layout

```
TUI-OP-HUB/
├── bp.md                  # Business / project plan
├── STACK_AND_TOOLS_GUIDE.md
├── USER_STORIES.md
├── DB/                    # Database design artifacts (draw.io)
├── SKETCHES/              # UI / UX sketches (SVG, Figma exports)
└── TUI-OP-HUB/            # Rust workspace / application source
    ├── Cargo.toml
    ├── RUST_COMMANDS.md
    └── src/
        └── main.rs
```

## Tech Stack

- **Language:** Rust
- **Build system:** Cargo
- **UI:** TUI (see `STACK_AND_TOOLS_GUIDE.md` for chosen crates)

See `STACK_AND_TOOLS_GUIDE.md` for the full list of tools, libraries, and
conventions used in this project.

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Cargo (ships with Rust)

### Build & Run

```bash
cd TUI-OP-HUB
cargo run
```

### Build (release)

```bash
cd TUI-OP-HUB
cargo build --release
```

### Run tests

```bash
cd TUI-OP-HUB
cargo test
```

## Documentation

- `bp.md` — Business plan and project goals
- `STACK_AND_TOOLS_GUIDE.md` — Chosen stack, libraries, and tooling
- `USER_STORIES.md` — User stories and requirements
- `TUI-OP-HUB/RUST_COMMANDS.md` — Useful Rust / Cargo commands cheat sheet

## License

TBD
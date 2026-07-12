# Rust Commands for `TUI-OP-HUB`

Quick command reference for setup, development, and database work.

## 1) Toolchain Setup

```bash
rustup update
rustup component add rustfmt clippy
rustc --version
cargo --version
```

## 2) Build and Run

```bash
cd /home/gerard/Documents/Fabian/Development/Projects/TUI-OP-HUB/TUI-OP-HUB
cargo build
cargo run
```

## 3) Daily Quality Checks

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## 4) Add Useful Dependencies

```bash
cargo add serde --features derive
cargo add serde_json
cargo add anyhow
cargo add thiserror
cargo add tokio --features full
cargo add sqlx --features runtime-tokio-rustls,sqlite,macros
cargo add ratatui
cargo add crossterm
cargo add mlua --features lua54,vendored
```

## 5) SQLx Migrations (Flyway-like)

```bash
cargo install sqlx-cli --no-default-features --features rustls,sqlite
export DATABASE_URL=sqlite://tuihub.db
sqlx database create
sqlx migrate add init
sqlx migrate run
```

## 6) Fast Repeat Loop

Use this loop while coding:

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test && cargo run
```

## 7) Rust Learning Sources (Trusted)

### Official Core Learning

- The Rust Book: <https://doc.rust-lang.org/book/>
- Rust by Example: <https://doc.rust-lang.org/rust-by-example/>
- Rust Standard Library docs: <https://doc.rust-lang.org/std/>
- Cargo Book: <https://doc.rust-lang.org/cargo/>

### Practice and Exercises

- Rustlings (hands-on exercises): <https://github.com/rust-lang/rustlings>
- Comprehensive Rust (Google): <https://google.github.io/comprehensive-rust/>

### Sources for This Project Stack

- SQLx docs: <https://docs.rs/sqlx/latest/sqlx/>
- SQLx CLI (migrations): <https://github.com/launchbadge/sqlx/tree/main/sqlx-cli>
- Tokio tutorial: <https://tokio.rs/tokio/tutorial>
- Axum docs: <https://docs.rs/axum/latest/axum/>
- Ratatui docs: <https://ratatui.rs/>
- mlua docs: <https://docs.rs/mlua/latest/mlua/>

### Architecture and Patterns

- Rust Design Patterns: <https://rust-unofficial.github.io/patterns/>
- Zero To Production in Rust: <https://www.zero2prod.com/>

### How to Use These Sources Efficiently

1. Read one topic in the Rust Book.
2. Do the matching Rustlings exercise.
3. Implement that concept in this project.
4. Check crate docs only when you need an API.

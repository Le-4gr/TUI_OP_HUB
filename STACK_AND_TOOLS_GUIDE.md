# TUI-OP-HUB Stack and Tools Guide

This document recommends a practical stack for your project and explains how to use each part.

---

## 1) Recommended Language Layers

## Layer 1 (Core): Rust

Use Rust as the single source of truth for:

- process execution/orchestration
- database access
- secrets and security boundaries
- workflow engine and scheduling
- API/IPC surface used by all clients

Why:

- strong safety guarantees
- great performance for system-level tasks
- predictable concurrency and fewer runtime surprises

How to use in this project:

- keep all privileged operations in Rust only
- expose a narrow internal API to Lua/Python layers
- organize by domain (`core`, `db`, `workflow`, `integrations`, `api`)

---

## Layer 2 (Automation): Lua

Use Lua for:

- user scripts and workflow steps
- lightweight plugin behavior
- fast customization without recompiling Rust

Why:

- small, fast, easy to embed
- great for scriptability in terminal tools
- lower complexity than embedding Python in-process

How to use in this project:

- embed with `mlua`
- expose only safe host functions (example: `run_task`, `query_entity`, `emit_event`)
- pass structured data in JSON-like tables
- enforce capability flags per script/plugin

---

## Layer 3 (Optional Integrations): Python

Use Python only when needed for:

- external API integrations
- data processing utilities
- optional AI/ML helpers

Why:

- huge ecosystem
- fast prototyping for non-core features

How to use in this project:

- run Python as subprocess/RPC worker (out-of-process)
- never give Python direct DB write access by default
- route privileged actions through Rust API

---

## 2) Database and Data Access

## Primary DB: SQLite (local-first)

Why:

- perfect for desktop/TUI local workflows
- easy backups and portability
- no server required

How:

- use one database file per context (`personal.db`, `work.db`)
- enable WAL mode for better read concurrency
- keep schema migrations in versioned files

Recommended Rust tooling:

- `sqlx` for async DB + compile-time checked queries
- `sqlx-cli` for migrations

Suggested PRAGMAs:

- `journal_mode=WAL`
- `foreign_keys=ON`
- `synchronous=NORMAL`
- `busy_timeout=5000`

### Flyway-equivalent in Rust

Use `sqlx migrate` as the Flyway-style migration workflow:

- create migration files in `migrations/`
- run them in CI and app startup
- never mutate schema ad-hoc in runtime code

---

## 3) API, App Structure, and “Spring-like” Mapping

If you think in Spring Boot terms, this mapping works well:

- DataSource -> `SqlitePool` / `PgPool`
- Entity -> Rust `struct` + `FromRow`
- Repository -> Rust module with query methods
- Service -> domain logic layer
- Controller -> `axum` handlers
- Flyway -> `sqlx migrate`

Recommended server stack:

- `axum` (HTTP/API)
- `tokio` (async runtime)
- `serde` (JSON serialization)
- `tracing` + `tracing-subscriber` (logging)

---

## 4) Plugin and Script Safety Model

Design principle:

- Rust core is trusted
- Lua/Python are untrusted by default

Use capabilities:

- `read_entities`
- `run_whitelisted_commands`
- `network_access`
- `secret_access` (off by default)

Implementation notes:

- plugin manifest declares required capabilities
- user approves capabilities per plugin
- audit-log sensitive calls (`who`, `what`, `when`)

---

## 5) Search and Metadata Strategy

Use a normalized model + flexible metadata:

- normalized core tables for critical fields and relations
- `metadata_json` for plugin-specific extras
- FTS5 for full-text search on command/script content

Why:

- keeps core queries fast and stable
- still allows extensibility for future plugins

---

## 6) Recommended First Milestones

1. Initialize Rust workspace and module boundaries.
2. Add SQLite + `sqlx` and first migration.
3. Implement base entities/tags/types schema.
4. Add `axum` API and health + entity endpoints.
5. Embed Lua (`mlua`) with 2-3 safe host functions.
6. Add workflow run logging and search.

---

## 7) Tooling Checklist

Core build/dev:

- Rust stable toolchain (`rustup`)
- `cargo`
- `sqlx-cli`
- `just` (optional command runner)

Quality:

- `cargo fmt`
- `cargo clippy`
- `cargo test`

Optional DX:

- `watchexec` for auto-reload workflows
- `bacon` for fast Rust feedback loops

---

## 8) What to Avoid Early

- too many languages in v1 core
- direct plugin writes to sensitive tables
- premature microservice split
- over-generic schema without clear use-cases

Keep v1 simple:

- Rust core + SQLite + Lua scripts
- Python only where it clearly adds value

---

## 9) Final Recommendation (Short)

Start with:

- Rust core
- SQLite + `sqlx` migrations
- Lua scripting via `mlua`
- `axum` API for TUI/web/automation clients

Add Python later as an out-of-process integration layer.

This gives you the best mix of performance, safety, extensibility, and long-term maintainability.

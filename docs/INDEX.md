# 📚 TUI-OP-HUB — Documentation Index

> Start here. Every document in the repo, what it is for, and when to read it.

## 🚀 For users

| Document | What it covers |
|:---|:---|
| [`README.md`](../README.md) | Features, install/build, **configuration reference** (Hyprland-style `config.conf`), theming (8 presets incl. Catppuccin), keybindings, full API endpoint list, dev mode |
| [`docs/GUIDE.md`](GUIDE.md) | **User guide — everything the program can do and how to do it**: install (script/service/update/release), the service+TUI model, Knowledge tab (commands/scripts/apps/chains/options), jobs panel, workflows & visual builder, secrets, configs, projects, plugins, themes, import/export, headless plugins, REST API, troubleshooting |
| [`docs/IMPORT_EXPORT.md`](IMPORT_EXPORT.md) | **Knowledge import/export**: bundle JSON schema, the copy-paste LLM prompt for generating commands/scripts/apps, duplicate strategies (skip/overwrite/rename), file pickers, cron scheduling, headless setup |
| [`docs/PLUGINS.md`](PLUGINS.md) | **Plugin/mod API**: Lua mods with manifests + capabilities, event hooks (`project_created` for git automation), **UI actions** (`[[actions]]` → buttons on the Projects tab), **project scaffold templates** (`templates/*.toml` with preview/customize), **headless trust workflow**, TUI management, security model |

## 🤖 For AI agents & contributors

| Document | What it covers |
|:---|:---|
| [`docs/development/AGENTS.md`](development/AGENTS.md) | **Entry point for AI coding agents**: repo layout, conventions, architecture rules, test workflow (unit + BDD), git commit workflow, do/don't list |
| [`docs/development/WORK.md`](development/WORK.md) | Live work log — sessions 1-35, the story backlog at the top. Update it at the end of every session |
| [`docs/reference/ARCHITECTURE.md`](reference/ARCHITECTURE.md) | Module responsibilities, data flow, threading/async model, security notes |
| [`docs/reference/STACK_AND_TOOLS_GUIDE.md`](reference/STACK_AND_TOOLS_GUIDE.md) | Why Rust/Lua/SQLite — the rationale for every layer |
| [`docs/reference/MODERN_TUI_GUIDE.md`](reference/MODERN_TUI_GUIDE.md) | TUI design guide: color palette, login/dashboard layout |
| [`docs/reference/USER_STORIES.md`](reference/USER_STORIES.md) | The spec: `US-XXX-NN` stories with priorities and phase status |
| [`TUI-OP-HUB/RUST_COMMANDS.md`](../TUI-OP-HUB/RUST_COMMANDS.md) | Cargo cheat sheet |

## 🏢 Business

| Document | What it covers |
|:---|:---|
| [`docs/business/VISION.md`](business/VISION.md) | Product vision, target users, problem statement |
| [`docs/business/ROADMAP.md`](business/ROADMAP.md) | Phase plan from MVP to the "system control center" |
| [`docs/business/GOVERNANCE.md`](business/GOVERNANCE.md) | Roles, decision process, release & quality rules |

**Source of truth for scope**: [`docs/business/bp.md`](business/bp.md) — the original business plan.
The business docs above summarize it; `bp.md` wins in case of conflict.

## 🗂️ Other assets

- [`docs/design/DB_LOGICAL.drawio`](design/DB_LOGICAL.drawio) — database design diagram (draw.io)
- [`docs/design/FIGMA/`](design/FIGMA/) — UI/UX sketches (Figma exports)
- [`TUI-OP-HUB/tests/bdd_scenarios.rs`](../TUI-OP-HUB/tests/bdd_scenarios.rs) — BDD integration tests (48 scenarios)

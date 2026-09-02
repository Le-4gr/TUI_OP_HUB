# Roadmap — TUI-OP-HUB

> Authoritative story list with priorities: [`USER_STORIES.md`](../../USER_STORIES.md)

## ✅ Phase 1 — Core hub (v0.2.0, shipped)
Entity CRUD (commands/scripts/apps), projects, tags, FTS5 search, Lua workflows with run
history, encrypted secrets with per-user keys, user profiles + Argon2 auth, REST API,
modern TUI (login + 7 tabs), **Settings screen** (editor, page size, theme presets,
custom themes, live keybinding rebinds), Hyprland-style config file.

## 🔄 Phase 2 — Foundations ready, integration pending
Models/schema/DB functions exist; TUI + API integration pending:

| Area | Stories | Status |
|:---|:---|:---|
| Scheduler daemon (cron workflows) | US-WF-07 | backend ready, wiring pending |
| Plugin approval UI + enforcement | US-PLG-05/06, 07/10 | foundation ready |
| SSH host manager TUI + quick-connect | US-SSH-01..06 | foundation ready |
| Workflow stop/cancel | US-WF-09 | not started |
| Systemd integration | US-DEP-04 | not started |

## 🟢 Phase 3+ — Planned (future)
- Process monitor TUI (sysinfo backend exists) — US-PROC-01..07
- Package manager integration (apt/pacman/nix) — US-PKG-01..09
- Environment/venv & config-file management — US-ENV-01..08, US-CFG-01..08
- Multi-database contexts — US-DB-01..06
- Web UI — US-WEB-01..04 · Cross-machine sync — US-SYNC-01..04 · Backup/export — US-BAK-01..04

## Release cadence
Patch releases for fixes; minor versions per completed phase milestone; the
[`WORK.md`](../../WORK.md) log doubles as the changelog source.

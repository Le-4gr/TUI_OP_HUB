# Roadmap — TUI-OP-HUB

> Authoritative story list with priorities: [`USER_STORIES.md`](../reference/USER_STORIES.md)
> Live status: [`WORK.md`](../development/WORK.md)

## ✅ Phase 1 — Core hub (v0.2.0, shipped)
Entity CRUD (commands/scripts/apps), projects, tags, FTS5 search, Lua workflows with run
history, encrypted secrets with per-user keys, user profiles + Argon2 auth, REST API,
modern TUI (login + tabs), **Settings screen** (editor, page size, theme presets,
custom themes, live keybinding rebinds), Hyprland-style config file.

## ✅ Phase 2 — Integration complete (shipped)
Scheduler daemon (cron workflows, US-WF-07), plugin approval UI + enforcement
(US-PLG-05/06/07/10), SSH host manager panel (US-SSH-01..03/06), workflow
stop/cancel (US-WF-09), systemd/cron integration (US-DEP-04), secrets v2
(groups, passphrase layer, ssh-agent), config management foundation, keygen.

## ✅ Phase 3 — TUI expansion (shipped)
- **Navigation restructure** (US-TUI-11/12): Knowledge Base parent tab, digits
  1-9/0 from any screen, configurable tab order
- **Config management page** (US-CFG-09..12): register form with file browser +
  metadata, multi-target deploy (symlink/hard link/copy), update + drift, git
- **Plugin UI actions** (US-PLG-13) and **project scaffold templates**
  (US-PLG-14/15, US-PROJ-08) with preview/customize
- **Logic gates** (US-FUT-07): AND/OR/NOT/XOR/compare/if-else nodes, `results`
  flow, `run_when` gates
- **SSH host tags + connection test** (US-SSH-04/05); project folder options
  (delete-with-folder, merge-into-existing)

## 🟢 Phase 4+ — Planned (future)
- Plugin approval workflow for headless service runs (leftover)
- Process monitor (sysinfo backend exists) — US-PROC-01..07 (deliberately parked:
  prefer btop/htop)
- Package manager integration (apt/pacman/nix) — US-PKG-01..09
- Environment/venv deep-dive — US-ENV-01..08
- Multi-database contexts — US-DB-01..06
- Web UI — US-WEB-01..04 · Cross-machine sync — US-SYNC-01..04 · Backup/export — US-BAK-01..04

## Release cadence
Patch releases for fixes; minor versions per completed phase milestone; the
[`WORK.md`](../development/WORK.md) log doubles as the changelog source.

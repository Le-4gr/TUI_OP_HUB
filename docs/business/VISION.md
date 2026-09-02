# Vision — TUI-OP-HUB

> Full product plan: [`bp.md`](../../bp.md) · Implementation status: [`USER_STORIES.md`](../../USER_STORIES.md)

## One-liner

**A local-first terminal control center**: every command, script, workflow, secret and
config a developer uses daily — searchable, taggable, automatable — in one keyboard-driven
Rust binary.

## Problem

Power users scatter their tooling across shell history, dotfiles, password managers,
cron tabs and wiki pages. Nothing is searchable across all of it, nothing is portable,
and automation requires gluing tools together by hand.

## Target users

| Persona | Need |
|:---|:---|
| **Developer / Power User** | Store and run commands/scripts without leaving the terminal; compose workflows from saved commands |
| **Sysadmin / Homelab operator** | SSH hosts, secrets, systemd-adjacent automation, per-project context |
| **Tinkerer** | Themes, keybindings, Lua scripting, plugins — make the hub *theirs* |

## Principles

1. **Local-first** — SQLite file, single static binary, no required network services
2. **Search-first** — FTS5 search, tags and projects over hierarchies
3. **Keyboard-first** — everything reachable without a mouse
4. **Honest secrets** — XChaCha20Poly1305 AEAD, per-user keys, plaintext never at rest
5. **Hackable** — Lua workflows, plugins, theming, Hyprland-style config

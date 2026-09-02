# 🚀 Release & Installation Guide

> How to make a release, install it, and get the **always-running background
> service** plus the **on-demand TUI** working together.

---

## 1. The model: service in the background, TUI on demand

After installation you have two entry points to the same hub:

| What | How it runs | Provides |
|:---|:---|:---|
| **Background service** | systemd user service, enabled at boot (`tui-op-hub --headless`) | Workflow scheduler (cron jobs) + REST API on `127.0.0.1:3001` |
| **TUI** | run `tui-op-hub` whenever you want | The full interactive interface (login, 8 tabs) |

Both processes share the same SQLite database (WAL mode supports multiple
readers/writers). If the service's API already owns port 3001, the TUI detects
it, logs `API port already in use ... skipping local API`, and continues —
you never end up with two API servers or a broken start.

## 2. Install (one command)

From the repository root:

```bash
./install.sh
```

This does, in order:

1. `cargo build --release`
2. Installs the binary to `~/.local/bin/tui-op-hub` (or `/usr/local/bin` when run as root)
3. Runs `tui-op-hub --install-service`, which:
   - generates a master secrets key into `~/.config/tui-op-hub/env` (mode 600)
     — **once**; re-running never overwrites an existing key
   - writes `~/.config/systemd/user/tui-op-hub.service`
   - `systemctl --user daemon-reload` + `enable --now` (best effort)

Skip the service with `./install.sh --no-service`.

## 3. Verify

```bash
# Background service
systemctl --user status tui-op-hub
journalctl --user -u tui-op-hub -f

# REST API (served by the service)
curl -s http://127.0.0.1:3001/health

# TUI (shares the database with the service)
tui-op-hub
```

## 4. Day-to-day

| Task | Command |
|:---|:---|
| Launch the TUI | `tui-op-hub` |
| Stop the background service | `systemctl --user stop tui-op-hub` |
| Start it again | `systemctl --user start tui-op-hub` |
| Follow scheduler/API logs | `journalctl --user -u tui-op-hub -f` |
| Update to a new release | `git pull && ./install.sh` (idempotent: keeps your key and data) |
| Remove everything | `tui-op-hub --uninstall-service` (keeps the key file and database) |

## 5. Secrets key

- The env file `~/.config/tui-op-hub/env` holds `TUI_OP_HUB_SECRETS_KEY`
  (base64, 32 bytes) and is referenced by the unit via `EnvironmentFile=`.
- Per-user keys stored in the database (`user_keys` table) take precedence
  over the env fallback; the env key is the safety net for the headless
  service and for fresh profiles.
- **Back the key up** (encrypted, somewhere safe). Without it, existing
  encrypted secrets cannot be decrypted.
- Never commit the env file (it is generated locally and should be gitignored).

## 6. Systems without systemd

Any init that can start a long-running process works. The only contract is:

```text
tui-op-hub --headless
```

- **cron** (watchdog style, restarts the daemon if it dies):
  `* * * * * pgrep -f 'tui-op-hub --headless' >/dev/null || tui-op-hub --headless`
- The unit file is still written by `--install-service` even without
  `systemctl`, ready to adapt for runit/s6/OpenRC/launchd.

Inspect what would be installed without installing: `tui-op-hub --print-unit`.

## 7. Making a release

1. Bump the version in `TUI-OP-HUB/Cargo.toml`.
2. `cargo build --release` — the artifact is `TUI-OP-HUB/target/release/tui-op-hub`
   (statically linked SQLite via sqlx; no runtime dependencies).
3. Ship the binary (or the repo). On a target machine: run `./install.sh`
   or drop the binary anywhere on `$PATH` and run `tui-op-hub --install-service`.

---

*Implementation: `TUI-OP-HUB/src/service/mod.rs`, `src/main.rs` (CLI flags),
`install.sh`. Related: [IMPORT_EXPORT.md](IMPORT_EXPORT.md) for cron scheduling
of workflows.*

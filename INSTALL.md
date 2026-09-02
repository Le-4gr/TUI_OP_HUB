# 📥 Installation

> Install TUI-OP-HUB as an **always-running background service** (workflow
> scheduler + REST API) and launch the **interactive TUI** whenever you want.
> Both talk to the same database.

---

## Requirements

- Linux (systemd optional — see §6 for cron/other inits) · macOS/Windows run the TUI but not the systemd parts
- [Rust](https://rustup.rs) 1.75+ (edition 2021) — `rustc --version`
- `openssl` development headers are **not** required (SQLite is bundled via sqlx)

## 1. Quick install (recommended)

```bash
git clone <this-repo> tui-op-hub
cd tui-op-hub
./install.sh
```

The script:

1. Builds the release binary (`cargo build --release`)
2. Installs it to `~/.local/bin/tui-op-hub` (`/usr/local/bin` if run as root)
3. Generates a **master secrets key** into `~/.config/tui-op-hub/env`
   (mode `600`, only if it does not exist yet — never overwritten)
4. Installs and **enables** the systemd user service so the hub always runs
   in the background, including after reboot

Skip the service with `./install.sh --no-service`.

## 2. Manual install

```bash
cargo build --release
install -m 755 TUI-OP-HUB/target/release/tui-op-hub ~/.local/bin/

# set up the background service (key file + unit + enable)
tui-op-hub --install-service

# or inspect first:
tui-op-hub --print-unit
```

## 3. Verify

```bash
# Background service (scheduler + REST API, port 127.0.0.1:3001)
systemctl --user status tui-op-hub
curl -s http://127.0.0.1:3001/health

# Logs
journalctl --user -u tui-op-hub -f

# Launch the TUI
tui-op-hub
```

## 4. Running the TUI

```bash
tui-op-hub
```

- The TUI and the background service **run side by side**: they share the same
  SQLite database (WAL). If the service already owns the API port, the TUI
  detects it and skips starting a second API server automatically.
- To run the TUI without any background service (e.g. `--no-service` install),
  just run `tui-op-hub` — it starts its own API for the session.
- First launch: create your user on the login screen (Argon2-hashed).

## 5. Secrets key

- Location: `~/.config/tui-op-hub/env` → `TUI_OP_HUB_SECRETS_KEY=<base64 32 bytes>`
- Referenced by the systemd unit via `EnvironmentFile=`
- **Back it up somewhere safe (encrypted).** Without it, existing encrypted
  secrets cannot be decrypted. Deleting it and re-running `--install-service`
  generates a *new* key.
- Per-user keys stored in the database (`user_keys` table) take precedence
  over this fallback.

## 6. Systems without systemd

The only contract is the long-running command:

```bash
tui-op-hub --headless   # scheduler + REST API, no TUI
```

- **cron** (watchdog, restarts on death):
  ```cron
  * * * * * pgrep -f 'tui-op-hub --headless' >/dev/null || tui-op-hub --headless
  ```
- The unit file is still written by `--install-service` even without
  `systemctl`, ready to adapt for runit / s6 / OpenRC / launchd.

## 7. Update

```bash
git pull
./install.sh
```

Idempotent: your database, config, and secrets key are preserved. Migrations
run automatically on startup.

## 8. Uninstall

```bash
tui-op-hub --uninstall-service   # disable + remove the unit (key file kept)
rm ~/.local/bin/tui-op-hub       # remove the binary
# data (kept unless you delete it): tuihub.db next to the working directory
# config: ~/.config/tui-op-hub/config.conf
```

## 9. Files & paths

| Path | What |
|:---|:---|
| `~/.local/bin/tui-op-hub` | binary (default install prefix) |
| `~/.config/tui-op-hub/config.conf` | Hyprland-style config (themes, keybindings, API bind, DB path) |
| `~/.config/tui-op-hub/env` | master secrets key (mode 600) |
| `~/.config/systemd/user/tui-op-hub.service` | the user unit |
| `./tuihub.db` | SQLite database (default, relative to working directory — set `database.path` in the config for a fixed location, e.g. `~/.local/share/tui-op-hub/tuihub.db`) |

---

Deeper guides: [docs/INSTALL.md](docs/INSTALL.md) (service internals, CLI
flags) · [docs/IMPORT_EXPORT.md](docs/IMPORT_EXPORT.md) (knowledge bundles,
cron workflows)

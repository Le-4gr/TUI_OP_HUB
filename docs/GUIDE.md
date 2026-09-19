# 📖 TUI-OP-HUB — User Guide

> What the program can do, and exactly how to do it. This merges the old
> install guides — everything about running and installing lives in §1–4.

---

## 1. Install

### Requirements

- Linux (systemd optional — see §6 for cron/other inits) · macOS/Windows run
  the TUI but not the systemd parts
- [Rust](https://rustup.rs) 1.75+ (edition 2021) — `rustc --version`
- `openssl` dev headers are **not** required (SQLite is bundled via sqlx)

### One-command install (recommended)

```bash
git clone <this-repo> tui-op-hub
cd tui-op-hub
./install.sh
```

The script, in order:

1. Builds the release binary (`cargo build --release`)
2. Installs it to `~/.local/bin/tui-op-hub` (`/usr/local/bin` when run as root)
3. Generates a **master secrets key** into `~/.config/tui-op-hub/env`
   (mode `600`, only if it does not exist yet — never overwritten)
4. Installs and **enables** the systemd user service so the hub always runs
   in the background, including after reboot

Skip the service with `./install.sh --no-service`.

### Manual install

```bash
cargo build --release
install -m 755 TUI-OP-HUB/target/release/tui-op-hub ~/.local/bin/
tui-op-hub --install-service   # key file + systemd unit + enable
# or inspect first: tui-op-hub --print-unit
```

### Update / uninstall

```bash
git pull && ./install.sh              # update (idempotent, keeps data + key)
tui-op-hub --uninstall-service        # remove the unit (key + data kept)
rm ~/.local/bin/tui-op-hub            # remove the binary
```

### Making a release

1. Bump the version in `TUI-OP-HUB/Cargo.toml`.
2. `cargo build --release` — the artifact is
   `TUI-OP-HUB/target/release/tui-op-hub` (statically linked SQLite via sqlx;
   no runtime dependencies).
3. Ship the binary (or the repo). On a target machine: run `./install.sh`,
   or drop the binary anywhere on `$PATH` and run
   `tui-op-hub --install-service`.

## 2. The model: service in the background, TUI on demand

After installation there are two entry points to the same hub:

| What | How it runs | Provides |
|:---|:---|:---|
| **Background service** | systemd user unit, enabled at boot (`tui-op-hub --headless`) | Workflow scheduler (cron jobs) + REST API on `127.0.0.1:3001` |
| **TUI** | run `tui-op-hub` whenever you want | The full interactive interface (login, 8 tabs) |

Both share the same SQLite database (WAL supports concurrent access). If the
service already owns the API port, the TUI detects it and skips starting a
second API server — you never get two API servers or a broken start.

### Verify the install

```bash
systemctl --user status tui-op-hub    # background service
curl -s http://127.0.0.1:3001/health  # REST API
journalctl --user -u tui-op-hub -f    # logs
tui-op-hub                            # launch the TUI
```

First launch shows the signup screen — the **first user becomes the admin**.
After login you land on the **Knowledge launcher** (search auto-focused — type
to filter, `Enter` runs; Dashboard is tab `1`).

### Day-to-day

| Task | Command |
|:---|:---|
| Launch the TUI | `tui-op-hub` |
| Stop / start the service | `systemctl --user stop\|start tui-op-hub` |
| Follow logs | `journalctl --user -u tui-op-hub -f` |
| Update | `git pull && ./install.sh` |
| Remove | `tui-op-hub --uninstall-service` (keeps key + data) |

### Secrets key

- `~/.config/tui-op-hub/env` holds `TUI_OP_HUB_SECRETS_KEY` (base64, 32
  bytes), referenced by the systemd unit via `EnvironmentFile=`.
- Per-user keys stored in the database (`user_keys` table) take precedence;
  the env key is the fallback for the headless service and fresh profiles.
- **Back it up (encrypted, somewhere safe).** Without it existing encrypted
  secrets cannot be decrypted.
- Never commit the env file.

### Files & paths

| Path | What |
|:---|:---|
| `~/.local/bin/tui-op-hub` | binary (default install prefix) |
| `~/.config/tui-op-hub/config.conf` | Hyprland-style config (themes, keybindings, API bind, DB path) |
| `~/.config/tui-op-hub/env` | master secrets key (mode 600) |
| `~/.config/systemd/user/tui-op-hub.service` | the user unit |
| `./tuihub.db` | SQLite database (set `database.path` for a fixed location, e.g. `~/.local/share/tui-op-hub/tuihub.db`) |
| `/tmp/tui-op-hub-logs/` | nohup job output logs |

### Systems without systemd

The only contract is the long-running command:

```bash
tui-op-hub --headless   # scheduler + REST API, no TUI
```

- **cron** (watchdog, restarts on death):
  `* * * * * pgrep -f 'tui-op-hub --headless' >/dev/null || tui-op-hub --headless`
- The unit file is still written by `--install-service` even without
  `systemctl`, ready to adapt for runit / s6 / OpenRC / launchd.
- Inspect without installing: `tui-op-hub --print-unit`.

---

## 3. First run: what you see

1. **Login/signup screen** — create your user (Argon2-hashed). The first user
   becomes the admin.
2. **Knowledge (launcher)** — after login you land on the Knowledge tab with the
   search bar **auto-focused**: type to filter across commands, scripts, apps,
   chains and workflows (FTS5); Enter runs the selection. This is the launcher
   screen — on minimal/headless machines the TUI is the whole desktop.
3. **Dashboard** — `1` from any screen: card layout with stats,
   CPU/memory/network/temperature
   mini-btop (auto-refresh 2s), quick launches (`g` lazygit, `d` lazydocker,
   `k` k9s, `n` lazynpm), `` ` `` opens a new terminal window.
4. **Eight tabs** — digits `1`–`9`/`0` switch from any screen:

| Tab | Contents |
|:---|:---|
| `1` Dashboard | stats + mini-btop + quick launches + jobs panel (`j`) |
| `2` Knowledge | **launcher (default screen)** — commands/scripts/apps, type filter, FTS5 search |
| `3` Projects | workspaces + register-dir + detail view + plugin actions |
| `4` Workflows | visual builder, cron scheduling, run history, cancel |
| `5` Secrets | encrypted secrets, SSH/GPG keygen, ssh-agent, SSH hosts (`H`) |
| `6` Configs | managed config files, deploy/update/git |
| `7` Plugins | approve/enable Lua mods, headless trust |
| `0` Settings | editor, page size, themes (8 presets + custom), keybindings |

Selected rows show a `❯` cursor marker. `?` opens the keybind helper on any
screen. `/` opens a visible search bar.

---

## 4. Knowledge tab (commands, scripts, apps, chains)

The heart of the hub: your personal command knowledge base.

### Create

- `f` cycles the type filter: All → Commands → Apps → Scripts → **Chains**
- `n` creates an item of the **filtered type** (form: Name, Type, Description,
  Content, Tags)
- **Command chains** (`chain` type): pipe/semicolon one-liners exactly as you'd
  type them — e.g. `cat /proc/meminfo | grep Dirt;`. Quoted separators are
  protected.

### Read & inspect

- `Enter` opens a **read-only detail popup**: description, full content,
  tags, timestamps — nothing editable, purely for reading. For chains it opens
  the ⛓ segment-info popup instead (see below).
- `e` opens the **edit form** (pre-filled) — or, from the detail popup, edits
  the selected option.
- `i` on a command **family** opens the **options panel**; on a **chain**
  opens the segment-info popup.

### Options (the invisible chain)

Commands can have **options** — each option is a runnable child command:

```
❯ -H          curl -H
     Add a request header
  -I          curl -I
     Fetch headers only
```

Inside the options panel:

| Key | Action |
|:---|:---|
| `n` | add an option (flag, extra args, **big description** field) |
| `e` | edit the selected option |
| `d` | delete (Enter confirms) |
| `r` | run the option's composed command (`parent flag args`) |
| `Enter`/`c` | copy the composed command |
| `Esc` | close |

Options persist as child entities of the command — the same structure the
seeded families use. Enter on a command **with** options opens this panel
instead of the plain detail view.

### Chains: segment info + notes

`i`/`Enter` on a chain parses it at top-level `|` and `;` (quote-aware) and
shows each segment with its joiner. Per segment you can store an editable
**note** (`e`, `Enter` saves) — persisted in the entity's metadata. `r` runs
the whole chain through the shell.

### Run modes

`r` opens the **run-mode chooser** (instead of running blindly):

```
command: df -h
Enter/t  open in a NEW terminal window (interactive)
f        foreground: run here with captured output
b        background: detached, tracked in jobs (j)
n        nohup: detached, survives the hub, logs to /tmp
copy     c copies the content; Esc cancels
```

- **Terminal mode** makes interactive TUI tools (nvim, htop, lazygit) work —
  they open in your `$TERMINAL` (or a detected emulator).
- **Background** and **nohup** jobs are tracked — see §5.

### Copy, run, sudo

- `c` copies the content · `r` opens the run chooser · `R` runs it with
  sudo/doas/su (asks for the password in a popup)
- The **run result popup** shows exit code + stdout + stderr

---

## 5. Jobs panel: everything running, stoppable

`j` on the Dashboard lists **everything the hub started**:

- Running **workflows** (with run id) — stop with the cooperative cancel
- **Background** and **nohup** processes — pid, kind, started time, status

| Key | Action |
|:---|:---|
| `↑↓` | navigate |
| `s` | stop (SIGTERM to the process; cooperative cancel for workflows) |
| `K` | force-kill (SIGKILL) |
| `r` | refresh (finished jobs are pruned) |
| `Esc` | close |

nohup jobs log stdout/stderr to `/tmp/tui-op-hub-logs/<name>-<stamp>.log` and
survive closing the hub.

---

## 6. Workflows & the visual builder

`v` on the Workflows tab opens the **visual builder** — compose workflows from
saved commands without writing Lua.

- `Enter`/`a` picks a saved command as a step; `←→` reorders; `d` removes
- **`l` adds a logic node**: AND, OR, NOT, XOR, Compare (== ≠ < ≤ > ≥),
  If/Else — the node editor cycles the kind with `←→` and edits its fields
  (inputs are names of **earlier** steps)
- **`o`** edits the selected step: kind, inputs, operands, and a `run_when`
  gate (any Lua expression; skipped steps show as `◌ skipped`)
- The engine captures every step's return into `results["<step name>"]` so
  later steps and gates can reference earlier outputs
- `Ctrl+S` saves the compiled definition as a JSON workflow entity
- `r` runs it; `X` cancels a running one; `s` schedules it on a cron
  expression (`s` → cron input, e.g. `30 2 * * *`)

---

## 7. Secrets

`n` creates a secret (Name, Value, Group, Username, URL, Email, optional
passphrase, ssh-agent flag). Values are XChaCha20Poly1305-encrypted with your
per-user key.

- Passphrase-protected secrets (`[locked]`) wrap the value a second time and
  re-ask for the passphrase on every use
- `S` offers all ssh-agent-flagged SSH keys to ssh-agent (also automatic after
  login); `t` opens an SSH terminal to a host
- `k` generates SSH or GPG keys (`ssh-keygen`/`gpg`); the private key location
  and passphrase are stored encrypted
- `H` opens the **SSH host manager**: CRUD, tags, live filter (`f`),
  **connection test** (`t` — non-interactive probe with 5s timeout),
  quick-connect (`Enter`/`c` in a new terminal window)
- Workflow scripts can read secrets as `secrets.<name>` or
  `get_secret("<name>")` (US-SEC-02)

---

## 8. Configs (managed config files)

The Configs tab manages your dotfiles-style configs with versioning.

- `n` **registers an existing file** with a form: Path (Ctrl+O = built-in
  file browser, Ctrl+P = external picker), Name (auto-filled), Description,
  Tags, **Deploy to** (comma-sep target paths — they may live anywhere and
  don't need to exist), Deploy mode (←/→: symlink / hardlink / copy)
- `t` adds another deploy target later; `l` deploys to all targets
  (creating missing parent folders); `u` syncs the source into the versioned
  store and re-deploys, reporting **drift**; `g` commits the store with git
- Enter on a registered config shows its info + deploy targets

---

## 9. Projects

- `N` creates a **workspace**: directory + git init + kind scaffold
  (Python/Rust/Node/Docker/Kubernetes/Generic) + optional **plugin template**
  (picker with preview: exclude files with `x`, edit contents with `e`)
- `n` registers an **existing directory** as a project
- `O` opens the workspace in your editor, `E` opens a shell inside the
  project environment (venv activation), `Enter` opens the project detail
  (its entities + stats)
- `d` delete — with an `f` toggle to also delete the workspace folder from
  disk (default off); creating into an existing folder offers **merge**
  (`Ctrl+O`) which never overwrites existing files
- `a` runs **plugin UI actions** on the selected project (capability-gated)

---

## 10. Plugins

Lua mods extend the hub: capabilities require approval, event hooks
(`project_created` …) automate tasks, `[[actions]]` add buttons on the
Projects tab, and `templates/` ships project scaffold starters. See
[PLUGINS.md](PLUGINS.md) for the full API — including the **headless trust
workflow** (`H` on the Plugins tab) that lets the background service load
approved mods without interactive consent.

---

## 11. Themes & settings

Settings (`0`) → `a` (advanced):

- **8 theme presets**: dark, light, nord, dracula, gruvbox, solarized,
  **catppuccin** (mocha), **catppuccin-latte** — cycled with `←/→`, applied
  live; every color overridable in `config.conf` for a custom palette
- Editor, page size, live keybinding rebinds — all persisted with `Ctrl+S`

Everything is stored in `~/.config/tui-op-hub/config.conf`
(Hyprland-style: `section { key = value }`, `#` comments).

---

## 12. Import / export

`x` exports the knowledge base to a JSON bundle (secrets excluded); `I`
imports leniently — a full bundle, a bare AI-generated entity array (all types
including **chains**), or an `entities`-only object. Per-import duplicate
strategy: skip / overwrite / rename. `Ctrl+O`/`Ctrl+P` browse for the file.
See [IMPORT_EXPORT.md](IMPORT_EXPORT.md) for the copy-paste **LLM prompt** that
generates entries.

---

## 13. Headless service & plugins

`--headless` runs the scheduler + API without the TUI. Because there is no
interactive consent flow there, only plugins an admin explicitly marked
**trusted for headless** (`H` on the Plugins tab) auto-approve on service
startup. Untrusted mods still require approval in the TUI first.

---

## 14. REST API

The hub serves a local API on `127.0.0.1:3001` (configurable). Entities,
projects, workflows, schedules, secrets (user-scoped), export/import, health.

```bash
curl -s localhost:3001/entities | jq
curl -X POST localhost:3001/entities/<id>/run
curl -X POST localhost:3001/workflows/<id>/schedule   -d '{"cron_expr": "30 2 * * *"}'
```

The full endpoint list is in the [README](README.md#rest-api).

---

## 15. Troubleshooting

| Symptom | Fix |
|:---|:---|
| Secrets won't decrypt after reinstall | the master key changed — restore the backed-up `~/.config/tui-op-hub/env` |
| API port in use | harmless: the background service owns it and the TUI reuses it |
| A key feels dead | press `Esc` once (a popup may be open — every popup shows its keys) |
| Interactive command exits instantly | run it via the run chooser with `Enter/t` (new terminal window) |
| Plugin not loaded headless | approve it in the TUI and press `H` to trust it for headless |

---

*Related: [PLUGINS.md](PLUGINS.md) (plugin API) · [IMPORT_EXPORT.md](IMPORT_EXPORT.md)
(bundles + LLM prompt) · [reference/ARCHITECTURE.md](reference/ARCHITECTURE.md)
(module map) · [reference/USER_STORIES.md](reference/USER_STORIES.md) (the spec).*

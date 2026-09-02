# 📦 Knowledge Import / Export Guide

> How to move your command/script/app knowledge base in and out of TUI-OP-HUB —
> including a ready-made prompt template so an **AI can generate importable
> bundles** for you.

---

## 1. What is a knowledge bundle?

A bundle is a single **portable JSON file**. It contains your commands, scripts,
apps and command options — everything the TUI can run, copy or open in an
editor. Secrets are **never exported by default**.

```json
{
  "version": 1,
  "exported_at": "2026-02-09T12:00:00+00:00",
  "secret_mode": "excluded",
  "entities": [
    {
      "name": "git commit",
      "type_id": "cmd",
      "description": "Record changes to the repository",
      "content": "git commit",
      "parent": null
    },
    {
      "name": "--amend",
      "type_id": "opt",
      "description": "Rewrite the last commit",
      "content": "--amend",
      "parent": "git commit"
    },
    {
      "name": "backup-home",
      "type_id": "script",
      "description": "Rsync home to the NAS",
      "content": "#!/usr/bin/env bash\nrsync -a --delete ~/ /mnt/nas/home/",
      "parent": null
    },
    {
      "name": "Firefox",
      "type_id": "app",
      "description": "Web browser",
      "content": "firefox",
      "parent": null
    }
  ],
  "secrets": []
}
```

### Field reference

| Field | Required | Meaning |
|:---|:---|:---|
| `version` | yes | Bundle format version — always `1` today |
| `exported_at` | yes | RFC 3339 timestamp (informational) |
| `secret_mode` | yes | `excluded`, `encrypted` or `plaintext` |
| `entities` | yes | The knowledge entries (see below) |
| `entities[].name` | yes | Display name; unique **per (name, type)** pair |
| `entities[].type_id` | yes | `cmd` (shell command), `script` (script file/content), `app` (GUI/TUI app), `opt` (option of a command family), `wf` (Lua workflow) |
| `entities[].description` | no | One-line human description |
| `entities[].content` | no | What runs/copies: the command line, script body (shebang supported), or the app launcher command |
| `entities[].parent` | no | **Name** of the parent command (required for `opt` children; the parent must be in the bundle or already exist locally) |
| `secrets` | no | Only present in `encrypted`/`plaintext` mode — see §4 |

## 2. How to import / export

### In the TUI (Commands / Apps / Scripts / Workflows tabs)

| Key | Action |
|:---|:---|
| `x` | **Export** everything (secrets excluded) to `~/tui-op-hub-export.json` |
| `I` | **Import** from any file — a path-input popup opens with the default path pre-filled; edit it (Backspace), type any path (`~` expands to `$HOME`), `Enter` imports |

### Accepted import formats (lenient)

The importer accepts three JSON shapes — so an AI-generated file works as-is:

1. **Full bundle** — the schema below, exactly as export produces
2. **Bare entity array** — just `[ {"name": ..., "type_id": ...}, ... ]`
   (missing bundle fields are defaulted: `version 1`, `secret_mode "excluded"`, no secrets)
3. **Entities-only object** — `{"entities": [ ... ]}` without the other fields

All shapes merge by (name, type) with the same local-wins rule.

### Over the REST API (default `127.0.0.1:3001`)

```bash
# Export (always secrets-excluded on the API)
curl -s http://127.0.0.1:3001/export -o bundle.json

# Import (wrap the bare JSON array/object into a POST body as-is)
curl -s -X POST http://127.0.0.1:3001/import \
  -H 'Content-Type: application/json' \
  -d @bundle.json
# -> {"imported": 42, "skipped": 7}
```

### Merge rules (important)

1. Entities are matched by **(name, type_id)**.
2. If an entry already exists locally, it is **skipped** — your local edits
   are never overwritten by an import.
3. `opt` children are attached to their parent by the parent's **name**; if
   the parent is missing, the child is skipped.
4. The return value tells you exactly what happened: `{imported, skipped}`.

## 3. 🤖 AI generation recipe

Give this prompt to any LLM, paste the resulting JSON into a file, and import
it (rename the file to `~/tui-op-hub-export.json` and press `I` in the TUI,
or `POST` it to `/import`):

```text
You are generating an import bundle for TUI-OP-HUB, a terminal operations hub.

Output ONLY a JSON object matching this schema (no markdown fences, no commentary):

{
  "version": 1,
  "exported_at": "<RFC3339 timestamp>",
  "secret_mode": "excluded",
  "entities": [
    { "name": "", "type_id": "cmd|script|app|opt", "description": "",
      "content": "", "parent": null }
  ],
  "secrets": []
}

Rules:
- type_id meanings: "cmd" = a shell command family; "opt" = one option/flag
  variant of a command (set "parent" to the parent command's name);
  "script" = a multi-line script (start the content with a shebang);
  "app" = a GUI/TUI application launcher command.
- Names are unique per (name, type_id). Prefer lowercase, human-readable names.
- "content" must be a complete, runnable command line or script body.
  Never include secrets, tokens, passwords or machine-specific absolute paths.
- Descriptions: one concise line each.

Topic: <describe what you want, e.g.>
  "docker commands with their most useful option variants, plus 5 system
   maintenance scripts and 3 apps I listed"
```

**Validation checklist for AI output** (the importer does not run your commands):

- [ ] Parses as JSON with `version: 1`
- [ ] Every `type_id` is one of `cmd`, `script`, `app`, `opt`, `wf`
- [ ] Every `opt` has a `parent` that exists in the bundle (or locally) as a `cmd`
- [ ] No secrets or machine-specific paths in `content`

## 4. Secrets in bundles (advanced)

The TUI export key (`x`) always uses `SecretMode::Exclude`. The library also
supports two opt-in modes for migrating secrets between machines:

| Mode | Behaviour |
|:---|:---|
| `excluded` | No secrets in the bundle (default, safest) |
| `encrypted` | Secrets re-encrypted with an **export passphrase** (Argon2-derived key, XChaCha20-Poly1305, random 24-byte nonce, base64 payload). Portable; decryptable only with the passphrase. |
| `plaintext` | **Explicit opt-in.** Values in clear text. Handle the file accordingly. |

Importing secrets requires a user context (the machine-bound per-user key), so
it is TUI/library-only by design — the REST API never exports or imports secrets.

## 5. Scheduled workflows (cron)

Workflows can run on a cron schedule; the built-in scheduler daemon executes
them and records the runs in the workflow history.

### In the TUI

Workflows tab → select a workflow → `s` → type a cron expression → `Enter`.
Classic 5-field syntax (`30 2 * * *`), seconds-prefixed (`0 30 2 * * *`) and
aliases (`@daily`, `@hourly`) are all accepted; 5-field input is normalized
with a `0` seconds field when stored.

### Over the REST API

```bash
# Schedule workflow <id> daily at 02:30
curl -s -X POST http://127.0.0.1:3001/workflows/<id>/schedule \
  -H 'Content-Type: application/json' \
  -d '{"cron_expr": "30 2 * * *"}'

# List all schedules
curl -s http://127.0.0.1:3001/schedules

# Remove a schedule
curl -s -X DELETE http://127.0.0.1:3001/schedules/<task-id>
```

The scheduler daemon starts with the app (TUI or headless) and executes due
workflows automatically; failed executions are logged and recorded as
workflow runs.

## 6. Running headless: systemd & cron

### systemd (user service)

```bash
tui-op-hub --print-unit         # inspect the unit file first
tui-op-hub --install-service    # write + daemon-reload + enable --now (best effort)
tui-op-hub --uninstall-service  # disable + remove
systemctl --user status tui-op-hub
journalctl --user -u tui-op-hub -f
```

The unit runs `tui-op-hub --headless` (scheduler + REST API, no TUI) with
`Restart=on-failure`. The unit file **never embeds your secrets key**; provide
it either way below:

```bash
# Session-scoped environment:
systemctl --user set-environment TUI_OP_HUB_SECRETS_KEY="$(openssl rand -base64 32)"

# Or an environment file (the unit references it, commented):
echo 'TUI_OP_HUB_SECRETS_KEY=<base64-32-bytes>' >> ~/.config/tui-op-hub/env
```

### cron-only systems

Generate the watchdog line with `tui-op-hub --print-unit` and adapt, or add:

```cron
* * * * * pgrep -f '/path/to/tui-op-hub --headless' >/dev/null || /path/to/tui-op-hub --headless # tui-op-hub
```

Other inits (runit, s6, OpenRC, launchd): point a long-running service at
`tui-op-hub --headless` — that is the only contract.

---

*Schema source: `TUI-OP-HUB/src/share.rs` (`KnowledgeBundle`). Cron handling:
`src/scheduler/mod.rs`. Service generation: `src/service/mod.rs`.*

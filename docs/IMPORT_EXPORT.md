# 📦 Knowledge Import / Export Guide

> Move your command/script/app knowledge base in and out of TUI-OP-HUB —
> including a **copy-paste prompt for any LLM** to generate importable bundles,
> duplicate handling, cron scheduling and headless service setup.

---

## 1. The bundle format

A bundle is one portable JSON file:

```json
{
  "version": 1,
  "exported_at": "2026-02-09T12:00:00+00:00",
  "secret_mode": "excluded",
  "entities": [
    { "name": "git commit", "type_id": "cmd",
      "description": "Record changes to the repository",
      "content": "git commit", "parent": null },
    { "name": "--amend", "type_id": "opt",
      "description": "Rewrite the last commit",
      "content": "--amend", "parent": "git commit" },
    { "name": "backup-home", "type_id": "script",
      "description": "Rsync home to the NAS",
      "content": "#!/usr/bin/env bash\nrsync -a --delete ~/ /mnt/nas/home/",
      "parent": null },
    { "name": "Firefox", "type_id": "app",
      "description": "Web browser", "content": "firefox", "parent": null }
  ],
  "secrets": []
}
```

### Field reference

| Field | Required | Meaning |
|:---|:---|:---|
| `version` | yes | Always `1` |
| `exported_at` | yes | RFC 3339 timestamp (informational) |
| `secret_mode` | yes | `excluded`, `encrypted` or `plaintext` |
| `entities[].name` | yes | Unique per (name, type) pair |
| `entities[].type_id` | yes | `cmd` (shell command), `script` (script body, shebang supported), `app` (launcher command), `opt` (option of a command family), `wf` (Lua workflow) |
| `entities[].description` | no | One-line description |
| `entities[].content` | no | The runnable command line / script body / launcher |
| `entities[].parent` | no | **Name** of the parent command (for `opt` children) |
| `secrets` | no | Only in `encrypted`/`plaintext` mode — see §5 |

## 2. Export

Press **`x`** on the Commands/Apps/Scripts/Workflows/Secrets tab:

1. A **save-path popup** opens, pre-filled with `~/tui-op-hub-export.json`
2. **`Ctrl+O`** opens a **file/directory picker** — yazi, nnn, ranger, lf, then
   GUI dialogs (zenity/kdialog), in that order, first installed wins
3. Edit the path or accept it, press **`Enter`** — secrets are always excluded

Over the API: `curl -s http://127.0.0.1:3001/export -o bundle.json`

## 3. Import

Press **`I`** — the import popup opens:

1. Path pre-filled with `~/tui-op-hub-export.json` (edit it, `~` = `$HOME`)
2. **`Ctrl+O`** opens the file picker (yazi preferred, see the chain above);
   if **no picker is installed** you simply type the path — the popup is the
   production fallback
3. **`Ctrl+D`** cycles the **duplicate strategy** (shown live in the popup):
   `skip` → `overwrite` → `rename` → `skip`
4. **`Enter`** imports; the status bar reports
   `imported / skipped / overwritten / renamed`

### Duplicate handling (US-CMD-01)

The merge key is **(name, type_id)**. For every conflict you choose upfront:

| Mode | Behaviour |
|:---|:---|
| `skip` (default) | Local version **wins** — your edits are never overwritten |
| `overwrite` | Incoming version **replaces** the local one (local edits lost for those entries) |
| `rename` | Incoming version is imported as `name-imported` (`-imported-2`, `-imported-3`, … if needed) — **both** versions coexist |

Children (`opt`) attach to their parent by name; if the parent was renamed
during the same import, the child follows it.

Over the API: `curl -X POST 'http://127.0.0.1:3001/import?duplicates=overwrite' -d @bundle.json`
— response: `{"imported": 5, "skipped": 0, "overwritten": 2, "renamed": 0, "duplicates": "overwrite"}`

## 4. 🤖 Generate a bundle with any LLM (copy-paste)

**Step 1 — paste this prompt into ChatGPT / Claude / any LLM** (fill in the
`Topic` line at the end):

```text
You are an import-bundle generator for TUI-OP-HUB.

OUTPUT RULES (strict):
- Output ONLY a JSON array. No markdown fences, no prose, no comments.
- Each element is one entity object with EXACTLY these fields:
  {"name": string, "type_id": string, "description": string, "content": string, "parent": string|null}
- type_id is one of:
    "cmd"    = a shell command family a human will run
    "opt"    = one option/flag variant of a cmd (set "parent" to that cmd's name)
    "script" = a multi-line script; content MUST start with a shebang line
    "app"    = a GUI/TUI application launcher command
- Names: lowercase, human-readable, unique per (name, type_id) pair.
- content: complete and runnable. Use POSIX shell. No placeholders like
  <your-key>, no secrets, no tokens, no machine-specific absolute paths.
- description: one concise line saying what it does and when to use it.

QUALITY BAR:
- Prefer 10 high-quality entries over 30 shallow ones.
- For every cmd, add 2-5 of its most useful option variants as "opt" children.
- Scripts must be production-safe: set -euo pipefail where appropriate.

Topic: <WHAT YOU WANT, e.g.>
  "docker management: 8 command families with their most useful option
   variants, plus 5 server-maintenance scripts and 4 dev apps"
```

**Step 2 — save the reply** to any file, e.g. `~/ai-docker.json`.

**Step 3 — import**:
- TUI: press `I` → `Ctrl+O` → pick the file (or type the path) → `Enter`
- API: `curl -X POST http://127.0.0.1:3001/import -d @ai-docker.json`

The bare array is accepted **directly** — no bundle wrapper needed. Conflicts
follow the duplicate mode you selected (default: your local versions win).

**Validation checklist for LLM output** (the importer enforces the structural
parts; sanity-check the rest):

- [ ] Parses as a JSON array (or a full bundle)
- [ ] Every `type_id` ∈ {`cmd`, `script`, `app`, `opt`, `wf`}
- [ ] Every `opt` has a `parent` present in the array (as a `cmd`)
- [ ] No secrets or machine-specific paths in `content`

## 5. Secrets in bundles (advanced)

The TUI `x` export always excludes secrets. Library modes for machine
migration: `encrypted` (re-encrypted with an export passphrase — Argon2 key,
XChaCha20-Poly1305, base64) and explicit-opt-in `plaintext`. Importing secrets
requires a local user context; the REST API never touches secrets.

## 6. Scheduled workflows (cron)

- **TUI**: Workflows tab → select → `s` → cron expression → `Enter`
  (classic `30 2 * * *`, seconds-prefixed, or `@daily` all accepted; 5-field
  input is normalized with a `0` seconds field)
- **API**: `POST /workflows/{id}/schedule` `{"cron_expr": "30 2 * * *"}` ·
  `GET /schedules` · `DELETE /schedules/{id}`
- The scheduler daemon runs with the app (TUI or headless) and records runs.

## 7. Headless: systemd & cron

```bash
tui-op-hub --print-unit         # inspect
tui-op-hub --install-service    # key file (0600) + unit + enable
tui-op-hub --uninstall-service  # remove (key kept)
```

See [`docs/INSTALL.md`](INSTALL.md) for the full service/TUI coexistence guide
and non-systemd inits.

---

*Implementation: `src/share.rs` (bundle, lenient parse, duplicate modes),
`src/filepicker.rs` (picker chain), `src/scheduler/mod.rs` (cron),
`src/service/mod.rs` (systemd).*

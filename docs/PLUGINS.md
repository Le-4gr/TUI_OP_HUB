# 🧩 Plugin / Mod API

> Write mods (Lua scripts + a manifest) that load into TUI-OP-HUB, react to
> app events, contribute UI actions and ship project scaffold templates —
> e.g. wire up a git repository automatically whenever a new project workspace
> is created.

---

## 1. Where mods live

```text
~/.config/tui-op-hub/plugins/<plugin-id>/
├── plugin.toml    # manifest: identity + capabilities + UI actions
├── main.lua       # the script (any file name; referenced by entry_point)
└── templates/     # optional: project scaffold templates (*.toml, US-PLG-14)
```

Create the folder, write the files, and open the **Plugins tab** (`9`) in
the TUI: the mod appears immediately. Mods load on app startup once approved.

## 2. The manifest (`plugin.toml`)

```toml
id = 'git.automation'              # unique id; the directory name
name = 'Git Automation'
version = '1.0.0'
plugin_type = 'lua'                # only 'lua' is implemented today
entry_point = 'main.lua'

# Capabilities the mod needs (US-PLG-05). The user approves these (US-PLG-06).
required_capabilities = ['execute_commands']
optional_capabilities = []

# Optional: commands callable via the PluginManager
[commands]
hello = 'Say hello'
```

### Capabilities

| Capability | Grants |
|:---|:---|
| `execute_commands` | the `run_command(cmd)` host function |
| others (`read_secrets`, `network_access`, …) | declared for future host functions; always shown to the user in the approval step |

Without `execute_commands`, the sandbox exposes **only** `log` — a mod cannot
run programs unless you explicitly approve it.

## 3. Event hooks

A mod subscribes to an event by defining a global Lua function named
`on_<event>`. Inside the hook, the `event` global table carries the payload.

### `project_created`

Fired when a new project workspace is created (`N` on the Projects tab) **and**
when an existing directory is registered as a project (`n`). Payload:

| Field | Meaning |
|:---|:---|
| `event.name` | project name (the directory's base name) |
| `event.path` | absolute path of the new workspace |

```lua
-- ~/.config/tui-op-hub/plugins/git.automation/main.lua
-- Git automation: every new project gets a repo, a remote and a first commit.
function on_project_created()
    local path = event.path

    -- 1. git init (create_project_directory already did this; make it idempotent)
    local init = run_command('git -C ' .. path .. ' init -q 2>/dev/null')

    -- 2. add your remote (adjust to your workflow)
    local remote = run_command(
        'git -C ' .. path .. ' remote add origin git@github.com:you/' .. event.name .. '.git'
    )

    -- 3. initial commit
    run_command('git -C ' .. path .. ' add -A')
    run_command('git -C ' .. path .. ' commit -q -m "Initial commit" --allow-empty')

    log('git automation finished for ' .. event.name)
    return 'git repo ready'  -- shown in the app logs; hook result
end
```

### Available host functions

| Function | Requires | Behaviour |
|:---|:---|:---|
| `log(msg)` | always | writes to the tracing log (`journalctl --user -u tui-op-hub`) |
| `run_command(cmd)` | `execute_commands` | runs via `sh -c`, returns a table `{success, exit_code, stdout, stderr}` |
| `event` (table) | (global) | the event payload for this hook invocation |

## 4. Managing mods in the TUI

Open the **Plugins tab** (`9`):

| Key | Action |
|:---|:---|
| `a` | **Approve** the selected mod (grants its declared capabilities) |
| `e` | **Enable / disable** the selected mod (disabled mods are unloaded immediately) |
| `↑ ↓` | Navigate |

Each row shows `name vX.Y.Z [approved|unapproved] [lua]`. On the next startup,
approved + enabled mods load automatically and begin receiving events.

## 5. UI actions (US-PLG-13)

Mods can register **labeled on-screen actions** in their manifest. Actions on
the `project` surface appear on the **Projects tab**: press `a` on a selected
project and pick one.

```toml
[[actions]]
id = 'starter-files'
label = 'Create starting files'
command = 'create_starting_files'   # Lua function to call
surface = 'project'                 # where the action shows
requires = ['filesystem_write']     # capability subset check
```

```lua
-- args = [project_name, project_path]
function create_starting_files(args)
    local out = run_command('mkdir -p ' .. args[2] .. '/src')
    assert(out.success)
    return 'created src/'
end
```

**Gating (two layers)**: `requires` must be a subset of the plugin's declared
capabilities (checked at discovery AND at run time), and only actions from
**loaded** (approved) plugins are offered. The result string is shown in the
actions popup and the status bar.

## 6. Project scaffold templates (US-PLG-14/15)

Ship project starters as **inert data** — no plugin code runs when one is
applied. Place `templates/*.toml` in your plugin folder:

```toml
[template]
name = "Python dev"
description = "venv starter with README and pyproject"
git_init = true            # optional: run `git init` in the new project

[[files]]
path = "README.md"
content = "# {{project_name}}"

[[files]]
path = "src/main.py"
content = "print('hello from {{project_name}}')"

[[commands]]
run = "python3 -m venv .venv"   # optional post-create commands (sh -c, cwd = project)
```

- `{{project_name}}` is substituted in paths, contents and commands
- Applied via the **New Project Workspace** form (`N` on Projects): the
  **Template** field cycles through discovered templates (default `(none)`),
  `Enter` opens a **preview** where you can exclude files (`x`) and edit
  contents (`e`) before creating
- Existing files are **never overwritten** (reported as skipped); missing
  target folders are created
- Application runs off the UI thread and reports
  `N file(s) written, M skipped, git initialized, K command(s) run`

## 7. Writing automation ideas

- **Git automation** (above): init + remote + first commit on project creation.
- **Scaffolding**: ship a template (section 6) or write language-specific
  starter files into `event.path`.
- **Notifications**: run a `notify-send` or `curl` command when projects are
  created (requires `execute_commands`).
- **Housekeeping**: `on_entity_created` style hooks as the event set grows.

## 8. Security model

1. Mods run in a fresh mlua sandbox per hook/command invocation (no shared
   state, no lingering references).
2. Only the capability-gated host functions exist — no `io`, `os.execute` or
   other stdlib access beyond what the host provides.
3. Capabilities require explicit user approval per mod (stored in
   `plugin_approvals`); unapproved mods fail to load and are clearly marked in
   the Plugins tab. UI actions additionally re-check their `requires` subset
   at run time.
4. Templates are inert data materialized by core — applying one never runs
   plugin code.
5. Hook failures are logged and surfaced as a status message — they never
   break project creation or the TUI loop.

---

*Implementation: `TUI-OP-HUB/src/plugin.rs`. Schema: migration `0006`.*

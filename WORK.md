# WORK.md — Active Work Log (AI Agents)

> **STATUS: ONGOING NOW — this file is the live hand-off sheet for AI agents working on the TUI.**
> Update it at the end of every work session: what was done, what broke, what's next.

---

## 🎯 Session 3 (current): Settings screen — editor, defaults, theming, keybindings

**Goal: a real Settings screen where the user can change the text editor, defaults,
theme and keybindings — persisted to `config.toml`.**

### What was done and how
1. **Config layer** (`src/config/mod.rs`):
   - `GeneralConfig.editor` — external editor (empty = `$EDITOR` → `"vi"`)
   - `TuiConfig.page_size` (default 15) — list page size
   - `AppConfig::save()` — writes TOML, creating parent dirs (used by Settings `Ctrl+S`)
   - `parse_color` now supports hex `"#rrggbb"`
   - `KeybindingsConfig`: `ACTIONS` list, `get()`/`set()`/`key_for()` per action,
     `keycode_to_string()` (inverse of `to_keycode` for rebind capture).
     `to_keycode` now keeps single characters case-sensitive (uppercase bindings work;
     named keys like `tab`/`esc` stay case-insensitive)
2. **Theme presets** (`src/tui/modern_ui.rs`): `ModernTheme::PRESETS`
   (`dark`, `light`, `nord`, `dracula`, `gruvbox`), `preset()`, `from_config()`
   (unknown names fall back to the default palette)
3. **Settings screen** (`src/tui/modern_app.rs`, state in `list_state.rs`):
   - 12 rows: Text editor, List page size, Theme, then the 9 keybinding actions
   - `↑/↓` navigate · `Enter` edits a value inline (editor/page size) or starts
     **key-rebind capture** (press any key → bound; `Esc` cancels) · `←/→` cycles the
     theme preset **applied live** · `Ctrl+S` saves to `config.toml` · `Esc` back
   - Page-size changes apply to all four list states immediately
   - Keybindings are no longer hardcoded: `handle_dashboard_key` resolves
     create/edit/delete/copy/run/search/filter/help/quit from the config
4. **External editor integration** (US-CMD-05): `o` on the Commands tab opens the
   selected command's content in the configured editor — TUI suspends
   (`LeaveAlternateScreen` + raw-mode off), `run_external_editor()` (temp file +
   `sh -c "$EDITOR $FILE"`), then re-reads, re-enables the TUI and saves the entity.
5. **`main.rs`** now passes the loaded `AppConfig` into `ModernApp::new(pool, config)`;
   the theme and page size are applied at startup.
6. **Docs**: README config sample gains `[general].editor`, `page_size = 15`, theme
   presets, a Settings-screen key table, and the `o` keybinding row.

### Tests (per the AGENTS.md workflow)
- Unit: config round-trip save/load, defaults, missing-dir creation,
  `to_keycode`/`keycode_to_string` round trips, rebind resolution + invalid fallback,
  hex color parsing, `SettingsState` navigation, theme presets (`modern_ui`)
- Behavioral (`modern_app::tests`): settings navigation clamping, editor value editing,
  key-rebind capture → binding stored **and the rebound key opens the create form**,
  live theme cycling (light → dark), invalid page size rejected, `Ctrl+S` persists and
  reloads from a temp path
- BDD (`tests/bdd_scenarios.rs`): config round trip, keybinding action resolution for
  all 9 actions + capture round trip, theme preset mapping/fallback

### Validation
- `cargo fmt` ✔ · `cargo clippy --all-targets` → 0 errors ✔ · `cargo build` ✔
- `cargo test` → **79 passed / 0 failed** (67 lib + 12 BDD)

---

## 🎯 Session 2: tests + BDD + agent workflow hardening (completed)

**Goal: test everything (unit + BDD), and bake testing + git into the agent workflow.**

### What was done and how
1. **Refactored for testability** — extracted the pure logic that was buried in save
   handlers into `src/tui/list_state.rs`:
   - `parse_tags_text()` — comma-separated tags → clean tag list
   - `build_workflow_definition()` — visual builder state → validated `WorkflowDefinition`
     (used by `save_visual_workflow`, so TUI and tests share one code path)
2. **Unit tests added** (`#[cfg(test)] mod tests`):
   - `list_state.rs` — ListState selection/pagination clamping, picker navigation,
     visual builder (add/reorder/remove steps, field cycling), helpers, type constants
   - `modern_app.rs` — `cycle_field` wrap-around + **behavioral overlay tests** that build
     a `ModernApp` on an in-memory SQLite pool and drive `handle_key(KeyEvent)` with no
     terminal: Esc cancels forms, typing lands in fields, Tab moves focus, picker Enter
     adds a step, delete confirmation consumed, run-result closes, Ctrl+S validation
   - `repository/mod.rs` — full CRUD round trips for entities, projects (incl. the new
     `update_project`), secrets (per-user), workflow runs, FTS search, tag filtering,
     FK `NotFound` errors
   - `workflow/mod.rs` — JSON definition parsing, Lua fallback, multi-step execution,
     failing-step error reporting, non-workflow entity rejection
3. **BDD integration scenarios** — new `TUI-OP-HUB/tests/bdd_scenarios.rs`:
   Gherkin-style `given_…_when_…_then_…` tests over the public API, one per feature:
   command lifecycle, FTS search, project grouping, secret encryption-at-rest round trip,
   visual workflow execution + run history, step reordering → execution order, failing
   step reporting, form input normalization.
4. **🐛 Real bug found by the tests and fixed**: secrets were stored with the *username*
   as `user_id`, but `secrets.user_id` has a foreign key to `user_profiles(id)` (a UUID)
   — so creating any secret always failed with `FOREIGN KEY constraint failed` once FK
   enforcement is on. Fix: resolve the username to the profile id first via
   `get_or_create_user()` in the TUI (`current_user_profile_id()`) and in the API secret
   handlers (`default_user_id()`); decrypt now uses the stored `s.user_id`.
5. **Fixed pre-existing clippy error** in `process::tests` (`assert!(len() >= 0)` on usize).
6. **`AGENTS.md` workflow updated**:
   - §4 now lists the test commands (`cargo test --test bdd_scenarios`, module filters)
   - Step 4 is now **Test & Validate**: unit tests + BDD scenarios are mandatory for every
     behavior change, with the in-memory-pool pattern documented
   - New **Step 5 — Commit with git**: `git status`/`git diff` before committing, commit
     after every validated unit, conventional commits referencing story IDs, never commit
     secrets/`*.db`/`target/`/sync-conflict files
   - §5 rule 8: tests are part of the feature; Do-NOT list extended

### Validation
- `cargo fmt` applied
- `cargo clippy --all-targets` → 0 errors
- `cargo test` → **55 lib tests + 9 BDD scenarios = 64 passed, 0 failed**
- `cargo build` → OK

---

## 🎯 Session 1: TUI CRUD + visual workflows (completed)

**Goal: make the TUI fully usable — real create/edit/delete, base scripting, and a visual
workflow mode built from saved commands.**

### Problem
The Modern TUI (`src/tui/modern_app.rs`) had `n` (new) / `e` (edit) / `d` (delete) key
bindings, but:
- `n` and `e` only filled form-state structs — **no form was ever rendered and nothing was
  ever saved**, so users could not create or edit anything.
- `d` deleted immediately (no confirmation) and did not work on Secrets.
- `c` (copy) and `r` (run) were `// TODO` stubs.
- The Workflows tab could only store raw Lua text — no way to compose workflows from the
  commands already saved in the Commands tab.

### How it is being fixed (approach)
All UI work happens as **overlays inside `ModernApp`** (`src/tui/modern_app.rs`) so the
existing state machine keeps working:

1. **Overlay routing** — `handle_key()` now checks, in priority order:
   delete-confirmation popup → visual workflow builder → active form (command/project/
   workflow/secret) → search bar → run-result popup → normal tab handling.
2. **Forms** — each form state in `src/tui/list_state.rs` got `editing_id: Option<String>`
   (create vs. update) and `error_message: Option<String>`. Forms are rendered as centered
   popups reusing the existing `render_field()` helper. Keys: `Tab`/`↓`/`Enter` next field,
   `BackTab`/`↑` previous, `Ctrl+S` save, `Esc` cancel, `Enter` in content/script fields
   inserts a newline.
3. **Command form** has a Type field cycling `cmd` → `script` → `app` with `←`/`→`, so base
   scripting entities are creatable/editable from the same form.
4. **Secrets form** encrypts with `secrets::encrypt_for_user()` before insert (never stores
   plaintext); the logged-in username is now used as `user_id` (was hardcoded `"default"`).
5. **Visual workflow mode** (`v` on the Workflows tab) — `VisualWorkflowState` in
   `list_state.rs` + picker over saved `cmd`/`script` entities. `a`/`Enter` adds a step,
   `d` removes, `←`/`→` reorder, `Ctrl+S` saves the step list as a JSON
   `WorkflowDefinition` entity (type `wf`) that the existing Lua engine executes linearly.
6. **Delete** now asks for confirmation (`Enter` = yes, `Esc` = no) and also covers Secrets.
7. **Run** executes commands via `sh -c` (tokio process) and workflows via
   `workflow::execute_workflow_by_id` (results persisted to `workflow_runs`).
8. **Copy** uses `arboard` (already a dependency).
9. Fixed dashboard stats counting `type_id = 'command'/'workflow'` when the DB seeds
   `'cmd'/'wf'` (stats were always 0).
10. Added the missing `repository::update_project()`.

### Files touched
- `TUI-OP-HUB/src/tui/modern_app.rs` — overlay routing, all form/visual/confirm/search/run
  handlers and renderers (bulk of the work)
- `TUI-OP-HUB/src/tui/list_state.rs` — form struct extensions, `VisualWorkflowState`,
  `CommandPickerState`
- `TUI-OP-HUB/src/repository/mod.rs` — `update_project()`
- `TUI-OP-HUB/src/secrets/mod.rs` — `decrypt_for_user_id()` (per-user decrypt)
- `TUI-OP-HUB/src/scheduler/mod.rs` — fixed pre-existing cron test (seconds field)
- `AGENTS.md`, `WORK.md` — docs

### Keys cheat sheet (after this change)
| Context | Keys |
|:---|:---|
| Any list tab | `n` new · `e` edit · `d` delete (confirm) · `c` copy · `r` run · `/` search · `v` visual mode (Workflows tab) |
| Any form | `Tab`/`↓` next field · `BackTab`/`↑` previous · `←`/`→` cycle Type (command form) · `Enter` newline in content/script · `Ctrl+S` save · `Esc` cancel |
| Visual builder | `Tab` cycle Name/Description/Steps · `Enter`/`a` pick saved command · `d` remove step · `←`/`→` move step · `Ctrl+S` save · `Esc` cancel |
| Popups | `Enter` confirm delete / close result · `Esc` cancel |

---

## ✅ Done this session
- [x] Diagnosed why create/edit silently did nothing (forms never rendered/saved)
- [x] Overlay routing + form rendering + save/cancel for all 4 form types
- [x] Create/edit for commands, scripts, apps (type switcher), projects, workflows, secrets
- [x] Secrets create/edit with per-user encryption; logged-in user used for secrets
- [x] Delete confirmation popup incl. Secrets tab
- [x] Copy to clipboard (commands/workflows content, decrypted secret values)
- [x] Run commands (`sh -c`) and workflows (engine + `workflow_runs` history) with result popup
- [x] Search bar (`/`) filtering for lists
- [x] Visual workflow builder from saved commands (see approach §5)
- [x] `repository::update_project()` added
- [x] Fixed dashboard stat counters (`cmd`/`wf` type ids)
- [x] **Validated**: `cargo check` clean, `cargo test` → 19 passed / 0 failed
      (also fixed pre-existing `scheduler::tests::test_cron_parsing`: the `cron` crate
      requires a seconds field — `* * * * *` → `* * * * * *`)

## ⏭️ Next up (not started)
- [ ] Multi-line cursor movement inside content/script fields (currently type-only)
- [ ] Tags/project pickers in the command form (tags are comma-separated text for now)
- [ ] Edit workflows in visual mode from existing JSON definitions (`v` on selected workflow)
- [ ] DAG steps (`depends_on`) in visual mode — engine already supports it
- [ ] Wire scheduler daemon into `main.rs` (US-WF-07)
- [ ] Plugin approval UI (US-PLG-05/06), SSH host manager TUI (US-SSH-01..06)

## ⚠️ Known constraints for the next agent
- `handle_key` only passes `KeyCode` in some paths; modifiers are available on the
  `KeyEvent` — extend signatures if you need more Ctrl-combos.
- The Lua engine executes steps linearly in definition order; `depends_on` is stored but
  not yet topologically sorted in `workflow/mod.rs`.
- Secrets are listed per `user_id`; users created via signup start with an empty secrets
  list unless `TUI_OP_HUB_SECRETS_KEY` is set (env fallback key).
- Do not touch `*.sync-conflict-*` files; do not edit applied migrations.

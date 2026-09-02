# WORK.md — Active Work Log (AI Agents)

> **STATUS: ONGOING NOW — this file is the live hand-off sheet for AI agents working on the TUI.**
> Update it at the end of every work session: what was done, what broke, what's next.

---

## 🎯 Session 15 (current): black-screen fix, visible search bar, type badges, project workspaces

**Goal: fix black screen after external commands, make `/` search visible,
add type badges to lists, add project workspace creation with editors and
container support.**

### What was done and how
1. **Black-screen fix**: after external commands (man, editor, btop, shell, etc.)
   the TUI showed a black screen because ratatui's diff buffer thought nothing
   changed. Fix: added `needs_full_redraw: bool` flag — set to true after every
   external command resume; the run loop checks it and calls `terminal.clear()`
   before the next draw. All 6 resume points now set this flag.
2. **Visible search bar**: `/` search was filtering the list but the user
   couldn't see what they typed. Added a visible **search bar** that replaces
   the footer when search is active — shows `/ query│` with the cursor and
   `Enter: apply · Esc: cancel` hints. The list title also shows
   `[SEARCH: query]` and the filtered count.
3. **List styling**: selected items now use the theme accent color as
   background with dark text (instead of the barely-visible highlight color).
4. **Project workspaces** (`src/project_workspace.rs`, new module):
   - `ProjectKind`: Python (venv), Rust (cargo), Node (npm), Docker (compose),
     Kubernetes (manifests), Generic
   - `ProjectEditor`: Neovim, Vim, VS Code, lazygit, lazydocker, yazi, Helix
   - `create_project_directory()`: mkdir + git init + branch `main` +
     .gitignore per kind + starter files per kind + README + initial commit
   - `N` on Projects tab opens the creation form; `O` opens in chosen editor
5. **Keybinds overlay updated**: all new actions listed (N, O, R, `, f, p, etc.)
6. **Footer hints updated**: Projects tab now shows N/O alongside E; Dashboard
   shows f/p/`; Commands shows all 11 actions.

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **146 passed / 0 failed** (113 lib + 33 BDD)

---

## 🎯 Session 14: docs overhaul, keybinds overlay, import/export (completed)

**Goal: comprehensive documentation update, keybinds overlay with all actions,
import/export functions for every entity type, industry-standard code quality.**

### What was done and how
1. **Keybinds overlay fully updated**: the `?` overlay now shows ALL actions
   per screen, including the newer additions (N new project, O editor, R sudo,
   ` terminal, i options, m man, E shell, v visual, k keygen, f fetch, p processes,
   a advanced, Ctrl+S save). Footer hints for Projects tab also include N and O.
2. **Import/export extended** (`share` module): `import_secrets()` now handles
   all three secret modes (plaintext/encrypted/excluded) with proper error
   handling. The export/import pair covers entities (families + options with
   parent links), projects, secrets, and configs.
3. **Stolen-DB protection** (`auth::derive_user_key`): per-machine secret
   (`machine.key`) is combined with the password during Argon2 derivation.
   Verified by BDD scenario (machine A decrypts, machine B fails).
4. **Login dev manager fix**: `refresh_dev_user_list()` now called when `u`
   opens the manager.
5. **Code quality**: all public functions have doc comments; modules have
   `//!` module-level docs explaining their purpose and US-XXX references.

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **146 passed / 0 failed** (113 lib + 33 BDD)

---

## 🎯 Session 13: sudo compat + embedded terminal (completed)

**Goal: run commands with elevated privileges (sudo/doas/su) and drop into a
subshell without leaving the hub.**

### What was done and how
1. **`src/privilege.rs`** (new module):
   - `PrivTool` enum (Sudo/Doas/Su) with `detect_priv_tool()` (PATH search)
   - `needs_password()` — probes `sudo -n true` for NOPASSWD detection
   - `build_invocation()` — builds the right invocation per tool:
     `sudo [-S] cmd`, `doas cmd`, `sh -c 'su -c "cmd"'`
   - `run_privileged()` — spawns and optionally pipes the password via stdin
     to `sudo -S`; returns `std::process::Output`
   - 7 unit tests covering detection, invocation building per tool,
     stdin password support flags
2. **TUI sudo integration** (`src/tui/modern_app.rs`):
   - `R` (Shift+R) on the Commands tab runs the selected command with
     elevated privileges
   - If the tool needs a password, a **secure popup** appears (`●●●●` masked
     input); `Enter` pipes it via `sudo -S`, `Esc` cancels
   - Result shown in the existing run-result popup
3. **Embedded terminal**: `` ` `` (backtick) on any screen suspends the TUI
   (leave alternate screen + raw mode off), spawns `$SHELL`, restores the TUI
   on exit. Same pattern as `start_project_shell` and `show_man_page`.
4. **Keybinds updated**: `?` overlay and footer hints now show `R` (sudo) and
   `` ` `` (terminal) on Commands/Dashboard.

### Tests
- Unit: privilege detection, invocation building for sudo/doas/su with and
  without password, stdin password support flags
- All prior tests still pass (no functional changes to existing code)

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **139 passed / 0 failed** (106 lib + 33 BDD)

---

## 🎯 Session 12: `/` search fix, dev DB ops, numpad + keybinds polish (completed)

**Goal: fix `/` fuzzy search (was still substring), add dev DB wipe/tab-delete,
and fix the login dev user manager not listing users.**

### What was done and how
1. **`/` search fixed**: `apply_search_filter` was still using substring
   `contains()` — replaced with `fuzzy_rank` for Commands, Projects and
   Workflows tabs (Secrets keeps substring since names are short). Added a
   **visible search bar** in the list header showing the typed query with
   `│` cursor while `/` is active.
2. **Dev DB wipe** (`Shift+D`): deletes all rows from all tables
   (entities, secrets, users, projects, tags, workflow_runs, seed_meta).
   Clears in-memory lists too; does NOT re-fetch (that would recreate a user
   profile via `get_or_create_user`).
3. **Dev tab delete** (`Shift+A`): deletes all entities of the current tab's
   type(s) — commands+scripts+apps on Commands, workflows on Workflows,
   projects on Projects. User remains.
4. **Login dev user manager fix**: `refresh_dev_user_list()` is now called
   when `u` opens the manager (was showing empty list).
5. **BDD tests added**: dev DB wipe, dev tab delete, `/` live filtering,
   login dev manager listing.

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **133 passed / 0 failed** (100 lib + 33 BDD)

---

## 🎯 Session 11: stolen-DB protection, login dev manager, sharing, dotfiles (completed)

**Goal: stolen-DB security, login-screen dev user manager, knowledge-base
sharing, and a dotfiles/config manager.**

### What was done and how
1. **Stolen-DB protection** (`src/auth/mod.rs`): user encryption keys are now
   derived from **password + per-machine secret**
   (`~/.config/tui-op-hub/machine.key`, auto-generated 0600). A DB copied to
   another machine can't decrypt secrets even knowing the password. Dev mode
   does NOT bypass this — the wipe deletes users, it never unlocks secrets.
   `derive_user_key(password, salt, machine_secret)` is the testable core.
2. **Login-screen dev user manager** (cargo run only): `u` on the login
   screen opens a DEV user manager — ↑↓ select, `x` **deletes** the user
   (secrets cascade = forgotten-password escape hatch), `r` **resets** the
   password to `reset-me` (old secrets become undecryptable, documented).
   Overlay renders over the login screen; guarded by `dev_mode_enabled()`.
3. **Knowledge-base sharing** (`src/share.rs` + `src/share_crypto.rs`):
   - `export_knowledge(pool, user_id, SecretMode, passphrase)` builds a
     portable JSON bundle of all entities (families + options with parent
     links) with secrets **excluded / encrypted / plaintext**
   - Encrypted mode re-encrypts with an **export passphrase** via
     `share_crypto` (Argon2-derived key + XChaCha20, non-deterministic
     nonces) — portable across machines, unlike the machine-bound user key
   - `import_knowledge` merges by (name, type): existing entries win, so
     local edits are never overwritten; options re-link to families by name
4. **Dotfiles manager**: `config_manager` already had store/version/symlink —
   documented as the "hyprlinks" flow: master copy in the hub, symlink into
   `$HOME` (point it where it should go); entities can hold content inline or
   reference the source path.
5. Entity model now exposes `parent_id`; picker literals fixed.

### Tests
- Unit: `dev_mode_enabled_for` matrix, `delete_all_users` cascade, share_crypto
  round-trip / wrong-passphrase rejection / non-deterministic ciphertext
- BDD: stolen-DB scenario (machine A decrypts, machine B fails), login dev
  manager delete cascade
- **Validation**: `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
  · `cargo test` → **129 passed / 0 failed** (100 lib + 29 BDD)

---

## 🎯 Session 10: keybinds helper + footer hints (completed)

**Goal: a keybind helper and the per-screen key hints at the bottom, next to
the existing ones.**

### What was done and how
1. **Keybind helper overlay** (US-TUI-09): `?` now opens a full cheat-sheet
   overlay from **any** screen (Global / Dashboard / Commands / Projects /
   Workflows / Secrets / Settings / Developer-mode sections). Esc or `?`
   closes it and returns to the exact screen you came from. Implemented as a
   global hook in `handle_key` (before the state machine) so no screen can
   miss it; the old per-dashboard help arm was removed.
2. **Footer hints** (`render_keybind_footer` + `keybind_hints`): the dashboard
   and all four list tabs render a live, context-specific hint bar
   (`render_keybind_footer(f, chunks[2], &self.keybind_hints())`), replacing
   the old hardcoded Navigate/New/Edit/Delete/Quit line. Hints now include the
   newer actions: r/c/o/i/m on Commands, v on Workflows, E on Projects, k on
   Secrets, f/p on the Dashboard — plus `?` everywhere.
3. **BDD hooks**: `bdd_keybinds_open`, `bdd_state`, `bdd_set_state`,
   `bdd_keybind_hints` added alongside the existing `bdd_*` helpers.
4. **BDD scenarios** (Feature: Keybinds helper overlay):
   - `?` toggles the overlay on the dashboard, Esc closes, and it opens/closes
     from Settings without losing the screen
   - every main state has non-empty hints that always include `?`

### Gotcha fixed during testing
The hints scenario initially panicked inside sqlx ("timers are disabled"):
the manual single-thread tokio runtime needed `.enable_all()` — and pools must
be created *inside* the runtime they're dropped in.

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **124 passed / 0 failed** (97 lib + 27 BDD)

---

## 🎯 Session 9: tooling, structured commands, man/help, fuzzy search, fetch (completed)

**Goal: graceful handling of missing tools; man/help; fuzzy search; yazi & co.;
systemd + OpenRC; fetch; structured prepopulated command families with options.**

### What was done and how
1. **Graceful tool handling** (user note: "openssh isn't installed by default —
   notify, don't fail"): every external tool goes through `keygen::which()`.
   Keygen (`k`) checks ssh-keygen/gpg **before** opening the form; man (`m`)
   checks man; process viewer (`p`) picks the first of btop/htop/top; all
   surface friendly status messages instead of errors.
2. **Man pages & structured help** (US-CMD-05/01):
   - `m` on the Commands tab suspends the TUI and runs `man <name>`; a missing
     entry or missing man binary shows a status notification
   - `i` opens an **options popup**: the selected command family's options are
     structured child entities (flag + description), rendered as a two-column
     popup (new `repository::list_child_entities`)
3. **Structured command model** (user note: "command is the family entity, options
   have descriptions"):
   - Migration **0004_tooling.sql**: `entities.parent_id` (self-reference, FK
     cascade) + new `opt` entity type + `seed_meta` marker table
   - Command = family (`cmd`), options = `opt` children — `do like minded things`
     applied: the same parent/child pattern is ready for SSH hosts and service
     families later
4. **Seeded knowledge base** (`src/seed.rs` + `src/seed_data.rs`, called from
   `main.rs`, idempotent via existence checks + `seed_meta` marker):
   - Families: git, docker, **systemctl AND rc-service/rc-update (systemd +
     OpenRC per user note)**, journalctl, ssh, curl, grep, find, tar, python3,
     cargo — each with 4–8 described options
   - Tools as tagged `app` entities: **yazi/ranger/lf** (file browsers per user
     note), fastfetch/neofetch, btop/htop, nvim/vim/nano, lazygit, fzf
5. **Fuzzy search** (user note: "fuzzy search capabilities") — new
   `src/fuzzy.rs`: subsequence scorer with word-start/consecutive bonuses and
   short-haystack preference; `fuzzy_rank` orders `/`-search results best-first
   across name + description for Commands and Workflows.
6. **Fetch support** (user note: "add fetch support"): `f` on the dashboard
   opens a fetch panel built from sysinfo (user@host, OS, kernel, **init system
   detection: systemd/OpenRC/unknown**, uptime, CPU, RAM, swap).

### Tests
- Unit: fuzzy scorer (subsequences, case, bonuses, ranking), seed idempotency
  helper expectations, init-system detection values
- BDD: seeded families + options + tools exist (git/git/docker/systemctl/
  rc-service/rc-update/yazi/fastfetch), seeding is idempotent, fuzzy ranking
  picks the best match, init detection returns a supported value
- **Validation**: `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
  · `cargo test` → **122 passed / 0 failed** (97 lib + 25 BDD)

---

## 🎯 Session 8: developer mode — wipe users without login (completed)

**Goal: while developing/testing (`cargo run`), be able to delete every user
without logging in — never available that way in production.**

### What was done and how
1. **Detection** (`src/auth/mod.rs`): `dev_mode_enabled()` = debug build
   (`cfg!(debug_assertions)` — true for `cargo run`/`cargo test`, false for
   `--release`) OR `TUI_OP_HUB_DEV=1`. Pure testable helper
   `dev_mode_enabled_for(debug_build, env_value)` covers both branches in tests.
2. **Repository**: `delete_all_users()` — deletes all `user_profiles`; secrets
   and user keys are removed with them via the FK cascade.
3. **Settings screen**: with dev mode on, the title shows `[DEV]`, the footer
   gains `d: DEV wipe users`, and pressing **`d` twice** (two-step confirm)
   deletes every user without logging in, showing
   "✓ DEV: deleted N user(s) — next signup is admin". The key is inert in
   release builds (guard `KeyCode::Char('d') if dev_mode_enabled()`).
4. **REST**: dev-gated `GET /users` (list) and `DELETE /users` (wipe) endpoints —
   both refuse with an explanatory error outside developer mode.
5. **BDD scenario** `given_dev_mode_when_d_pressed_twice_then_all_users_deleted`:
   two users exist → `d` once only arms the confirm → `d` again wipes all →
   next signup is admin again. Plus unit tests for `dev_mode_enabled_for`
   (all env spellings, debug/release matrix) and `delete_all_users`.

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **112 passed / 0 failed** (91 lib + 21 BDD)

---

## 🎯 Session 7: numpad support in Settings + BDD tests (completed)

**Goal: make the numpad work in the Settings screens (previously only Esc did
anything) and cover the TUI screens with BDD tests.**

### What was done and how
1. **Numpad fix** (`handle_settings_key`, `handle_advanced_key`, `handle_keygen_key`):
   with NumLock on, keypad keys arrive as plain digits and were ignored — now:
   `2`=down, `8`=up, `4`=left, `6`=right (theme preset / color cycling),
   `7`/`9`=first row, `1`/`3`=last row. Works in Settings, Advanced mode and the
   keygen form; with NumLock off the keypad sends real arrows which already worked.
2. **BDD hooks**: `ModernApp` gained `#[doc(hidden)] pub bdd_*` methods
   (`bdd_press`, `bdd_open_settings/advanced/keygen`, `bdd_select_*`,
   `bdd_theme_name/bg`, `bdd_settings_selected`, `bdd_advanced_selected`,
   `bdd_keygen_field`) so `tests/bdd_scenarios.rs` can drive the TUI over the
   public API without a terminal.
3. **New BDD scenarios** (`Feature: Numpad support in Settings screens`):
   - settings navigation via numpad digits (`2`/`8`/`1`/`7`)
   - theme preset cycling via numpad `4`/`6` on the Theme row
   - Advanced mode: numpad navigation + bg color cycling `black → white → black`
   - keygen form field navigation via numpad `2`/`8`
4. README: numpad key notes for Settings and Advanced mode.

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **109 passed / 0 failed** (89 lib + 20 BDD)

---

## 🎯 Session 6: Phase 2 kick-off (completed)

**Goal: start Phase 2 and implement the user's requested features.**

### What was done and how
1. **Migration 0003_phase2.sql** (new file — 0002 was NOT edited):
   - `user_profiles.is_admin INTEGER DEFAULT 0`
   - `secrets.secret_kind TEXT DEFAULT 'password'`, `secrets.requires_reauth DEFAULT 0`
   - `projects.env_type TEXT`, `projects.env_cmd TEXT`
2. **Admin user management** (US-SEC):
   - `auth::create_user` sets `is_admin = 1` for the **first** user (`count_users == 0`)
   - `repository::{count_users, is_admin, set_admin, delete_user, set_password_hash}`
   - `auth::admin_reset_password(pool, admin_user_id, target_username, new_password)`:
     admin-only; documented that secrets stay encrypted with the old password-derived
     key — deleting the user (cascade) is the clean escape hatch
3. **Secrets as workflow variables** (US-SEC-02, US-WF-04):
   - `WorkflowContext.user_id` + `create_workflow_context_for_user(...)`
   - Before execution the user's secrets are decrypted and installed as a
     `secrets` table plus a `get_secret(name)` Lua function; `requires_reauth`
     secrets are never exposed automatically
4. **Type-aware execution** (US-CMD-09 differentiation):
   - `workflow::RunPlan` enum + `build_run_plan(entity)`:
     `cmd` → `sh -c` (captured), `script` → interpreter from shebang
     (`python3 -c`), `app` → spawned detached (fire & forget), `wf` → engine only
   - `metadata_json {"file": path}` → run the file; interpreter by extension
     (py→python3, lua→lua, js→node, rb→ruby, pl→perl, sh→sh), fallback `sh <file>`
   - `execute_workflow_by_id_for_user(...)` resolves file-backed definitions:
     `.json` → WorkflowDefinition from file; other text → single-step Lua
5. **SSH & GPG keygen** (US-SEC-01) — new `src/keygen/mod.rs`:
   - `which()`, `pick_process_viewer()` (btop→htop→top), `generate_ssh_key()`
     (ssh-keygen ed25519, rejects overwrite), `generate_gpg_key()` (batch
     `--quick-generate-key`)
   - Secrets tab: `k` opens a keygen form (Name/Email/Passphrase/Kind, ←/→
     cycles kind); Ctrl+S generates and stores `<name>_private_key` +
     `<name>_passphrase` as encrypted secrets of the matching kind, both
     flagged `requires_reauth` (viewing needs more than being logged in)
6. **Projects as environments** (US-ENV-01): `repository::set_project_env`;
   `E` on the Projects tab starts an interactive shell with the project's
   activation command applied (`env_cmd; exec $SHELL`), TUI suspended.
7. **Processes** (US-PROC): `p` on any dashboard state suspends the TUI and runs
   btop/htop/top — no custom process UI built (per user note: use known programs).
8. **Scheduler** (US-WF-07): `WorkflowScheduler` daemon spawned in `main.rs`.
9. **Docs**: README Phase-2 feature list; USER_STORIES phase status updated.

### Tests
- Unit: `build_run_plan` (cmd/script/app/file/lua/wf), `is_json_path`,
  keygen (`which`, viewer pick, ssh keypair generation, overwrite rejection),
  secrets-in-workflow (get_secret reads value; reauth secrets excluded)
- BDD: first-user-is-admin + user deletion cascade; file-backed workflow
  execution from a JSON definition file
- **Validation**: `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
  · `cargo test` → **105 passed / 0 failed** (89 lib + 16 BDD)

---

## 🎯 Session 5: Advanced mode — visual config editor in Settings (completed)

**Goal: an Advanced mode inside Settings with a visual style/config editor
(color swatches, live palette cycling, hex editing) plus system options.**

### What was done and how
1. **State** (`src/tui/list_state.rs`):
   - `ADVANCED_ROWS` (14 rows): 11 theme colors (`fg`, `bg`, `accent`, `status_bg`,
     `primary`, `secondary`, `success`, `warning`, `error`, `border`, `highlight`)
     then `db_path`, `api_bind`, `busy_timeout`
   - `ADVANCED_PALETTE` (named + hex colors) for visual `←`/`→` cycling
   - `is_advanced_color_row` / `is_advanced_optional_color` helpers +
     `AdvancedState` (active/selected/editing/buffer/error, clamped navigation)
2. **Behavior** (`src/tui/modern_app.rs`):
   - `a` in Settings opens Advanced; `Esc` returns to Settings (state stays `Settings`,
     render dispatches on `advanced.active`)
   - Color rows show a **live swatch** (`██` styled with the actual color);
     `←`/`→`/`Enter` cycle through the palette **applied instantly to the running
     theme**; `Enter` (or any row) also supports exact text input (hex `#rrggbb`,
     paths, numbers); `Backspace` resets — optional colors back to `(preset)`,
     base palette fields and system options to their defaults
   - System rows: database path, API bind address (non-empty validation) and
     busy timeout (number validation); all changes mark the config dirty and
     `Ctrl+S` persists to `config.conf` from anywhere
   - Design fix found by tests: `fg`/`bg`/`accent` are applied **directly** to the
     live theme (not via `from_config`), because preset names intentionally ignore
     those base fields — the visual editor must give instant feedback regardless
3. **Docs**: README Settings section gains the Advanced-mode tables.

### Tests
- Unit (`list_state`): advanced navigation clamping, color/optional/system row
  classification, palette contains named + hex entries
- Behavioral (`modern_app`): `a` opens/`Esc` closes Advanced, color cycling updates
  config **and** live theme, hex text edit stores + applies an optional override,
  Backspace clears to preset, busy timeout validation (rejects `5000x`, accepts fix)
- BDD: `given_advanced_visual_edits_when_saved_then_persisted` — cycled color + hex
  override + system options survive a save/load round trip in Hyprland format
- **Validation**: `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
  · `cargo test` → **90 passed / 0 failed** (76 lib + 14 BDD)

---

## 🎯 Session 4: Hyprland-style config, custom theming, docs & business structure (completed)

**Goal: Hyprland-style config file, user-defined themes, restructured docs — no
functional changes; all tests still pass.**

### What was done and how
1. **Hyprland-style config file** (`src/config/mod.rs`):
   - Replaced TOML with a custom `section { key = value }` parser
     (`parse_hypr_config`): `#` full-line comments, quoted or bare values, one-line
     `section { key = value }` blocks, unknown sections/keys ignored, missing keys
     keep defaults, booleans accept `true/yes/on/1` / `false/no/off/0`
   - `AppConfig::save()` now writes documented `config.conf` text (comments per key);
     default path renamed `config.toml` → **`config.conf`**
   - `AppConfig::load/save` signatures unchanged — main.rs/Settings call sites untouched
   - Hex color values (`#101010`) survive round trips (quoted on write, never
     comment-stripped)
2. **Custom theming** (US-APP-01):
   - `ThemeConfig` gained optional override colors: `primary`, `secondary`, `success`,
     `warning`, `error`, `border`, `highlight` (unset = preset/default color)
   - `ModernTheme::from_config`: known preset → preset palette; **any other name =
     custom theme built from the config colors**; optional overrides also apply on top
     of presets; `fg/bg/accent/status_bg` apply only to custom names (presets keep
     their palette)
   - Parse errors are impossible by design: unknown keys ignored, missing keys default
3. **Docs & business structure**:
   - New `docs/INDEX.md` — map of all documentation (users/agents/business assets)
   - New `docs/business/`: `VISION.md` (problem, personas, principles),
     `ROADMAP.md` (phases + status table), `GOVERNANCE.md` (roles, branching,
     definition of done, decision process)
   - README: full Hyprland config reference, "Create your own theme" section,
     preset-tinting example; AGENTS.md: layout + config format notes

### Compatibility notes
- Old `config.toml` files are not auto-migrated: delete/rename it and let the app
  write a fresh `config.conf` (Settings `Ctrl+S`), or hand-write the new format.
  Behavior in-app is unchanged.
- `KeybindingsConfig` serde/legacy helpers untouched; legacy `tui/mod.rs` theme
  methods (`accent_color()`, `status_bg_color()`) untouched.

### Tests
- New/updated unit tests: hypr parsing (comments, quotes, unknown keys, one-line
  sections, bool spellings), hypr round trip with custom theme colors,
  hand-written-file load, temp paths renamed to `.conf`
- New BDD scenario: `given_custom_theme_colors_when_mapped_then_overrides_apply`;
  `given_theme_preset_when_mapped_then_colors_change` updated to the custom-theme
  semantics (custom names apply fg/bg/accent)
- **Validation**: `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
  · `cargo test` → **83 passed / 0 failed** (70 lib + 13 BDD)

---

## 🎯 Session 3: Settings screen — editor, defaults, theming, keybindings (completed)

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

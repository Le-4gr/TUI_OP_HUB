# WORK.md — Active Work Log (AI Agents)

> **STATUS: ONGOING NOW — this file is the live hand-off sheet for AI agents working on the TUI.**
> Update it at the end of every work session: what was done, what broke, what's next.

---

## 📋 Story Backlog — priority order (say "continue stories" to work the next item)

1. **✅ DONE (Session 18) — Navigation restructure (US-TUI-11/12)**: Knowledge Base parent
   page (key 2) with live counts; digits now 1-9 + 0 (0 = Settings); Tab cycle configurable
   via `[tui].tab_order` in config.conf, Settings last by default.
2. **✅ DONE (Session 19) — Config management page (US-CFG-09..12)**: Configs tab (key 6) — register via `n`, deploy targets via `t`, mode cycle via `m`, deploy `l`, update+drift `u`, git `g`.
3. **🟡 P2 — Plugin project templates (US-PLG-14/15, US-PROJ-08)**: plugins ship scaffold
   templates (files/folders/git/commands); optional (never default) template picker in the
   project creation flow; preview + edit before apply.
4. **🟡 P2 — Plugin UI actions (US-PLG-13)**: plugins register on-screen buttons
   (label/target defined in plugin code), capability-gated; e.g. "Create starting files"
   button on a project view.
5. **🟡 P2 — Visual-scripting logic gates (US-FUT-07)**: AND/OR/NOT/XOR/comparison/if-else
   nodes with typed boolean ports in the visual workflow builder.
6. **🟢 P3 — Parked future areas** (backend exists, UI not planned yet): US-PROC
   (process manager — use btop/htop, don't rebuild), US-PKG, US-ENV, US-BAK, US-DB.
7. **🟡 leftovers**: US-SSH-04 host grouping/tags; US-SSH-05 connection *test* (currently
   quick-connect only); plugin approval workflow for headless service runs.

---

## 🎯 Session 19c: Configs numpad + sticky-search hardening (completed)

User still saw dead keys on Configs. Two causes addressed:

1. **Numpad navigation added to Configs** (NumLock on): `2`/`8` up+down, `4`/`6` cycle deploy
   mode, `7`/`9` home/end. Note: on Configs these digits are list navigation, NOT tab jumps —
   leave the tab with `0` or arrows. Hints updated.
2. **`/` was NOT state-gated**: pressing it on any non-list screen activated an INVISIBLE search
   mode that silently ate every following keypress (exactly "all keys dead"). Now `/` only
   starts search on the five list tabs (Knowledge, Projects, Workflows, Secrets, Configs).

Tests: numpad nav/cycle test + real-flow test (6 → n → Esc → digits). 225 total (177 lib +
48 BDD), clippy 0, build ok.

IF THE USER STILL SEES DEAD KEYS: have them press Esc first (a modal may be open — import
popup `I`, keybinds `?`, options `i`, ssh panel `H` all swallow keys while open) and make
sure they rebuild with `cargo run` (a stale release binary predates the Configs tab).

---

🎯 Session 19b: Configs tab keybind fix (completed)

User report: none of the keybinds work on the Configs page. Root cause: `AppState::Configs`
was missing from ALL the state-gated navigation matches — Up/Down/PageUp/PageDown/Home/End
and the search filter. The `n/t/m/l/u/g` block worked, but everything else fell into `_ => {}
silently. Fix: added Configs to all six navigation matches + `apply_search_filter`.

Regression tests: arrows/Home/End move the configs selection; `/` search filters and Esc
restores. **223 total (175 lib + 48 BDD), clippy 0, build ok.**

LESSON for future agents: when adding a new list-tab AppState, grep for `AppState::Secrets => self.secrets_list`
and add the new state to every one of those matches — there are 8 of them (Up/Down/PgUp/PgDn/Home/End/
search/refresh) plus the dispatcher arm, hints arm, render_base arm and the action arms.

---

🎯 Session 19: story #2 — Config management page (US-CFG-09..12) (completed)

**New Configs tab (key 6)** for managed config files (dotfiles etc.):

- **`n` register**: path popup (~ expanded) stores a master copy of an existing file in the
  central store (`~/.config/tui-op-hub/configs/<id>/v1.conf`) and persists it in a JSON registry.
- **`t` add target / `l` deploy / `m` mode (US-CFG-10)**: one config can deploy to MANY targets;
  modes: symlink, hard link (same-inode verified), or plain copy.
- **`u` update (US-CFG-11)**: syncs the source into the master (bumps the version when it changed),
  re-deploys all targets and reports DRIFT (stale copies / broken links) per target.
- **`g` git (US-CFG-12)**: the whole config store is one git repo — init (idempotent), commit, log.
- **`d` delete** with confirmation removes the entry + master copy.

Backend: `config_manager.rs` gained `DeployMode`, `Deployment`, registry load/save,
`register_existing`, `remove_entry`, `sync_source`, `deploy`/`deploy_one`, `link_points_to`,
`git_init/commit/log`. Digit mapping now 1-7 + 0 (6=Configs, 7=Plugins); Tab cycle 8 entries.

Tests: +13 (10 config_manager unit tests incl. drift/hard-link/git-skip, 3 TUI tests driving
the popups over real temp files, 1 BDD lifecycle scenario register->deploy->drift->update->delete).
**221 total (173 lib + 48 BDD), clippy 0, build ok.**

Gotchas: fresh deploys are NOT drift (only stale/broken ones); `DeployMode` field missing in old
test literals was a compile error, serde defaults keep old registry JSONs loading.

---

## 🎯 Session 18c: Knowledge redesign — ONE tab + type filter (completed)

User feedback: Commands/Apps/Scripts must NOT be separate tabs — one parent tab with a
filter; and the KB page swallowed keys (its `_ => {}` ate digits/Tab/q).

- **AppState::Commands/Apps/Scripts REMOVED** (65 sites collapsed into `AppState::Knowledge`);
  the Knowledge tab now hosts the entity list directly (navigation/run/copy/edit work there).
- **`KbFilter` enum** (All -> Cmd -> App -> Script -> All): **`f` cycles the filter** and refetches;
  title shows `Knowledge . <label>`; entity rows carry type icons.
- **`n`** creates an item pre-set to the CURRENT filter type (All -> cmd; form can still cycle).
- **New digit mapping**: 1 Dash . 2 Knowledge . 3 Projects . 4 Workflows . 5 Secrets . 6 Plugins . 0 Settings.
  Unmapped digits (7-9) do nothing. Default Tab cycle = 7 entries, Settings last.
- Removed: cards page, kb_selected, knowledge_counts, kb_all_view, fetch_commands, open_knowledge_subpage;
  `fetch_knowledge()` replaces them. `delete_all_in_tab` (dev) deletes per the active filter.
- Tests rewritten as filter tests; digit tests + 2 BDD scenarios re-pinned; fuzzy `dck` scenario now
  covers the All list (docker, kubectl, docker compose, lazydocker). **209 total (162 lib + 47 BDD),
  clippy 0, build ok.**

Docs: README (digits, `f` filter). **Next: backlog item 2 — Config Management page (US-CFG-09..12).**

---

🎯 Session 18b: Knowledge page — type picker + combined All view (completed)

User follow-up to US-TUI-11: the Knowledge Base page is now the real hub for creating.

- **4th row “All”**: opens the Commands list in a combined view (`kb_all_view`) showing
  cmd + app + script together (workflows/options excluded), sorted by name, with the list
  title “📚 Knowledge — All”. Direct digit jumps (3/4/5) reset the combined view.
- **`n` on the Knowledge page = type picker**: creates a new item pre-set to the type of
  the SELECTED row (row 0 → cmd, row 1 → app, row 2 → script; “All” defaults to cmd).
  Footer hint and `?` overlay updated.
- **Entity type icons** in `Display` (💻 cmd, 🚀 app, 📜 script, ⚙️ wf, 🔘 opt) so the
  mixed list stays readable — icons now also show on the single-type lists.
- Tests +3 (type-picker mapping incl. the “cmd/script/app” index swap gotcha, All-view
  filtering with wf/opt excluded + reset on digit jump, Display icons). **209 total
  (162 lib + 47 BDD), clippy 0, build ok.**

---

## 🎯 Session 18: story #1 — Knowledge Base parent page + configurable tab order (completed)

Implemented backlog item 1 (US-TUI-11/12 ✅):

- **`AppState::Knowledge`** — a parent page (key **2**) with a description and LIVE item
  count for each of Commands / Apps / Scripts; ↑↓ select, Enter opens the subpage
  (3/4/5 also jump directly). `fetch_knowledge_counts()` queries per-type counts.
- **Digit remap (US-TUI-11/12)**: 1 Dashboard · 2 Knowledge · 3 Commands · 4 Apps ·
  5 Scripts · 6 Projects · 7 Workflows · 8 Secrets · 9 Plugins · **0 Settings (last)**.
  `handle_tab_digit` now accepts `0` too.
- **Configurable Tab cycle (US-TUI-12)**: `TuiConfig.tab_order` (comma-separated ids:
  dash,kb,cmd,app,script,proj,wf,sec,plug,set) read from `config.conf` (`tui.tab_order`),
  saved back on Ctrl+S. Garbage/empty values fall back to the default order whose LAST
  entry is Settings. Settings screen still consumes Tab for row navigation (documented).
- Tests: 4 new (KB open→select→Enter opens Scripts, default cycle ends with Settings,
  custom order honored, garbage order falls back); 2 digit tests + 2 BDD scenarios
  updated for the new mapping. **206 total (159 lib + 47 BDD), clippy 0, build ok.**
- Gotcha: `connect_lazy` in non-async tests still needs a tokio context for pool internals
  — mark such tests `#[tokio::test]`.

Docs: README (tab table, `[tui].tab_order` reference, Settings key = 0), AGENTS.md layout,
USER_STORIES (US-TUI-11/12 ✅), backlog item 1 marked done — **next up: backlog item 2,
the Config Management page (US-CFG-09..12)**.

---

## 🎯 Session 17: story batch — workflow cancel, admin users panel, SSH host manager (completed)

**Prioritised the full remaining-story list** (see the Story Backlog at the top of this file):
implemented the three most valuable/ready items this session; the rest are queued in order.

### 1. US-WF-09 — Workflow stop/cancel ✅
- `WorkflowContext.cancel: Arc<AtomicBool>` checked before every step; cancelled runs are
  recorded as failed with "cancelled by user" and partial `steps_completed`.
- Run registry (`ACTIVE_RUNS`) shared by TUI + API: `cancel_run(run_id)`, `active_run_ids()`,
  `unregister_run` on all three engine exit paths. Runs are registered *before* the spawned
  task starts, so cancellation works immediately after spawn (race fixed).
- `workflow::spawn_workflow_run()` — TUI no longer awaits workflows inline; runs execute in
  the background and are polled in `handle_key` + the `run()` loop; result popup + history
  still recorded (US-WF-08). Second concurrent run is blocked with a status hint.
- **`X` on Workflows cancels**; API `GET /runs` + `POST /runs/{run_id}/cancel`.

### 2. Admin user management panel ✅ (backend existed, UI was missing)
- **Settings → `u`** (admin only; non-admins get a status message): lists all users with
  admin badges, `d` + Enter deletes a user (secrets cascade), Enter on a user resets their
  password to `reset-me` (old key undecryptable — documented). Esc closes.
- Wired inside `handle_settings_key` (after text-edit/capture modes) so it does not swallow
  typing; the global `u` arm in `handle_key` is unreachable for Settings and left for other
  screens.

### 3. US-SSH-01..03/06 — SSH host manager ✅
- New `repository::update_ssh_host` (NotFound on unknown id) alongside existing CRUD.
- **Secrets → `H`** opens the host panel: list with user@host:port, `n` create / `e` edit
  (5-field form: name, hostname, port, username, key path — Tab cycles, Ctrl+S saves),
  `d`+Enter delete with confirm, **Enter/c quick-connects** via
  `ssh -i key -p port user@host` in a NEW terminal window (US-SSH-05).

### Tests: 202 total (155 lib + 47 BDD), 0 failures
- workflow cancel_tests (pre-cancelled context, unknown-id), repository ssh round-trip,
- TUI panel_tests: cancel-without-run status, non-admin blocked, admin delete flow,
  SSH create→connect-command shape→delete, SshForm::connect_command unit test,
- BDD: background run cancelled cooperatively (sleep steps, run unregistered),
  SSH host CRUD round trip.

### Gotchas for next agents
- `SshForm.port` is pre-filled "22" — tests must Backspace before typing a port.
- Users list is sorted by username: "bob" < "default" (test selection order).
- `X` (cancel) vs `x` (export): deliberate case distinction on Workflows.

---


## 🎯 Session 16 — COMPLETE SESSION SUMMARY (all work in this chat)

> **Update 16 (same session):** Requirements captured — 11 new user stories (USER_STORIES.md
> §22 + dashboard updated to 141 total). Five features from user feedback: (1) navigation
> restructure — Commands/Apps/Scripts become subpages of a Knowledge Base parent page,
> Settings last, tab order configurable (US-TUI-11/12); (2) Config Management page completion
> — register existing files/folders or create new, deploy via symlink/hardlink/copy with
> update + drift, per-config git (US-CFG-09..12); (3) plugin UI actions — mods register
> buttons with labels from plugin code, e.g. Create starting files on a project (US-PLG-13);
> (4) plugin-driven project scaffold templates — opt-in only, never by default, preview +
> customize before apply (US-PLG-14/15, US-PROJ-08); (5) logic gates for visual scripting —
> AND/OR/NOT/XOR/comparison/if-else nodes with typed boolean ports (US-FUT-07). Docs
> refreshed: README rewritten (449 -> ~340 lines; merged duplicate structure/license/tech
> sections, fixed port 3000->3001 refs and corrupted table row, full keybinding + API +
> layout tables matching the 9-tab UI), docs/INDEX.md table repaired, ARCHITECTURE.md
> module paths updated to the flat layout, MODERN_TUI_GUIDE.md future-checklist synced.

> **Update 15 (same session):** Full file-tree normalisation pass (continued restructure).
> Rule now enforced uniformly: a folder exists ONLY when the module has real submodules;
> every single-file module is a flat <name>.rs. Flattened: auth, keygen, plugin, api,
> models, repository, workflow, config (mod.rs -> <name>.rs). Merged seed_data.rs into
> seed.rs (its only consumer). Grouped share.rs + share_crypto.rs into share/ (mod.rs +
> crypto.rs — the one new folder, justified by the crypto submodule); all crate::share_crypto
> paths repointed to crate::share::crypto. lib.rs shrank to 23 module lines. Result: 32 files,
> 18.4k lines, 4 folders (share, secrets, db, tui) + flat everything else, zero orphan mod.rs.
> 147 lib + 45 BDD = 192 pass, clippy 0 errors.

> **Update 14 (same session):** Structure normalised (uniform folder rule).
> Rule: folders only when a module has multiple files; single-file modules are flat.
> Flattened: config_manager, environment, process, scheduler, service (mod.rs -> <name>.rs).
> Deleted dead code: the 1342-line legacy TUI in tui/mod.rs (App/Tab/EntityForm/SecretForm/
> ProjectForm — zero external references, fully superseded by modern_app/modern_ui; slim
> 12-line module file written) and the unreferenced tui/login_view.rs (331 lines, LoginView
> never constructed; modern_app has its own render_login/render_signup). src/ is now 33 files,
> 18.4k lines (was 34 files / 20.1k): 10 folders with real submodules (api, auth, config, db,
> keygen, models, plugin, repository, secrets, tui, workflow), everything else flat.
> 147 lib + 45 BDD = 192 pass, clippy 0 errors.

> **Update 13 (same session):** Continuation pass — dedup, dead code, sudo routing fix.
> **Bug found & fixed:** `handle_sudo_password_key` had zero call sites (same bug class as
> the project form) — the `R` sudo-password popup rendered but typed characters leaked into
> the list handler. Now routed top-of-stack; regression test added (chars stay in the
> password buffer, Esc cancels and clears both fields). Dedup: `secrets::decrypt_for_user_id`
> was a byte-identical copy of `decrypt_for_user` — deleted, 5 call sites (api, share,
> modern_app x2, bdd) repointed. Dead code removed: `import_knowledge_base` (superseded by
> import_knowledge_from_path), `render_dashboard_content` + `render_card` (old UI),
> `default_import_path` (duplicate of default_bundle_path). Moved `dirs_home` to helpers.rs.
> modern_app.rs 7276 -> 7263; modern_ui.rs 1140 -> 1063. 150 lib + 45 BDD = 195 pass.

> **Update 12 (same session):** Picker fixes + code-base slimming. Picker chain no longer
> falls through after a backend runs — quitting yazi with `q` closes the picker instead of
> opening dolphin/zenity (cancelled = stop; only failed-to-start moves to the next backend).
> GUI dialogs (zenity/kdialog) now run exactly once capturing stdout (previously ran twice).
> Blackscreen after yazi fixed: callers set needs_full_redraw after suspended pickers.
> Reorganised: free helpers (default_bundle_path, expand_tilde, humans, some_if_not_empty,
> TERMINAL_EMULATORS, terminal_window_command/_for, which_program) extracted to the new
> `src/tui/helpers.rs`; dead `new_project_git_name` removed; duplicate default_import/_export
> paths consolidated into default_bundle_path. modern_app.rs 7421 -> 7276 lines. 194 pass.

> **Update 11 (same session):** File pickers + duplicate strategies (US-CMD-01).
> New `src/filepicker.rs`: picker chain yazi (--chooser-file) -> nnn (-p) -> ranger
> (--choosefile/--choosedir) -> lf -> zenity -> kdialog, running TUI pickers suspended
> (raw mode off, alt screen left, restored after). Import popup: Ctrl+O opens the picker,
> Ctrl+D cycles the duplicate strategy (skip/overwrite/rename) shown live in the popup;
> share.rs gained DuplicateMode + ImportReport {imported, skipped, overwritten, renamed}
> and import_knowledge_with_mode (rename produces -imported suffixed names, children
> follow renamed parents); API POST /import?duplicates=overwrite|rename|skip. Export: `x`
> opens a save-path popup with Ctrl+O directory picker, Enter writes the bundle. Docs:
> IMPORT_EXPORT.md rewritten with a strict copy-paste LLM prompt + validation checklist +
> duplicate docs. Tests: +3 picker unit tests, +2 BDD (overwrite replaces local, rename
> creates -imported copy). 149 lib + 45 BDD = 194 pass.

> **Update 10 (same session):** Dashboard mini-btop monitor + quick launches (US-PROC-01).
> New `src/monitor.rs`: persistent sysinfo state; snapshot = CPU overall + per-core usage,
> RAM/swap, physical network interfaces with live RX/TX rates (docker bridges, veth pairs,
> loopback, VPN tunnels filtered out by `is_physical_interface`), temperature sensors
> (sysinfo Components), GPU stats via nvidia-smi (best effort), and installed TUI tool
> detection (lazygit / lazydocker / k9s / lazynpm). Dashboard layout: stat cards row on top,
> four compact monitor boxes with ASCII bars + per-core mini bars, quick-launch row at the
> bottom listing installed tools with their keys. Auto-refresh every 2s while on the
> Dashboard (persistent Monitor instance keeps rate deltas meaningful). Quick launches
> `g` lazygit / `d` lazydocker / `k` k9s / `n` lazynpm open the tool in a NEW terminal
> window (terminal_window_command_for with cwd = ~/projects; skipped with a message when
> not installed). Tests: +3 monitor unit tests (virtual-interface filter, bar renderer,
> snapshot builds). 146 lib + 43 BDD = 189 pass.

### What this session delivered (chronological, 14 commits on `dev`)

Final state: **186 tests pass (143 lib + 43 BDD), clippy 0 errors, release build clean.**
Detailed per-change notes are in the Update 1-9 blocks further down.

| # | Commit | What was done |
|:--|:--|:--|
| 1 | `57cc1ca` | **Terminal fix + visible search bar + x/I keybinds**: backtick sets a flag handled in run() (old approach bypassed ratatui and black-screened); `/` replaces the footer with a bordered search bar; `x`/`I` export/import knowledge base |
| 2 | `001d72f` | **Search bar polish + real subshell routing** (intermediate fix; superseded by #3) |
| 3 | `102cdf3` | **Backtick opens a NEW terminal window** (detached emulator spawn: $TERMINAL override, then alacritty, kitty, wezterm, gnome-terminal, konsole, xfce4, tilix, foot, xterm, st, uxterm); dead spawn_terminal removed; unit tests for the emulator probe |
| 4 | `8d47923` | **Apps + Scripts tabs** (8 tabs, keys 1-8): one entity list filtered by type_id (cmd/script/app); `n` pre-sets the form type per tab; **project detail view** (Enter: description + entities, `c` copy, `o` editor, Esc close - US-PROJ-07); duplicate overlay rows fixed |
| 5 | `e4d2439` | **Cron scheduling + systemd/cron service + import/export docs**: validate_cron normalizes 5-field crontab to the cron crate seconds syntax; API POST /workflows/{id}/schedule, GET/DELETE /schedules, GET /export, POST /import; TUI `s` on Workflows (cron popup); new src/service/ (systemd user unit generation, --headless, --install-service, --print-unit); docs/IMPORT_EXPORT.md with AI prompt template |
| 6 | `d6d40d3` | **Release install flow**: --install-service is a full idempotent setup (generates master key ~/.config/tui-op-hub/env mode 600, unit EnvironmentFile=, enable); **TUI/service coexistence** (shared WAL DB; API port-in-use warns + skips, no panic); root install.sh; home-injectable *_in() service functions; docs/INSTALL.md |
| 7 | `ca389db` | **Root INSTALL.md**: quick install, manual, verify, TUI launch, secrets key, non-systemd inits, update/uninstall, file paths |
| 8 | `bc628d4` | **Better import**: `I` opens a path popup (default pre-filled, ~ expansion); lenient parsing - full bundle, bare AI entity array, or entities-only object; API import accepts raw lenient bodies |
| 9 | `5c779a6` | **AGENTS.md updated**: 8 tabs, service/scheduler modules, CLI flags, new docs, and two hard-won Do-NOT lessons (verify scripted edits by grepping; never use unicode escapes in Python heredocs) |
| 10 | `9e26a64` | **Project creation flow fixed**: root cause - handle_new_project_key was never routed, so form keystrokes leaked into other handlers (typing `p` spawned a process viewer mid-form!); migration 0005 projects.path; workspace path stored; `O` opens the real directory; `n` = register existing directory |
| 11 | `d27697c` | **Plugin/mod system**: migration 0006; Lua mods in ~/.config/tui-op-hub/plugins/<id>/ (plugin.toml + main.lua); capability-gated sandbox (run_command only with execute_commands); event hooks - **project_created fires on workspace create/register** (the git-automation point); Plugins tab (key 9): list/approve/enable; docs/PLUGINS.md |
| 12 | `810e4d5` | **Digits 1-9 switch tabs from ANY screen**: root cause - Settings/Advanced ate digits as numpad navigation; new jump_to_tab_digit/handle_tab_digit; numpad handlers removed (arrows/L-R remain); 3 BDD scenarios updated |
| 13 | `a80a529` | **Secrets v2**: migration 0007 (secret_group, username, url, email, passphrase_protected, ssh_agent); 8-field secret form; passphrase double-encryption (Argon2+XChaCha layer over the user-key layer); copy prompts for locked secrets; **ssh-agent integration** (S loads flagged ssh_key secrets, auto-load after login, agent auto-spawned); **`t` opens an SSH terminal** using the stored key to url |

### Key architecture changes

- `src/service/` — systemd/init integration (unit generation, install/uninstall, cron line)
- `src/secrets/ssh_agent.rs` — agent ensure/spawn + key loading
- Migrations 0005 (projects.path), 0006 (plugins/plugin_approvals), 0007 (secrets v2)
- AppState grew: Apps, Scripts, Plugins (now 9 tabs, keys 1-9)
- ModernApp popups: project detail, cron input, import path, register dir, secret passphrase
- Global behaviors: digits 1-9 = tabs everywhere; backtick = new terminal window

### Known limitations / next steps

- Passphrase-locked secrets are skipped by `S` (agent load); use `t` for those
- Plugin event set currently only project_created; more hooks easy to add
- Rust/Python/Go plugin types still unimplemented (Lua is the mod language)
- plugin/, process/, environment/ test coverage still thin

---

### Detailed update-by-update log (chronological, newest last)


> **Update 9 (same session):** Secrets v2 (US-SEC). Migration 0007 adds `secret_group`,
> `username`, `url`, `email`, `passphrase_protected`, `ssh_agent` to secrets. The secret
> form grew to 8 fields (name, value, group, username, url, email, passphrase, ssh-agent
> toggle via Space). Passphrase-protected secrets are double-encrypted (passphrase layer
> via share_crypto Argon2+XChaCha, then the user-key layer); copy prompts for the
> passphrase in a popup and reports wrong passphrases. ssh-agent integration: `S` on
> Secrets offers all flagged ssh_key secrets to the running agent (starting one if needed),
> keys auto-load after login; `t` opens an ssh terminal to `url` using the stored key
> (temp 0600 keyfile, cleaned up after). Secrets list shows group + username + [locked].
> Tests: +3 passphrase round-trip unit tests, +1 ssh-agent test, +1 repo meta round trip
> (via existing suites). 143 lib + 43 BDD = 186 pass.

> **Update 8 (same session):** Digits 1-9 now switch tabs from ANY screen (US-APP-02).
> Root cause of "stuck in Settings": the Settings/Advanced screens repurposed the digits
> as vim-style numpad navigation (2=down, 8=up, 1/3=end, 7/9=home, 4/6=cycle theme), so
> pressing any digit inside Settings never reached the tab switcher and users had to Esc.
> New `jump_to_tab_digit` / `handle_tab_digit` helpers intercept plain digits 1-9 at the
> top of both screens (after keybinding-capture and text-editing flows so digits can still
> be bound or typed); the numpad handlers were removed — arrows/Tab move rows, Left/Right
> cycle theme colors. Updated 3 BDD scenarios that pinned the old numpad behavior + added
> a regression test (Settings -> digit -> correct tab). 139 lib + 43 BDD = 182 pass.

> **Update 7 (same session):** Plugin/mod system implemented (US-PLG-05/06/07/09/10).
> New migration 0006 replaces the never-used 0001 plugin stubs (plugins + plugin_approvals
> with the runtime schema). Plugin sandbox: fresh mlua state per hook, host functions gated
> by capabilities (log always; run_command only with execute_commands); `event` global
> carries the hook payload. PluginManager gained: discovery (scan plugins dir),
> `emit_event` dispatch, approve-before-load stub rows, enable/disable with unload/reload,
> approval + enabled gating in `load_plugin`. Wired into main (startup load from
> ~/.config/tui-op-hub/plugins) and ModernApp (manager field); project creation and
> directory registration fire `project_created` {name, path} — the git-automation hook
> point. New **Plugins tab** (key 9): list discovered mods with status, `a` approve,
> `e` enable/disable. New doc: `docs/PLUGINS.md` (manifest, hook API, full git example,
> security model). Tests: +5 plugin unit tests (discovery, approval gating, hook execution
> with side effects, no-hook noop, enable/disable round trip) + 1 BDD git-automation
> scenario (hook runs real `git init` in the new project). 138 lib + 43 BDD = 181 pass.

> **Update 6 (same session):** Fixed the project creation flow (US-PROJ-01/04).
> ROOT CAUSE found: `handle_new_project_key` was never routed from `handle_key` — the
> workspace form captured no keys, so typed characters leaked into other handlers
> (typing a name containing `p` spawned a process viewer mid-form!). Now routed top-of-stack.
> Also: new migration 0005 adds `projects.path`; workspace creation stores the created
> directory; `O` opens the project's real path (fallback `~/projects/<name>`, clear error
> if neither exists — previously it opened the CWD, which was wrong); `n` on Projects now
> registers an **existing directory** (path popup, base name becomes project name) instead
> of a duplicate DB-only form; project detail shows `dir: <path>`; overlay + footer hints
> updated (N = New ws, n = Register). Tests: +2 TUI (workspace path stored, register flow),
> +1 BDD (path persisted). 134 lib + 42 BDD = 176 pass.

> **Update 5 (same session):** Better file import (US-CMD-01). `I` in the TUI now opens a
> **path-input popup** (default `~/tui-op-hub-export.json` pre-filled, `~` expansion, edit +
> Enter) instead of importing a fixed file. `share::bundle_from_json` parses **leniently**:
> full bundle, bare AI-generated entity arrays, or `entities`-only objects (defaults applied).
> API `POST /import` accepts raw bodies in all three shapes too. Tests: +4 share unit tests,
> +1 TUI popup end-to-end test, +1 BDD bare-array scenario. 132 lib + 41 BDD = 173 pass.

> **Update 4 (same session):** Release/install flow with always-on backend (US-DEP-04).
> `--install-service` is now a full idempotent setup: generates the master key into
> `~/.config/tui-op-hub/env` (mode 600, never overwritten), unit references `EnvironmentFile=`,
> enables the user service. Service runs `--headless` (scheduler + API) permanently; the TUI
> still launches on demand and **coexists** with the service (shared WAL database; API
> port-in-use is now handled gracefully instead of panicking). New root `install.sh`:
> build release + install binary + enable service. Service functions are home-injectable
> (`*_in(home)`) for testing. New doc: `docs/INSTALL.md`. Tests: +3 service unit tests
> (env file generation/key validity/permissions/idempotency, unit+env install, idempotent key).

> **Update 3 (same session):** Cron + init-system + import/export docs (US-WF-07, US-DEP-04).
> New `src/service/mod.rs`: systemd user-unit generation/install/uninstall + cron watchdog
> line; CLI flags `--headless`, `--print-unit`, `--install-service`, `--uninstall-service`
> (no clap, manual parsing in `main.rs`). `validate_cron` normalizes classic 5-field crontab
> to the `cron` crate's seconds-field syntax; API endpoints `POST /workflows/{id}/schedule`,
> `GET /schedules`, `DELETE /schedules/{id}`, `GET /export`, `POST /import`; repo fn
> `delete_scheduled_task`; TUI `s` on Workflows opens a cron input popup. New doc:
> `docs/IMPORT_EXPORT.md` (bundle schema + AI prompt template for generating commands/
> scripts/apps + cron/systemd setup). Tests: 4 service unit tests, 5 new BDD scenarios
> (export/import round trip, local-wins merge, option re-attachment, schedule CRUD,
> invalid cron rejection). 123 lib + 40 BDD = 163 pass.

> **Update 2 (same session):** Added **Apps** and **Scripts** tabs — the TUI now has
> 8 tabs (1=Dashboard, 2=Commands, 3=Apps, 4=Scripts, 5=Projects, 6=Workflows,
> 7=Secrets, 8=Settings). All three entity tabs share one list state filtered by
> `type_id` (cmd/script/app); `n` pre-sets the form type to the current tab.
> Finished the **Projects flow**: Enter opens a project detail popup (US-PROJ-07)
> showing description + all project entities with type icons, Up/Down navigation,
> `c` copy entity content, `o` open workspace editor, Esc close. Fixed duplicated
> rows in the keybinds overlay, updated footer hints (1-8 tabs, Projects Enter/O).
> Tests: +4 unit tests (tab filtering, number-key mapping, form type preset,
> project detail) + `test_app_db` helper + 2 BDD scenarios. 119 lib + 35 BDD = 154 pass.

> **Update (same session):** backtick now opens a **NEW terminal window** instead of
> suspending the TUI. The run loop spawns a detached terminal emulator
> (overrides: `TUI_OP_HUB_TERMINAL` / `TERMINAL`; fallback order: alacritty, kitty,
> wezterm, gnome-terminal, konsole, xfce4-terminal, tilix, foot, xterm, st, uxterm)
> and shows a status message with the chosen program, or an error if none found
> (set `$TERMINAL`). Dead `spawn_terminal` method removed, duplicated keybinds-overlay
> row fixed, footer hint renamed to "New term", unit tests added for
> `which_program` / `terminal_window_command`. 148 lib + 33 BDD tests pass.

**Goal: fix the terminal (black screen after shell), make the `/` search bar
prominently visible, wire import/export to TUI keybinds, update keybinds overlay.**

### What was done and how
1. **Terminal fix**: `spawn_terminal` now clears the screen before spawning
   the shell (so the shell gets a clean terminal), and after the shell exits
   the `needs_full_redraw` flag forces ratatui to do a full repaint. The
   `terminal.clear()` call in the run loop ensures no stale buffer.
2. **Search bar**: When `/` is active, the footer area is **replaced** with a
   dedicated search bar showing `/ query█` (block cursor) in warning color.
   The list title changes to `🔍 query (N results)`. When Esc is pressed the
   footer reverts to keybind hints.
3. **Import/export TUI keybinds**:
   - `x` on Commands/Workflows/Secrets tabs exports the knowledge base to
     `~/tui-op-hub-export.json` (excludes secrets for safety)
   - `I` on any tab imports from `~/tui-op-hub-export.json`
   - Both wired in `handle_dashboard_key`, available in all list states
4. **Keybinds overlay + footer updated**: `x` Export, `I` Import added to
   Commands section of the overlay and the Commands footer hints.

### Validation
- `cargo fmt` ✔ · `clippy --all-targets` 0 errors ✔ · `cargo build` ✔
- `cargo test` → **146 passed / 0 failed** (113 lib + 33 BDD)

---

## 🎯 Session 15: black-screen fix, visible search bar, project workspaces (completed)

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

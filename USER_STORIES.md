# 🎛️ TUI-OP-HUB — User Stories

> 📌 **Source:** `bp.md` · `STACK_AND_TOOLS_GUIDE.md` · `DB/LOGICAL.drawio` · `TUI-OP-HUB/src/main.rs`
>
> 📝 **Format:** `As a <role>, I want to <action>, so that <value>.`
>
> 🏷️ **Priority legend:** 🔴 P0 MVP · 🟡 P1 Near-term · 🟢 P2 Future
>
> 📊 **Story points:** Fibonacci scale (1, 2, 3, 5, 8, 13) — rough effort estimates.

---

## � Phase Completion Status

### ✅ Phase 1 — Complete

**Core Features Implemented**:
- ✅ `US-CMD-01` through `US-CMD-09`: Full command/script storage and management (CRUD, copy, run)
- ✅ `US-PROJ-01` through `US-PROJ-07`: Project grouping and context switching
- ✅ `US-SRCH-01` through `US-SRCH-04`: Full-text search (FTS5) and filtering by tags/type/project
- ✅ `US-TUI-01` through `US-TUI-10`: Complete TUI with 7 tabs, keyboard navigation, help overlay, theming
- ✅ `US-APP-01`, `US-APP-02`, `US-APP-06`: Configuration system with TOML, themes, keybindings
- ✅ `US-API-01`, `US-API-02`: Local API with entity CRUD endpoints
- ✅ `US-WF-01`, `US-WF-03`, `US-WF-04`, `US-WF-06`, `US-WF-08`: Workflow system with Lua scripting and run history
- ✅ `US-SEC-02`, `US-SEC-03`, `US-SEC-04`, `US-SEC-09`, `US-SEC-10`: Secrets storage with XChaCha20Poly1305 encryption
- ✅ **User Profiles**: Multi-user encryption support with per-user keys (foundation for Phase 2)

**Database Schema**: 12 tables (entities, projects, tags, types, workflow_runs, secrets, user_profiles, user_keys, plugins, ssh_hosts, scheduled_tasks, entities_fts)

**Technology Stack**:
- Language: Rust 2021 edition
- Async Runtime: tokio
- Web: axum with tower-http middleware
- Database: sqlx + SQLite (WAL mode)
- TUI: ratatui + crossterm
- Scripting: mlua (Lua 5.4)
- Crypto: chacha20poly1305 (XChaCha20Poly1305 AEAD)

---

### 🔄 Phase 2 — In progress (started this session)

**Implemented in this phase kick-off**:
- ✅ `US-WF-07`: Scheduler daemon wired into `main.rs` (cron workflows execute automatically)
- ✅ Secrets as workflow variables: `secrets.<name>` + `get_secret()` host function
  (reauth-protected secrets excluded; US-SEC-02/05)
- ✅ First user is **admin** (migration 0003 `is_admin`); admin can **delete users**
  (secrets cascade via FK) and **reset forgotten passwords** (`admin_reset_password`)
- ✅ **Secret classification** (`password`/`ssh_key`/`gpg_key`/`api_key`) +
  `requires_reauth` access control (US-SEC-01/05)
- ✅ **SSH & GPG keygen** using known tools (`ssh-keygen`/`gpg`); private key location
  and passphrase stored as encrypted secrets with tags
- ✅ **File-backed scripts & workflows** (US-WF-03, US-CMD-05): metadata
  `{"file": "…"}` runs Lua/JSON/Python/shell files with interpreter picked by
  extension/shebang
- ✅ **Type-aware execution** (US-CMD-09): `cmd` → shell, `script` → interpreter,
  `app` → launched detached
- ✅ **Projects as environments** (US-ENV): `env_type`/`env_cmd` per project,
  `E` opens a shell inside the environment
- ✅ **Processes use known programs** (US-PROC): `p` launches btop/htop/top

**Remaining Phase 2 work**:
- ✅ `US-PLG-05`, `US-PLG-06`: Plugin manifest + capability approval (Plugins tab, DB-enforced)
- ✅ `US-PLG-07`, `US-PLG-10`: Lua mod loading, event hooks, Plugins tab (list/approve/enable)
- ✅ `US-SSH-01`..`03`, `06`: SSH host manager panel (Secrets → `H`) — CRUD + quick-connect; host grouping/tags still open
- ✅ `US-WF-09`: background workflow runs with `X` cancel (TUI) + `POST /runs/{id}/cancel` (API)
- ✅ `US-DEP-04`: Systemd integration (user service, `--install-service`, `--headless` daemon mode)
- 🔄 Admin user management TUI panel (backend + API done)

**Not Yet Started** (Future/P2+):
- 🟢 `US-PROC-01` through `US-PROC-07`: Process management (future feature)
- 🟢 `US-PKG-01` through `US-PKG-09`: Package manager integration (future)
- 🟢 `US-ENV-01` through `US-ENV-08`: Environment variable/venv management (future)
- 🟢 `US-CFG-01` through `US-CFG-08`: Config file management (future)
- 🟢 `US-DB-01` through `US-DB-06`: Multi-database support (future)
- 🟢 `US-WEB-01` through `US-WEB-04`: Web UI (future)
- 🟢 `US-SYNC-01` through `US-SYNC-04`: Cross-machine sync (future)
- 🟢 `US-BAK-01` through `US-BAK-04`: Backup/export (future)

**Implementation Strategy**:
- Phase 1 ✅: Complete CRUD for entities, projects, workflows, and secrets with full TUI integration
- Phase 2 🔄: Extend Phase 1 with plugins, SSH, and scheduler (foundation laid, TUI/API integration pending)
- Phase 3+: Advanced features (process management, package management, environments, web UI, sync)

---

## �👥 Roles & Personas

| 🎭 Role | 📖 Description |
|:---|:---|
| **👤 User** | The primary operator of the Control Center on their local machine. |
| **⚡ Power User** | Writes scripts, builds workflows, and extends the hub via plugins. |
| **🛡️ Admin** | Manages system-wide configuration, secrets policy, and multi-machine sync. *(Often the same as User in a local-first app.)* |
| **🔌 Plugin Developer** | Builds Rust / Lua / Python / Go plugins to extend the hub. |
| **🤖 System** | Automated background process — scheduler, sync worker, health checks, systemd integration. |

---

## 1. 📊 Process & Resource Management

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-PROC-01` | As a **User**, I want to view a live overview of **CPU and memory usage**, so that I can monitor system health at a glance. | 🟡 | 5 |
| `US-PROC-02` | As a **User**, I want to see a list of **running processes** (btop-style), so that I can identify resource-heavy or stuck processes. | 🟡 | 5 |
| `US-PROC-03` | As a **User**, I want to **start a process** from within the hub, so that I can launch apps without leaving the terminal. | 🟡 | 3 |
| `US-PROC-04` | As a **User**, I want to **stop or kill** a process, so that I can free resources or recover from a hang. | 🟡 | 3 |
| `US-PROC-05` | As a **User**, I want to **attach metadata and labels** to a process, so that I can organize and find it later. | 🟢 | 3 |
| `US-PROC-06` | As a **User**, I want to **filter and sort** the process list, so that I can quickly locate specific processes. | 🟡 | 2 |
| `US-PROC-07` | As a **User**, I want to **restart** a managed process, so that I can apply changes without manual stop/start. | 🟢 | 2 |

---

## 2. 📚 Command & Knowledge Base

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-CMD-01` | As a **User**, I want to **store commands** with a name and description, so that I can build a personal knowledge base. | 🔴 | 3 |
| `US-CMD-02` | As a **User**, I want to **save scripts and sequences** of commands, so that I can reuse complex operations. | 🔴 | 5 |
| `US-CMD-03` | As a **User**, I want to **tag** commands and scripts, so that I can filter by topic or context. | 🔴 | 2 |
| `US-CMD-04` | As a **User**, I want **full-text search** across my command knowledge base, so that I can find a command by keyword. | 🔴 | 5 |
| `US-CMD-05` | As a **User**, I want to **attach examples** to a command, so that I remember usage and flags. | 🟡 | 2 |
| `US-CMD-06` | As a **User**, I want to **categorize** commands (e.g., network, git, system), so that the knowledge base stays organized. | 🟡 | 2 |
| `US-CMD-07` | As a **User**, I want to **edit or delete** stored commands, so that my knowledge base stays accurate. | 🔴 | 2 |
| `US-CMD-08` | As a **User**, I want to **copy** a stored command to my clipboard, so that I can paste it into another shell. | 🟡 | 1 |
| `US-CMD-09` | As a **User**, I want to **run** a stored command directly from the hub, so that I don't need to leave the app. | 🟡 | 3 |

---

## 3. ⚙️ Automation & Workflows

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-WF-01` | As a **Power User**, I want to create **linear workflows** that chain commands, so that I can automate repetitive multi-step tasks. | 🟡 | 8 |
| `US-WF-02` | As a **Power User**, I want to create **DAG-based workflows** with conditional execution, so that I can model complex dependencies. | 🟡 | 13 |
| `US-WF-03` | As a **Power User**, I want to define workflows **declaratively in YAML**, so that they are versionable and readable. | 🟡 | 5 |
| `US-WF-04` | As a **Power User**, I want to script workflow steps in **Lua**, so that I can add logic without recompiling Rust. | 🟡 | 5 |
| `US-WF-05` | As a **Power User**, I want to script workflow steps in **Python**, so that I can leverage Python's ecosystem. | 🟢 | 5 |
| `US-WF-06` | As a **User**, I want to **run a workflow** on demand, so that I can execute an automation when needed. | 🟡 | 3 |
| `US-WF-07` | As a **User**, I want to **schedule** workflows (internal scheduler or systemd), so that they run automatically at set times. | ✅ | 8 |
| `US-WF-08` | As a **User**, I want to view **run history and status** of each workflow execution, so that I can verify success or debug failures. | 🟡 | 5 |
| `US-WF-09` | As a **User**, I want to **stop** a running workflow, so that I can cancel long or mistaken runs. | ✅ | 3 |
| `US-WF-10` | As a **Power User**, I want **reusable workflow pipelines**, so that I can compose automations from smaller pieces. | 🟢 | 5 |
| `US-WF-11` | As a **Power User**, I want a **visual workflow editor** (future), so that I can build DAGs without writing YAML. | 🟢 | 13 |

---

## 4. 📦 System & Package Management

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-PKG-01` | As a **User**, I want to **search packages** across multiple package managers (apt, pacman, nix, etc.), so that I can find software in one place. | 🟢 | 5 |
| `US-PKG-02` | As a **User**, I want to **install** a package, so that I can add software without remembering manager-specific syntax. | 🟢 | 3 |
| `US-PKG-03` | As a **User**, I want to **update** packages, so that my system stays current. | 🟢 | 3 |
| `US-PKG-04` | As a **User**, I want to **remove** packages, so that I can clean up unused software. | 🟢 | 3 |
| `US-PKG-05` | As a **User**, I want a **unified view** of installed packages across managers, so that I can audit what's on my system. | 🟢 | 5 |
| `US-PKG-06` | As a **User**, I want **Nix integration** to inspect configurations, so that I can understand my Nix setup. | 🟢 | 5 |
| `US-PKG-07` | As a **User**, I want to **search Nix packages**, so that I can find reproducible software definitions. | 🟢 | 3 |
| `US-PKG-08` | As a **User**, I want to **automate Nix updates**, so that my flake/config stays fresh. | 🟢 | 5 |
| `US-PKG-09` | As a **User**, I want to manage **reproducible Nix environments**, so that project setups are portable. | 🟢 | 8 |

---

## 5. 🌍 Environment Management

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-ENV-01` | As a **User**, I want to manage **environment variables**, so that I can configure tool behavior per context. | 🟡 | 3 |
| `US-ENV-02` | As a **User**, I want to create **project-specific environments**, so that each project has isolated settings. | 🟡 | 5 |
| `US-ENV-03` | As a **User**, I want to manage **Python virtual environments**, so that I can isolate Python dependencies. | 🟡 | 5 |
| `US-ENV-04` | As a **User**, I want to manage **multiple Python versions**, so that I can work across projects with different requirements. | 🟡 | 5 |
| `US-ENV-05` | As a **User**, I want to manage **other language runtimes**, so that I can switch between Node, Go, etc. | 🟢 | 5 |
| `US-ENV-06` | As a **User**, I want to **switch** between environments, so that I can context-shift quickly. | 🟡 | 2 |
| `US-ENV-07` | As a **User**, I want to **persist** environment configurations, so that setups survive restarts. | 🟡 | 3 |
| `US-ENV-08` | As a **User**, I want to **associate** environments with projects, so that opening a project loads the right environment. | 🟡 | 3 |

---

## 6. 📝 Config Management

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-CFG-01` | As a **User**, I want to **store configuration files** centrally in the hub, so that all my configs live in one place. | 🟡 | 3 |
| `US-CFG-02` | As a **User**, I want to **symlink configs** to their system locations, so that apps read them from the expected paths. | 🟡 | 3 |
| `US-CFG-03` | As a **User**, I want to **track config versions**, so that I can roll back to a previous configuration. | 🟡 | 5 |
| `US-CFG-04` | As a **User**, I want to **tag configs**, so that I can filter by purpose (e.g., shell, firewall, Hyprland). | 🟡 | 2 |
| `US-CFG-05` | As a **User**, I want to **group configs** per project or system, so that related configs stay together. | 🟡 | 2 |
| `US-CFG-06` | As a **User**, I want to **share configs** across machines, so that my setup is portable. | 🟢 | 5 |
| `US-CFG-07` | As a **User**, I want to **edit** a config file inline, so that I can make quick changes without leaving the hub. | 🟡 | 3 |
| `US-CFG-08` | As a **User**, I want to **diff config versions**, so that I can see what changed. | 🟢 | 3 |

---

## 7. 🔐 Secrets & Key Management

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-SEC-01` | As a **User**, I want to store **SSH keys** securely (encrypted at rest), so that my private keys are protected. | 🟡 | 5 |
| `US-SEC-02` | As a **User**, I want to store **API keys and tokens** (e.g., GitHub), so that I don't hardcode them in scripts. | 🟡 | 3 |
| `US-SEC-03` | As a **User**, I want to store **generic credentials**, so that I have a single vault for secrets. | 🟡 | 3 |
| `US-SEC-04` | As a **User**, I want to **group and tag** secrets, so that I can organize them by project or service. | 🟡 | 2 |
| `US-SEC-05` | As a **User**, I want **SSH agent integration**, so that keys are available to SSH sessions automatically. | 🟡 | 5 |
| `US-SEC-06` | As a **User**, I want **optional auto-unlock** of secrets on login, so that I avoid repeated passphrase entry. | 🟢 | 5 |
| `US-SEC-07` | As a **User**, I want **Git credential system integration**, so that git operations can use stored tokens. | 🟢 | 3 |
| `US-SEC-08` | As an **Admin**, I want **access control** over secrets (future), so that only authorized plugins/users can read sensitive values. | 🟢 | 8 |
| `US-SEC-09` | As a **User**, I want to **rotate or revoke** a stored secret, so that I can respond to compromise. | 🟡 | 3 |
| `US-SEC-10` | As a **User**, I want secrets **referenced (not copied)** by projects/workflows, so that a single source of truth is maintained. | 🟡 | 3 |
| `US-SEC-11` | As a **User**, I want to **set a master password** for my user profile, so that I can decrypt my secrets with a password instead of relying on environment variables. | 🔴 | 5 |
| `US-SEC-12` | As a **User**, I want to **login with my password** at startup, so that my encryption key is derived from my password and secrets are automatically decrypted. | 🔴 | 5 |
| `US-SEC-13` | As a **User**, I want to **choose between password-based or environment-based** secret encryption, so that I can use the method that fits my workflow. | 🟡 | 3 |
| `US-SEC-14` | As a **User**, I want my **session to auto-lock** after inactivity, so that my secrets are protected when I step away. | 🟡 | 3 |
| `US-SEC-15` | As a **User**, I want to **change my master password**, so that I can update my security credentials without losing access to my secrets. | 🟡 | 5 |

---

## 8. 🖥️ SSH Integration

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-SSH-01` | As a **User**, I want to manage **SSH host configurations**, so that I can keep connection details organized. | ✅ | 3 |
| `US-SSH-02` | As a **User**, I want to store **key-based authentication** per host, so that connections are secure and automatic. | ✅ | 3 |
| `US-SSH-03` | As a **User**, I want a **quick-connect interface**, so that I can open an SSH session with a single action. | ✅ | 3 |
| `US-SSH-04` | As a **User**, I want to **group hosts** by tags or projects, so that I can find the right server quickly. | 🟡 | 2 |
| `US-SSH-05` | As a **User**, I want to **test** an SSH connection, so that I can verify reachability before relying on it. | 🟢 | 2 |
| `US-SSH-06` | As a **User**, I want to **edit host details** (address, port, user, key), so that connection info stays current. | ✅ | 2 |

---

## 9. 📁 Projects

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-PROJ-01` | As a **User**, I want to **create a project**, so that I can group related resources together. | 🔴 | 3 |
| `US-PROJ-02` | As a **User**, I want a project to **contain scripts, environments, configs, workflows, and secret references**, so that everything for a project is in one place. | 🟡 | 5 |
| `US-PROJ-03` | As a **User**, I want **isolation** between projects, so that one project's settings don't leak into another. | 🟡 | 5 |
| `US-PROJ-04` | As a **User**, I want to **switch** between projects quickly, so that I can context-shift without friction. | 🔴 | 2 |
| `US-PROJ-05` | As a **User**, I want **reproducible project setups**, so that I can recreate the same environment on another machine. | 🟢 | 8 |
| `US-PROJ-06` | As a **User**, I want to **delete or archive** a project, so that my workspace stays uncluttered. | 🟡 | 2 |
| `US-PROJ-07` | As a **User**, I want to view a **project dashboard** summarizing its resources, so that I can understand its state at a glance. | ✅ | 5 |

---

## 10. 🗄️ Multi-Database / Multi-Context Support

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-DB-01` | As a **User**, I want to **switch between databases** (e.g., `personal.db`, `work.db`), so that I can separate contexts. | 🟢 | 3 |
| `US-DB-02` | As a **User**, I want to **share items** between databases, so that common commands/configs are available everywhere. | 🟢 | 5 |
| `US-DB-03` | As a **User**, I want to **mark items as local-only**, so that sensitive data stays on one machine. | 🟢 | 2 |
| `US-DB-04` | As a **User**, I want to **mark items as shared/syncable**, so that they can propagate to other machines. | 🟢 | 2 |
| `US-DB-05` | As a **User**, I want **machine-specific overrides**, so that shared configs can be tailored per host. | 🟢 | 3 |
| `US-DB-06` | As a **User**, I want to **create a new database/context**, so that I can start a fresh isolated workspace. | 🟢 | 2 |

---

## 11. ⚙️ Configuration System (App Settings)

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-APP-01` | As a **User**, I want a **global config for themes**, so that I can personalize the TUI appearance. | 🔴 | 2 |
| `US-APP-02` | As a **User**, I want to configure **UI behavior** (keybindings, layouts), so that the app fits my workflow. | 🔴 | 3 |
| `US-APP-03` | As an **Admin**, I want to configure **backend behavior**, so that the engine runs per my preferences. | 🟡 | 3 |
| `US-APP-04` | As a **User**, I want **local config overrides**, so that machine-specific settings don't affect others. | 🟡 | 2 |
| `US-APP-05` | As a **User**, I want **shared config** across machines, so that my preferences follow me. | 🟢 | 3 |
| `US-APP-06` | As a **User**, I want config in **TOML / YAML / JSON**, so that it is human-readable and version-controllable. | 🔴 | 2 |

---

## 12. 🖱️ TUI (Terminal User Interface)

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-TUI-01` | As a **User**, I want a **searchable command palette**, so that I can find and run any action quickly. | 🔴 | 5 |
| `US-TUI-02` | As a **User**, I want **tabbed windows** for Processes, Commands, Workflows, Packages, Projects, Secrets, and Config, so that I can navigate features easily. | 🔴 | 8 |
| `US-TUI-03` | As a **User**, I want **keyboard-driven navigation**, so that I can operate the hub without a mouse. | 🔴 | 3 |
| `US-TUI-04` | As a **User**, I want to **filter lists by tags**, so that I can narrow down long lists. | 🔴 | 3 |
| `US-TUI-05` | As a **User**, I want **theming support**, so that the interface matches my terminal aesthetic. | 🟡 | 3 |
| `US-TUI-06` | As a **User**, I want **extensible layouts**, so that plugins can add their own panels. | 🟢 | 5 |
| `US-TUI-07` | As a **User**, I want the TUI to be **optional**, so that the backend can run headless. | 🟡 | 3 |
| `US-TUI-08` | As a **User**, I want a **status bar** showing current context (DB, project, user), so that I always know my active environment. | 🔴 | 2 |
| `US-TUI-09` | As a **User**, I want a **help overlay** listing keybindings, so that I can learn the UI in-app. | 🟡 | 2 |
| `US-TUI-10` | As a **User**, I want the TUI to **resize gracefully** to my terminal, so that layouts remain usable at any size. | 🔴 | 3 |

---

## 13. 🔌 Plugin System

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-PLG-01` | As a **Plugin Developer**, I want to write plugins in **Rust**, so that I get native performance. | 🟡 | 8 |
| `US-PLG-02` | As a **Plugin Developer**, I want to write plugins in **Lua**, so that I can iterate quickly without recompiling. | 🟡 | 5 |
| `US-PLG-03` | As a **Plugin Developer**, I want to write plugins in **Python**, so that I can reuse Python libraries. | 🟢 | 5 |
| `US-PLG-04` | As a **Plugin Developer**, I want to write plugins in **Go**, so that I can integrate Go-based tooling. | 🟢 | 5 |
| `US-PLG-05` | As a **Plugin Developer**, I want a **plugin manifest** declaring required capabilities, so that the host can enforce permissions. | ✅ | 5 |
| `US-PLG-06` | As a **User**, I want to **approve** a plugin's requested capabilities, so that untrusted code cannot access secrets or network without consent. | ✅ | 3 |
| `US-PLG-07` | As a **Plugin Developer**, I want to **add commands/tools** via plugins, so that the hub's feature set grows. | 🟡 | 5 |
| `US-PLG-08` | As a **Plugin Developer**, I want to **extend UI modules** via plugins, so that plugins can render their own panels. | 🟢 | 8 |
| `US-PLG-09` | As a **Plugin Developer**, I want to **hook into workflows** via plugins, so that automation can call plugin logic. | 🟢 | 5 |
| `US-PLG-10` | As a **User**, I want to **list, enable, and disable** installed plugins, so that I control what runs on my system. | ✅ | 3 |
| `US-PLG-11` | As a **User**, I want sensitive plugin calls **audit-logged** (who, what, when), so that I can trace misuse. | 🟡 | 5 |
| `US-PLG-12` | As a **User**, I want plugins **loaded dynamically** (FFI / subprocess / WASM), so that I can add functionality without restarting the core. | 🟢 | 8 |

---

## 14. 🚀 Deployment Modes & Systemd Integration

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-DEP-01` | As an **Admin**, I want to run the hub in **backend-only mode**, so that it can serve as a headless service. | 🟡 | 3 |
| `US-DEP-02` | As a **User**, I want to run **backend + built-in TUI**, so that I get the full interactive experience. | 🔴 | 2 |
| `US-DEP-03` | As a **Power User**, I want to connect a **custom UI** to the backend API, so that I can build my own interface. | 🟢 | 5 |
| `US-DEP-04` | As an **Admin**, I want to run the hub as a **systemd service**, so that it starts automatically and survives reboots. | ✅ | 5 |
| `US-DEP-05` | As a **User**, I want the hub to **launch and manage** user-defined scripts/services, so that I can orchestrate system tasks. | 🟢 | 5 |
| `US-DEP-06` | As a **User**, I want the hub to act as a **control layer for systemd units**, so that I can start/stop/enable services from one place. | 🟢 | 5 |

---

## 15. 🔗 API & IPC (Backend Surface)

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-API-01` | As a **Power User**, I want a **local API** (IPC/HTTP) exposed by the backend, so that the TUI, web UI, and external clients share one interface. | 🔴 | 5 |
| `US-API-02` | As a **Power User**, I want to **query and mutate entities** via the API, so that I can script the hub from outside. | 🔴 | 5 |
| `US-API-03` | As a **Power User**, I want to **trigger workflows** via the API, so that automations can be invoked remotely. | 🟡 | 3 |
| `US-API-04` | As a **Plugin Developer**, I want **safe host functions** exposed to Lua (e.g., `run_task`, `query_entity`, `emit_event`), so that plugins can interact with the core securely. | 🟡 | 5 |
| `US-API-05` | As an **Admin**, I want the API to **enforce capability flags**, so that untrusted clients cannot perform privileged actions. | 🟡 | 5 |

---

## 16. 🔍 Search & Metadata

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-SRCH-01` | As a **User**, I want **full-text search (FTS5)** across commands and scripts, so that I can find content by keyword. | 🔴 | 5 |
| `US-SRCH-02` | As a **User**, I want to **filter entities** by tags, type, and project, so that I can narrow results. | 🔴 | 3 |
| `US-SRCH-03` | As a **Plugin Developer**, I want a `metadata_json` field on entities, so that I can store plugin-specific extras without schema changes. | 🟡 | 2 |
| `US-SRCH-04` | As a **User**, I want search results **ranked by relevance**, so that the best match appears first. | 🟡 | 3 |

---

## 17. 🔄 Sync & Sharing *(Future)*

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-SYNC-01` | As a **User**, I want to **sync shared items** between machines, so that my knowledge base follows me. | 🟢 | 8 |
| `US-SYNC-02` | As a **User**, I want **selective sync** (choose what to share), so that private items stay local. | 🟢 | 5 |
| `US-SYNC-03` | As a **User**, I want **conflict resolution** on sync, so that divergent edits are handled gracefully. | 🟢 | 8 |
| `US-SYNC-04` | As a **User**, I want **cross-device secrets sync** (future), so that keys are available where needed (with consent). | 🟢 | 13 |

---

## 18. 🌐 Web Interface *(Future)*

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-WEB-01` | As a **User**, I want an **optional web UI**, so that I can access the hub from a browser. | 🟢 | 13 |
| `US-WEB-02` | As a **User**, I want **remote access** via the web UI, so that I can manage a headless machine. | 🟢 | 5 |
| `US-WEB-03` | As a **User**, I want **visual dashboards** in the web UI, so that I can monitor system state graphically. | 🟢 | 8 |
| `US-WEB-04` | As a **User**, I want **multi-device usage**, so that I can interact from phone or laptop. | 🟢 | 5 |

---

## 19. 💾 Backup & Export

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-BAK-01` | As a **User**, I want to **export the full hub state**, so that I can back up or migrate. | 🟢 | 5 |
| `US-BAK-02` | As a **User**, I want to **import a hub state**, so that I can restore on a new machine. | 🟢 | 5 |
| `US-BAK-03` | As a **User**, I want to **export a single project**, so that I can share it with a colleague. | 🟢 | 3 |
| `US-BAK-04` | As an **Admin**, I want **scheduled backups**, so that recovery points are always available. | 🟢 | 5 |

---

## 20. 🌟 Future / Advanced Ideas

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-FUT-01` | As a **User**, I want **AI-assisted** command and workflow suggestions, so that the hub helps me discover better automations. | 🟢 | 13 |
| `US-FUT-02` | As a **User**, I want a **plugin marketplace**, so that I can discover and install community plugins. | 🟢 | 13 |
| `US-FUT-03` | As a **User**, I want **multi-user collaboration**, so that a team can share a hub instance. | 🟢 | 13 |
| `US-FUT-04` | As a **User**, I want **cloud integration**, so that I can offload heavy tasks or sync via cloud storage. | 🟢 | 8 |
| `US-FUT-05` | As a **Power User**, I want **distributed workflow execution**, so that jobs can run across multiple machines. | 🟢 | 13 |
| `US-FUT-06` | As a **User**, I want a **visual drag-and-drop DAG builder**, so that I can design workflows without code. | 🟢 | 13 |

---

## 21. 📐 Non-Functional / Cross-Cutting

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-NF-01` | As a **User**, I want the TUI to remain **responsive** (<200ms input latency), so that interaction feels instant. | 🔴 | 5 |
| `US-NF-02` | As an **Admin**, I want the backend to handle **concurrent operations**, so that workflows and TUI don't block each other. | 🔴 | 5 |
| `US-NF-03` | As an **Admin**, I want all secrets **encrypted at rest**, so that a stolen DB file doesn't expose credentials. | 🟡 | 5 |
| `US-NF-04` | As an **Admin**, I want **structured logging** via `tracing`, so that logs can feed external observability tools. | 🔴 | 3 |
| `US-NF-05` | As an **Admin**, I want the app packaged as a **single Rust binary**, so that deployment is simple and dependency-free. | 🔴 | 2 |
| `US-NF-06` | As an **Admin**, I want **SQLite with WAL mode** and foreign keys, so that concurrent reads and data integrity are guaranteed. | 🔴 | 2 |
| `US-NF-07` | As an **Admin**, I want **versioned schema migrations** via `sqlx migrate`, so that DB changes are reproducible and safe. | 🔴 | 3 |
| `US-NF-08` | As a **User**, I want **graceful error handling** with clear messages, so that failures are understandable and actionable. | 🔴 | 3 |
| `US-NF-09` | As an **Admin**, I want **cross-platform support** (Linux, BSD, macOS), so that the hub runs on all my machines. | 🟡 | 5 |
| `US-NF-10` | As a **User**, I want **confirmation prompts** before destructive actions, so that I avoid accidental data loss. | 🔴 | 2 |

---

## 22. 🆕 New Requirements — Navigation, Config Management, Templates & Logic Gates

> Added from user feedback (this iteration). These stories reshape the TUI
> navigation, complete the Config Management domain, make project scaffolding
> plugin-driven and opt-in, and add logic nodes to the visual workflow builder.

### 22.1 🧭 Navigation restructure: category pages (US-TUI)

The Commands / Apps / Scripts tabs become **subpages of a parent page** that
describes what each subpage is for, so the top level stays small and
self-explanatory. Settings moves to the **last** position.

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-TUI-11` | As a **User**, I want a **"Knowledge Base" parent page** that briefly describes the Commands, Apps and Scripts subpages (with live item counts), so that the tab bar stays clean and I always know where things live. | ✅ | 3 |
| `US-TUI-12` | As a **User**, I want **Settings to be the last tab** and the tab order to be **configurable** in `config.conf`, so that the layout matches my workflow. | ✅ | 2 |

**Notes**:
- The parent page lists its subpages (`Commands — shell command families`,
  `Apps — GUI/TUI launchers`, `Scripts — multi-line scripts`); Enter or a
  number key opens the subpage. Subpages keep all existing keys.
- Proposed order: Dashboard · Knowledge Base (Commands/Apps/Scripts) · Projects ·
  Workflows · Secrets · Configs · Plugins · **Settings (last)**.

### 22.2 📝 Config Management completion (US-CFG)

Builds on the existing `config_manager` module (US-CFG-01..08). The page manages
real config files/folders on disk — dotfiles, app configs, project configs.

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-CFG-09` | As a **User**, I want to **register an existing file or folder** as a managed config, or **create a new one** from the Configs page, so that all my configs are tracked in one place. | 🟡 | 3 |
| `US-CFG-10` | As a **User**, I want each managed config to support **link or copy deployment** — symlink, hard link, or plain copy to one or more target locations — so that one config can serve many consumers. | 🟡 | 5 |
| `US-CFG-11` | As a **User**, I want an **"update" action** that re-deploys a config to its targets and shows drift between source and deployed copies, so that changes propagate predictably. | 🟡 | 3 |
| `US-CFG-12` | As a **User**, I want **git integration per managed config** (init, commit, log/diff, optional remote push/pull), so that my dotfiles are versioned without leaving the hub. | 🟡 | 5 |

### 22.3 🧩 Plugin-driven project templates & UI actions (US-PLG, US-PROJ)

Project scaffolding is **never applied by default** — a project starts bare
unless the user explicitly picks a template. Templates and their UI entry
points come from plugins, so the community can ship Python/Rust/Node/etc.
starter kits without touching core.

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-PLG-13` | As a **Plugin Developer**, I want to **register UI actions with labels** from plugin code (e.g. a "Create starting files" button on a project), so that my mod visibly extends the interface. | 🟡 | 5 |
| `US-PLG-14` | As a **Plugin Developer**, I want to ship **project scaffold templates** (files, folders, git init, post-create commands) as plugin data — e.g. a Python template that creates a venv, README, pyproject and a repo — so that starters are maintained outside core. | 🟡 | 5 |
| `US-PLG-15` | As a **User**, I want to **preview and customize a template** (file list and contents editable) before it is applied, so that scaffolding never surprises me. | 🟢 | 3 |
| `US-PROJ-08` | As a **User**, I want an **optional template picker at project creation** — default is none — so that plain projects stay plain and templated ones are an explicit choice. | 🟡 | 3 |

### 22.4 🔣 Logic gates for visual scripting (US-FUT)

| ID | Story | Priority | Points |
|:---|:---|:---:|:---:|
| `US-FUT-07` | As a **User**, I want **logic nodes** (AND, OR, NOT, XOR, comparisons, if/else) with typed boolean ports in the visual DAG builder (extends `US-FUT-06`), so that I can express conditions and branching without writing Lua. | 🟢 | 8 |

---

## 📊 Summary Dashboard

### Story counts by priority

| Priority | Count | Description |
|:---:|:---:|:---|
| 🔴 P0 — MVP | 30 | Must-have for first usable release |
| 🟡 P1 — Near-term | 67 | Core feature completeness |
| 🟢 P2 — Future | 44 | Advanced / long-term vision |
| **Total** | **141** | |

### Story counts by domain

| # | Domain | Stories |
|:---:|:---|:---:|
| 1 | 📊 Process & Resource Management | 7 |
| 2 | 📚 Command & Knowledge Base | 9 |
| 3 | ⚙️ Automation & Workflows | 11 |
| 4 | 📦 System & Package Management | 9 |
| 5 | 🌍 Environment Management | 8 |
| 6 | 📝 Config Management | 12 |
| 7 | 🔐 Secrets & Key Management | 10 |
| 8 | 🖥️ SSH Integration | 6 |
| 9 | 📁 Projects | 8 |
| 10 | 🗄️ Multi-Database / Multi-Context | 6 |
| 11 | ⚙️ Configuration System | 6 |
| 12 | 🖱️ TUI | 12 |
| 13 | 🔌 Plugin System | 15 |
| 14 | 🚀 Deployment & Systemd | 6 |
| 15 | 🔗 API & IPC | 5 |
| 16 | 🔍 Search & Metadata | 4 |
| 17 | 🔄 Sync & Sharing | 4 |
| 18 | 🌐 Web Interface | 4 |
| 19 | 💾 Backup & Export | 4 |
| 20 | 🌟 Future / Advanced | 7 |
| 21 | 📐 Non-Functional | 10 |

### MVP scope (P0) at a glance

> 🏗️ **Build first:** TUI core · Command & Knowledge Base · Projects (basic) · SQLite + sqlx · axum API health · Lua embedding (basic) · Search (FTS5)

```
┌─────────────────────────────────────────────────────────┐
│  P0  ████████████████████████████  30 stories           │
│  P1  ████████████████████████████████████████████████████│
│  P2  ██████████████████████████████████████████          │
└─────────────────────────────────────────────────────────┘
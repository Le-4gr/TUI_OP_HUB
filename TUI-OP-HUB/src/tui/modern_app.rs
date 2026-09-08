//! Modern Application Runner with Login/Signup Integration
//!
//! This module integrates the modern UI with authentication and provides
//! a complete login/signup flow before accessing the main application.

use super::helpers::*;
use super::list_state::*;
use super::modern_ui::{AppState, LoginField, LoginState, ModernTheme, ModernUI};
use crate::auth::AuthManager;
use crate::config::{AppConfig, KeybindingsConfig};
use crate::keygen;
use crate::models::{CreateEntity, CreateProject};
use crate::privilege;
use crate::project_workspace;
use crate::repository;
use crate::secrets;
use crate::workflow::{self, WorkflowDefinition};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame, Terminal,
};
use sqlx::SqlitePool;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Application mode - Login or Signup
#[derive(Debug, Clone, Copy, PartialEq)]
enum AuthMode {
    Login,
    Signup,
}

/// Signup form state
#[derive(Debug, Clone)]
struct SignupState {
    username: String,
    password: String,
    confirm_password: String,
    focused_field: SignupField,
    error_message: Option<String>,
    is_creating: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SignupField {
    Username,
    Password,
    ConfirmPassword,
}

impl Default for SignupState {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            confirm_password: String::new(),
            focused_field: SignupField::Username,
            error_message: None,
            is_creating: false,
        }
    }
}

impl SignupState {
    fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            SignupField::Username => SignupField::Password,
            SignupField::Password => SignupField::ConfirmPassword,
            SignupField::ConfirmPassword => SignupField::Username,
        };
    }

    fn validate(&self) -> Result<(), String> {
        if self.username.is_empty() {
            return Err("Username is required".to_string());
        }
        if self.username.len() < 3 {
            return Err("Username must be at least 3 characters".to_string());
        }
        if self.password.is_empty() {
            return Err("Password is required".to_string());
        }
        if self.password.len() < 8 {
            return Err("Password must be at least 8 characters".to_string());
        }
        if self.password != self.confirm_password {
            return Err("Passwords do not match".to_string());
        }
        Ok(())
    }

    fn clear_sensitive_data(&mut self) {
        self.password.clear();
        self.confirm_password.clear();
    }
}

/// Dashboard statistics
#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    pub commands_count: usize,
    pub workflows_count: usize,
    pub secrets_count: usize,
    pub projects_count: usize,
}

/// Item pending deletion confirmation (US-CMD-07, US-SEC delete)
struct ConfirmDelete {
    id: String,
    label: String,
    kind: DeleteKind,
    /// Project workspace path (projects only): enables the "also delete the
    /// folder on disk" option (US-PROJ).
    path: Option<String>,
    /// Toggle for the folder-removal option; default OFF — deleting the DB
    /// row never touches the disk unless explicitly chosen.
    delete_folder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DeleteKind {
    Entity,
    Project,
    Secret,
    Config,
}

/// Result popup after running a command or workflow
struct RunResult {
    title: String,
    success: bool,
    text: String,
}

/// A workflow running in the background; polled every loop iteration and
/// cancellable with `X` on the Workflows tab (US-WF-09).
struct RunningWorkflow {
    run_id: String,
    name: String,
    task: tokio::task::JoinHandle<Result<crate::workflow::WorkflowResult, crate::error::AppError>>,
}

/// Admin user-management panel, opened from Settings with `u` (US-SEC).
#[derive(Clone)]
struct UsersPanel {
    users: Vec<crate::models::UserProfile>,
    selected: usize,
    /// When true, the next Enter deletes the selected user.
    confirm_delete: bool,
    message: Option<String>,
}

/// SSH host manager panel, opened from Secrets with `H` (US-SSH-01..05).
struct SshPanel {
    hosts: Vec<crate::models::SshHost>,
    selected: usize,
    form: Option<SshForm>,
    /// Id of the host being edited (`None` while creating).
    editing_id: Option<String>,
    confirm_delete: bool,
    message: Option<String>,
}

/// Create/edit form for an SSH host (US-SSH-02/03).
#[derive(Clone)]
struct SshForm {
    name: String,
    hostname: String,
    port: String,
    username: String,
    key_path: String,
    field: usize,
}

/// Plugin UI actions popup for the selected project (US-PLG-13): lists the
/// labeled actions contributed by approved plugins and runs the chosen one.
#[derive(Debug, Clone)]
struct ProjectActionsPanel {
    entries: Vec<crate::plugin::PluginActionEntry>,
    selected: usize,
    project_name: String,
    project_path: String,
    last_result: Option<(bool, String)>,
}

/// Preview/customize popup state for a project template (US-PLG-15):
/// file inclusion toggles and inline content editing before apply.
#[derive(Debug, Clone)]
struct TemplatePreview {
    template_idx: usize,
    template: crate::plugin::ProjectTemplate,
    selected: usize,
    included: Vec<bool>,
    editing: bool,
    edit_buf: String,
}

impl TemplatePreview {
    fn new(template_idx: usize, template: crate::plugin::ProjectTemplate) -> Self {
        let included = vec![true; template.files.len()];
        Self {
            template_idx,
            template,
            selected: 0,
            included,
            editing: false,
            edit_buf: String::new(),
        }
    }

    /// Drop excluded files so creation uses the customized template.
    fn into_customized(self) -> crate::plugin::ProjectTemplate {
        let mut template = self.template;
        template.files = template
            .files
            .into_iter()
            .zip(&self.included)
            .filter_map(|(f, inc)| if *inc { Some(f) } else { None })
            .collect();
        template
    }
}

/// Register-config form (Configs tab, `n`, US-CFG-09): path + optional
/// metadata fields. Path supports Ctrl+O external file browsing and
/// auto-fills the Name from the chosen file.
#[derive(Debug, Clone, Default)]
struct ConfigRegisterForm {
    path: String,
    name: String,
    /// False until the user edits the Name manually, so typing the path can
    /// keep auto-filling it from the file name.
    name_touched: bool,
    description: String,
    /// Comma-separated; parsed on submit.
    tags: String,
    /// Where the file should be deployed (comma-separated paths). May differ
    /// from where the file lives and may not exist yet (US-CFG-09/10).
    deploy_to: String,
    deploy_mode: crate::config_manager::DeployMode,
    /// 0=path, 1=name, 2=description, 3=tags, 4=deploy-to, 5=deploy mode
    focus: usize,
    error: Option<String>,
}

impl ConfigRegisterForm {
    const FIELDS: usize = 6;

    fn new() -> Self {
        Self::default()
    }

    /// Derive the Name from the current path text unless it was edited.
    fn sync_name_from_path(&mut self) {
        if self.name_touched {
            return;
        }
        self.name = std::path::Path::new(&self.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
    }
}

/// One row of the in-TUI file browser.
#[derive(Debug, Clone)]
struct FileRow {
    name: String,
    is_dir: bool,
    /// The `..` pseudo-entry for the parent directory.
    is_parent: bool,
}

/// Where the browser writes the picked path (US-CFG-09/10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum BrowserDest {
    /// Register form: replace the Path field (file pick).
    #[default]
    RegisterPath,
    /// Register form: append the picked folder to the Deploy-to list.
    DeployTo,
    /// Target popup: replace the destination path.
    TargetPath,
}

/// In-TUI file browser modal (US-CFG-09): pick an existing file/folder or
/// create new files/folders without leaving the app. Opens over the config
/// register/target forms with Ctrl+O; the external system picker remains
/// available on Ctrl+P.
#[derive(Debug, Clone)]
struct FileBrowser {
    /// true = picking a directory (deploy target), false = picking a file.
    pick_dir: bool,
    /// Where the accepted path goes.
    dest: BrowserDest,
    cwd: std::path::PathBuf,
    entries: Vec<FileRow>,
    selected: usize,
    show_hidden: bool,
    /// Some((is_dir, name)) while a new-entry name is being typed.
    new_entry: Option<(bool, String)>,
    /// One-shot status/error line inside the browser.
    status: Option<String>,
}

impl FileBrowser {
    fn new(pick_dir: bool, dest: BrowserDest, start: std::path::PathBuf) -> Self {
        let mut fb = Self {
            pick_dir,
            dest,
            cwd: start,
            entries: Vec::new(),
            selected: 0,
            show_hidden: false,
            new_entry: None,
            status: None,
        };
        fb.reload();
        fb
    }

    /// Re-read the current directory: `..` first, then folders, then files.
    fn reload(&mut self) {
        let mut rows: Vec<FileRow> = Vec::new();
        if let Some(parent) = self.cwd.parent() {
            if parent != self.cwd {
                rows.push(FileRow {
                    name: "..".to_string(),
                    is_dir: true,
                    is_parent: true,
                });
            }
        }
        let mut dirs: Vec<FileRow> = Vec::new();
        let mut files: Vec<FileRow> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.cwd) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !self.show_hidden && name.starts_with('.') {
                    continue;
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let row = FileRow {
                    name,
                    is_dir,
                    is_parent: false,
                };
                if is_dir {
                    dirs.push(row);
                } else {
                    files.push(row);
                }
            }
        }
        let sort_key = |r: &FileRow| r.name.to_lowercase();
        dirs.sort_by_key(sort_key);
        files.sort_by_key(sort_key);
        rows.extend(dirs);
        rows.extend(files);
        self.entries = rows;
        if self.selected >= self.entries.len() {
            self.selected = 0;
        }
    }

    fn current(&self) -> Option<&FileRow> {
        self.entries.get(self.selected)
    }

    fn goto_parent(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            let name = self
                .cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            self.cwd = parent.to_path_buf();
            self.reload();
            self.select(&name);
        }
    }

    fn enter_dir(&mut self, row: &FileRow) {
        if !row.is_dir || row.is_parent {
            return;
        }
        self.cwd = self.cwd.join(&row.name);
        self.selected = 0;
        self.reload();
    }

    /// Jump the cursor to an entry by name (used after creating one).
    fn select(&mut self, name: &str) {
        if let Some(i) = self.entries.iter().position(|r| r.name == name) {
            self.selected = i;
        }
    }
}

impl SshForm {
    const FIELDS: [&'static str; 5] = ["Name", "Hostname", "Port", "Username", "Key path"];
    fn new() -> Self {
        Self {
            name: String::new(),
            hostname: String::new(),
            port: "22".to_string(),
            username: String::new(),
            key_path: String::new(),
            field: 0,
        }
    }
    /// Build the quick-connect command: `ssh [-i key] [-p port] user@host`.
    fn connect_command(&self) -> Option<String> {
        if self.hostname.trim().is_empty() {
            return None;
        }
        let mut cmd = String::from("ssh");
        if !self.key_path.trim().is_empty() {
            cmd.push_str(&format!(" -i {}", self.key_path.trim()));
        }
        let port: u32 = self.port.trim().parse().unwrap_or(22);
        cmd.push_str(&format!(" -p {}", port));
        let user = self.username.trim();
        let host = self.hostname.trim();
        if user.is_empty() {
            cmd.push(' ');
            cmd.push_str(host);
        } else {
            cmd.push_str(&format!(" {}@{}", user, host));
        }
        Some(cmd)
    }
}

/// Modern application with integrated auth
pub struct ModernApp {
    ui: ModernUI,
    auth_mode: AuthMode,
    signup_state: SignupState,
    pool: Arc<SqlitePool>,
    should_quit: bool,
    stats: DashboardStats,
    current_user_id: Option<String>,
    /// Loaded application config (editor, theme, keybindings, page size)
    config: AppConfig,
    /// Where `Ctrl+S` in Settings persists the config (US-APP-06)
    config_path: PathBuf,
    // List states for each tab
    commands_list: CommandsListState,
    projects_list: ProjectsListState,
    workflows_list: WorkflowsListState,
    secrets_list: SecretsListState,
    // Form states
    command_form: CommandFormState,
    project_form: ProjectFormState,
    project_detail: Option<ProjectDetailState>,
    cron_input: Option<String>,
    import_input: Option<String>,
    import_dup_mode: crate::share::DuplicateMode,
    export_input: Option<String>,
    register_input: Option<String>,
    plugins: Arc<crate::plugin::PluginManager>,
    secret_pass_prompt: Option<(String, String)>,
    monitor: crate::monitor::Monitor,
    monitor_snap: Option<crate::monitor::MonitorSnapshot>,
    monitor_refreshed: std::time::Instant,
    wants_terminal_cmd: Option<(String, String)>, // (command, cwd)
    plugins_list: PluginsListState,
    workflow_form: WorkflowFormState,
    secret_form: SecretFormState,
    // Search state
    search_state: SearchState,
    // Settings screen state
    settings: SettingsState,
    // Advanced settings sub-screen (visual config)
    advanced: AdvancedState,
    // SSH/GPG key generation form (US-SEC-01)
    keygen: KeygenState,
    // Developer mode: two-step confirmation for wiping all users
    dev_confirm_wipe: bool,
    // Sudo password popup for privileged runs (US-CMD-09)
    sudo_password: Option<String>,
    // Pending command to run after sudo password is entered
    sudo_pending_command: Option<String>,
    /// Background workflow run, polled each loop iteration (US-WF-09)
    running_workflow: Option<RunningWorkflow>,
    /// Admin user management panel (Settings → `u`)
    users_panel: Option<UsersPanel>,
    /// Type filter of the Knowledge tab (US-TUI-11/12): one tab, filtered list
    kb_filter: KbFilter,
    /// Managed-config list (Configs tab, US-CFG-09..12)
    configs_list: ListState<crate::config_manager::ConfigEntry>,
    /// Register-config form popup (path + metadata, US-CFG-09)
    config_input: Option<ConfigRegisterForm>,
    /// In-TUI file browser opened over the config forms (Ctrl+O, US-CFG-09)
    file_browser: Option<FileBrowser>,
    /// Target-path popup for deploying the selected config
    config_target_input: Option<String>,
    /// Where managed configs are stored (temp dir in tests)
    config_store_dir: std::path::PathBuf,

    /// SSH host manager panel (Secrets → `H`)
    ssh_panel: Option<SshPanel>,
    // New project creation form (US-PROJ)
    new_project_open: bool,
    new_project_name: String,
    new_project_kind: usize,
    new_project_editor: usize,
    new_project_error: Option<String>,
    new_project_field_idx: usize,
    /// Injectable parent dir for the workspace form (tests); None = default
    /// `$HOME/projects` (US-PROJ).
    new_project_parent: Option<std::path::PathBuf>,
    /// Templates discovered from plugins (US-PLG-14), loaded when the
    /// workspace form opens. Selection None = plain project (US-PROJ-08).
    templates: Vec<crate::plugin::ProjectTemplate>,
    new_project_template: Option<usize>,
    /// Template preview/customize popup (US-PLG-15)
    template_preview: Option<TemplatePreview>,
    /// Plugin UI actions popup for the selected project (US-PLG-13)
    project_actions: Option<ProjectActionsPanel>,
    // Login-screen dev user manager (cargo run only)
    dev_user_manager: bool,
    dev_user_list: Vec<crate::models::UserProfile>,
    dev_user_selected: usize,
    dev_user_error: Option<String>,
    // Structured options popup for a command family (parent name, options)
    options_popup: Option<(String, Vec<(String, String)>)>,
    // Keybind helper overlay (? key; US-TUI-09)
    keybinds_overlay: bool,
    wants_terminal: bool,
    needs_full_redraw: bool,
    // Overlay states (forms, visual builder, popups)
    visual_form: Option<VisualWorkflowState>,
    confirm_delete: Option<ConfirmDelete>,
    run_result: Option<RunResult>,
    status_message: Option<String>,
}

fn cycle_field(current: usize, total: usize, forward: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if forward {
        (current + 1) % total
    } else {
        (current + total - 1) % total
    }
}

/// Project detail view state (US-PROJ-02, US-PROJ-07): a project and all
/// entities that belong to it, shown as a popup from the Projects tab.
#[derive(Debug, Clone)]
struct ProjectDetailState {
    project: crate::models::Project,
    entities: Vec<crate::models::Entity>,
    selected: usize,
}

/// One discovered plugin for the Plugins tab (US-PLG-10).
#[derive(Debug, Clone)]
struct PluginEntry {
    manifest: crate::plugin::PluginManifest,
    approved: bool,
    enabled: bool,
}

impl std::fmt::Display for PluginEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} v{}", self.manifest.name, self.manifest.version)?;
        let mut tags = String::new();
        if self.approved {
            tags.push_str(" [approved]");
        } else {
            tags.push_str(" [unapproved]");
        }
        tags.push_str(" [");
        tags.push_str(&self.manifest.plugin_type);
        tags.push_str("]");
        write!(f, "{}", tags)
    }
}

type PluginsListState = super::list_state::ListState<PluginEntry>;

/// Type filter on the Knowledge tab (US-TUI-11): ONE tab, filtered list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KbFilter {
    #[default]
    All,
    Cmd,
    App,
    Script,
}

impl KbFilter {
    /// Cycle order: All → Commands → Apps → Scripts → All.
    pub fn next(self) -> Self {
        match self {
            KbFilter::All => KbFilter::Cmd,
            KbFilter::Cmd => KbFilter::App,
            KbFilter::App => KbFilter::Script,
            KbFilter::Script => KbFilter::All,
        }
    }

    /// Label shown in the list title.
    pub fn label(self) -> &'static str {
        match self {
            KbFilter::All => "All",
            KbFilter::Cmd => "Commands",
            KbFilter::App => "Apps",
            KbFilter::Script => "Scripts",
        }
    }

    /// Entity type ids covered by this filter.
    pub fn type_ids(self) -> &'static [&'static str] {
        match self {
            KbFilter::All => &["cmd", "app", "script"],
            KbFilter::Cmd => &["cmd"],
            KbFilter::App => &["app"],
            KbFilter::Script => &["script"],
        }
    }

    /// Index into `ENTITY_TYPE_IDS` ([\"cmd\", \"script\", \"app\"]) for the
    /// create-form type selector; `All` defaults to Command.
    pub fn form_type_index(self) -> usize {
        match self {
            KbFilter::All | KbFilter::Cmd => 0,
            KbFilter::App => 2,
            KbFilter::Script => 1,
        }
    }
}

impl ModernApp {
    pub fn new(pool: Arc<SqlitePool>, config: AppConfig) -> Self {
        let page_size = config.tui.page_size.max(1);
        let pool_for_plugins = pool.clone();
        let theme = ModernTheme::from_config(&config.theme);
        Self {
            ui: ModernUI {
                theme,
                ..ModernUI::new()
            },
            auth_mode: AuthMode::Login,
            signup_state: SignupState::default(),
            pool,
            should_quit: false,
            stats: DashboardStats::default(),
            current_user_id: None,
            config,
            config_path: AppConfig::default_path(),
            // Initialize list states (page size from config)
            commands_list: ListState::new(page_size),
            projects_list: ListState::new(page_size),
            workflows_list: ListState::new(page_size),
            secrets_list: ListState::new(page_size),
            // Initialize form states
            command_form: CommandFormState::default(),
            project_form: ProjectFormState::default(),
            project_detail: None,
            cron_input: None,
            import_input: None,
            import_dup_mode: crate::share::DuplicateMode::default(),
            export_input: None,
            register_input: None,
            plugins: Arc::new(crate::plugin::PluginManager::new(
                pool_for_plugins,
                crate::plugin::PluginManager::default_dir(),
            )),
            secret_pass_prompt: None,
            monitor: crate::monitor::Monitor::new(),
            monitor_snap: None,
            monitor_refreshed: std::time::Instant::now(),
            wants_terminal_cmd: None,
            plugins_list: ListState::new(page_size),
            workflow_form: WorkflowFormState::default(),
            secret_form: SecretFormState::default(),
            // Initialize search state
            search_state: SearchState::default(),
            settings: SettingsState::default(),
            advanced: AdvancedState::default(),
            keygen: KeygenState::default(),
            dev_confirm_wipe: false,
            sudo_password: None,
            sudo_pending_command: None,
            running_workflow: None,
            users_panel: None,
            kb_filter: KbFilter::default(),
            configs_list: ListState::default(),
            config_input: None,
            file_browser: None,
            config_target_input: None,
            config_store_dir: dirs_home().join(".config/tui-op-hub/configs"),
            ssh_panel: None,
            dev_user_manager: false,
            dev_user_list: Vec::new(),
            dev_user_selected: 0,
            dev_user_error: None,
            new_project_open: false,
            new_project_name: String::new(),
            new_project_kind: 0,
            new_project_editor: 0,
            new_project_error: None,
            new_project_field_idx: 0,
            new_project_parent: None,
            templates: Vec::new(),
            new_project_template: None,
            template_preview: None,
            project_actions: None,
            options_popup: None,
            keybinds_overlay: false,
            wants_terminal: false,
            needs_full_redraw: true,
            // Overlays start closed
            visual_form: None,
            confirm_delete: None,
            run_result: None,
            status_message: None,
        }
    }

    /// `KeyCode` bound to a keybinding action (US-APP-02).
    /// Switch to the tab for digit 1-9 (US-APP-02 tab keys). Works from ANY
    /// screen, including Settings/Advanced, so digits always mean tabs.
    /// Default Tab-cycle order (US-TUI-12): Settings is intentionally LAST.
    const DEFAULT_TAB_ORDER: [&'static str; 8] =
        ["dash", "kb", "proj", "wf", "sec", "cfg", "plug", "set"];

    /// Resolve the configured Tab-cycle order to AppState values (US-TUI-12).
    /// Unknown ids are dropped; an empty result falls back to the default.
    fn tab_cycle(&self) -> Vec<AppState> {
        let parse = |id: &str| match id.trim().to_lowercase().as_str() {
            "dash" => Some(AppState::Dashboard),
            "kb" => Some(AppState::Knowledge),

            "proj" => Some(AppState::Projects),
            "wf" => Some(AppState::Workflows),
            "sec" => Some(AppState::Secrets),
            "cfg" => Some(AppState::Configs),
            "plug" => Some(AppState::Plugins),
            "set" => Some(AppState::Settings),
            _ => None,
        };
        let configured: Vec<AppState> = self
            .config
            .tui
            .tab_order
            .as_deref()
            .map(|s| s.split(',').filter_map(parse).collect())
            .unwrap_or_default();
        if configured.is_empty() {
            Self::DEFAULT_TAB_ORDER
                .iter()
                .filter_map(|id| parse(id))
                .collect()
        } else {
            configured
        }
    }

    /// Next tab in the configured cycle (Tab key, US-TUI-12).
    fn next_tab(&self) -> AppState {
        let cycle = self.tab_cycle();
        let idx = cycle.iter().position(|s| *s == self.ui.state);
        match idx {
            Some(i) => cycle[(i + 1) % cycle.len()].clone(),
            None => cycle.first().cloned().unwrap_or(AppState::Dashboard),
        }
    }

    async fn jump_to_tab_digit(&mut self, c: char) {
        self.ui.state = match c {
            '2' => AppState::Knowledge,
            '3' => AppState::Projects,
            '4' => AppState::Workflows,
            '5' => AppState::Secrets,
            '6' => AppState::Configs,
            '7' => AppState::Plugins,
            '0' => AppState::Settings,
            '1' => AppState::Dashboard,
            _ => return, // unmapped digits do nothing
        };
        match self.ui.state {
            AppState::Dashboard => {
                let _ = self.fetch_stats().await;
            }
            AppState::Knowledge => {
                let _ = self.fetch_knowledge().await;
            }
            AppState::Projects => {
                let _ = self.fetch_projects().await;
            }
            AppState::Workflows => {
                let _ = self.fetch_workflows().await;
            }
            AppState::Secrets => {
                let _ = self.fetch_secrets().await;
            }
            AppState::Plugins => {
                self.fetch_plugins().await;
            }
            _ => {}
        }
    }

    /// If `key` is a plain digit 1-9, switch tabs and consume it.
    /// Returns true when the key was handled. Used by screens that would
    /// otherwise eat digits for their own navigation (Settings/Advanced).
    async fn handle_tab_digit(&mut self, key: &KeyEvent) -> bool {
        if !key.modifiers.is_empty() {
            return false;
        }
        if let KeyCode::Char(c) = key.code {
            // Digits 1-9 plus 0 (Settings last, US-TUI-12) switch tabs
            if c.is_ascii_digit() {
                self.jump_to_tab_digit(c).await;
                return true;
            }
        }
        false
    }

    fn action_keycode(&self, action: &str) -> Option<KeyCode> {
        KeybindingsConfig::to_keycode(self.config.keybindings.get(action))
    }

    /// Effective external editor: config value, else `$EDITOR`, else "vi".
    fn effective_editor(&self) -> String {
        let configured = self.config.general.editor.trim().to_string();
        if !configured.is_empty() {
            return configured;
        }
        std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string())
    }

    /// Fetch dashboard statistics from database
    async fn fetch_stats(&mut self) -> anyhow::Result<()> {
        use sqlx::Row;

        // Count commands (entities with type_id = 'cmd' — see 0001_init.sql seed)
        let commands_count: i64 =
            sqlx::query("SELECT COUNT(*) as count FROM entities WHERE type_id = 'cmd'")
                .fetch_one(&*self.pool)
                .await?
                .try_get("count")?;

        // Count workflows (entities with type_id = 'wf' — see 0001_init.sql seed)
        let workflows_count: i64 =
            sqlx::query("SELECT COUNT(*) as count FROM entities WHERE type_id = 'wf'")
                .fetch_one(&*self.pool)
                .await?
                .try_get("count")?;

        // Count secrets
        let secrets_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM secrets")
            .fetch_one(&*self.pool)
            .await?
            .try_get("count")?;

        // Count projects
        let projects_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM projects")
            .fetch_one(&*self.pool)
            .await?
            .try_get("count")?;

        self.stats = DashboardStats {
            commands_count: commands_count as usize,
            workflows_count: workflows_count as usize,
            secrets_count: secrets_count as usize,
            projects_count: projects_count as usize,
        };

        Ok(())
    }

    /// Fetch commands list from database
    /// Fetch the Knowledge list for the current type filter (US-TUI-11):
    /// one tab, filtered by `kb_filter` (All = cmd+app+script).
    async fn fetch_knowledge(&mut self) -> anyhow::Result<()> {
        let mut items = Vec::new();
        for ty in self.kb_filter.type_ids() {
            items.extend(repository::list_entities(&*self.pool, Some(ty), None).await?);
        }
        items.sort_by(|a, b| a.name.cmp(&b.name));
        let total = items.len();
        self.commands_list.set_items(items, total);
        Ok(())
    }

    /// `n` on the Knowledge tab: create a new item pre-set to the CURRENT
    /// filter's type (All → Command; the form can still cycle the type with
    /// ←/→).
    fn new_knowledge_item(&mut self) {
        self.command_form = CommandFormState {
            mode: Some(FormMode::Create),
            entity_type: self.kb_filter.form_type_index(),
            ..Default::default()
        };
    }

    /// Validate the entered cron expression and persist a scheduled task for
    /// the selected workflow (US-WF-07). The scheduler daemon picks it up on
    /// its next poll.
    async fn save_workflow_schedule(&mut self, expr: &str) {
        let (normalized, _) = match crate::scheduler::validate_cron(expr) {
            Ok(v) => v,
            Err(e) => {
                self.status_message = Some(format!("\u{2717} {}", e));
                return;
            }
        };
        let Some(entity) = self.workflows_list.get_selected() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        match repository::create_scheduled_task(&*self.pool, &entity.id, &normalized).await {
            Ok(task) => {
                self.status_message = Some(format!(
                    "\u{2713} Scheduled (cron: {}) \u{2014} id {}",
                    task.cron_expr, task.id
                ));
            }
            Err(e) => self.status_message = Some(format!("\u{2717} {}", e)),
        }
    }

    /// Open the detail popup for the selected project: description plus every
    /// entity that belongs to it (US-PROJ-02, US-PROJ-07).
    async fn open_project_detail(&mut self) {
        let Some(project) = self.projects_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        match repository::list_entities_by_project(&*self.pool, &project.id).await {
            Ok(entities) => {
                self.project_detail = Some(ProjectDetailState {
                    project,
                    entities,
                    selected: 0,
                });
            }
            Err(e) => self.status_message = Some(format!("\u{2717} {}", e)),
        }
    }

    /// Refresh the dashboard monitor snapshot (US-PROC-01, mini-btop panels).
    async fn fetch_monitor(&mut self) {
        self.monitor_snap = Some(self.monitor.snapshot());
        self.monitor_refreshed = std::time::Instant::now();
    }
    /// Discover plugins and combine with their DB approval/enabled state (US-PLG-10).
    async fn fetch_plugins(&mut self) {
        let mut entries = Vec::new();
        for manifest in self.plugins.discover_plugins() {
            let approved = self
                .plugins
                .is_plugin_approved(&manifest.id)
                .await
                .unwrap_or(false);
            let enabled = self
                .plugins
                .is_plugin_enabled(&manifest.id)
                .await
                .unwrap_or(false);
            entries.push(PluginEntry {
                manifest,
                approved,
                enabled,
            });
        }
        let total = entries.len();
        self.plugins_list.set_items(entries, total);
    }

    /// Fetch projects list from database
    async fn fetch_projects(&mut self) -> anyhow::Result<()> {
        let projects = repository::list_projects(&*self.pool).await?;
        let total = projects.len();
        self.projects_list.set_items(projects, total);
        Ok(())
    }

    /// Fetch workflows list from database
    async fn fetch_workflows(&mut self) -> anyhow::Result<()> {
        let entities = repository::list_entities(&*self.pool, Some("wf"), None).await?;
        let total = entities.len();
        self.workflows_list.set_items(entities, total);
        Ok(())
    }

    /// Fetch secrets list from database (for the logged-in user)
    async fn fetch_secrets(&mut self) -> anyhow::Result<()> {
        let user_id = self.current_user_profile_id().await;
        let secrets_list = repository::list_secrets(&*self.pool, &user_id).await?;
        let total = secrets_list.len();
        self.secrets_list.set_items(secrets_list, total);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Configs tab (US-CFG-09..12)
    // ------------------------------------------------------------------

    /// ConfigManager bound to this app's store directory (created lazily so
    /// tests can point `config_store_dir` at a temp dir).
    fn config_manager(&self) -> crate::config_manager::ConfigManager {
        crate::config_manager::ConfigManager::new(self.config_store_dir.clone())
    }

    /// Load the managed-config registry into the list.
    async fn fetch_configs(&mut self) -> anyhow::Result<()> {
        let entries = self.config_manager().load_registry();
        let total = entries.len();
        self.configs_list.set_items(entries, total);
        Ok(())
    }

    /// Register an existing file as a managed config (source-path popup).
    /// Inline validation for the register-config form (US-CFG-09).
    /// Returns the error message to display, or None when valid.
    fn validate_config_form(&self, form: &ConfigRegisterForm) -> Option<String> {
        if form.path.trim().is_empty() {
            return Some("Path is required".to_string());
        }
        let expanded = expand_tilde(form.path.trim());
        if !std::path::Path::new(&expanded).exists() {
            return Some(format!("File not found: {}", expanded));
        }
        None
    }

    /// Submit the register-config form: register the file with its metadata
    /// (name, description, tags, deploy mode) and refresh the list (US-CFG-09).
    async fn register_config_from_form(&mut self, form: &ConfigRegisterForm) {
        let source = expand_tilde(form.path.trim());
        let name = if form.name.trim().is_empty() {
            std::path::Path::new(&source)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "config".to_string())
        } else {
            form.name.trim().to_string()
        };
        let description = if form.description.trim().is_empty() {
            None
        } else {
            Some(form.description.trim().to_string())
        };
        let tags: Vec<String> = form
            .tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        // Deploy targets: where the file should end up (may differ from the
        // source location and need not exist yet, US-CFG-10)
        let targets: Vec<String> = form
            .deploy_to
            .split(',')
            .map(|t| expand_tilde(t.trim()))
            .filter(|t| !t.is_empty())
            .collect();
        match self.config_manager().register_existing_full(
            std::path::Path::new(&source),
            &name,
            description,
            tags,
            targets,
            form.deploy_mode,
        ) {
            Ok(entry) => {
                self.status_message = Some(format!(
                    "✓ Managed '{}' (v{} stored, {} target{})",
                    entry.name,
                    entry.version,
                    entry.targets.len(),
                    if entry.targets.len() == 1 { "" } else { "s" }
                ));
                self.fetch_configs().await.ok();
            }
            Err(e) => self.status_message = Some(format!("✗ {}", e)),
        }
    }

    /// Open the in-TUI file browser over the current config popup (US-CFG-09).
    /// Starts in the parent directory of the typed path when it exists,
    /// otherwise in $HOME.
    fn open_file_browser(&mut self, pick_dir: bool, dest: BrowserDest, from_path: &str) {
        let expanded = expand_tilde(from_path.trim());
        let start = std::path::Path::new(&expanded);
        let start = if start.is_file() {
            start.parent().map(|p| p.to_path_buf())
        } else if start.is_dir() {
            Some(start.to_path_buf())
        } else {
            None
        };
        let start = start.unwrap_or_else(|| {
            std::env::var("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
        self.file_browser = Some(FileBrowser::new(pick_dir, dest, start));
    }

    /// Keys for the in-TUI file browser (US-CFG-09).
    fn handle_file_browser_key(&mut self, key: KeyEvent) {
        // Name-entry mode while creating a new file/folder
        if self
            .file_browser
            .as_ref()
            .is_some_and(|fb| fb.new_entry.is_some())
        {
            match key.code {
                KeyCode::Esc => {
                    if let Some(fb) = self.file_browser.as_mut() {
                        fb.new_entry = None;
                        fb.status = None;
                    }
                }
                KeyCode::Enter => {
                    let pending = self
                        .file_browser
                        .as_mut()
                        .and_then(|fb| fb.new_entry.take());
                    if let Some((is_dir, name)) = pending {
                        self.create_browser_entry(is_dir, &name);
                    }
                }
                KeyCode::Backspace => {
                    if let Some(fb) = self.file_browser.as_mut() {
                        if let Some((_, name)) = fb.new_entry.as_mut() {
                            name.pop();
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(fb) = self.file_browser.as_mut() {
                        if let Some((_, name)) = fb.new_entry.as_mut() {
                            name.push(c);
                        }
                    }
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc => self.file_browser = None,
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.selected = fb.selected.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(fb) = self.file_browser.as_mut() {
                    if fb.selected + 1 < fb.entries.len() {
                        fb.selected += 1;
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.goto_parent();
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(fb) = self.file_browser.as_mut() {
                    if let Some(row) = fb.current().cloned() {
                        fb.enter_dir(&row);
                    }
                }
            }
            KeyCode::Enter => {
                // Enter: descend into folders; pick files in file mode
                let (is_parent, is_dir) = self
                    .file_browser
                    .as_ref()
                    .and_then(|fb| fb.current())
                    .map(|r| (r.is_parent, r.is_dir))
                    .unwrap_or((false, false));
                if is_parent {
                    if let Some(fb) = self.file_browser.as_mut() {
                        fb.goto_parent();
                    }
                } else if is_dir {
                    if let Some(fb) = self.file_browser.as_mut() {
                        if let Some(row) = fb.current().cloned() {
                            fb.enter_dir(&row);
                        }
                    }
                } else if !self.file_browser.as_ref().is_some_and(|fb| fb.pick_dir) {
                    // File mode: pick the file under the cursor
                    let path = self.file_browser.as_ref().and_then(|fb| {
                        let name = fb.current()?.name.clone();
                        Some(fb.cwd.join(name))
                    });
                    if let Some(p) = path {
                        self.accept_file_browser(&p);
                    }
                } else if let Some(fb) = self.file_browser.as_mut() {
                    fb.status = Some("Press Space to pick the current folder".to_string());
                }
            }
            KeyCode::Char(' ') => {
                // Space: pick the current selection
                let (is_parent, is_dir, name) = self
                    .file_browser
                    .as_ref()
                    .and_then(|fb| fb.current())
                    .map(|r| (r.is_parent, r.is_dir, r.name.clone()))
                    .unwrap_or((false, false, String::new()));
                let pick_dir = self.file_browser.as_ref().is_some_and(|fb| fb.pick_dir);
                if pick_dir {
                    // Directory mode: pick the folder under the cursor (or
                    // the parent for `..`)
                    let path = if is_parent {
                        self.file_browser
                            .as_ref()
                            .and_then(|fb| fb.cwd.parent().map(|p| p.to_path_buf()))
                    } else if is_dir {
                        self.file_browser.as_ref().map(|fb| fb.cwd.join(&name))
                    } else {
                        None
                    };
                    if let Some(p) = path {
                        self.accept_file_browser(&p);
                    } else if let Some(fb) = self.file_browser.as_mut() {
                        fb.status = Some("Pick a folder".to_string());
                    }
                } else if !is_dir && !is_parent {
                    let path = self.file_browser.as_ref().map(|fb| fb.cwd.join(&name));
                    if let Some(p) = path {
                        self.accept_file_browser(&p);
                    }
                } else if let Some(fb) = self.file_browser.as_mut() {
                    fb.status = Some("Not a file".to_string());
                }
            }
            KeyCode::Char('a') => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.new_entry = Some((false, String::new()));
                    fb.status = None;
                }
            }
            KeyCode::Char('A') => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.new_entry = Some((true, String::new()));
                    fb.status = None;
                }
            }
            KeyCode::Char('.') => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.show_hidden = !fb.show_hidden;
                    fb.reload();
                }
            }
            _ => {}
        }
    }

    /// Create a new file or folder inside the browser's current directory
    /// (US-CFG-09) and move the cursor onto it.
    fn create_browser_entry(&mut self, is_dir: bool, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() || name.contains('/') {
            if let Some(fb) = self.file_browser.as_mut() {
                fb.status = Some("Invalid name".to_string());
            }
            return;
        }
        let target = self.file_browser.as_ref().map(|fb| fb.cwd.join(&name));
        let Some(target) = target else { return };
        let result = if is_dir {
            std::fs::create_dir_all(&target)
        } else {
            std::fs::File::create(&target).map(|_| ())
        };
        let kind = if is_dir { "Folder" } else { "File" };
        match result {
            Ok(_) => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.reload();
                    fb.select(&name);
                    fb.status = Some(format!("✓ {} created: {}", kind, name));
                }
            }
            Err(e) => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.status = Some(format!("✗ {}", e));
                }
            }
        }
    }

    /// Close the browser and write the picked path into whatever opened it
    /// (US-CFG-09/10). For the register form's Path field the path replaces
    /// the value; for the Deploy-to list it is appended; for the target
    /// popup it replaces the destination.
    fn accept_file_browser(&mut self, path: &std::path::Path) {
        let dest = match self.file_browser.as_ref() {
            Some(fb) => fb.dest,
            None => return,
        };
        let path_str = path.to_string_lossy().to_string();
        self.file_browser = None;
        match dest {
            BrowserDest::RegisterPath => {
                if let Some(form) = self.config_input.as_mut() {
                    form.path = path_str;
                    form.error = None;
                    form.sync_name_from_path();
                }
            }
            BrowserDest::DeployTo => {
                if let Some(form) = self.config_input.as_mut() {
                    let mut items: Vec<String> = form
                        .deploy_to
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                    if !items.contains(&path_str) {
                        items.push(path_str);
                    }
                    form.deploy_to = items.join(", ");
                }
            }
            BrowserDest::TargetPath => {
                if let Some(target) = self.config_target_input.as_mut() {
                    *target = path_str;
                }
            }
        }
    }

    /// `m`: cycle the selected config's deploy mode (US-CFG-10).
    async fn cycle_config_deploy_mode(&mut self) {
        let Some(selected) = self.configs_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        let mut entries = self.config_manager().load_registry();
        let mut label = String::new();
        if let Some(entry) = entries.iter_mut().find(|e| e.id == selected.id) {
            entry.deploy_mode = entry.deploy_mode.next();
            label = entry.deploy_mode.label().to_string();
        }
        self.config_manager().save_registry(&entries).ok();
        if !label.is_empty() {
            self.status_message = Some(format!("Deploy mode: {}", label));
        }
        self.fetch_configs().await.ok();
    }

    /// `t`: add the entered target path to the selected config (US-CFG-10).
    async fn add_config_target(&mut self, raw: &str) {
        let target = expand_tilde(raw);
        if target.trim().is_empty() {
            return;
        }
        let Some(selected) = self.configs_list.get_selected().cloned() else {
            return;
        };
        let mut entries = self.config_manager().load_registry();
        let mut total = 0usize;
        if let Some(entry) = entries.iter_mut().find(|e| e.id == selected.id) {
            if !entry.targets.contains(&target) {
                entry.targets.push(target);
            }
            total = entry.targets.len();
        }
        self.config_manager().save_registry(&entries).ok();
        if total > 0 {
            self.status_message = Some(format!("✓ Target added ({} total)", total));
        }
        self.fetch_configs().await.ok();
    }

    /// `l`: deploy the selected config to all targets (US-CFG-10).
    async fn deploy_selected_config(&mut self) {
        let Some(selected) = self.configs_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        if selected.targets.is_empty() {
            self.status_message = Some("No targets yet — press t to add one".to_string());
            return;
        }
        match self.config_manager().deploy(&selected) {
            Ok(results) => {
                let ok = results.iter().filter(|r| r.ok).count();
                let drift = results.iter().filter(|r| r.had_drift).count();
                self.status_message = Some(format!(
                    "✓ Deployed {}/{} ({}) — drift on {}",
                    ok,
                    results.len(),
                    selected.deploy_mode.label(),
                    drift
                ));
            }
            Err(e) => self.status_message = Some(format!("✗ {}", e)),
        }
    }

    /// `u`: sync source into the master, then re-deploy (US-CFG-11).
    async fn update_selected_config(&mut self) {
        let Some(selected) = self.configs_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        let mut entry = selected;
        let synced = match self.config_manager().sync_source(&mut entry) {
            Ok(v) => v,
            Err(e) => {
                self.status_message = Some(format!("✗ sync failed: {}", e));
                return;
            }
        };
        // Persist the (possibly version-bumped) entry
        let mut entries = self.config_manager().load_registry();
        if let Some(e) = entries.iter_mut().find(|e| e.id == entry.id) {
            e.version = entry.version;
        }
        self.config_manager().save_registry(&entries).ok();

        if entry.targets.is_empty() {
            self.status_message = Some(if synced {
                "✓ Master updated (no targets to deploy)".to_string()
            } else {
                "No changes — master already current".to_string()
            });
            self.fetch_configs().await.ok();
            return;
        }
        match self.config_manager().deploy(&entry) {
            Ok(results) => {
                let drift = results.iter().filter(|r| r.had_drift).count();
                self.status_message = Some(format!(
                    "✓ Updated{} and deployed {} target(s) (drift fixed on {})",
                    if synced { "+version bump" } else { "" },
                    results.len(),
                    drift
                ));
            }
            Err(e) => self.status_message = Some(format!("✗ {}", e)),
        }
        self.fetch_configs().await.ok();
    }

    /// `g`: commit the config store to git (US-CFG-12).
    async fn git_commit_configs(&mut self) {
        match self.config_manager().git_commit("update managed configs") {
            Ok(out) => {
                let log = self.config_manager().git_log().unwrap_or_default();
                self.run_result = Some(RunResult {
                    title: "Config git".to_string(),
                    success: true,
                    text: format!(
                        "commit: {}

log:
{}",
                        out, log
                    ),
                });
            }
            Err(e) => self.status_message = Some(format!("✗ {}", e)),
        }
    }

    /// Resolve the current user's `user_profiles.id` (creates the profile row if
    /// missing). Secrets are keyed by profile id, not by username.
    async fn current_user_profile_id(&self) -> String {
        let username = self
            .current_user_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        repository::get_or_create_user(&*self.pool, &username)
            .await
            .map(|p| p.id)
            .unwrap_or(username)
    }

    /// Refresh data for current tab
    async fn refresh_current_tab(&mut self) -> anyhow::Result<()> {
        match self.ui.state {
            AppState::Knowledge => self.fetch_knowledge().await?,
            AppState::Projects => self.fetch_projects().await?,
            AppState::Workflows => self.fetch_workflows().await?,
            AppState::Secrets => self.fetch_secrets().await?,
            AppState::Configs => self.fetch_configs().await?,
            AppState::Dashboard => self.fetch_stats().await?,
            AppState::Dashboard => self.fetch_monitor().await,
            AppState::Plugins => self.fetch_plugins().await,
            _ => {}
        }
        Ok(())
    }

    /// Run the modern application
    pub async fn run(&mut self) -> anyhow::Result<()> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;

        loop {
            // Dashboard auto-refresh: mini-btop panels tick every 2 seconds
            if self.ui.state == AppState::Dashboard
                && self.monitor_refreshed.elapsed() > std::time::Duration::from_secs(2)
            {
                self.fetch_monitor().await;
            }

            // Collect finished background workflow runs (US-WF-09)
            self.poll_workflow_task().await;

            // Quick launch: open a NEW terminal window running a tool (lazygit, ...)
            if let Some((cmd, cwd)) = self.wants_terminal_cmd.take() {
                match terminal_window_command_for(&cmd, Some(&cwd)) {
                    Some((prog, args)) => {
                        let spawned = std::process::Command::new(&prog)
                            .args(&args)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                        self.status_message = match spawned {
                            Ok(_) => Some(format!("Opened {} in new terminal", cmd)),
                            Err(e) => Some(format!("Failed to open {}: {}", prog, e)),
                        };
                    }
                    None => {
                        self.status_message =
                            Some("No terminal emulator found (set $TERMINAL)".to_string());
                    }
                }
            }

            // `: open a NEW terminal window (detached; the TUI keeps running)
            if self.wants_terminal {
                self.wants_terminal = false;
                match terminal_window_command() {
                    Some((prog, args)) => {
                        let spawned = std::process::Command::new(&prog)
                            .args(&args)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                        self.status_message = match spawned {
                            Ok(_) => Some(format!("\u{2713} Opened new terminal: {}", prog)),
                            Err(e) => Some(format!("\u{2717} Failed to open {}: {}", prog, e)),
                        };
                    }
                    None => {
                        self.status_message =
                            Some("\u{2717} No terminal emulator found (set $TERMINAL)".to_string());
                    }
                }
            }
            if self.needs_full_redraw {
                terminal.clear()?;
                self.needs_full_redraw = false;
            }
            terminal.draw(|f| self.render(f))?;

            if self.should_quit {
                break;
            }

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    self.handle_key(key).await;
                }
            }
        }

        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn render(&self, f: &mut Frame) {
        self.render_base(f);

        // Overlays render on top of the base screen (draw order = priority)
        if self.visual_form.is_some() {
            self.render_visual(f);
        }
        // Logic-node editor renders on top of the builder (US-FUT-07)
        if self
            .visual_form
            .as_ref()
            .map_or(false, |v| v.node_editor.is_some())
        {
            self.render_node_editor(f);
        }
        if self.command_form.mode.is_some() {
            self.render_command_form(f);
        }
        if self.project_form.mode.is_some() {
            self.render_project_form(f);
        }
        if self.workflow_form.mode.is_some() {
            self.render_workflow_form(f);
        }
        if self.secret_form.mode.is_some() {
            self.render_secret_form(f);
        }
        if self.run_result.is_some() {
            self.render_run_result(f);
        }
        if self.keygen.open {
            self.render_keygen_form(f);
        }
        if self.options_popup.is_some() {
            self.render_options_popup(f);
        }
        if let Some(path) = &self.register_input {
            self.render_register_input(f, path);
        }
        // Configs popups render independently — they were once nested
        // inside the register_input block, so pressing `n`/`t` on Configs set
        // the state but drew nothing (US-CFG-09)
        if let Some(form) = &self.config_input {
            self.render_config_register_form(f, form);
        }
        if let Some(path) = &self.config_target_input {
            let cfg_name = self
                .configs_list
                .get_selected()
                .map(|c| c.name.clone())
                .unwrap_or_default();
            self.render_config_target_modal(f, path, &cfg_name);
        }
        // In-TUI file browser renders on top of the config popups (US-CFG-09)
        if let Some(fb) = &self.file_browser {
            self.render_file_browser(f, fb);
        }
        if let Some(path) = &self.import_input {
            self.render_import_input(f, path);
        }
        if let Some(cron) = &self.cron_input {
            self.render_cron_input(f, cron);
        }
        if let Some(detail) = &self.project_detail {
            self.render_project_detail(f, detail);
        }
        if self.ssh_panel.is_some() {
            self.render_ssh_panel(f);
        }
        if self.users_panel.is_some() {
            self.render_users_panel(f);
        }
        if self.keybinds_overlay {
            self.render_keybinds_overlay(f);
        }
        if self.sudo_password.is_some() {
            self.render_sudo_password_popup(f);
        }
        if self.new_project_open {
            self.render_new_project_form(f);
        }
        // Plugin UI actions popup renders on top of the Projects list (US-PLG-13)
        if self.project_actions.is_some() {
            self.render_project_actions(f);
        }
        // Template preview renders on top of the workspace form (US-PLG-15)
        if self.template_preview.is_some() {
            self.render_template_preview(f);
        }
        if let Some(ref confirm) = self.confirm_delete {
            self.render_confirm_delete(f, confirm);
        }
        if let Some(ref msg) = self.status_message {
            self.render_status(f, msg);
        }
    }

    fn render_base(&self, f: &mut Frame) {
        match self.ui.state {
            AppState::Login => {
                if self.dev_user_manager {
                    self.render_login_user_manager(f);
                    return;
                }
                if self.auth_mode == AuthMode::Login {
                    self.render_login(f);
                } else {
                    self.render_signup(f);
                }
            }
            AppState::Dashboard => {
                // Render dashboard with real stats
                self.render_dashboard_with_stats(f);
            }
            AppState::Knowledge => {
                // ONE knowledge tab: filtered list (US-TUI-11/12)
                self.render_commands_list(f);
            }
            AppState::Knowledge => {
                self.render_commands_list(f);
            }
            AppState::Projects => {
                self.render_projects_list(f);
            }
            AppState::Workflows => {
                self.render_workflows_list(f);
            }
            AppState::Secrets => {
                self.render_secrets_list(f);
            }
            AppState::Configs => {
                self.render_configs_list(f);
            }
            AppState::Settings => {
                // Dedicated settings screen (US-APP-01/02/06)
                if self.advanced.active {
                    self.render_advanced_screen(f);
                } else {
                    self.render_settings_screen(f);
                }
            }
            AppState::Plugins => {
                self.render_plugins_list(f);
            }
            AppState::Help => {
                // Delegate to UI for help
                self.ui.render(f);
            }
        }
    }

    fn render_dashboard_with_stats(&self, f: &mut Frame) {
        let area = f.area();

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        // Render header, content with stats, and footer using UI methods
        let header_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.primary))
            .style(Style::default().bg(self.ui.theme.bg));

        let inner = header_block.inner(chunks[0]);
        f.render_widget(header_block, chunks[0]);

        let header_text = Line::from(vec![
            Span::styled("🎛️ ", Style::default().fg(self.ui.theme.accent)),
            Span::styled(
                "TUI-OP-HUB",
                Style::default()
                    .fg(self.ui.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" › "),
            Span::styled("Dashboard", Style::default().fg(self.ui.theme.highlight)),
        ]);

        let header = Paragraph::new(header_text).alignment(Alignment::Center);
        f.render_widget(header, inner);

        // Dashboard content: stat cards on top, mini-btop monitor boxes below,
        // quick-launch row at the bottom
        let content_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8), // stat cards
                Constraint::Min(4),    // monitor boxes
                Constraint::Length(3), // quick launches
            ])
            .margin(1)
            .split(chunks[1]);

        let card_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(content_rows[0]);

        // Commands card
        self.render_stat_card(
            f,
            card_chunks[0],
            "📝 Commands",
            &self.stats.commands_count.to_string(),
            self.ui.theme.primary,
        );

        // Projects card
        self.render_stat_card(
            f,
            card_chunks[1],
            "📁 Projects",
            &self.stats.projects_count.to_string(),
            self.ui.theme.secondary,
        );

        // Workflows card
        self.render_stat_card(
            f,
            card_chunks[2],
            "⚙️ Workflows",
            &self.stats.workflows_count.to_string(),
            self.ui.theme.accent,
        );

        // Secrets card
        self.render_stat_card(
            f,
            card_chunks[3],
            "🔐 Secrets",
            &self.stats.secrets_count.to_string(),
            self.ui.theme.warning,
        );

        // Mini-btop monitor boxes (CPU / RAM / Network / Temp+GPU)
        self.render_monitor_boxes(f, content_rows[1]);

        // Quick-launch row for installed TUI tools
        self.render_quick_launches(f, content_rows[2]);

        // Footer: keybind hints for this screen
        self.render_keybind_footer(f, chunks[2], &self.keybind_hints());
    }

    /// The four mini-btop boxes: CPU, Memory, Network, Temps+GPU (US-PROC-01).
    fn render_monitor_boxes(&self, f: &mut Frame, area: Rect) {
        let boxes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(area);

        let Some(snap) = &self.monitor_snap else {
            for b in boxes.iter() {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(self.ui.theme.border))
                    .title(" loading... ");
                f.render_widget(block, *b);
            }
            return;
        };

        // CPU box: overall gauge + per-core mini bars
        {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(self.ui.theme.primary))
                .title(format!(" CPU {:.0}% ", snap.cpu_overall));
            let inner = block.inner(boxes[0]);
            f.render_widget(block, boxes[0]);
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(format!(
                "{} {:.0}%",
                crate::monitor::MonitorSnapshot::bar(snap.cpu_overall, 14),
                snap.cpu_overall
            )));
            let width = 8.min(inner.width.saturating_sub(8) as usize);
            for (i, usage) in snap.cpu_per_core.iter().skip(1).enumerate() {
                if lines.len() >= inner.height as usize {
                    break;
                }
                lines.push(Line::from(format!(
                    "c{:02} {} {:.0}%",
                    i,
                    crate::monitor::MonitorSnapshot::bar(*usage, width),
                    usage
                )));
            }
            f.render_widget(Paragraph::new(lines), inner);
        }

        // Memory box: RAM + swap gauges
        {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(self.ui.theme.success))
                .title(" Memory ");
            let inner = block.inner(boxes[1]);
            f.render_widget(block, boxes[1]);
            let ram_pct = if snap.ram_total > 0 {
                snap.ram_used as f32 / snap.ram_total as f32 * 100.0
            } else {
                0.0
            };
            let used_gb = snap.ram_used as f64 / 1024.0 / 1024.0 / 1024.0;
            let total_gb = snap.ram_total as f64 / 1024.0 / 1024.0 / 1024.0;
            let mut lines = vec![Line::from(format!(
                "{} {:.1}/{:.1}GB",
                crate::monitor::MonitorSnapshot::bar(ram_pct, 14),
                used_gb,
                total_gb
            ))];
            if snap.swap_total > 0 {
                let swap_pct = snap.swap_used as f32 / snap.swap_total as f32 * 100.0;
                let su = snap.swap_used as f64 / 1024.0 / 1024.0 / 1024.0;
                let st = snap.swap_total as f64 / 1024.0 / 1024.0 / 1024.0;
                lines.push(Line::from(format!(
                    "{} swap {:.1}/{:.1}GB",
                    crate::monitor::MonitorSnapshot::bar(swap_pct, 14),
                    su,
                    st
                )));
            }
            f.render_widget(Paragraph::new(lines), inner);
        }

        // Network box: physical interfaces with live RX/TX (virtual filtered)
        {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(self.ui.theme.accent))
                .title(" Network ");
            let inner = block.inner(boxes[2]);
            f.render_widget(block, boxes[2]);
            let mut lines = Vec::new();
            if snap.interfaces.is_empty() {
                lines.push(Line::from("no physical interfaces"));
            }
            for iface in snap.interfaces.iter().take(inner.height as usize) {
                lines.push(Line::from(format!(
                    "{} v{} / ^{}",
                    iface.name,
                    humans(iface.rx),
                    humans(iface.tx)
                )));
            }
            f.render_widget(Paragraph::new(lines), inner);
        }

        // Temps + GPU box
        {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(self.ui.theme.warning))
                .title(if snap.gpus.is_empty() {
                    " Temps "
                } else {
                    " Temps + GPU "
                });
            let inner = block.inner(boxes[3]);
            f.render_widget(block, boxes[3]);
            let mut lines = Vec::new();
            for t in snap.temps.iter().take(inner.height as usize / 2) {
                lines.push(Line::from(format!("{} {:.0}\u{2103}", t.label, t.celsius)));
            }
            for (name, util, temp) in snap.gpus.iter().take(2) {
                lines.push(Line::from(format!(
                    "GPU {} {:.0}% {:.0}\u{2103}",
                    name, util, temp
                )));
            }
            if lines.is_empty() {
                lines.push(Line::from("no sensors"));
            }
            f.render_widget(Paragraph::new(lines), inner);
        }
    }

    /// The quick-launch row: installed TUI tools (lazygit, lazydocker, k9s, ...).
    fn render_quick_launches(&self, f: &mut Frame, area: Rect) {
        let launches = self
            .monitor_snap
            .as_ref()
            .map(|s| s.quick_launches.clone())
            .unwrap_or_default();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.highlight))
            .title(" Quick launch ");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let text = if launches.is_empty() {
            "no TUI tools found (install lazygit / lazydocker / k9s / lazynpm)".to_string()
        } else {
            launches
                .iter()
                .map(|q| format!("[{}] {} - {}", q.key, q.name, q.description))
                .collect::<Vec<_>>()
                .join("   ")
        };
        f.render_widget(Paragraph::new(text), inner);
    }

    fn render_stat_card(
        &self,
        f: &mut Frame,
        area: Rect,
        title: &str,
        count: &str,
        color: ratatui::style::Color,
    ) {
        let card = Block::default()
            .title(title)
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(color))
            .padding(Padding::uniform(2))
            .style(Style::default().bg(self.ui.theme.bg));

        let inner = card.inner(area);
        f.render_widget(card, area);

        let count_text = Paragraph::new(count)
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        f.render_widget(count_text, inner);
    }

    fn render_login(&self, f: &mut Frame) {
        let area = f.area();

        // Create centered login box
        let login_width = 60.min(area.width - 4);
        let login_height = 22.min(area.height - 4);

        let vertical_margin = (area.height.saturating_sub(login_height)) / 2;
        let horizontal_margin = (area.width.saturating_sub(login_width)) / 2;

        let login_area = Rect {
            x: horizontal_margin,
            y: vertical_margin,
            width: login_width,
            height: login_height,
        };

        f.render_widget(Clear, login_area);

        let login_block = Block::default()
            .title(" 🔐 TUI-OP-HUB Login ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.primary))
            .padding(Padding::uniform(2))
            .style(Style::default().bg(self.ui.theme.bg));

        let inner = login_block.inner(login_area);
        f.render_widget(login_block, login_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Welcome
                Constraint::Length(3), // Username
                Constraint::Length(3), // Password
                Constraint::Length(2), // Error
                Constraint::Length(1), // Progress
                Constraint::Min(1),    // Help
            ])
            .split(inner);

        // Welcome
        let welcome = Paragraph::new(vec![
            Line::from(Span::styled(
                "Welcome to TUI-OP-HUB",
                Style::default()
                    .fg(self.ui.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Please enter your credentials",
                Style::default().fg(self.ui.theme.fg),
            )),
        ])
        .alignment(Alignment::Center);
        f.render_widget(welcome, chunks[0]);

        // Username field
        let username_focused = self.ui.login_state.focused_field == LoginField::Username;
        let username_style = if username_focused {
            Style::default()
                .fg(self.ui.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.ui.theme.border)
        };

        let username_block = Block::default()
            .title(if username_focused {
                " ▶ Username "
            } else {
                " Username "
            })
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(username_style);

        let username_text = Paragraph::new(self.ui.login_state.username.as_str())
            .block(username_block)
            .style(Style::default().fg(self.ui.theme.fg));
        f.render_widget(username_text, chunks[1]);

        // Password field
        let password_focused = self.ui.login_state.focused_field == LoginField::Password;
        let password_style = if password_focused {
            Style::default()
                .fg(self.ui.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.ui.theme.border)
        };

        let password_block = Block::default()
            .title(if password_focused {
                " ▶ Password "
            } else {
                " Password "
            })
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(password_style);

        let masked_password = "•".repeat(self.ui.login_state.password.len());
        let password_text = Paragraph::new(masked_password.as_str())
            .block(password_block)
            .style(Style::default().fg(self.ui.theme.fg));
        f.render_widget(password_text, chunks[2]);

        // Error message
        if let Some(ref error) = self.ui.login_state.error_message {
            let error_text = Paragraph::new(error.as_str())
                .style(
                    Style::default()
                        .fg(self.ui.theme.error)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center);
            f.render_widget(error_text, chunks[3]);
        }

        // Progress bar
        if self.ui.login_state.is_authenticating {
            let progress_text = Paragraph::new("Authenticating...")
                .style(Style::default().fg(self.ui.theme.primary))
                .alignment(Alignment::Center);
            f.render_widget(progress_text, chunks[4]);
        }

        // Help text
        let help_lines = vec![
            Line::from(vec![
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(self.ui.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" / "),
                Span::styled(
                    "↑↓",
                    Style::default()
                        .fg(self.ui.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Switch fields  "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(self.ui.theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Login"),
            ]),
            Line::from(vec![
                Span::styled(
                    "Ctrl+S",
                    Style::default()
                        .fg(self.ui.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Sign up  "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(self.ui.theme.error)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Quit"),
            ]),
        ];
        let help = Paragraph::new(help_lines).alignment(Alignment::Center);
        f.render_widget(help, chunks[5]);
    }

    fn render_signup(&self, f: &mut Frame) {
        let area = f.area();

        let signup_width = 60.min(area.width - 4);
        let signup_height = 24.min(area.height - 4);

        let vertical_margin = (area.height.saturating_sub(signup_height)) / 2;
        let horizontal_margin = (area.width.saturating_sub(signup_width)) / 2;

        let signup_area = Rect {
            x: horizontal_margin,
            y: vertical_margin,
            width: signup_width,
            height: signup_height,
        };

        f.render_widget(Clear, signup_area);

        let signup_block = Block::default()
            .title(" ✨ Create Account ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .padding(Padding::uniform(2))
            .style(Style::default().bg(self.ui.theme.bg));

        let inner = signup_block.inner(signup_area);
        f.render_widget(signup_block, signup_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Welcome
                Constraint::Length(3), // Username
                Constraint::Length(3), // Password
                Constraint::Length(3), // Confirm Password
                Constraint::Length(2), // Error
                Constraint::Min(1),    // Help
            ])
            .split(inner);

        // Welcome
        let welcome = Paragraph::new(vec![
            Line::from(Span::styled(
                "Create Your Account",
                Style::default()
                    .fg(self.ui.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Join TUI-OP-HUB today",
                Style::default().fg(self.ui.theme.fg),
            )),
        ])
        .alignment(Alignment::Center);
        f.render_widget(welcome, chunks[0]);

        // Username field
        let username_focused = self.signup_state.focused_field == SignupField::Username;
        self.render_field(
            f,
            chunks[1],
            "Username",
            &self.signup_state.username,
            username_focused,
            false,
        );

        // Password field
        let password_focused = self.signup_state.focused_field == SignupField::Password;
        self.render_field(
            f,
            chunks[2],
            "Password",
            &self.signup_state.password,
            password_focused,
            true,
        );

        // Confirm Password field
        let confirm_focused = self.signup_state.focused_field == SignupField::ConfirmPassword;
        self.render_field(
            f,
            chunks[3],
            "Confirm Password",
            &self.signup_state.confirm_password,
            confirm_focused,
            true,
        );

        // Error message
        if let Some(ref error) = self.signup_state.error_message {
            let error_text = Paragraph::new(error.as_str())
                .style(
                    Style::default()
                        .fg(self.ui.theme.error)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center);
            f.render_widget(error_text, chunks[4]);
        }

        // Help text
        let help_lines = vec![
            Line::from(vec![
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(self.ui.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" / "),
                Span::styled(
                    "↑↓",
                    Style::default()
                        .fg(self.ui.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Switch fields  "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(self.ui.theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Create Account"),
            ]),
            Line::from(vec![
                Span::styled(
                    "Ctrl+L",
                    Style::default()
                        .fg(self.ui.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Back to Login  "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(self.ui.theme.error)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Quit"),
            ]),
        ];
        let help = Paragraph::new(help_lines).alignment(Alignment::Center);
        f.render_widget(help, chunks[5]);
    }

    fn render_field(
        &self,
        f: &mut Frame,
        area: Rect,
        title: &str,
        value: &str,
        focused: bool,
        mask: bool,
    ) {
        let style = if focused {
            Style::default()
                .fg(self.ui.theme.secondary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.ui.theme.border)
        };

        let block = Block::default()
            .title(if focused {
                format!(" ▶ {} ", title)
            } else {
                format!(" {} ", title)
            })
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style);

        let display_value = if mask {
            "•".repeat(value.len())
        } else {
            value.to_string()
        };

        let text = Paragraph::new(display_value.as_str())
            .block(block)
            .style(Style::default().fg(self.ui.theme.fg));
        f.render_widget(text, area);
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        // Collect finished background workflow runs (US-WF-09) before routing
        self.poll_workflow_task().await;
        // Overlays take priority over normal tab handling (top of the input stack)
        if self.sudo_password.is_some() {
            // The sudo password popup renders last = topmost; without this
            // route, typed password characters leak into the list handler.
            self.handle_sudo_password_key(key).await;
            return;
        }
        // In-TUI file browser is the topmost overlay when open (US-CFG-09)
        if self.file_browser.is_some() {
            self.handle_file_browser_key(key);
            return;
        }
        // Plugin UI actions popup is topmost when open (US-PLG-13)
        if self.project_actions.is_some() {
            self.handle_project_actions_key(key).await;
            return;
        }
        if self.keybinds_overlay {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter) {
                self.keybinds_overlay = false;
            }
            return;
        }
        if self.users_panel.is_some() {
            self.handle_users_panel_key(key).await;
            return;
        }
        if self.ssh_panel.is_some() {
            self.handle_ssh_panel_key(key).await;
            return;
        }
        if self.options_popup.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.options_popup = None;
            }
            return;
        }
        if self.keygen.open {
            self.handle_keygen_key(key).await;
            return;
        }
        if self.confirm_delete.is_some() {
            self.handle_confirm_key(key).await;
            return;
        }
        if self.visual_form.is_some() {
            self.handle_visual_key(key).await;
            return;
        }
        if self.command_form.mode.is_some() {
            self.handle_command_form_key(key).await;
            return;
        }
        if self.project_form.mode.is_some() {
            self.handle_project_form_key(key).await;
            return;
        }
        if self.workflow_form.mode.is_some() {
            self.handle_workflow_form_key(key).await;
            return;
        }
        if self.secret_form.mode.is_some() {
            self.handle_secret_form_key(key).await;
            return;
        }
        if let Some(detail) = self.project_detail.as_mut() {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                    self.project_detail = None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    detail.selected = detail.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if detail.selected + 1 < detail.entities.len() {
                        detail.selected += 1;
                    }
                }
                KeyCode::Char('o') => {
                    // Open the project workspace in the configured editor
                    self.open_project_in_editor().await;
                }
                KeyCode::Char('c') => {
                    // Copy the selected entity's content to the clipboard
                    if let Some(text) = detail
                        .entities
                        .get(detail.selected)
                        .and_then(|e| e.content.clone())
                    {
                        match arboard::Clipboard::new() {
                            Ok(mut cb) => {
                                let _ = cb.set_text(text);
                                self.status_message =
                                    Some("\u{2713} Copied to clipboard".to_string());
                            }
                            Err(e) => {
                                self.status_message =
                                    Some(format!("\u{2717} Clipboard unavailable: {}", e))
                            }
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some((id, buf)) = self.secret_pass_prompt.as_mut() {
            match key.code {
                KeyCode::Esc => self.secret_pass_prompt = None,
                KeyCode::Enter => {
                    let (secret_id, pass) = (id.clone(), buf.clone());
                    self.secret_pass_prompt = None;
                    self.copy_secret_with_passphrase(&secret_id, &pass).await;
                }
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) => buf.push(c),
                _ => {}
            }
            return;
        }
        if self.new_project_open {
            // The workspace creation form captures all keys (US-PROJ); without
            // this guard, typed characters leak into other handlers (e.g. `p`
            // spawning a process viewer mid-form).
            self.handle_new_project_key(key).await;
            return;
        }
        if let Some(path) = self.register_input.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    self.register_input = None;
                }
                KeyCode::Enter => {
                    let entered = path.clone();
                    self.register_input = None;
                    self.register_existing_directory(&entered).await;
                }
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Char(c) => {
                    path.push(c);
                }
                _ => {}
            }
            return;
        }
        if let Some(path) = self.export_input.as_mut() {
            if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
                // System directory picker (yazi / nnn / ranger / lf / zenity / kdialog)
                let picked = crate::filepicker::pick(crate::filepicker::PickKind::Directory);
                self.needs_full_redraw = true; // picker suspended the TUI
                match picked {
                    Ok(Some(dir)) => {
                        let name = path
                            .rsplit('/')
                            .next()
                            .unwrap_or("tui-op-hub-export.json")
                            .to_string();
                        *path = dir.join(name).to_string_lossy().to_string();
                    }
                    Ok(None) => {
                        self.status_message =
                            Some(String::from("No picker available - type the path"));
                    }
                    Err(e) => self.status_message = Some(format!("{}", e)),
                }
                return;
            }
            match key.code {
                KeyCode::Esc => {
                    self.export_input = None;
                }
                KeyCode::Enter => {
                    let target = path.clone();
                    self.export_input = None;
                    self.export_knowledge_to_path(&target).await;
                }
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Char(c) => path.push(c),
                _ => {}
            }
            return;
        }
        if let Some(mut form) = self.config_input.take() {
            // Ctrl+O: built-in file browser (pick existing or create new
            // files/folders in-app, US-CFG-09). On the Path field it picks a
            // file; on the Deploy-to field it picks a folder to append.
            // Ctrl+P: external system picker (yazi / nnn / ranger / lf /
            // zenity / kdialog) as fallback.
            if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if form.focus == 4 {
                    // Deploy-to field: pick a folder to append to the list
                    let start = form.deploy_to.clone();
                    self.config_input = Some(form);
                    self.open_file_browser(true, BrowserDest::DeployTo, &start);
                } else {
                    let start = form.path.clone();
                    self.config_input = Some(form);
                    self.open_file_browser(false, BrowserDest::RegisterPath, &start);
                }
                return;
            }
            if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
                let picked = crate::filepicker::pick(crate::filepicker::PickKind::File);
                self.needs_full_redraw = true; // picker suspended the TUI
                match picked {
                    Ok(Some(p)) => {
                        form.path = p.to_string_lossy().to_string();
                        form.sync_name_from_path();
                    }
                    Ok(None) => {
                        self.status_message =
                            Some(String::from("No file picker available - type the path"));
                    }
                    Err(e) => self.status_message = Some(format!("{}", e)),
                }
                self.config_input = Some(form);
                return;
            }
            match key.code {
                KeyCode::Esc => {
                    // form already taken; stays closed
                }
                KeyCode::Enter => match self.validate_config_form(&form) {
                    Some(err) => {
                        form.error = Some(err);
                        self.config_input = Some(form);
                    }
                    None => self.register_config_from_form(&form).await,
                },
                KeyCode::Tab | KeyCode::Down => {
                    form.focus = cycle_field(form.focus, ConfigRegisterForm::FIELDS, true);
                    self.config_input = Some(form);
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.focus = cycle_field(form.focus, ConfigRegisterForm::FIELDS, false);
                    self.config_input = Some(form);
                }
                KeyCode::Left | KeyCode::Right if form.focus == 5 => {
                    // Deploy-mode selector row: cycle with arrow keys
                    // (3 variants, so Left = 2 forward steps)
                    let steps = if key.code == KeyCode::Left { 2 } else { 1 };
                    for _ in 0..steps {
                        form.deploy_mode = form.deploy_mode.next();
                    }
                    self.config_input = Some(form);
                }
                KeyCode::Backspace => {
                    match form.focus {
                        0 => {
                            form.path.pop();
                            form.sync_name_from_path();
                        }
                        1 => {
                            form.name.pop();
                            form.name_touched = true;
                        }
                        2 => {
                            form.description.pop();
                        }
                        3 => {
                            form.tags.pop();
                        }
                        4 => {
                            form.deploy_to.pop();
                        }
                        _ => {}
                    }
                    self.config_input = Some(form);
                }
                KeyCode::Char(c) => {
                    match form.focus {
                        0 => {
                            form.path.push(c);
                            form.sync_name_from_path();
                        }
                        1 => {
                            // Typing replaces the auto-filled suggestion
                            if !form.name_touched {
                                form.name.clear();
                                form.name_touched = true;
                            }
                            form.name.push(c);
                        }
                        2 => {
                            form.description.push(c);
                        }
                        3 => {
                            form.tags.push(c);
                        }
                        4 => {
                            form.deploy_to.push(c);
                        }
                        _ => {}
                    }
                    self.config_input = Some(form);
                }
                _ => self.config_input = Some(form),
            }
            return;
        }
        if let Some(path) = self.config_target_input.as_mut() {
            if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
                // Built-in folder browser (US-CFG-10)
                let start = path.clone();
                self.open_file_browser(true, BrowserDest::TargetPath, &start);
                return;
            }
            if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
                // External system picker as fallback
                let picked = crate::filepicker::pick(crate::filepicker::PickKind::File);
                self.needs_full_redraw = true; // picker suspended the TUI
                match picked {
                    Ok(Some(p)) => *path = p.to_string_lossy().to_string(),
                    Ok(None) => {
                        self.status_message =
                            Some(String::from("No file picker available - type the path"));
                    }
                    Err(e) => self.status_message = Some(format!("{}", e)),
                }
                return;
            }
            match key.code {
                KeyCode::Esc => self.config_target_input = None,
                KeyCode::Enter => {
                    let entered = path.clone();
                    self.config_target_input = None;
                    self.add_config_target(&entered).await;
                }
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Char(c) => path.push(c),
                _ => {}
            }
            return;
        }
        if let Some(path) = self.import_input.as_mut() {
            if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.import_dup_mode = self.import_dup_mode.next();
                return;
            }
            if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
                // System file picker (yazi / nnn / ranger / lf / zenity / kdialog)
                let picked = crate::filepicker::pick(crate::filepicker::PickKind::File);
                self.needs_full_redraw = true; // picker suspended the TUI
                match picked {
                    Ok(Some(p)) => *path = p.to_string_lossy().to_string(),
                    Ok(None) => {
                        self.status_message =
                            Some(String::from("No file picker available - type the path"));
                    }
                    Err(e) => self.status_message = Some(format!("{}", e)),
                }
                return;
            }
            match key.code {
                KeyCode::Esc => {
                    self.import_input = None;
                }
                KeyCode::Enter => {
                    let entered = path.clone();
                    self.import_input = None;
                    let mode = self.import_dup_mode;
                    self.import_knowledge_from_path(&entered, mode).await;
                }
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Char(c) => {
                    path.push(c);
                }
                _ => {}
            }
            return;
        }
        if let Some(cron) = self.cron_input.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    self.cron_input = None;
                }
                KeyCode::Enter => {
                    let expr = cron.clone();
                    self.cron_input = None;
                    self.save_workflow_schedule(&expr).await;
                }
                KeyCode::Backspace => {
                    cron.pop();
                }
                KeyCode::Char(c) => {
                    cron.push(c);
                }
                _ => {}
            }
            return;
        }
        if self.search_state.active {
            self.handle_search_key(key).await;
            return;
        }
        if self.run_result.is_some() {
            // Any of these closes the result popup
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.run_result = None;
            }
            return;
        }
        self.status_message = None;

        // Keybinds helper is global: `?` toggles it from any screen (US-TUI-09)
        if key.code == KeyCode::Char('?') {
            self.keybinds_overlay = !self.keybinds_overlay;
            return;
        }

        match self.ui.state {
            AppState::Login => {
                if self.dev_user_manager {
                    self.handle_login_dev_users_key(key).await;
                    return;
                }
                if crate::auth::dev_mode_enabled()
                    && key.code == KeyCode::Char('u')
                    && key.modifiers.is_empty()
                {
                    self.open_login_user_manager();
                    self.refresh_dev_user_list().await;
                    return;
                }
                if self.auth_mode == AuthMode::Login {
                    self.handle_login_key(key).await;
                } else {
                    self.handle_signup_key(key).await;
                }
            }
            AppState::Dashboard
            | AppState::Knowledge
            | AppState::Projects
            | AppState::Workflows
            | AppState::Secrets
            | AppState::Configs
            | AppState::Knowledge => {
                // Handle dashboard and other app state keys
                self.handle_dashboard_key(key).await;
            }
            AppState::Settings => {
                // Settings screen has its own key handling (US-APP-01/02/06)
                self.handle_settings_key(key).await;
            }
            AppState::Plugins => {
                self.handle_dashboard_key(key).await;
            }
            AppState::Help => {
                // Handle help screen keys
                match key.code {
                    KeyCode::Char('?') | KeyCode::Esc => {
                        // Close help and return to dashboard
                        self.ui.state = AppState::Dashboard;
                    }
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                    }
                    _ => {}
                }
            }
        }
    }

    async fn handle_login_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab | KeyCode::Down => self.ui.login_state.next_field(),
            KeyCode::Up => self.ui.login_state.next_field(),
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+S to switch to signup
                self.auth_mode = AuthMode::Signup;
                self.signup_state = SignupState::default();
            }
            KeyCode::Enter => {
                if !self.ui.login_state.is_authenticating {
                    self.attempt_login().await;
                }
            }
            KeyCode::Backspace => match self.ui.login_state.focused_field {
                LoginField::Username => {
                    self.ui.login_state.username.pop();
                }
                LoginField::Password => {
                    self.ui.login_state.password.pop();
                }
            },
            KeyCode::Char(c) => match self.ui.login_state.focused_field {
                LoginField::Username => self.ui.login_state.username.push(c),
                LoginField::Password => self.ui.login_state.password.push(c),
            },
            _ => {}
        }
    }

    async fn handle_signup_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab | KeyCode::Down => self.signup_state.next_field(),
            KeyCode::Up => self.signup_state.next_field(),
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+L to switch to login
                self.auth_mode = AuthMode::Login;
                self.ui.login_state = LoginState::default();
            }
            KeyCode::Enter => {
                if !self.signup_state.is_creating {
                    self.attempt_signup().await;
                }
            }
            KeyCode::Backspace => match self.signup_state.focused_field {
                SignupField::Username => {
                    self.signup_state.username.pop();
                }
                SignupField::Password => {
                    self.signup_state.password.pop();
                }
                SignupField::ConfirmPassword => {
                    self.signup_state.confirm_password.pop();
                }
            },
            KeyCode::Char(c) => match self.signup_state.focused_field {
                SignupField::Username => self.signup_state.username.push(c),
                SignupField::Password => self.signup_state.password.push(c),
                SignupField::ConfirmPassword => self.signup_state.confirm_password.push(c),
            },
            _ => {}
        }
    }

    async fn handle_dashboard_key(&mut self, key: KeyEvent) {
        // Configs tab: register/deploy/update/git keys; everything else
        // (navigation, delete) falls through to the list handling (US-CFG).
        if self.ui.state == AppState::Configs {
            match key.code {
                KeyCode::Char('n') => {
                    self.config_input = Some(ConfigRegisterForm::new());
                    return;
                }
                KeyCode::Char('t') => {
                    if self.configs_list.get_selected().is_some() {
                        self.config_target_input = Some(String::new());
                    } else {
                        self.status_message = Some("Nothing selected".to_string());
                    }
                    return;
                }
                KeyCode::Char('m') => {
                    self.cycle_config_deploy_mode().await;
                    return;
                }
                KeyCode::Char('l') => {
                    self.deploy_selected_config().await;
                    return;
                }
                KeyCode::Char('u') => {
                    self.update_selected_config().await;
                    return;
                }
                KeyCode::Char('g') => {
                    self.git_commit_configs().await;
                    return;
                }
                // Numpad navigation (NumLock on): 2/8 move, 4/6 cycle mode,
                // 7/9 home/end (US-APP-06)
                KeyCode::Char('2') | KeyCode::Char('8') if !self.configs_list.items.is_empty() => {
                    let d = if key.code == KeyCode::Char('2') {
                        1
                    } else {
                        -1
                    };
                    let len = self.configs_list.items.len();
                    let cur = self.configs_list.selected as isize;
                    self.configs_list.selected = ((cur + d).rem_euclid(len as isize)) as usize;
                    return;
                }
                KeyCode::Char('4') | KeyCode::Char('6') => {
                    self.cycle_config_deploy_mode().await;
                    return;
                }
                KeyCode::Char('7') => {
                    self.configs_list.selected = 0;
                    return;
                }
                KeyCode::Char('9') => {
                    if !self.configs_list.items.is_empty() {
                        self.configs_list.selected = self.configs_list.items.len() - 1;
                    }
                    return;
                }
                _ => {}
            }
        }
        // Knowledge tab: `f` cycles the type filter, `n` creates an item of
        // the filtered type; EVERY other key falls through to the normal
        // list handling so navigation/run/copy/etc. work here too (US-TUI-11).
        if self.ui.state == AppState::Knowledge {
            match key.code {
                KeyCode::Char('f') => {
                    self.kb_filter = self.kb_filter.next();
                    self.status_message = Some(format!("Filter: {}", self.kb_filter.label()));
                    let _ = self.fetch_knowledge().await;
                    return;
                }
                KeyCode::Char('n') => {
                    // Create a new item of the filtered type
                    self.new_knowledge_item();
                    return;
                }
                _ => {}
            }
        }
        // Action keys come from the config (US-APP-02) and can be rebound in Settings
        let kb_quit = self.action_keycode("quit");
        let kb_search = self.action_keycode("search");
        let kb_filter = self.action_keycode("filter");
        let kb_create = self.action_keycode("create");
        let kb_edit = self.action_keycode("edit");
        let kb_delete = self.action_keycode("delete");
        let kb_copy = self.action_keycode("copy");
        let kb_run = self.action_keycode("run");

        match key.code {
            // Quit (Esc always works)
            k if k == KeyCode::Esc || Some(k) == kb_quit => {
                self.should_quit = true;
            }
            KeyCode::Enter => {
                // Projects: open the detail view (US-PROJ-07)
                if self.ui.state == AppState::Projects {
                    self.open_project_detail().await;
                }
            }
            // Navigation - Number keys (US-TUI-11/12: 0 = Settings last)
            KeyCode::Char('1') => {
                self.ui.state = AppState::Dashboard;
                let _ = self.fetch_stats().await;
                self.fetch_monitor().await;
            }
            KeyCode::Char('2') => {
                self.ui.state = AppState::Knowledge;
                let _ = self.fetch_knowledge().await;
            }
            KeyCode::Char('3') => {
                self.ui.state = AppState::Projects;
                let _ = self.fetch_projects().await;
            }
            KeyCode::Char('4') => {
                self.ui.state = AppState::Workflows;
                let _ = self.fetch_workflows().await;
            }
            KeyCode::Char('5') => {
                self.ui.state = AppState::Secrets;
                let _ = self.fetch_secrets().await;
            }
            KeyCode::Char('6') => {
                self.ui.state = AppState::Configs;
                let _ = self.fetch_configs().await;
            }
            KeyCode::Char('7') => {
                self.ui.state = AppState::Plugins;
                self.fetch_plugins().await;
            }
            KeyCode::Char('0') => {
                self.ui.state = AppState::Settings;
            }
            // Tab - Cycle through the configured order (US-TUI-12)
            KeyCode::Tab => {
                let next = self.next_tab();
                self.ui.state = next;
            }
            // Action keybindings
            k if Some(k) == kb_create => {
                // Create new item (context-dependent)
                match self.ui.state {
                    AppState::Knowledge => {
                        // Pre-select the type matching the current filter
                        // (All → Command; the form can cycle with ←/→)
                        self.command_form = CommandFormState {
                            mode: Some(FormMode::Create),
                            entity_type: self.kb_filter.form_type_index(),
                            ..Default::default()
                        };
                    }
                    AppState::Projects => {
                        // Register an existing directory as a project (US-PROJ);
                        // use N for a brand-new workspace instead.
                        self.register_input = Some(String::new());
                    }
                    AppState::Workflows => {
                        self.workflow_form = WorkflowFormState {
                            mode: Some(FormMode::Create),
                            ..Default::default()
                        };
                    }
                    AppState::Secrets => {
                        self.secret_form = SecretFormState {
                            mode: Some(FormMode::Create),
                            ..Default::default()
                        };
                    }
                    _ => {}
                }
            }
            k if Some(k) == kb_edit => {
                // Edit selected item
                match self.ui.state {
                    AppState::Knowledge => {
                        let selected = self.commands_list.get_selected().cloned();
                        if let Some(entity) = selected {
                            let entity_type = ENTITY_TYPE_IDS
                                .iter()
                                .position(|&t| t == entity.type_id.as_str())
                                .unwrap_or(0);
                            let tags_text = repository::get_entity_tags(&*self.pool, &entity.id)
                                .await
                                .map(|tags| {
                                    tags.iter()
                                        .map(|t| t.name.clone())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default();
                            self.command_form = CommandFormState {
                                mode: Some(FormMode::Edit),
                                name: entity.name.clone(),
                                description: entity.description.clone().unwrap_or_default(),
                                content: entity.content.clone().unwrap_or_default(),
                                project_id: entity.project_id.clone(),
                                editing_id: Some(entity.id.clone()),
                                entity_type,
                                tags_text,
                                ..Default::default()
                            };
                        }
                    }
                    AppState::Projects => {
                        let selected = self.projects_list.get_selected().cloned();
                        if let Some(project) = selected {
                            self.project_form = ProjectFormState {
                                mode: Some(FormMode::Edit),
                                name: project.name.clone(),
                                description: project.description.clone().unwrap_or_default(),
                                editing_id: Some(project.id.clone()),
                                ..Default::default()
                            };
                        }
                    }
                    AppState::Workflows => {
                        let selected = self.workflows_list.get_selected().cloned();
                        if let Some(entity) = selected {
                            self.workflow_form = WorkflowFormState {
                                mode: Some(FormMode::Edit),
                                name: entity.name.clone(),
                                description: entity.description.clone().unwrap_or_default(),
                                content: entity.content.clone().unwrap_or_default(),
                                project_id: entity.project_id.clone(),
                                editing_id: Some(entity.id.clone()),
                                ..Default::default()
                            };
                        }
                    }
                    AppState::Secrets => {
                        let selected = self.secrets_list.get_selected().cloned();
                        if let Some(secret) = selected {
                            self.secret_form = SecretFormState {
                                mode: Some(FormMode::Edit),
                                name: secret.name.clone(),
                                group: secret.secret_group.clone().unwrap_or_default(),
                                username: secret.username.clone().unwrap_or_default(),
                                url: secret.url.clone().unwrap_or_default(),
                                email: secret.email.clone().unwrap_or_default(),
                                ssh_agent: secret.ssh_agent,
                                editing_id: Some(secret.id.clone()),
                                ..Default::default()
                            };
                        }
                    }
                    _ => {}
                }
            }
            k if Some(k) == kb_delete => {
                // Delete selected item — ask for confirmation first
                match self.ui.state {
                    AppState::Knowledge => {
                        if let Some(entity) = self.commands_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: entity.id.clone(),
                                label: entity.name.clone(),
                                kind: DeleteKind::Entity,
                                path: None,
                                delete_folder: false,
                            });
                        }
                    }
                    AppState::Projects => {
                        if let Some(project) = self.projects_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: project.id.clone(),
                                label: project.name.clone(),
                                kind: DeleteKind::Project,
                                path: project.path.clone(),
                                delete_folder: false,
                            });
                        }
                    }
                    AppState::Workflows => {
                        if let Some(entity) = self.workflows_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: entity.id.clone(),
                                label: entity.name.clone(),
                                kind: DeleteKind::Entity,
                                path: None,
                                delete_folder: false,
                            });
                        }
                    }
                    AppState::Secrets => {
                        if let Some(secret) = self.secrets_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: secret.id.clone(),
                                label: secret.name.clone(),
                                kind: DeleteKind::Secret,
                                path: None,
                                delete_folder: false,
                            });
                        }
                    }
                    AppState::Configs => {
                        if let Some(config) = self.configs_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: config.id.clone(),
                                label: config.name.clone(),
                                kind: DeleteKind::Config,
                                path: None,
                                delete_folder: false,
                            });
                        }
                    }
                    _ => {}
                }
            }
            k if Some(k) == kb_copy => {
                // Copy selected item content to clipboard
                self.copy_selected().await;
            }
            k if Some(k) == kb_run => {
                // Run/execute selected item (commands run via sh, workflows via the Lua engine)
                self.run_selected().await;
            }
            KeyCode::Char('v') => {
                // Visual workflow builder (compose workflows from saved commands)
                if self.ui.state == AppState::Workflows {
                    self.open_visual().await;
                }
            }
            KeyCode::Char('o') => {
                // Open selected command in the configured external editor (US-CMD-05)
                if matches!(self.ui.state, AppState::Knowledge) {
                    self.open_in_editor().await;
                }
            }
            KeyCode::Char('x') => {
                // Export knowledge base to a chosen path (US-CMD-01, sharing)
                if matches!(
                    self.ui.state,
                    AppState::Knowledge | AppState::Workflows | AppState::Secrets
                ) {
                    self.export_input = Some(default_bundle_path());
                }
            }
            KeyCode::Char('X') => {
                // Cancel a running background workflow (US-WF-09)
                if self.ui.state == AppState::Workflows {
                    self.cancel_running_workflow();
                }
            }
            KeyCode::Char('H') => {
                // SSH host manager (US-SSH-01..05)
                if self.ui.state == AppState::Secrets {
                    self.open_ssh_panel().await;
                }
            }
            KeyCode::Char('u') => {
                // Admin user management (Settings → `u`, US-SEC)
                if self.ui.state == AppState::Settings {
                    self.open_users_panel().await;
                }
            }
            KeyCode::Char('I') => {
                // Import knowledge base from a file path (default pre-filled)
                self.import_input = Some(default_bundle_path());
            }
            KeyCode::Char('k') => {
                // SSH/GPG key generation (US-SEC-01) — notify when the tools
                // are missing instead of failing later
                if self.ui.state == AppState::Secrets {
                    if keygen::which("ssh-keygen") || keygen::which("gpg") {
                        self.keygen = KeygenState {
                            open: true,
                            ..Default::default()
                        };
                    } else {
                        self.status_message = Some(
                            "\u{2717} Neither ssh-keygen nor gpg is installed \u{2014} install openssh/gnupg first"
                                .to_string(),
                        );
                    }
                }
            }
            KeyCode::Char('p') => {
                // Processes: launch a known viewer (btop/htop/top) (US-PROC)
                self.run_process_viewer().await;
            }
            KeyCode::Char('D') if crate::auth::dev_mode_enabled() => {
                // DEV: wipe entire database (users + entities + secrets)
                self.dev_wipe_database().await;
            }
            KeyCode::Char('A') if crate::auth::dev_mode_enabled() => {
                // DEV: delete all entities in the current tab
                self.delete_all_in_tab().await;
            }
            KeyCode::Char(c) if matches!(c, 'g' | 'd' | 'k' | 'n') => {
                // Quick launch: lazygit / lazydocker / k9s / lazynpm (US-PROC)
                if self.ui.state == AppState::Dashboard {
                    let tool = match c {
                        'g' => "lazygit",
                        'd' => "lazydocker",
                        'k' => "k9s",
                        _ => "lazynpm",
                    };
                    let projects_dir = dirs_home().join("projects");
                    if crate::keygen::which(tool) {
                        self.wants_terminal_cmd =
                            Some((tool.to_string(), projects_dir.to_string_lossy().to_string()));
                    } else {
                        self.status_message = Some(format!("{} is not installed", tool));
                    }
                }
            }
            KeyCode::Char('m') => {
                // Man page for the selected command (graceful when missing)
                if matches!(self.ui.state, AppState::Knowledge) {
                    self.show_man_page().await;
                }
            }
            KeyCode::Char('`') => {
                // Open a new terminal window (run loop spawns it)
                self.wants_terminal = true;
            }
            KeyCode::Char('s') => {
                // Workflows: schedule the selected workflow on a cron expression (US-WF-07)
                if self.ui.state == AppState::Workflows {
                    if self.workflows_list.get_selected().is_some() {
                        self.cron_input = Some(String::new());
                    } else {
                        self.status_message = Some("Nothing selected".to_string());
                    }
                }
            }
            KeyCode::Char('a') => {
                // Plugins: approve the selected plugin (US-PLG-06)
                if self.ui.state == AppState::Plugins {
                    if let Some(entry) = self.plugins_list.get_selected().cloned() {
                        let user_id = self.current_user_profile_id().await;
                        match self
                            .plugins
                            .approve_plugin(&entry.manifest.id, &user_id)
                            .await
                        {
                            Ok(()) => {
                                self.status_message =
                                    Some(format!("Approved {}", entry.manifest.id));
                                self.fetch_plugins().await;
                            }
                            Err(e) => self.status_message = Some(format!("{}", e)),
                        }
                    }
                }
                // Projects: plugin UI actions on the selected project (US-PLG-13)
                if self.ui.state == AppState::Projects {
                    self.open_project_actions().await;
                }
            }
            KeyCode::Char('e') => {
                // Plugins: toggle enabled state (US-PLG-10)
                if self.ui.state == AppState::Plugins {
                    if let Some(entry) = self.plugins_list.get_selected().cloned() {
                        match self
                            .plugins
                            .set_plugin_enabled(&entry.manifest.id, !entry.enabled)
                            .await
                        {
                            Ok(()) => {
                                let verb = if entry.enabled { "Disabled" } else { "Enabled" };
                                self.status_message =
                                    Some(format!("{} {}", verb, entry.manifest.id));
                                self.fetch_plugins().await;
                            }
                            Err(e) => self.status_message = Some(format!("{}", e)),
                        }
                    }
                }
            }
            KeyCode::Char('S') => {
                // Secrets: offer all ssh-agent keys to the running agent (US-SEC)
                if self.ui.state == AppState::Secrets {
                    self.load_ssh_agent_keys().await;
                }
            }
            KeyCode::Char('t') => {
                // Secrets: open an ssh terminal for the selected key/host (US-SEC)
                if self.ui.state == AppState::Secrets {
                    self.open_ssh_terminal().await;
                }
            }
            KeyCode::Char('R') => {
                // Run with elevated privileges (sudo/doas/su)
                if matches!(self.ui.state, AppState::Knowledge) {
                    self.run_privileged_command().await;
                }
            }
            KeyCode::Char('i') => {
                // Structured options of the selected command family (US-CMD-01)
                if matches!(self.ui.state, AppState::Knowledge) {
                    self.show_options_popup().await;
                }
            }
            KeyCode::Char('f') => {
                // System fetch panel (US-PROC/US-NF)
                if self.ui.state == AppState::Dashboard {
                    self.run_fetch().await;
                }
            }
            KeyCode::Char('E') => {
                // Projects: open a shell inside the project environment (US-ENV)
                if self.ui.state == AppState::Projects {
                    self.start_project_shell().await;
                }
            }
            KeyCode::Char('N') => {
                // Create a new project workspace (US-PROJ)
                if self.ui.state == AppState::Projects {
                    self.new_project_open = true;
                    self.new_project_name.clear();
                    self.new_project_error = None;
                    // Discover plugin templates (US-PLG-14); plain = default
                    self.templates = self.plugins.discover_templates();
                    self.new_project_template = None;
                    self.template_preview = None;
                }
            }
            KeyCode::Char('O') => {
                // Open project in the selected editor (US-PROJ)
                if self.ui.state == AppState::Projects {
                    self.open_project_in_editor().await;
                }
            }
            k if Some(k) == kb_search => {
                // Start search mode — only on list tabs; on other screens a
                // stray `/` used to activate an invisible search that ate
                // every following keypress (looked like "keys don't work")
                if matches!(
                    self.ui.state,
                    AppState::Knowledge
                        | AppState::Projects
                        | AppState::Workflows
                        | AppState::Secrets
                        | AppState::Configs
                ) {
                    self.search_state.active = true;
                    self.search_state.query.clear();
                }
            }
            k if Some(k) == kb_filter => {
                // Toggle filter mode
                match self.ui.state {
                    AppState::Knowledge => {
                        self.commands_list.filter = if self
                            .commands_list
                            .filter
                            .as_ref()
                            .map_or(true, |s| s.is_empty())
                        {
                            Some(String::new())
                        } else {
                            None
                        };
                    }
                    AppState::Projects => {
                        self.projects_list.filter = if self
                            .projects_list
                            .filter
                            .as_ref()
                            .map_or(true, |s| s.is_empty())
                        {
                            Some(String::new())
                        } else {
                            None
                        };
                    }
                    AppState::Workflows => {
                        self.workflows_list.filter = if self
                            .workflows_list
                            .filter
                            .as_ref()
                            .map_or(true, |s| s.is_empty())
                        {
                            Some(String::new())
                        } else {
                            None
                        };
                    }
                    _ => {}
                }
            }
            // Arrow keys for navigation
            KeyCode::Up => match self.ui.state {
                AppState::Knowledge => self.commands_list.select_previous(),
                AppState::Projects => self.projects_list.select_previous(),
                AppState::Workflows => self.workflows_list.select_previous(),
                AppState::Secrets => self.secrets_list.select_previous(),
                AppState::Configs => self.configs_list.select_previous(),
                AppState::Plugins => self.plugins_list.select_previous(),
                _ => {}
            },
            KeyCode::Down => match self.ui.state {
                AppState::Knowledge => self.commands_list.select_next(),
                AppState::Projects => self.projects_list.select_next(),
                AppState::Workflows => self.workflows_list.select_next(),
                AppState::Secrets => self.secrets_list.select_next(),
                AppState::Configs => self.configs_list.select_next(),
                AppState::Plugins => self.plugins_list.select_next(),
                _ => {}
            },
            KeyCode::PageUp => match self.ui.state {
                AppState::Knowledge => self.commands_list.previous_page(),
                AppState::Projects => self.projects_list.previous_page(),
                AppState::Workflows => self.workflows_list.previous_page(),
                AppState::Secrets => self.secrets_list.previous_page(),
                AppState::Configs => self.configs_list.previous_page(),
                AppState::Plugins => self.plugins_list.previous_page(),
                _ => {}
            },
            KeyCode::PageDown => match self.ui.state {
                AppState::Knowledge => self.commands_list.next_page(),
                AppState::Projects => self.projects_list.next_page(),
                AppState::Workflows => self.workflows_list.next_page(),
                AppState::Secrets => self.secrets_list.next_page(),
                AppState::Configs => self.configs_list.next_page(),
                AppState::Plugins => self.plugins_list.next_page(),
                _ => {}
            },
            KeyCode::Home => match self.ui.state {
                AppState::Knowledge => self.commands_list.selected = 0,
                AppState::Projects => self.projects_list.selected = 0,
                AppState::Workflows => self.workflows_list.selected = 0,
                AppState::Secrets => self.secrets_list.selected = 0,
                AppState::Configs => self.configs_list.selected = 0,
                AppState::Plugins => self.plugins_list.selected = 0,
                _ => {}
            },
            KeyCode::End => match self.ui.state {
                AppState::Knowledge => {
                    if !self.commands_list.items.is_empty() {
                        self.commands_list.selected = self.commands_list.items.len() - 1;
                    }
                }
                AppState::Projects => {
                    if !self.projects_list.items.is_empty() {
                        self.projects_list.selected = self.projects_list.items.len() - 1;
                    }
                }
                AppState::Workflows => {
                    if !self.workflows_list.items.is_empty() {
                        self.workflows_list.selected = self.workflows_list.items.len() - 1;
                    }
                }
                AppState::Secrets => {
                    if !self.secrets_list.items.is_empty() {
                        self.secrets_list.selected = self.secrets_list.items.len() - 1;
                    }
                }
                AppState::Configs => {
                    if !self.configs_list.items.is_empty() {
                        self.configs_list.selected = self.configs_list.items.len() - 1;
                    }
                }
                AppState::Plugins => {
                    if !self.plugins_list.items.is_empty() {
                        self.plugins_list.selected = self.plugins_list.items.len() - 1;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    async fn attempt_login(&mut self) {
        self.ui.login_state.start_auth();

        let auth_manager = AuthManager::new(self.pool.clone());

        match auth_manager
            .verify_password_by_username(
                &self.ui.login_state.username,
                &self.ui.login_state.password,
            )
            .await
        {
            Ok(true) => {
                self.ui.login_state.auth_success();
                // Track the logged-in user for per-user secrets (US-SEC)
                self.current_user_id = Some(self.ui.login_state.username.clone());

                // Offer stored SSH keys to ssh-agent right after login (US-SEC)
                self.load_ssh_agent_keys().await;

                // Fetch dashboard statistics + monitor after successful login
                if let Err(e) = self.fetch_stats().await {
                    eprintln!("Warning: Failed to fetch stats: {}", e);
                }

                self.ui.state = AppState::Dashboard;
                self.fetch_monitor().await;
            }
            Ok(false) => {
                self.ui
                    .login_state
                    .auth_failed("Invalid username or password".to_string());
            }
            Err(e) => {
                self.ui
                    .login_state
                    .auth_failed(format!("Authentication error: {}", e));
            }
        }
    }

    async fn attempt_signup(&mut self) {
        // Validate
        if let Err(e) = self.signup_state.validate() {
            self.signup_state.error_message = Some(e);
            return;
        }

        self.signup_state.is_creating = true;
        self.signup_state.error_message = None;

        let auth_manager = AuthManager::new(self.pool.clone());

        match auth_manager
            .create_user(&self.signup_state.username, &self.signup_state.password)
            .await
        {
            Ok(_user_id) => {
                // Auto-login after signup
                self.ui.login_state.username = self.signup_state.username.clone();
                self.signup_state.clear_sensitive_data();
                self.auth_mode = AuthMode::Login;
                self.ui.login_state.error_message =
                    Some("✓ Account created! Please login.".to_string());
            }
            Err(e) => {
                self.signup_state.is_creating = false;
                self.signup_state.error_message = Some(format!("Signup failed: {}", e));
                self.signup_state.clear_sensitive_data();
            }
        }
    }

    fn render_commands_list(&self, f: &mut Frame) {
        let title = format!(
            "📚 Knowledge · {} · f: filter · n: new",
            self.kb_filter.label()
        );
        let title = title.as_str();
        self.render_list(
            f,
            title,
            &self.commands_list.items,
            self.commands_list.selected,
            self.ui.theme.primary,
        );
    }

    /// Configs list (US-CFG-09..12).
    fn render_configs_list(&self, f: &mut Frame) {
        let title = format!(
            "Managed configs ({}) \u{2014} deploy mode cycles with m",
            self.configs_list.items.len()
        );
        self.render_list(
            f,
            &title,
            &self.configs_list.items,
            self.configs_list.selected,
            self.ui.theme.secondary,
        );
    }

    /// Generic single-line path input popup (shared by the config popups).
    /// Register-config form modal (Configs `n`, US-CFG-09): path + name +
    /// description + tags + deploy mode, with inline error line.
    fn render_config_register_form(&self, f: &mut Frame, form: &ConfigRegisterForm) {
        let area = self.centered_rect(64, 12, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" Register existing config ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.warning))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Path
                Constraint::Length(1), // Name
                Constraint::Length(1), // Description
                Constraint::Length(1), // Tags
                Constraint::Length(1), // Deploy to
                Constraint::Length(1), // Deploy mode
                Constraint::Length(1), // spacer
                Constraint::Length(1), // help / error
            ])
            .split(inner);

        let row = |f: &mut Frame, area: Rect, label: &str, value: &str, focused: bool| {
            let (marker, style) = if focused {
                (
                    "▶ ",
                    Style::default()
                        .fg(self.ui.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default().fg(self.ui.theme.border))
            };
            let cursor = if focused { "\u{2588}" } else { "" };
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{}{} ", marker, label), style),
                    Span::styled(
                        format!("{}{}", value, cursor),
                        Style::default().fg(self.ui.theme.fg),
                    ),
                ])),
                area,
            );
        };

        row(f, rows[0], "Path", &form.path, form.focus == 0);
        row(f, rows[1], "Name", &form.name, form.focus == 1);
        row(
            f,
            rows[2],
            "Description",
            &form.description,
            form.focus == 2,
        );
        row(f, rows[3], "Tags (comma-sep)", &form.tags, form.focus == 3);
        row(
            f,
            rows[4],
            "Deploy to (comma-sep)",
            &form.deploy_to,
            form.focus == 4,
        );
        row(
            f,
            rows[5],
            "Deploy mode (←→)",
            form.deploy_mode.label(),
            form.focus == 5,
        );
        f.render_widget(
            self.form_help_line(
                form.error.as_ref(),
                "~ = $HOME | Ctrl+O: browse · Ctrl+P: picker | Tab/arrows: field | Enter: register | Esc: cancel",
            ),
            rows[7],
        );
    }

    /// Deploy-target modal (Configs `t`, US-CFG-10): destination path for the
    /// selected config, with Ctrl+O browsing.
    fn render_config_target_modal(&self, f: &mut Frame, value: &str, config_name: &str) {
        let area = self.centered_rect(62, 7, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" Add deploy target ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // for <config>
                Constraint::Length(1), // path
                Constraint::Length(1), // spacer
                Constraint::Length(1), // help
            ])
            .split(inner);
        f.render_widget(
            Paragraph::new(Span::styled(
                format!("for config: {}", config_name),
                Style::default().fg(self.ui.theme.secondary),
            )),
            rows[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ", Style::default().fg(self.ui.theme.border)),
                Span::styled(
                    format!("{}\u{2588}", value),
                    Style::default().fg(self.ui.theme.fg),
                ),
            ])),
            rows[1],
        );
        f.render_widget(
            Paragraph::new(Span::styled(
                "~ = $HOME | Ctrl+O: browse · Ctrl+P: picker | Enter: add target | Esc: cancel",
                Style::default().fg(self.ui.theme.border),
            )),
            rows[3],
        );
    }

    /// In-TUI file browser modal (US-CFG-09): browse directories, pick an
    /// existing file/folder, or create new files (`a`) / folders (`A`).
    fn render_file_browser(&self, f: &mut Frame, fb: &FileBrowser) {
        let area = self.centered_rect(70, 22, f);
        f.render_widget(Clear, area);
        let title = if fb.pick_dir {
            " Select folder "
        } else {
            " Select file "
        };
        let block = Block::default()
            .title(title)
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // current directory
                Constraint::Min(1),    // listing
                Constraint::Length(1), // status / new-name entry
                Constraint::Length(1), // help
            ])
            .split(inner);

        // Current directory (with ~ shortening)
        let home = std::env::var("HOME").unwrap_or_default();
        let cwd_str = fb.cwd.to_string_lossy().to_string();
        let cwd_disp = if !home.is_empty() {
            if let Some(stripped) = cwd_str.strip_prefix(home.as_str()) {
                format!("~{}", stripped)
            } else {
                cwd_str.clone()
            }
        } else {
            cwd_str.clone()
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(" {} ", cwd_disp),
                Style::default()
                    .fg(self.ui.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            )),
            rows[0],
        );

        // Listing: window centered on the selection
        let avail = rows[1].height as usize;
        let total = fb.entries.len();
        let start = fb
            .selected
            .saturating_sub(avail / 2)
            .min(total.saturating_sub(avail));
        let end = (start + avail).min(total);
        let mut lines: Vec<Line> = Vec::new();
        for (i, row) in fb.entries[start..end].iter().enumerate() {
            let idx = start + i;
            let focused = idx == fb.selected;
            let (marker, style) = if focused {
                (
                    "▶ ",
                    Style::default()
                        .fg(self.ui.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                )
            } else if row.is_dir {
                ("  ", Style::default().fg(self.ui.theme.warning))
            } else {
                ("  ", Style::default().fg(self.ui.theme.fg))
            };
            let suffix = if row.is_dir { "/" } else { "" };
            let label = if row.is_parent {
                "..".to_string()
            } else {
                format!("{}{}", row.name, suffix)
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(label, style),
            ]));
        }
        f.render_widget(Paragraph::new(lines), rows[1]);

        // Status line: new-entry prompt takes precedence
        if let Some((is_dir, name)) = &fb.new_entry {
            let kind = if *is_dir { "folder" } else { "file" };
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!(" New {} name: ", kind),
                        Style::default()
                            .fg(self.ui.theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}\u{2588}", name),
                        Style::default().fg(self.ui.theme.fg),
                    ),
                ])),
                rows[2],
            );
        } else if let Some(status) = &fb.status {
            let color = if status.starts_with('✓') {
                self.ui.theme.success
            } else if status.starts_with('✗') {
                self.ui.theme.error
            } else {
                self.ui.theme.border
            };
            f.render_widget(
                Paragraph::new(Span::styled(status.clone(), Style::default().fg(color))),
                rows[2],
            );
        }

        f.render_widget(
            Paragraph::new(Span::styled(
                "↑↓ move · Enter open/pick file · Space pick · ←/h up · a new file · A new folder · . hidden · Esc cancel",
                Style::default().fg(self.ui.theme.border),
            )),
            rows[3],
        );
    }

    /// Register-directory popup (US-PROJ-01): type a path, Enter registers it.
    fn render_register_input(&self, f: &mut Frame, value: &str) {
        let area = self.centered_rect(70, 7, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .title(" Register existing directory as project ")
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);
        let shown = if value.is_empty() {
            format!("~/projects/{}", value)
        } else {
            value.to_string()
        };
        let cursor = format!("{}\u{2588}", shown);
        f.render_widget(Paragraph::new(cursor), rows[0]);
        let hint = "directory name becomes the project name | Enter: register | Esc: cancel";
        f.render_widget(
            Paragraph::new(Span::styled(
                hint,
                Style::default().fg(self.ui.theme.border),
            )),
            rows[1],
        );
    }

    /// Import path popup (US-CMD-01): type or edit a file path, Enter imports.
    fn render_import_input(&self, f: &mut Frame, value: &str) {
        let area = self.centered_rect(70, 7, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.success))
            .title(" Import knowledge base ")
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);
        f.render_widget(Paragraph::new(format!("{}\u{2588}", value)), rows[0]);
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    "~ = $HOME | Ctrl+O: picker | duplicates: {} | Enter: import \u{b7} Esc: cancel",
                    self.import_dup_mode.as_str()
                ),
                Style::default().fg(self.ui.theme.border),
            )),
            rows[1],
        );
    }

    /// Cron input popup (US-WF-07): type a cron expression and press Enter.
    fn render_cron_input(&self, f: &mut Frame, value: &str) {
        let area = self.centered_rect(56, 7, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.accent))
            .title(" Schedule workflow (cron) ")
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);
        f.render_widget(Paragraph::new(format!("{}\u{2588}", value)), rows[0]);
        f.render_widget(
            Paragraph::new(Span::styled(
                "e.g. 0 9 * * MON  |  @daily  |  Enter: save \u{b7} Esc: cancel",
                Style::default().fg(self.ui.theme.border),
            )),
            rows[1],
        );
    }

    /// Project detail popup (US-PROJ-07): description + all project entities.
    fn render_project_detail(&self, f: &mut Frame, detail: &ProjectDetailState) {
        let area = self.centered_rect(66, 70, f);
        f.render_widget(Clear, area);
        let rows: usize = detail.entities.len().min(8).max(4);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(rows as u16),
                Constraint::Length(1),
            ])
            .split(area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .title(format!(
                " \u{1f4c1} {} \u{2014} {} item(s) ",
                detail.project.name,
                detail.entities.len()
            ))
            .style(Style::default().bg(self.ui.theme.bg));
        f.render_widget(block, area);
        let desc = detail
            .project
            .description
            .clone()
            .unwrap_or_else(|| "(no description)".to_string());
        let mut desc_lines = vec![desc];
        if let Some(path) = &detail.project.path {
            desc_lines.push(format!("dir: {}", path));
        }
        f.render_widget(
            Paragraph::new(desc_lines.join("\n").to_string())
                .style(Style::default().fg(self.ui.theme.border)),
            chunks[0],
        );
        let items: Vec<Line> = detail
            .entities
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let icon = match e.type_id.as_str() {
                    "app" => "\u{1f680}",
                    "script" => "\u{1f4dc}",
                    "wf" => "\u{2699}",
                    _ => "\u{1f4bb}",
                };
                let selected = i == detail.selected;
                let style = if selected {
                    Style::default()
                        .fg(self.ui.theme.highlight)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.ui.theme.fg)
                };
                Line::from(Span::styled(
                    format!(
                        "{} {} {}{}",
                        if selected { ">" } else { " " },
                        icon,
                        e.name,
                        e.description
                            .as_deref()
                            .map(|d| format!(" \u{2014} {}", d))
                            .unwrap_or_default()
                    ),
                    style,
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(items), chunks[1]);
        f.render_widget(
            Paragraph::new(Span::styled(
                " Up/Down: select \u{b7} c: copy \u{b7} o: editor \u{b7} Esc: back",
                Style::default().fg(self.ui.theme.border),
            )),
            chunks[2],
        );
    }

    fn render_plugins_list(&self, f: &mut Frame) {
        self.render_list(
            f,
            "Plugins",
            &self.plugins_list.items,
            self.plugins_list.selected,
            self.ui.theme.accent,
        );
    }

    fn render_projects_list(&self, f: &mut Frame) {
        self.render_list(
            f,
            "📁 Projects",
            &self.projects_list.items,
            self.projects_list.selected,
            self.ui.theme.secondary,
        );
    }

    fn render_workflows_list(&self, f: &mut Frame) {
        self.render_list(
            f,
            "⚙️ Workflows",
            &self.workflows_list.items,
            self.workflows_list.selected,
            self.ui.theme.accent,
        );
    }

    fn render_secrets_list(&self, f: &mut Frame) {
        self.render_list(
            f,
            "🔐 Secrets",
            &self.secrets_list.items,
            self.secrets_list.selected,
            self.ui.theme.warning,
        );
    }

    fn render_list<T: std::fmt::Display>(
        &self,
        f: &mut Frame,
        title: &str,
        items: &[T],
        selected: usize,
        color: ratatui::style::Color,
    ) {
        use ratatui::widgets::{List, ListItem};

        let area = f.area();

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        // Header
        let header_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(color))
            .style(Style::default().bg(self.ui.theme.bg));

        let inner = header_block.inner(chunks[0]);
        f.render_widget(header_block, chunks[0]);

        let mut header_text = vec![
            Span::styled("🎛️ ", Style::default().fg(self.ui.theme.accent)),
            Span::styled(
                "TUI-OP-HUB",
                Style::default()
                    .fg(self.ui.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" › "),
            Span::styled(title, Style::default().fg(color)),
        ];

        // Live search bar while `/` is active (US-SRCH-01)
        if self.search_state.active {
            header_text.push(Span::raw("   "));
            header_text.push(Span::styled(
                "/",
                Style::default()
                    .fg(self.ui.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            header_text.push(Span::styled(
                format!("{}│", self.search_state.query),
                Style::default()
                    .fg(self.ui.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ));
            header_text.push(Span::styled(
                " (Enter: apply · Esc: cancel)",
                Style::default().fg(self.ui.theme.border),
            ));
        }

        let header = Paragraph::new(Line::from(header_text)).alignment(Alignment::Center);

        // List content
        let list_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(color))
            .title(format!(" {} ({} items) ", title, items.len()))
            .style(Style::default().bg(self.ui.theme.bg));

        if items.is_empty() {
            let empty_text = if self.ui.state == AppState::Configs {
                Paragraph::new(
                    "No configs registered yet. Press 'n' to register one (Ctrl+O browses).",
                )
            } else {
                Paragraph::new("No items found. Press 'n' to create a new one.")
            }
            .style(Style::default().fg(self.ui.theme.fg))
            .alignment(Alignment::Center)
            .block(list_block);
            f.render_widget(empty_text, chunks[1]);
        } else {
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    // Explicit cursor marker: color alone is hard to see on
                    // some themes/terminals (US-APP-01)
                    let marker = if i == selected { "\u{276f} " } else { "  " };
                    let content = format!("{}{}", marker, item);
                    let style = if i == selected {
                        Style::default()
                            .fg(color)
                            .add_modifier(Modifier::BOLD)
                            .bg(self.ui.theme.highlight)
                    } else {
                        Style::default().fg(self.ui.theme.fg)
                    };
                    ListItem::new(content).style(style)
                })
                .collect();

            let list = List::new(list_items)
                .block(list_block)
                .highlight_style(Style::default().fg(color).add_modifier(Modifier::BOLD));

            f.render_widget(list, chunks[1]);
        }
        // Search bar replaces the footer while search is active
        if self.search_state.active {
            f.render_widget(Clear, chunks[2]);
            let sb = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(self.ui.theme.warning))
                .title(" Search ")
                .style(Style::default().bg(self.ui.theme.bg));
            let si = sb.inner(chunks[2]);
            f.render_widget(sb, chunks[2]);
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "/ ",
                        Style::default()
                            .fg(self.ui.theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}\u{2588}", self.search_state.query),
                        Style::default()
                            .fg(self.ui.theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "   Enter: apply \u{b7} Esc: cancel",
                        Style::default().fg(self.ui.theme.border),
                    ),
                ])),
                si,
            );
        } else {
            self.render_keybind_footer(f, chunks[2], &self.keybind_hints());
        }
    }

    // ── Knowledge-base import/export (US-CMD-01, sharing) ────────────────

    /// Import a knowledge bundle from the path typed in the popup.
    /// Accepts full bundles, bare entity arrays and `entities`-only objects.
    async fn import_knowledge_from_path(
        &mut self,
        entered: &str,
        mode: crate::share::DuplicateMode,
    ) {
        let expanded = expand_tilde(entered);
        let path = std::path::PathBuf::from(expanded);
        if !path.exists() {
            self.status_message = Some(format!("\u{2717} No file at {}", path.display()));
            return;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.status_message = Some(format!("\u{2717} Read failed: {}", e));
                return;
            }
        };
        let bundle = match crate::share::bundle_from_json(&text) {
            Ok(b) => b,
            Err(e) => {
                self.status_message = Some(format!("\u{2717} Invalid bundle: {}", e));
                return;
            }
        };
        match crate::share::import_knowledge_with_mode(&*self.pool, &bundle, mode).await {
            Ok(report) => {
                self.status_message = Some(format!(
                    "✓ Imported {} ({} skipped, {} overwritten, {} renamed) from {}",
                    report.imported,
                    report.skipped,
                    report.overwritten,
                    report.renamed,
                    path.display()
                ));
                let _ = self.fetch_stats().await;
                self.refresh_current_tab().await;
            }
            Err(e) => self.status_message = Some(format!("✗ Import failed: {}", e)),
        }
    }

    /// Export the knowledge base to the chosen path (secrets excluded).
    async fn export_knowledge_to_path(&mut self, entered: &str) {
        let expanded = expand_tilde(entered);
        let path = std::path::PathBuf::from(expanded.clone());
        let user_id = self.current_user_profile_id().await;
        let user_id = self.current_user_profile_id().await;
        let bundle = match crate::share::export_knowledge(
            &*self.pool,
            Some(&user_id),
            crate::share::SecretMode::Exclude,
            None,
        )
        .await
        {
            Ok(b) => b,
            Err(e) => {
                self.status_message = Some(format!("\u{2717} Export failed: {}", e));
                return;
            }
        };
        let json = match crate::share::bundle_to_json(&bundle) {
            Ok(j) => j,
            Err(e) => {
                self.status_message = Some(format!("\u{2717} Serialize failed: {}", e));
                return;
            }
        };
        let path = std::path::PathBuf::from(expanded);
        match std::fs::write(&path, &json) {
            Ok(()) => {
                self.status_message = Some(format!(
                    "\u{2713} Exported {} entities to {}",
                    bundle.entities.len(),
                    path.display()
                ));
            }
            Err(e) => {
                self.status_message = Some(format!("\u{2717} Write failed: {}", e));
            }
        }
    }

    // ── Keybind helper (footer hints + `?` overlay; US-TUI-09) ─────────────

    /// Render a footer line of keybind hints. Keys are highlighted, labels
    /// use the normal text color.
    fn render_keybind_footer(&self, f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.border))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut spans: Vec<Span> = Vec::new();
        for (i, (key, label)) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                *key,
                Style::default()
                    .fg(self.ui.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(format!(" {}", label)));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
            inner,
        );
    }

    /// Context keybind hints for the current state (footer + `?` overlay).
    fn keybind_hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.ui.state {
            AppState::Dashboard => vec![
                ("1-9", "Tabs"),
                ("f", "Fetch"),
                ("p", "Processes"),
                ("g/d/k/n", "TUI tools"),
                ("`", "New term"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Configs => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("2/8", "Numpad nav"),
                ("4/6", "Numpad mode"),
                ("n", "Register"),
                ("t", "Add target"),
                ("m", "Mode"),
                ("l", "Deploy"),
                ("u", "Update"),
                ("g", "Git commit"),
                ("d", "Delete"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Knowledge => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("f", "Filter type"),
                ("n", "New"),
                ("e", "Edit"),
                ("d", "Delete"),
                ("r", "Run"),
                ("c", "Copy"),
                ("o", "Editor"),
                ("i", "Options"),
                ("m", "Man"),
                ("R", "Sudo"),
                ("/", "Find"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Projects => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("N", "New ws"),
                ("n", "Register"),
                ("e", "Edit"),
                ("a", "Actions"),
                ("d", "Delete"),
                ("E", "Shell"),
                ("O", "Editor"),
                ("Enter", "Open"),
                ("/", "Find"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Workflows => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("n", "New"),
                ("s", "Cron"),
                ("e", "Edit"),
                ("d", "Delete"),
                ("r", "Run"),
                ("X", "Cancel"),
                ("c", "Copy"),
                ("v", "Visual"),
                ("/", "Find"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Knowledge => vec![
                ("\u{2191}\u{2193}", "Select"),
                ("Enter", "Open"),
                ("n", "New (of type)"),
                ("3/4/5", "Quick jump"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Plugins => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("a", "Approve"),
                ("e", "Enable/Off"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Secrets => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("n", "New"),
                ("e", "Edit"),
                ("d", "Delete"),
                ("c", "Copy"),
                ("k", "Keygen"),
                ("H", "SSH hosts"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Settings => vec![
                ("\u{2191}\u{2193}", "Rows"),
                ("Enter", "Edit"),
                ("a", "Advanced"),
                ("Ctrl+S", "Save"),
                ("?", "Keybinds"),
                ("Esc", "Back"),
            ],
            _ => vec![("?", "Keybinds"), ("q", "Quit")],
        }
    }

    /// Delete confirmation popup: Enter confirms, Esc cancels. For projects
    /// with a workspace path, `f` toggles also deleting the folder on disk
    /// (default OFF — the DB row never touches the disk unless chosen).
    async fn handle_confirm_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('f') {
            if let Some(confirm) = self.confirm_delete.as_mut() {
                if confirm.kind == DeleteKind::Project && confirm.path.is_some() {
                    confirm.delete_folder = !confirm.delete_folder;
                }
            }
            return;
        }
        match key.code {
            KeyCode::Enter => {
                if let Some(confirm) = self.confirm_delete.take() {
                    let result = match confirm.kind {
                        DeleteKind::Entity => {
                            repository::delete_entity(&*self.pool, &confirm.id).await
                        }
                        DeleteKind::Project => {
                            repository::delete_project(&*self.pool, &confirm.id).await
                        }
                        DeleteKind::Secret => {
                            repository::delete_secret(&*self.pool, &confirm.id).await
                        }
                        DeleteKind::Config => self.config_manager().remove_entry(&confirm.id),
                    };
                    let mut note = String::new();
                    if result.is_ok() && confirm.kind == DeleteKind::Project {
                        // Optionally remove the workspace folder (US-PROJ)
                        if confirm.delete_folder {
                            if let Some(path) = &confirm.path {
                                let p = std::path::PathBuf::from(path);
                                let removed = tokio::task::spawn_blocking(move || {
                                    std::fs::remove_dir_all(&p)
                                })
                                .await;
                                match removed {
                                    Ok(Ok(())) => {
                                        note = format!(" \u{2014} folder '{}' removed", path);
                                    }
                                    Ok(Err(e)) => {
                                        note = format!(" \u{2014} folder delete failed: {}", e);
                                    }
                                    Err(e) => {
                                        note = format!(" \u{2014} folder task failed: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    match result {
                        Ok(()) => {
                            self.status_message =
                                Some(format!("✓ Deleted '{}'{}", confirm.label, note));
                        }
                        Err(e) => {
                            self.status_message = Some(format!("✗ Delete failed: {}", e));
                        }
                    }
                    let _ = self.refresh_current_tab().await;
                    let _ = self.fetch_stats().await;
                }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('n') => {
                self.confirm_delete = None;
            }
            _ => {}
        }
    }

    /// Search bar (`/`): live-filters the current tab's list.
    async fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search_state.active = false;
                self.search_state.clear();
                let _ = self.refresh_current_tab().await;
            }
            KeyCode::Enter => {
                self.search_state.active = false;
            }
            KeyCode::Backspace => {
                self.search_state.query.pop();
                self.apply_search_filter().await;
            }
            KeyCode::Char(c) => {
                self.search_state.query.push(c);
                self.apply_search_filter().await;
            }
            _ => {}
        }
    }

    /// Re-fetch the current tab and keep only items matching the search query.
    async fn apply_search_filter(&mut self) {
        let q = self.search_state.query.clone();
        match self.ui.state {
            AppState::Knowledge => {
                let _ = self.fetch_knowledge().await;
                self.commands_list.items = fuzzy_rank(&self.commands_list.items, &q, |e| {
                    (e.name.as_str(), e.description.as_deref().unwrap_or(""))
                });
                self.commands_list.selected = 0;
            }
            AppState::Projects => {
                let _ = self.fetch_projects().await;
                self.projects_list.items = fuzzy_rank(&self.projects_list.items, &q, |p| {
                    (p.name.as_str(), p.description.as_deref().unwrap_or(""))
                });
                self.projects_list.selected = 0;
            }
            AppState::Workflows => {
                let _ = self.fetch_workflows().await;
                self.workflows_list.items = fuzzy_rank(&self.workflows_list.items, &q, |e| {
                    (e.name.as_str(), e.description.as_deref().unwrap_or(""))
                });
                self.workflows_list.selected = 0;
            }
            AppState::Secrets => {
                let _ = self.fetch_secrets().await;
                self.secrets_list
                    .items
                    .retain(|s| s.name.to_lowercase().contains(&q.to_lowercase()));
                self.secrets_list.selected = 0;
            }
            AppState::Configs => {
                let _ = self.fetch_configs().await;
                self.configs_list.items = fuzzy_rank(&self.configs_list.items, &q, |c| {
                    (c.name.as_str(), c.source_path.as_str())
                });
                self.configs_list.selected = 0;
            }
            _ => {}
        }
    }

    /// Command/Script/App form (US-CMD-01..09): fields are
    /// 0=Name, 1=Type, 2=Description, 3=Content, 4=Tags.
    async fn handle_command_form_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_command_form().await;
            return;
        }
        let field = self.command_form.focused_field;
        match key.code {
            KeyCode::Esc => self.command_form.mode = None,
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter if field != 3 => {
                self.command_form.focused_field = cycle_field(field, COMMAND_FORM_FIELDS, true);
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.command_form.focused_field = cycle_field(field, COMMAND_FORM_FIELDS, false);
            }
            KeyCode::Left if field == 1 => {
                self.command_form.entity_type =
                    (self.command_form.entity_type + ENTITY_TYPE_IDS.len() - 1)
                        % ENTITY_TYPE_IDS.len();
            }
            KeyCode::Right if field == 1 => {
                self.command_form.entity_type =
                    (self.command_form.entity_type + 1) % ENTITY_TYPE_IDS.len();
            }
            KeyCode::Enter => {
                // Multi-line content field (3): Enter inserts a newline
                self.command_form.content.push('\n');
            }
            KeyCode::Backspace => match field {
                0 => {
                    self.command_form.name.pop();
                }
                2 => {
                    self.command_form.description.pop();
                }
                3 => {
                    self.command_form.content.pop();
                }
                4 => {
                    self.command_form.tags_text.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match field {
                0 => self.command_form.name.push(c),
                2 => self.command_form.description.push(c),
                3 => self.command_form.content.push(c),
                4 => self.command_form.tags_text.push(c),
                _ => {}
            },
            _ => {}
        }
    }

    /// Persist the command form (create or update; US-CMD-01/US-CMD-07).
    async fn save_command_form(&mut self) {
        let name = self.command_form.name.trim().to_string();
        if name.is_empty() {
            self.command_form.error_message = Some("Name is required".to_string());
            return;
        }
        let description = self.command_form.description.trim().to_string();
        let content = self.command_form.content.clone();
        let tags = parse_tags_text(&self.command_form.tags_text);
        let req = CreateEntity {
            name,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            content: if content.trim().is_empty() {
                None
            } else {
                Some(content)
            },
            type_id: ENTITY_TYPE_IDS[self.command_form.entity_type].to_string(),
            project_id: self.command_form.project_id.clone(),
            tags: if tags.is_empty() { None } else { Some(tags) },
            metadata_json: None,
        };
        let result = match self.command_form.editing_id.clone() {
            Some(id) => repository::update_entity(&*self.pool, &id, &req).await,
            None => repository::create_entity(&*self.pool, &req).await,
        };
        match result {
            Ok(_) => {
                self.command_form.mode = None;
                self.status_message = Some("✓ Saved".to_string());
                let _ = self.fetch_knowledge().await;
                let _ = self.fetch_stats().await;
            }
            Err(e) => self.command_form.error_message = Some(format!("Save failed: {}", e)),
        }
    }

    /// Project form (US-PROJ-01): fields are 0=Name, 1=Description.
    async fn handle_project_form_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_project_form().await;
            return;
        }
        let field = self.project_form.focused_field;
        match key.code {
            KeyCode::Esc => self.project_form.mode = None,
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                self.project_form.focused_field = cycle_field(field, PROJECT_FORM_FIELDS, true);
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.project_form.focused_field = cycle_field(field, PROJECT_FORM_FIELDS, false);
            }
            KeyCode::Backspace => match field {
                0 => {
                    self.project_form.name.pop();
                }
                1 => {
                    self.project_form.description.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match field {
                0 => self.project_form.name.push(c),
                1 => self.project_form.description.push(c),
                _ => {}
            },
            _ => {}
        }
    }

    /// Persist the project form (create or update; US-PROJ-01/US-PROJ-04).
    async fn save_project_form(&mut self) {
        let name = self.project_form.name.trim().to_string();
        if name.is_empty() {
            self.project_form.error_message = Some("Name is required".to_string());
            return;
        }
        let description = self.project_form.description.trim().to_string();
        let req = CreateProject {
            name,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
        };
        let result = match self.project_form.editing_id.clone() {
            Some(id) => repository::update_project(&*self.pool, &id, &req).await,
            None => repository::create_project(&*self.pool, &req).await,
        };
        match result {
            Ok(_) => {
                self.project_form.mode = None;
                self.status_message = Some("✓ Project saved".to_string());
                let _ = self.fetch_projects().await;
                let _ = self.fetch_stats().await;
            }
            Err(e) => self.project_form.error_message = Some(format!("Save failed: {}", e)),
        }
    }

    /// Workflow form (raw Lua script variant of US-WF-04): fields are
    /// 0=Name, 1=Description, 2=Script.
    async fn handle_workflow_form_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_workflow_form().await;
            return;
        }
        let field = self.workflow_form.focused_field;
        match key.code {
            KeyCode::Esc => self.workflow_form.mode = None,
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter if field != 2 => {
                self.workflow_form.focused_field = cycle_field(field, WORKFLOW_FORM_FIELDS, true);
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.workflow_form.focused_field = cycle_field(field, WORKFLOW_FORM_FIELDS, false);
            }
            KeyCode::Enter => {
                // Multi-line script field (2): Enter inserts a newline
                self.workflow_form.content.push('\n');
            }
            KeyCode::Backspace => match field {
                0 => {
                    self.workflow_form.name.pop();
                }
                1 => {
                    self.workflow_form.description.pop();
                }
                2 => {
                    self.workflow_form.content.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match field {
                0 => self.workflow_form.name.push(c),
                1 => self.workflow_form.description.push(c),
                2 => self.workflow_form.content.push(c),
                _ => {}
            },
            _ => {}
        }
    }

    /// Persist the workflow form (create or update; US-WF-01/US-WF-04).
    async fn save_workflow_form(&mut self) {
        let name = self.workflow_form.name.trim().to_string();
        if name.is_empty() {
            self.workflow_form.error_message = Some("Name is required".to_string());
            return;
        }
        let description = self.workflow_form.description.trim().to_string();
        let content = self.workflow_form.content.clone();
        let req = CreateEntity {
            name,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            content: if content.trim().is_empty() {
                None
            } else {
                Some(content)
            },
            type_id: "wf".to_string(),
            project_id: self.workflow_form.project_id.clone(),
            tags: None,
            metadata_json: None,
        };
        let result = match self.workflow_form.editing_id.clone() {
            Some(id) => repository::update_entity(&*self.pool, &id, &req).await,
            None => repository::create_entity(&*self.pool, &req).await,
        };
        match result {
            Ok(_) => {
                self.workflow_form.mode = None;
                self.status_message = Some("✓ Workflow saved".to_string());
                let _ = self.fetch_workflows().await;
                let _ = self.fetch_stats().await;
            }
            Err(e) => self.workflow_form.error_message = Some(format!("Save failed: {}", e)),
        }
    }

    /// Secret form (US-SEC-02/03): fields are 0=Name, 1=Value (masked).
    async fn handle_secret_form_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_secret_form().await;
            return;
        }
        let field = self.secret_form.focused_field;
        match key.code {
            KeyCode::Esc => self.secret_form.mode = None,
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                self.secret_form.focused_field = cycle_field(field, SECRET_FORM_FIELDS, true);
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.secret_form.focused_field = cycle_field(field, SECRET_FORM_FIELDS, false);
            }
            KeyCode::Char(' ') if field == 7 => {
                // Toggle ssh-agent flag
                self.secret_form.ssh_agent = !self.secret_form.ssh_agent;
            }
            KeyCode::Backspace => match field {
                0 => {
                    self.secret_form.name.pop();
                }
                1 => {
                    self.secret_form.value.pop();
                }
                2 => {
                    self.secret_form.group.pop();
                }
                3 => {
                    self.secret_form.username.pop();
                }
                4 => {
                    self.secret_form.url.pop();
                }
                5 => {
                    self.secret_form.email.pop();
                }
                6 => {
                    self.secret_form.passphrase.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match field {
                0 => self.secret_form.name.push(c),
                1 => self.secret_form.value.push(c),
                2 => self.secret_form.group.push(c),
                3 => self.secret_form.username.push(c),
                4 => self.secret_form.url.push(c),
                5 => self.secret_form.email.push(c),
                6 => self.secret_form.passphrase.push(c),
                _ => {}
            },
            _ => {}
        }
    }

    /// Persist the secret form: encrypt before insert, never store plaintext.
    async fn save_secret_form(&mut self) {
        let name = self.secret_form.name.trim().to_string();
        if name.is_empty() {
            self.secret_form.error_message = Some("Name is required".to_string());
            return;
        }
        if self.secret_form.value.is_empty() {
            self.secret_form.error_message = Some("Value is required".to_string());
            return;
        }
        let user_id = self.current_user_profile_id().await;
        // Optional passphrase layer: value -> passphrase wrap -> user key
        let mut passphrase_used = false;
        let plaintext = if self.secret_form.passphrase.is_empty() {
            self.secret_form.value.clone()
        } else {
            passphrase_used = true;
            match secrets::wrap_with_passphrase(
                &self.secret_form.value,
                &self.secret_form.passphrase,
            ) {
                Ok(wrapped) => {
                    self.secret_form.value.clear();
                    self.secret_form.passphrase.clear();
                    wrapped
                }
                Err(e) => {
                    self.secret_form.error_message = Some(format!("Passphrase wrap failed: {}", e));
                    return;
                }
            }
        };
        let value_enc = match secrets::encrypt_for_user(&*self.pool, &user_id, &plaintext).await {
            Ok(enc) => enc,
            Err(e) => {
                self.secret_form.error_message = Some(format!("Encryption failed: {}", e));
                return;
            }
        };
        let meta = repository::SecretMeta {
            secret_group: some_if_not_empty(&self.secret_form.group),
            username: some_if_not_empty(&self.secret_form.username),
            url: some_if_not_empty(&self.secret_form.url),
            email: some_if_not_empty(&self.secret_form.email),
            passphrase_protected: passphrase_used,
            ssh_agent: self.secret_form.ssh_agent,
        };
        let result = match self.secret_form.editing_id.clone() {
            Some(id) => repository::update_secret_meta(&*self.pool, &id, &value_enc, &meta).await,
            None => {
                repository::create_secret_meta(
                    &*self.pool,
                    &user_id,
                    &name,
                    &value_enc,
                    "password",
                    false,
                    &meta,
                )
                .await
            }
        };
        match result {
            Ok(_) => {
                self.secret_form.mode = None;
                self.secret_form.value.clear();
                self.status_message = Some("✓ Secret saved (encrypted)".to_string());
                let _ = self.fetch_secrets().await;
                let _ = self.fetch_stats().await;
            }
            Err(e) => self.secret_form.error_message = Some(format!("Save failed: {}", e)),
        }
    }

    /// Open the visual workflow builder. If a workflow is selected it is loaded
    /// for editing (steps parsed from its JSON definition when possible).
    async fn open_visual(&mut self) {
        let selected = self.workflows_list.get_selected().cloned();
        let mut visual = VisualWorkflowState::default();
        if let Some(entity) = selected {
            visual.editing_id = Some(entity.id.clone());
            visual.name = entity.name.clone();
            visual.description = entity.description.clone().unwrap_or_default();
            if let Some(content) = &entity.content {
                if let Ok(def) = serde_json::from_str::<WorkflowDefinition>(content) {
                    visual.steps = def
                        .steps
                        .iter()
                        .map(|s| VisualStep::command("", &s.name, &s.script))
                        .collect();
                }
            }
        }
        self.visual_form = Some(visual);
    }

    /// Visual workflow builder input (US-WF-03, US-WF-10).
    async fn handle_visual_key(&mut self, key: KeyEvent) {
        // Logic-node editor popup is topmost when open (US-FUT-07)
        let has_node_editor = self
            .visual_form
            .as_ref()
            .map_or(false, |v| v.node_editor.is_some());
        if has_node_editor {
            self.handle_node_editor_key(key);
            return;
        }
        // Command picker overlay is on top when open
        let has_picker = self
            .visual_form
            .as_ref()
            .map_or(false, |v| v.picker.is_some());
        if has_picker {
            match key.code {
                KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => {
                    if let Some(v) = self.visual_form.as_mut() {
                        v.picker = None;
                    }
                }
                KeyCode::Down | KeyCode::Tab => {
                    if let Some(v) = self.visual_form.as_mut() {
                        if let Some(p) = v.picker.as_mut() {
                            p.select_next();
                        }
                    }
                }
                KeyCode::Up | KeyCode::BackTab => {
                    if let Some(v) = self.visual_form.as_mut() {
                        if let Some(p) = v.picker.as_mut() {
                            p.select_previous();
                        }
                    }
                }
                KeyCode::Enter => {
                    let picked = self
                        .visual_form
                        .as_ref()
                        .and_then(|v| v.picker.as_ref())
                        .and_then(|p| p.get_selected().cloned());
                    if let (Some(v), Some(entity)) = (self.visual_form.as_mut(), picked) {
                        v.steps.push(VisualStep::command(
                            &entity.id.clone(),
                            &entity.name.clone(),
                            &entity.content.clone().unwrap_or_default(),
                        ));
                        v.selected_step = v.steps.len() - 1;
                        v.picker = None;
                    }
                }
                _ => {}
            }
            return;
        }
        self.handle_visual_builder_key(key).await;
    }

    /// Keys for the logic-node editor popup (US-FUT-07): Tab/arrows cycle
    /// fields, ←/→ cycles the Kind (field 0) or compare operator, Enter
    /// applies, Esc discards.
    fn handle_node_editor_key(&mut self, key: KeyEvent) {
        let Some(editor) = self
            .visual_form
            .as_mut()
            .and_then(|v| v.node_editor.as_mut())
        else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                if let Some(v) = self.visual_form.as_mut() {
                    v.node_editor = None;
                }
            }
            KeyCode::Tab | KeyCode::Down => {
                let count = editor.field_count();
                editor.focused = (editor.focused + 1) % count;
            }
            KeyCode::BackTab | KeyCode::Up => {
                let count = editor.field_count();
                editor.focused = (editor.focused + count.saturating_sub(1)) % count;
            }
            KeyCode::Left if editor.focused == 0 => {
                // Kind selector: cycle backwards through node kinds
                let all = VisualNodeKind::all();
                let i = all.iter().position(|k| *k == editor.kind).unwrap_or(0);
                editor.kind = all[(i + all.len() - 1) % all.len()];
            }
            KeyCode::Right if editor.focused == 0 => {
                editor.kind = editor.kind.next();
            }
            KeyCode::Left | KeyCode::Right
                if editor.kind == VisualNodeKind::Compare && editor.focused == 1 =>
            {
                if key.code == KeyCode::Right {
                    editor.op = editor.op.next();
                } else {
                    let all = CompareOp::all();
                    let i = all.iter().position(|o| *o == editor.op).unwrap_or(0);
                    editor.op = all[(i + all.len() - 1) % all.len()];
                }
            }
            KeyCode::Enter => {
                // Apply the editor onto the step; close on success
                let editor = self
                    .visual_form
                    .as_ref()
                    .and_then(|v| v.node_editor.clone());
                let result = if let Some(editor) = editor {
                    match self
                        .visual_form
                        .as_mut()
                        .unwrap()
                        .steps
                        .get_mut(editor.step_idx)
                    {
                        Some(step) => editor.apply_to_step(step),
                        None => Ok(()),
                    }
                } else {
                    Ok(())
                };
                if let Some(v) = self.visual_form.as_mut() {
                    match result {
                        Ok(()) => v.node_editor = None,
                        Err(msg) => {
                            if let Some(editor) = v.node_editor.as_mut() {
                                editor.error = Some(msg);
                            }
                        }
                    }
                }
            }
            KeyCode::Backspace => editor.pop_char(editor.focused),
            KeyCode::Char(c) => editor.push_char(editor.focused, c),
            _ => {}
        }
    }

    /// Render the logic-node editor popup (US-FUT-07).
    fn render_node_editor(&self, f: &mut Frame) {
        let Some(v) = self.visual_form.as_ref() else {
            return;
        };
        let Some(editor) = v.node_editor.as_ref() else {
            return;
        };
        let area = self.centered_rect(70, 16, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(format!(
                " \u{2699} Node '{}' (step {}) ",
                v.steps
                    .get(editor.step_idx)
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
                editor.step_idx + 1
            ))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.accent))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        // Row 0 = Kind selector (←/→ cycles; the ACTIVE kind is highlighted
        // inside the list so you can always see which one is selected), then
        // the kind-specific fields.
        let mut rows: Vec<(String, String, bool)> = Vec::new();
        rows.push((
            "Kind".to_string(),
            format!(
                "\u{25c4} {} \u{25ba}",
                VisualNodeKind::all()
                    .iter()
                    .map(|k| k.label())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            editor.focused == 0,
        ));
        for field in 0..editor.field_count() {
            rows.push((
                editor.field_label(field).to_string(),
                editor.field_value(field),
                editor.focused == field + 1,
            ));
        }

        let constraints: Vec<Constraint> = rows
            .iter()
            .map(|_| Constraint::Length(1))
            .chain(std::iter::once(Constraint::Length(1)))
            .chain(std::iter::once(Constraint::Length(1)))
            .collect();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        for (i, (label, value, focused)) in rows.iter().enumerate() {
            let (marker, marker_style, value_style, cursor) = if *focused {
                (
                    "\u{276f} ",
                    Style::default()
                        .fg(self.ui.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(self.ui.theme.fg)
                        .bg(self.ui.theme.highlight)
                        .add_modifier(Modifier::BOLD),
                    "\u{2588}",
                )
            } else {
                (
                    "  ",
                    Style::default().fg(self.ui.theme.border),
                    Style::default().fg(self.ui.theme.fg),
                    "",
                )
            };
            if i == 0 {
                // Kind row: draw every kind label, the ACTIVE one with a
                // highlighted background so the selection is unmistakable.
                let mut spans = vec![
                    Span::styled(format!("{}{}: ", marker, label), marker_style),
                    Span::styled("\u{25c4} ", Style::default().fg(self.ui.theme.border)),
                ];
                for k in VisualNodeKind::all() {
                    if *k == editor.kind {
                        spans.push(Span::styled(
                            format!("[{}]", k.label()),
                            Style::default()
                                .fg(self.ui.theme.fg)
                                .bg(self.ui.theme.highlight)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        spans.push(Span::styled(
                            format!(" {} ", k.label()),
                            Style::default().fg(self.ui.theme.border),
                        ));
                    }
                }
                spans.push(Span::styled(
                    " \u{25ba} \u{2190}/\u{2192}",
                    Style::default().fg(self.ui.theme.border),
                ));
                f.render_widget(Paragraph::new(Line::from(spans)), chunks[i]);
            } else {
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!("{}{}: ", marker, label), marker_style),
                        Span::styled(format!("{}{}", value, cursor), value_style),
                    ])),
                    chunks[i],
                );
            }
        }

        // Help / error line
        let help = match &editor.error {
            Some(err) => Span::styled(
                err.clone(),
                Style::default()
                    .fg(self.ui.theme.error)
                    .add_modifier(Modifier::BOLD),
            ),
            None => Span::styled(
                "Tab/arrows: field \u{b7} \u{2190}/\u{2192}: cycle kind/op \u{b7} Enter: apply \u{b7} Esc: discard \u{b7} inputs read results[\"step\"]",
                Style::default().fg(self.ui.theme.border),
            ),
        };
        f.render_widget(
            Paragraph::new(Line::from(help)).alignment(Alignment::Center),
            chunks[rows.len()],
        );
    }

    /// Builder keys (picker closed): edit name/description, manage steps.
    async fn handle_visual_builder_key(&mut self, key: KeyEvent) {
        let focused = self.visual_form.as_ref().map(|v| v.focused_field);
        // Ctrl+S saves from anywhere in the builder
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_visual_workflow().await;
            return;
        }
        match key.code {
            KeyCode::Esc => self.visual_form = None,
            KeyCode::Tab | KeyCode::Down => {
                let v = self.visual_form.as_mut().unwrap();
                if v.focused_field == VisualField::Steps {
                    v.select_step_next();
                } else {
                    v.next_field();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                let v = self.visual_form.as_mut().unwrap();
                if v.focused_field == VisualField::Steps {
                    v.select_step_previous();
                } else {
                    v.previous_field();
                }
            }
            KeyCode::Left if focused == Some(VisualField::Steps) => {
                self.visual_form.as_mut().unwrap().move_step_up();
            }
            KeyCode::Right if focused == Some(VisualField::Steps) => {
                self.visual_form.as_mut().unwrap().move_step_down();
            }
            KeyCode::Char('d') if focused == Some(VisualField::Steps) => {
                self.visual_form.as_mut().unwrap().remove_selected_step();
            }
            KeyCode::Char('l') if focused == Some(VisualField::Steps) => {
                // Add a logic node (US-FUT-07) and open its editor
                let v = self.visual_form.as_mut().unwrap();
                let n = v.steps.len() + 1;
                let name = format!("gate{}", n);
                v.steps.push(VisualStep::logic(VisualNodeKind::And, &name));
                v.selected_step = v.steps.len() - 1;
                let step = v.steps.last().unwrap();
                v.node_editor = Some(LogicNodeEditor::load(v.selected_step, step));
            }
            KeyCode::Char('o') if focused == Some(VisualField::Steps) => {
                // Edit the selected step's node config / run_when gate
                let v = self.visual_form.as_mut().unwrap();
                if let Some(step) = v.steps.get(v.selected_step) {
                    let idx = v.selected_step;
                    v.node_editor = Some(LogicNodeEditor::load(idx, step));
                }
            }
            KeyCode::Enter | KeyCode::Char('a') if focused == Some(VisualField::Steps) => {
                // Open the saved-command picker (commands + scripts)
                let mut items =
                    match repository::list_entities(&*self.pool, Some("cmd"), None).await {
                        Ok(items) => items,
                        Err(e) => {
                            if let Some(v) = self.visual_form.as_mut() {
                                v.error_message = Some(format!("Failed to load commands: {}", e));
                            }
                            return;
                        }
                    };
                if let Ok(scripts) =
                    repository::list_entities(&*self.pool, Some("script"), None).await
                {
                    items.extend(scripts);
                }
                if let Some(v) = self.visual_form.as_mut() {
                    v.picker = Some(CommandPickerState { items, selected: 0 });
                }
            }
            KeyCode::Enter => {
                self.visual_form.as_mut().unwrap().next_field();
            }
            KeyCode::Backspace => match focused {
                Some(VisualField::Name) => {
                    self.visual_form.as_mut().unwrap().name.pop();
                }
                Some(VisualField::Description) => {
                    self.visual_form.as_mut().unwrap().description.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match focused {
                Some(VisualField::Name) => self.visual_form.as_mut().unwrap().name.push(c),
                Some(VisualField::Description) => {
                    self.visual_form.as_mut().unwrap().description.push(c)
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Persist the visual builder as a JSON WorkflowDefinition entity (type `wf`).
    /// The Lua engine executes the steps in order; each step's script is the
    /// saved command/script content.
    async fn save_visual_workflow(&mut self) {
        let (name, description, steps, editing_id) = {
            let v = self.visual_form.as_ref().unwrap();
            (
                v.name.trim().to_string(),
                v.description.trim().to_string(),
                v.steps.clone(),
                v.editing_id.clone(),
            )
        };
        let definition = match build_workflow_definition(&name, &description, &steps) {
            Ok(def) => def,
            Err(msg) => {
                if let Some(v) = self.visual_form.as_mut() {
                    v.error_message = Some(msg);
                }
                return;
            }
        };
        let content = match serde_json::to_string_pretty(&definition) {
            Ok(c) => c,
            Err(e) => {
                if let Some(v) = self.visual_form.as_mut() {
                    v.error_message = Some(format!("Serialize failed: {}", e));
                }
                return;
            }
        };
        let req = CreateEntity {
            name: name.clone(),
            description: definition.description.clone(),
            content: Some(content),
            type_id: "wf".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        };
        let result = match editing_id {
            Some(id) => repository::update_entity(&*self.pool, &id, &req).await,
            None => repository::create_entity(&*self.pool, &req).await,
        };
        match result {
            Ok(_) => {
                self.visual_form = None;
                self.status_message = Some("✓ Workflow saved (visual)".to_string());
                let _ = self.fetch_workflows().await;
                let _ = self.fetch_stats().await;
            }
            Err(e) => {
                if let Some(v) = self.visual_form.as_mut() {
                    v.error_message = Some(format!("Save failed: {}", e));
                }
            }
        }
    }

    /// Copy the selected item's content to the clipboard (US-CMD-08).
    /// Secrets are decrypted first, then copied.
    async fn copy_selected(&mut self) {
        let content: Option<String> = match self.ui.state {
            AppState::Knowledge => self
                .commands_list
                .get_selected()
                .and_then(|e| e.content.clone()),
            AppState::Workflows => self
                .workflows_list
                .get_selected()
                .and_then(|e| e.content.clone()),
            AppState::Projects => self
                .projects_list
                .get_selected()
                .and_then(|p| p.description.clone()),
            AppState::Secrets => {
                let selected = self.secrets_list.get_selected().cloned();
                let user_id = self.current_user_profile_id().await;
                match selected {
                    Some(secret) => {
                        match secrets::decrypt_for_user(&*self.pool, &user_id, &secret.value_enc)
                            .await
                        {
                            Ok(value) => Some(value),
                            Err(e) => {
                                self.status_message = Some(format!("✗ Decrypt failed: {}", e));
                                None
                            }
                        }
                    }
                    None => None,
                }
            }
            _ => None,
        };
        match content {
            Some(text) if !text.is_empty() => match arboard::Clipboard::new() {
                Ok(mut clipboard) => match clipboard.set_text(text) {
                    Ok(()) => self.status_message = Some("✓ Copied to clipboard".to_string()),
                    Err(e) => self.status_message = Some(format!("✗ Clipboard failed: {}", e)),
                },
                Err(e) => self.status_message = Some(format!("✗ Clipboard unavailable: {}", e)),
            },
            Some(_) => self.status_message = Some("Nothing to copy".to_string()),
            None => self.status_message = Some("Nothing selected".to_string()),
        }
    }

    /// Run the selected item (US-CMD-09): commands via `sh`, scripts via their
    /// language interpreter (shebang/extension aware, file-backed supported),
    /// apps launched detached; workflows via the Lua engine with access to the
    /// user's secrets. Shows the result in a popup.
    async fn run_selected(&mut self) {
        match self.ui.state {
            AppState::Knowledge => {
                let selected = self.commands_list.get_selected().cloned();
                let Some(entity) = selected else {
                    self.status_message = Some("Nothing selected".to_string());
                    return;
                };
                match workflow::build_run_plan(&entity) {
                    Some(workflow::RunPlan::App(command)) => {
                        // Apps run detached: no captured output, fire and forget
                        match tokio::process::Command::new("sh")
                            .arg("-c")
                            .arg(&command)
                            .spawn()
                        {
                            Ok(_) => {
                                self.status_message =
                                    Some(format!("\u{2713} Launched app '{}'", entity.name))
                            }
                            Err(e) => {
                                self.status_message = Some(format!("\u{2717} Launch failed: {}", e))
                            }
                        }
                    }
                    Some(workflow::RunPlan::Interpreter { program, args }) => {
                        let mut cmd = tokio::process::Command::new(&program);
                        for arg in &args {
                            cmd.arg(arg);
                        }
                        self.run_invoke(cmd, &entity.name, &program).await;
                    }
                    Some(workflow::RunPlan::Shell(command)) => {
                        let mut cmd = tokio::process::Command::new("sh");
                        cmd.arg("-c").arg(&command);
                        self.run_invoke(cmd, &entity.name, &command).await;
                    }
                    None => {
                        self.status_message =
                            Some("Selected item has no content to run".to_string())
                    }
                }
            }
            AppState::Workflows => {
                if self.running_workflow.is_some() {
                    self.status_message =
                        Some("A workflow is already running — X to cancel".to_string());
                    return;
                }
                let selected = self.workflows_list.get_selected().cloned();
                let Some(entity) = selected else {
                    self.status_message = Some("Nothing selected".to_string());
                    return;
                };
                let user_id = self.current_user_profile_id().await;
                // Run in the background so the TUI stays responsive and the
                // run can be cancelled with `X` (US-WF-09).
                match workflow::spawn_workflow_run(
                    self.pool.clone(),
                    &entity.id,
                    None,
                    Some(user_id),
                )
                .await
                {
                    Ok((run_id, task)) => {
                        self.running_workflow = Some(RunningWorkflow {
                            run_id,
                            name: entity.name.clone(),
                            task,
                        });
                        self.status_message =
                            Some(format!("▶ Running '{}' — X to cancel", entity.name));
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Cannot start workflow: {}", e));
                    }
                }
            }
            _ => {
                self.status_message =
                    Some("Run is only available for commands and workflows".to_string());
            }
        }
    }

    /// Poll the background workflow task; when finished, record the result
    /// (history + popup). Called from `handle_key` and the run loop (US-WF-09).
    async fn poll_workflow_task(&mut self) {
        let finished = self
            .running_workflow
            .as_ref()
            .map(|rw| rw.task.is_finished())
            .unwrap_or(false);
        if !finished {
            return;
        }
        if let Some(rw) = self.running_workflow.take() {
            match rw.task.await {
                Ok(Ok(result)) => self.record_workflow_result(&rw.name, result).await,
                Ok(Err(e)) => {
                    self.status_message = Some(format!("Workflow '{}' failed: {}", rw.name, e))
                }
                Err(e) => self.status_message = Some(format!("Workflow task panicked: {}", e)),
            }
        }
    }

    /// Persist a finished workflow run and show the result popup (US-WF-08/09).
    async fn record_workflow_result(
        &mut self,
        name: &str,
        result: crate::workflow::WorkflowResult,
    ) {
        let run = crate::models::WorkflowRun {
            run_id: result.run_id.clone(),
            workflow_id: String::new(),
            success: result.success,
            output: Some(result.output.clone()),
            error: result.error.clone(),
            duration_ms: Some(result.duration_ms as i64),
            steps_completed: Some(result.steps_completed as i32),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let _ = repository::insert_workflow_run(&*self.pool, &run).await;
        let cancelled = result
            .error
            .as_deref()
            .map(|e| e.contains("cancelled"))
            .unwrap_or(false);
        self.status_message = Some(if cancelled {
            format!("✗ '{}' cancelled", name)
        } else if result.success {
            format!("✓ '{}' finished", name)
        } else {
            format!("✗ '{}' failed", name)
        });
        self.run_result = Some(RunResult {
            title: format!("Workflow: {}", name),
            success: result.success,
            text: format!(
                "run id: {}\nsteps completed: {}\nduration: {}ms\n\n--- output ---\n{}\n--- error ---\n{}",
                result.run_id,
                result.steps_completed,
                result.duration_ms,
                result.output,
                result.error.unwrap_or_else(|| "(none)".to_string())
            ),
        });
    }

    /// Cancel the running workflow, if any (US-WF-09).
    fn cancel_running_workflow(&mut self) {
        match &self.running_workflow {
            Some(rw) => {
                let ok = workflow::cancel_run(&rw.run_id);
                self.status_message = Some(if ok {
                    format!("Cancelling '{}'…", rw.name)
                } else {
                    "Run is no longer active".to_string()
                });
            }
            None => self.status_message = Some("No workflow is running".to_string()),
        }
    }

    /// Open the admin user-management panel (Settings → `u`, US-SEC).
    async fn open_users_panel(&mut self) {
        let admin_id = self.current_user_profile_id().await;
        match repository::is_admin(&*self.pool, &admin_id).await {
            Ok(true) => match repository::list_user_profiles(&*self.pool).await {
                Ok(users) => {
                    self.users_panel = Some(UsersPanel {
                        users,
                        selected: 0,
                        confirm_delete: false,
                        message: None,
                    });
                }
                Err(e) => self.status_message = Some(format!("Cannot list users: {}", e)),
            },
            Ok(false) => self.status_message = Some("Only admins can manage users".to_string()),
            Err(e) => self.status_message = Some(format!("Cannot check admin status: {}", e)),
        }
    }

    /// Handle keys while the users panel is open (US-SEC).
    async fn handle_users_panel_key(&mut self, key: KeyEvent) {
        let Some(panel) = &mut self.users_panel else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.users_panel = None;
            }
            KeyCode::Up => {
                panel.confirm_delete = false;
                if panel.selected > 0 {
                    panel.selected -= 1;
                }
            }
            KeyCode::Down => {
                panel.confirm_delete = false;
                if panel.selected + 1 < panel.users.len() {
                    panel.selected += 1;
                }
            }
            KeyCode::Char('d') => {
                panel.confirm_delete = !panel.confirm_delete;
                panel.message = panel
                    .confirm_delete
                    .then(|| "Enter to confirm delete".into());
            }
            KeyCode::Enter => {
                let (user, confirm) = {
                    let Some(panel) = &self.users_panel else {
                        return;
                    };
                    (
                        panel.users.get(panel.selected).cloned(),
                        panel.confirm_delete,
                    )
                };
                let Some(user) = user else { return };
                if confirm {
                    match repository::delete_user(&*self.pool, &user.id).await {
                        Ok(()) => {
                            self.open_users_panel().await;
                            if let Some(p) = &mut self.users_panel {
                                p.message = Some(format!("Deleted {}", user.username));
                            }
                        }
                        Err(e) => {
                            if let Some(p) = &mut self.users_panel {
                                p.message = Some(format!("Delete failed: {}", e));
                                p.confirm_delete = false;
                            }
                        }
                    }
                } else {
                    // Reset password to a known value; the old user key stays
                    // undecryptable (documented US-SEC behaviour).
                    let admin_id = self.current_user_profile_id().await;
                    match crate::auth::admin_reset_password(
                        &*self.pool,
                        &admin_id,
                        &user.username,
                        "reset-me",
                    )
                    .await
                    {
                        Ok(()) => {
                            if let Some(p) = &mut self.users_panel {
                                p.message = Some(format!(
                                    "Password of '{}' reset to 'reset-me'",
                                    user.username
                                ));
                            }
                        }
                        Err(e) => {
                            if let Some(p) = &mut self.users_panel {
                                p.message = Some(format!("Reset failed: {}", e));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Open the SSH host manager (Secrets → `H`, US-SSH-01).
    async fn open_ssh_panel(&mut self) {
        match repository::list_ssh_hosts(&*self.pool).await {
            Ok(hosts) => {
                self.ssh_panel = Some(SshPanel {
                    hosts,
                    selected: 0,
                    form: None,
                    editing_id: None,
                    confirm_delete: false,
                    message: None,
                });
            }
            Err(e) => self.status_message = Some(format!("Cannot list SSH hosts: {}", e)),
        }
    }

    /// Handle keys while the SSH host panel is open (US-SSH-01..05).
    async fn handle_ssh_panel_key(&mut self, key: KeyEvent) {
        // The create/edit form is topmost inside the panel
        if self.ssh_panel.as_ref().map(|p| p.form.is_some()) == Some(true) {
            self.handle_ssh_form_key(key).await;
            return;
        }
        let Some(panel) = &mut self.ssh_panel else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.ssh_panel = None;
            }
            KeyCode::Up => {
                panel.confirm_delete = false;
                if panel.selected > 0 {
                    panel.selected -= 1;
                }
            }
            KeyCode::Down => {
                panel.confirm_delete = false;
                if panel.selected + 1 < panel.hosts.len() {
                    panel.selected += 1;
                }
            }
            KeyCode::Char('n') => {
                if let Some(p) = &mut self.ssh_panel {
                    p.form = Some(SshForm::new());
                    p.editing_id = None;
                }
            }
            KeyCode::Char('e') => {
                let host = panel.hosts.get(panel.selected).cloned();
                if let Some(h) = host {
                    if let Some(p) = &mut self.ssh_panel {
                        p.form = Some(SshForm {
                            name: h.name.clone(),
                            hostname: h.hostname.clone(),
                            port: h.port.to_string(),
                            username: h.username.clone().unwrap_or_default(),
                            key_path: h.key_path.clone().unwrap_or_default(),
                            field: 0,
                        });
                        p.editing_id = Some(h.id);
                    }
                }
            }
            KeyCode::Char('d') => {
                panel.confirm_delete = !panel.confirm_delete;
                panel.message = panel
                    .confirm_delete
                    .then(|| "Enter to confirm delete".into());
            }
            KeyCode::Enter | KeyCode::Char('c') => {
                // Confirmed delete takes priority over connect
                if panel.confirm_delete {
                    let host = panel.hosts.get(panel.selected).cloned();
                    if let Some(h) = host {
                        match repository::delete_ssh_host(&*self.pool, &h.id).await {
                            Ok(()) => {
                                self.open_ssh_panel().await;
                                if let Some(p) = &mut self.ssh_panel {
                                    p.message = Some(format!("Deleted {}", h.name));
                                }
                            }
                            Err(e) => {
                                if let Some(p) = &mut self.ssh_panel {
                                    p.message = Some(format!("Delete failed: {}", e));
                                    p.confirm_delete = false;
                                }
                            }
                        }
                    }
                    return;
                }
                let host = panel.hosts.get(panel.selected).cloned();
                if let Some(h) = host {
                    // Quick connect (US-SSH-05): ssh in a NEW terminal window
                    let user = h.username.unwrap_or_default();
                    let target = if user.is_empty() {
                        h.hostname.clone()
                    } else {
                        format!("{}@{}", user, h.hostname)
                    };
                    let mut cmd = format!("ssh -p {}", h.port);
                    if let Some(key) = &h.key_path {
                        cmd.push_str(&format!(" -i {}", key));
                    }
                    cmd.push(' ');
                    cmd.push_str(&target);
                    self.wants_terminal_cmd =
                        Some((cmd, dirs_home().to_string_lossy().to_string()));
                    self.ssh_panel = None;
                }
            }
            _ => {}
        }
    }

    /// Handle keys inside the SSH host create/edit form (US-SSH-02/03).
    async fn handle_ssh_form_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if let Some(p) = &mut self.ssh_panel {
                    p.form = None;
                    p.editing_id = None;
                }
            }
            KeyCode::Tab => {
                if let Some(p) = &mut self.ssh_panel {
                    if let Some(f) = &mut p.form {
                        f.field = (f.field + 1) % SshForm::FIELDS.len();
                    }
                }
            }
            KeyCode::BackTab => {
                if let Some(p) = &mut self.ssh_panel {
                    if let Some(f) = &mut p.form {
                        f.field = (f.field + SshForm::FIELDS.len() - 1) % SshForm::FIELDS.len();
                    }
                }
            }
            KeyCode::Up => {
                if let Some(p) = &mut self.ssh_panel {
                    if let Some(f) = &mut p.form {
                        f.field = f.field.saturating_sub(1);
                    }
                }
            }
            KeyCode::Down => {
                if let Some(p) = &mut self.ssh_panel {
                    if let Some(f) = &mut p.form {
                        if f.field + 1 < SshForm::FIELDS.len() {
                            f.field += 1;
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(p) = &mut self.ssh_panel {
                    if let Some(f) = &mut p.form {
                        let buf = match f.field {
                            0 => &mut f.name,
                            1 => &mut f.hostname,
                            2 => &mut f.port,
                            3 => &mut f.username,
                            _ => &mut f.key_path,
                        };
                        buf.pop();
                    }
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_ssh_host().await;
            }
            KeyCode::Char(c) => {
                if let Some(p) = &mut self.ssh_panel {
                    if let Some(f) = &mut p.form {
                        let buf = match f.field {
                            0 => &mut f.name,
                            1 => &mut f.hostname,
                            2 => &mut f.port,
                            3 => &mut f.username,
                            _ => &mut f.key_path,
                        };
                        buf.push(c);
                    }
                }
            }
            _ => {}
        }
    }

    /// Validate + persist the SSH host form (US-SSH-02/03).
    async fn save_ssh_host(&mut self) {
        let (form, editing_id) = {
            let Some(p) = &self.ssh_panel else { return };
            (p.form.clone(), p.editing_id.clone())
        };
        let Some(form) = form else { return };
        if form.name.trim().is_empty() || form.hostname.trim().is_empty() {
            if let Some(p) = &mut self.ssh_panel {
                p.message = Some("Name and hostname are required".to_string());
            }
            return;
        }
        let port: i32 = form.port.trim().parse().unwrap_or(22);
        let result = match &editing_id {
            Some(id) => {
                let host = crate::models::SshHost {
                    id: id.clone(),
                    name: form.name.trim().to_string(),
                    hostname: form.hostname.trim().to_string(),
                    port,
                    username: some_if_not_empty(form.username.trim()),
                    key_path: some_if_not_empty(form.key_path.trim()),
                    created_at: String::new(),
                    updated_at: String::new(),
                };
                repository::update_ssh_host(&*self.pool, &host).await
            }
            None => {
                repository::create_ssh_host(
                    &*self.pool,
                    form.name.trim(),
                    form.hostname.trim(),
                    port,
                    some_if_not_empty(form.username.trim()).as_deref(),
                    some_if_not_empty(form.key_path.trim()).as_deref(),
                )
                .await
            }
        };
        match result {
            Ok(_) => {
                self.open_ssh_panel().await;
                if let Some(p) = &mut self.ssh_panel {
                    p.message = Some(if editing_id.is_some() {
                        "✓ Host updated".to_string()
                    } else {
                        "✓ Host created".to_string()
                    });
                }
            }
            Err(e) => {
                if let Some(p) = &mut self.ssh_panel {
                    p.message = Some(format!("Save failed: {}", e));
                }
            }
        }
    }

    /// Run an invocation and show its output in the result popup.
    async fn run_invoke(&mut self, mut cmd: tokio::process::Command, name: &str, label: &str) {
        match cmd.output().await {
            Ok(out) => {
                self.run_result = Some(RunResult {
                    title: format!("Run: {}", name),
                    success: out.status.success(),
                    text: format!(
                        "{}\n\nexit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
                        label,
                        out.status.code().unwrap_or(-1),
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    ),
                });
            }
            Err(e) => {
                self.run_result = Some(RunResult {
                    title: format!("Run: {}", name),
                    success: false,
                    text: format!("Failed to run {}: {}", label, e),
                });
            }
        }
    }
    /// Handle sudo password input (US-CMD-09).
    async fn handle_sudo_password_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.sudo_password = None;
                self.sudo_pending_command = None;
            }
            KeyCode::Backspace => {
                if let Some(pw) = self.sudo_password.as_mut() {
                    pw.pop();
                }
            }
            KeyCode::Enter => {
                let password = self.sudo_password.take();
                let command = self.sudo_pending_command.take();
                if let (Some(pw), Some(cmd_str)) = (password, command) {
                    if let Some(tool) = privilege::detect_priv_tool() {
                        match privilege::run_privileged(&tool, &cmd_str, Some(&pw)).await {
                            Ok(out) => {
                                self.run_result = Some(RunResult {
                                    title: format!("Sudo: {}", cmd_str),
                                    success: out.status.success(),
                                    text: format!(
                                        "exit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
                                        out.status.code().unwrap_or(-1),
                                        String::from_utf8_lossy(&out.stdout),
                                        String::from_utf8_lossy(&out.stderr)
                                    ),
                                });
                            }
                            Err(e) => {
                                self.status_message = Some(format!("\u{2717} Sudo failed: {}", e));
                            }
                        }
                    } else {
                        self.status_message = Some("\u{2717} No privilege tool found".to_string());
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(pw) = self.sudo_password.as_mut() {
                    pw.push(c);
                }
            }
            _ => {}
        }
    }

    /// Run the selected command with elevated privileges (US-CMD-09).
    async fn run_privileged_command(&mut self) {
        let Some(tool) = privilege::detect_priv_tool() else {
            self.status_message =
                Some("\u{2717} No privilege tool found (sudo/doas/su)".to_string());
            return;
        };
        let Some(entity) = self.commands_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        let content = match entity.content.clone() {
            Some(c) if !c.trim().is_empty() => c,
            _ => {
                self.status_message = Some("Selected item has no content to run".to_string());
                return;
            }
        };
        if privilege::needs_password(&tool) {
            self.sudo_password = Some(String::new());
            self.sudo_pending_command = Some(content);
        } else {
            match privilege::run_privileged(&tool, &content, None).await {
                Ok(out) => {
                    self.run_result = Some(RunResult {
                        title: format!("Sudo: {}", entity.name),
                        success: out.status.success(),
                        text: format!(
                            "exit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
                            out.status.code().unwrap_or(-1),
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        ),
                    });
                }
                Err(e) => {
                    self.status_message = Some(format!("\u{2717} Sudo failed: {}", e));
                }
            }
        }
    }

    /// Render the sudo password input popup (US-CMD-09).
    fn render_sudo_password_popup(&self, f: &mut Frame) {
        let area = self.centered_rect(52, 10, f);
        f.render_widget(Clear, area);
        let tool_name = privilege::detect_priv_tool()
            .map(|t| t.name())
            .unwrap_or("sudo");
        let cmd = self.sudo_pending_command.as_deref().unwrap_or("");
        let block = Block::default()
            .title(format!(
                " \u{1f512} {} password \u{2014} {} ",
                tool_name, cmd
            ))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.error))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(1)])
            .split(inner);
        let masked = "\u{2022}".repeat(self.sudo_password.as_ref().map_or(0, |p| p.len()));
        self.render_field(f, chunks[0], "Password", &masked, true, true);
        f.render_widget(
            Paragraph::new(Span::styled(
                "Enter: run \u{b7} Esc: cancel",
                Style::default().fg(self.ui.theme.border),
            ))
            .alignment(Alignment::Center),
            chunks[1],
        );
    }

    // ========================================================================
    // Overlay renderers
    // ========================================================================

    /// Centered popup rect clamped to the terminal area.
    fn centered_rect(&self, width: u16, height: u16, f: &Frame) -> Rect {
        let area = f.area();
        let w = width.min(area.width.saturating_sub(2)).max(10);
        let h = height.min(area.height.saturating_sub(2)).max(3);
        Rect {
            x: (area.width.saturating_sub(w)) / 2,
            y: (area.height.saturating_sub(h)) / 2,
            width: w,
            height: h,
        }
    }

    fn form_help_line<'a>(&self, error: Option<&String>, help: &'a str) -> Line<'a> {
        match error {
            Some(err) => Line::from(Span::styled(
                err.clone(),
                Style::default()
                    .fg(self.ui.theme.error)
                    .add_modifier(Modifier::BOLD),
            )),
            None => Line::from(Span::styled(
                help,
                Style::default().fg(self.ui.theme.border),
            )),
        }
    }

    fn render_command_form(&self, f: &mut Frame) {
        let area = self.centered_rect(72, 22, f);
        f.render_widget(Clear, area);
        let mode_label = if self.command_form.editing_id.is_some() {
            "Edit"
        } else {
            "New"
        };
        let block = Block::default()
            .title(format!(" {} Command / Script / App ", mode_label))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.primary))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Type
                Constraint::Length(3), // Description
                Constraint::Min(3),    // Content
                Constraint::Length(3), // Tags
                Constraint::Length(1), // Help/error
            ])
            .split(inner);

        let field = self.command_form.focused_field;
        self.render_field(
            f,
            chunks[0],
            "Name",
            &self.command_form.name,
            field == 0,
            false,
        );
        let type_text = format!("◄ {} ►", ENTITY_TYPE_NAMES[self.command_form.entity_type]);
        self.render_field(f, chunks[1], "Type", &type_text, field == 1, false);
        self.render_field(
            f,
            chunks[2],
            "Description",
            &self.command_form.description,
            field == 2,
            false,
        );
        self.render_field(
            f,
            chunks[3],
            "Content (Enter = newline)",
            &self.command_form.content,
            field == 3,
            false,
        );
        self.render_field(
            f,
            chunks[4],
            "Tags (comma separated)",
            &self.command_form.tags_text,
            field == 4,
            false,
        );
        let help = self.form_help_line(
            self.command_form.error_message.as_ref(),
            "Tab/↑↓: fields · ←/→: type · Enter: newline in content · Ctrl+S: save · Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[5]);
    }

    fn render_project_form(&self, f: &mut Frame) {
        let area = self.centered_rect(60, 12, f);
        f.render_widget(Clear, area);
        let mode_label = if self.project_form.editing_id.is_some() {
            "Edit"
        } else {
            "New"
        };
        let block = Block::default()
            .title(format!(" {} Project ", mode_label))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Description
                Constraint::Length(1), // Help/error
            ])
            .split(inner);

        let field = self.project_form.focused_field;
        self.render_field(
            f,
            chunks[0],
            "Name",
            &self.project_form.name,
            field == 0,
            false,
        );
        self.render_field(
            f,
            chunks[1],
            "Description",
            &self.project_form.description,
            field == 1,
            false,
        );
        let help = self.form_help_line(
            self.project_form.error_message.as_ref(),
            "Tab/↑↓: fields · Ctrl+S: save · Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[2]);
    }

    fn render_workflow_form(&self, f: &mut Frame) {
        let area = self.centered_rect(72, 18, f);
        f.render_widget(Clear, area);
        let mode_label = if self.workflow_form.editing_id.is_some() {
            "Edit"
        } else {
            "New"
        };
        let block = Block::default()
            .title(format!(" {} Workflow (Lua script) ", mode_label))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.accent))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Description
                Constraint::Min(3),    // Script
                Constraint::Length(1), // Help/error
            ])
            .split(inner);

        let field = self.workflow_form.focused_field;
        self.render_field(
            f,
            chunks[0],
            "Name",
            &self.workflow_form.name,
            field == 0,
            false,
        );
        self.render_field(
            f,
            chunks[1],
            "Description",
            &self.workflow_form.description,
            field == 1,
            false,
        );
        self.render_field(
            f,
            chunks[2],
            "Lua script (Enter = newline)",
            &self.workflow_form.content,
            field == 2,
            false,
        );
        let help = self.form_help_line(
            self.workflow_form.error_message.as_ref(),
            "Tip: press 'v' in the Workflows tab for the visual builder · Ctrl+S: save · Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[3]);
    }

    fn render_secret_form(&self, f: &mut Frame) {
        let area = self.centered_rect(64, 26, f);
        f.render_widget(Clear, area);
        let mode_label = if self.secret_form.editing_id.is_some() {
            "Edit"
        } else {
            "New"
        };
        let block = Block::default()
            .title(format!(" {} Secret 🔐 ", mode_label))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.warning))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Value (masked)
                Constraint::Length(3), // Group
                Constraint::Length(3), // Username
                Constraint::Length(3), // URL
                Constraint::Length(3), // Email
                Constraint::Length(3), // Passphrase (masked)
                Constraint::Length(1), // SSH-agent flag
                Constraint::Length(1), // Help/error
            ])
            .split(inner);

        let field = self.secret_form.focused_field;
        self.render_field(
            f,
            chunks[0],
            "Name",
            &self.secret_form.name,
            field == 0,
            false,
        );
        self.render_field(
            f,
            chunks[1],
            "Value",
            &self.secret_form.value,
            field == 1,
            true,
        );
        self.render_field(
            f,
            chunks[2],
            "Group (e.g. github, servers)",
            &self.secret_form.group,
            field == 2,
            false,
        );
        self.render_field(
            f,
            chunks[3],
            "Username",
            &self.secret_form.username,
            field == 3,
            false,
        );
        self.render_field(
            f,
            chunks[4],
            "URL",
            &self.secret_form.url,
            field == 4,
            false,
        );
        self.render_field(
            f,
            chunks[5],
            "Email",
            &self.secret_form.email,
            field == 5,
            false,
        );
        self.render_field(
            f,
            chunks[6],
            "Passphrase (extra layer; empty = none)",
            &self.secret_form.passphrase,
            field == 6,
            true,
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!(
                    "ssh-agent: {} (Space toggles)",
                    if self.secret_form.ssh_agent {
                        "ON"
                    } else {
                        "off"
                    }
                ),
                Style::default().fg(if self.secret_form.ssh_agent {
                    self.ui.theme.success
                } else {
                    self.ui.theme.border
                }),
            )])),
            chunks[7],
        );
        let help = self.form_help_line(
            self.secret_form.error_message.as_ref(),
            "XChaCha20Poly1305-encrypted · passphrase adds a 2nd layer · Space: toggle ssh-agent · Ctrl+S: save · Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[8]);
    }

    fn render_confirm_delete(&self, f: &mut Frame, confirm: &ConfirmDelete) {
        let with_folder = confirm.kind == DeleteKind::Project && confirm.path.is_some();
        let area = self.centered_rect(60, if with_folder { 12 } else { 9 }, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" \u{26a0} Confirm Delete ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.error))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);

        let mut text = vec![
            Line::from(Span::raw("Delete permanently?")),
            Line::from(Span::styled(
                confirm.label.clone(),
                Style::default()
                    .fg(self.ui.theme.warning)
                    .add_modifier(Modifier::BOLD),
            )),
        ];
        if with_folder {
            // Folder-removal option (US-PROJ): default OFF
            let state = if confirm.delete_folder { "[x]" } else { "[ ]" };
            let color = if confirm.delete_folder {
                self.ui.theme.error
            } else {
                self.ui.theme.border
            };
            text.push(Line::from(""));
            text.push(Line::from(Span::styled(
                format!("f: {} also delete the folder on disk", state),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
            text.push(Line::from(Span::styled(
                format!("  {}", confirm.path.as_deref().unwrap_or("")),
                Style::default().fg(self.ui.theme.border),
            )));
        }
        text.push(Line::from(""));
        text.push(Line::from(vec![
            Span::styled(
                "Enter",
                Style::default()
                    .fg(self.ui.theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Confirm   "),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(self.ui.theme.error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Cancel"),
        ]));
        f.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
    }

    fn render_run_result(&self, f: &mut Frame) {
        if let Some(ref result) = self.run_result {
            let area = self.centered_rect(84, 20, f);
            f.render_widget(Clear, area);
            let color = if result.success {
                self.ui.theme.success
            } else {
                self.ui.theme.error
            };
            let icon = if result.success { "✓" } else { "✗" };
            let block = Block::default()
                .title(format!(" {} {} ", icon, result.title))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(color))
                .style(Style::default().bg(self.ui.theme.bg));
            let inner = block.inner(area);
            f.render_widget(block, area);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);

            let text = Paragraph::new(result.text.as_str())
                .style(Style::default().fg(self.ui.theme.fg))
                .wrap(ratatui::widgets::Wrap { trim: false });
            f.render_widget(text, chunks[0]);

            let help = Paragraph::new(Span::styled(
                "Enter / Esc: close",
                Style::default().fg(self.ui.theme.border),
            ))
            .alignment(Alignment::Center);
            f.render_widget(help, chunks[1]);
        }
    }

    fn render_status(&self, f: &mut Frame, msg: &str) {
        let area = f.area();
        if area.height < 2 || area.width < 8 {
            return;
        }
        let w = (msg.len() as u16 + 4).min(area.width);
        let rect = Rect {
            x: (area.width - w) / 2,
            y: area.height - 2,
            width: w,
            height: 1,
        };
        f.render_widget(Clear, rect);
        let p = Paragraph::new(Span::styled(
            format!(" {} ", msg),
            Style::default()
                .fg(self.ui.theme.bg)
                .bg(self.ui.theme.success)
                .add_modifier(Modifier::BOLD),
        ));
        f.render_widget(p, rect);
    }

    /// Visual workflow builder: Name/Description fields + ordered step list.
    /// When the command picker is open it is drawn on top.
    fn render_visual(&self, f: &mut Frame) {
        let Some(v) = self.visual_form.as_ref() else {
            return;
        };
        let area = self.centered_rect(80, 24, f);
        f.render_widget(Clear, area);
        let mode_label = if v.editing_id.is_some() {
            "Edit"
        } else {
            "New"
        };
        let block = Block::default()
            .title(format!(" ⚙ Visual Workflow Builder ({}) ", mode_label))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.accent))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Description
                Constraint::Min(4),    // Steps
                Constraint::Length(1), // Help/error
            ])
            .split(inner);

        self.render_field(
            f,
            chunks[0],
            "Name",
            &v.name,
            v.focused_field == VisualField::Name,
            false,
        );
        self.render_field(
            f,
            chunks[1],
            "Description",
            &v.description,
            v.focused_field == VisualField::Description,
            false,
        );

        // Steps list
        let steps_color = if v.focused_field == VisualField::Steps {
            self.ui.theme.accent
        } else {
            self.ui.theme.border
        };
        let steps_title = if v.focused_field == VisualField::Steps {
            " ▶ Steps (saved commands) "
        } else {
            " Steps (saved commands) "
        };
        let steps_block = Block::default()
            .title(steps_title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(steps_color));
        let steps_inner = steps_block.inner(chunks[2]);
        f.render_widget(steps_block, chunks[2]);

        if v.steps.is_empty() {
            let empty =
                Paragraph::new("No steps yet — Enter/'a': pick command · l: add logic node")
                    .style(Style::default().fg(self.ui.theme.border))
                    .alignment(Alignment::Center);
            f.render_widget(empty, steps_inner);
        } else {
            let lines: Vec<Line> = v
                .steps
                .iter()
                .enumerate()
                .map(|(i, step)| {
                    // Logic nodes show their expression summary (US-FUT-07)
                    let first_line = step.summary().lines().next().unwrap_or("").to_string();
                    // Explicit cursor marker for the selected step (US-APP-01)
                    let selected = v.focused_field == VisualField::Steps && i == v.selected_step;
                    let marker = if selected { "❯ " } else { "  " };
                    let content = format!(
                        "{}{}. [{}] {}  →  {}",
                        marker,
                        i + 1,
                        step.kind.label(),
                        step.name,
                        first_line
                    );
                    let style = if selected {
                        Style::default()
                            .fg(self.ui.theme.accent)
                            .add_modifier(Modifier::BOLD)
                            .bg(self.ui.theme.highlight)
                    } else if step.kind != VisualNodeKind::Command {
                        Style::default().fg(self.ui.theme.warning)
                    } else {
                        Style::default().fg(self.ui.theme.fg)
                    };
                    Line::from(Span::styled(content, style))
                })
                .collect();
            f.render_widget(Paragraph::new(lines), steps_inner);
        }

        let help = self.form_help_line(
            v.error_message.as_ref(),
            "Tab: Name/Desc/Steps · Enter/a: pick command · l: logic node · o: edit node · d: remove step · ←/→: move · Ctrl+S: save · Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[3]);

        // Picker overlay on top
        if v.picker.is_some() {
            self.render_command_picker(f);
        }
    }

    fn render_command_picker(&self, f: &mut Frame) {
        let Some(v) = self.visual_form.as_ref() else {
            return;
        };
        let Some(ref picker) = v.picker else {
            return;
        };
        let area = self.centered_rect(70, 18, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" 📋 Pick a saved command ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.highlight))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        if picker.items.is_empty() {
            let empty =
                Paragraph::new("No saved commands found — create some in the Commands tab first")
                    .style(Style::default().fg(self.ui.theme.warning))
                    .alignment(Alignment::Center);
            f.render_widget(empty, chunks[0]);
        } else {
            let lines: Vec<Line> = picker
                .items
                .iter()
                .enumerate()
                .map(|(i, entity)| {
                    let first_line = entity
                        .content
                        .as_ref()
                        .and_then(|c| c.lines().next())
                        .unwrap_or("")
                        .to_string();
                    let marker = if i == picker.selected {
                        "\u{276f} "
                    } else {
                        "  "
                    };
                    let content = format!(
                        "{}[{}] {}  \u{2192}  {}",
                        marker, entity.type_id, entity.name, first_line
                    );
                    let style = if i == picker.selected {
                        Style::default()
                            .fg(self.ui.theme.highlight)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.ui.theme.fg)
                    };
                    Line::from(Span::styled(content, style))
                })
                .collect();
            f.render_widget(Paragraph::new(lines), chunks[0]);
        }

        let help = Paragraph::new(Span::styled(
            "↑↓: navigate · Enter: add as step · Esc: close",
            Style::default().fg(self.ui.theme.border),
        ))
        .alignment(Alignment::Center);
        f.render_widget(help, chunks[1]);
    }

    // ── Phase 2: processes, project environments, SSH/GPG keygen ───────────

    // ── Database operations (dev mode; US-NF) ──────────────────────────────

    /// Wipe the entire database (users + entities + secrets). Dev mode only.
    async fn dev_wipe_database(&mut self) {
        let tables = [
            "DELETE FROM entity_tags",
            "DELETE FROM entities",
            "DELETE FROM workflow_runs",
            "DELETE FROM secrets",
            "DELETE FROM user_keys",
            "DELETE FROM user_profiles",
            "DELETE FROM projects",
            "DELETE FROM tags",
            "DELETE FROM seed_meta",
        ];
        let mut wiped = 0;
        for sql in &tables {
            match sqlx::query(sql).execute(&*self.pool).await {
                Ok(r) => wiped += r.rows_affected(),
                Err(e) => {
                    self.status_message = Some(format!("\u{2717} Wipe failed: {}", e));
                    return;
                }
            }
        }
        self.status_message = Some(format!("\u{2713} DEV: database wiped ({} rows)", wiped));
        // Clear in-memory lists too
        self.commands_list.clear();
        self.projects_list.clear();
        self.workflows_list.clear();
        self.secrets_list.clear();
        self.stats = DashboardStats::default();
        // Don't fetch — get_or_create_user would recreate a profile
    }

    /// Delete all entities of the given type (US-CMD-07 group delete).
    async fn delete_entities_by_type(&mut self, type_id: &str, label: &str) {
        let entities = repository::list_entities(&*self.pool, Some(type_id), None).await;
        match entities {
            Ok(list) => {
                let count = list.len();
                for e in &list {
                    let _ = repository::delete_entity(&*self.pool, &e.id).await;
                }
                self.status_message = Some(format!("\u{2713} Deleted {} {}(s)", count, label));
                let _ = self.fetch_stats().await;
                self.refresh_current_tab().await;
            }
            Err(e) => self.status_message = Some(format!("\u{2717} {}", e)),
        }
    }

    /// Delete all entities in the current tab (dev mode group delete).
    async fn delete_all_in_tab(&mut self) {
        match self.ui.state {
            AppState::Knowledge => {
                // Delete according to the active filter (All = all three)
                for ty in self.kb_filter.type_ids() {
                    self.delete_entities_by_type(ty, ty).await;
                }
            }
            AppState::Projects => {
                let projects = repository::list_projects(&*self.pool)
                    .await
                    .unwrap_or_default();
                for p in &projects {
                    let _ = repository::delete_project(&*self.pool, &p.id).await;
                }
                self.status_message = Some(format!("\u{2713} Deleted {} projects", projects.len()));
                let _ = self.fetch_projects().await;
            }
            AppState::Workflows => {
                self.delete_entities_by_type("wf", "workflow").await;
            }
            _ => {
                self.status_message = Some("Multi-delete not available for this tab".to_string());
            }
        }
        let _ = self.fetch_stats().await;
    }
    /// TUI and runs `man <name>`; notifies gracefully when `man` is not
    /// installed or no entry exists instead of failing.
    async fn show_man_page(&mut self) {
        let Some(entity) = self.commands_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        if !keygen::which("man") {
            self.status_message = Some(
                "\u{2717} man is not installed \u{2014} install the man-db package to read manuals"
                    .to_string(),
            );
            return;
        }
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let status = tokio::process::Command::new("man")
            .arg(&entity.name)
            .status()
            .await;
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen);
        let _ = crossterm::terminal::enable_raw_mode();
        self.needs_full_redraw = true;
        match status {
            Ok(s) if s.success() => {}
            Ok(_) => self.status_message = Some(format!("No manual entry for '{}'", entity.name)),
            Err(e) => self.status_message = Some(format!("\u{2717} man failed: {}", e)),
        }
    }

    /// Show the structured options of the selected command family (US-CMD-01):
    /// the `opt` child entities with their descriptions.
    async fn show_options_popup(&mut self) {
        let Some(entity) = self.commands_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        match repository::list_child_entities(&*self.pool, &entity.id).await {
            Ok(children) if children.is_empty() => {
                self.status_message = Some(format!("'{}' has no documented options", entity.name));
            }
            Ok(children) => {
                let options = children
                    .iter()
                    .map(|c| (c.name.clone(), c.description.clone().unwrap_or_default()))
                    .collect();
                self.options_popup = Some((entity.name.clone(), options));
            }
            Err(e) => self.status_message = Some(format!("\u{2717} {}", e)),
        }
    }

    /// System fetch panel (US-PROC/US-NF): hostname, OS, kernel, init system,
    /// CPU/memory/swap/uptime — like fastfetch, built from sysinfo.
    async fn run_fetch(&mut self) {
        let mut pm = crate::process::ProcessManager::new();
        let overview = pm.overview();
        let gib = 1024 * 1024 * 1024;
        let text = format!(
            "{name}@{host}\n-----------\nOS       : {os} {os_ver}\nKernel   : {kernel}\nInit     : {init}\nUptime   : {days}d {hours}h {mins}m\nCPU      : {cpu:.1}%\nMemory   : {mem_used:.2} / {mem_total:.2} GiB\nSwap     : {swap_used:.2} / {swap_total:.2} GiB\n",
            name = std::env::var("USER").unwrap_or_else(|_| "user".to_string()),
            host = overview.hostname,
            os = overview.os_name,
            os_ver = overview.os_version,
            kernel = overview.kernel_version,
            init = crate::seed::detect_init_system(),
            days = overview.uptime / 86400,
            hours = (overview.uptime % 86400) / 3600,
            mins = (overview.uptime % 3600) / 60,
            cpu = overview.cpu_usage,
            mem_used = overview.memory_used as f64 / gib as f64,
            mem_total = overview.memory_total as f64 / gib as f64,
            swap_used = overview.swap_used as f64 / gib as f64,
            swap_total = overview.swap_total as f64 / gib as f64,
        );
        self.run_result = Some(RunResult {
            title: "System Fetch".to_string(),
            success: true,
            text,
        });
    }

    /// Suspend the TUI and launch a known process viewer (btop/htop/top).
    /// Users already know these tools — no custom UI needed (US-PROC).
    async fn run_process_viewer(&mut self) {
        let Some(viewer) = keygen::pick_process_viewer() else {
            self.status_message = Some("No process viewer found (btop/htop/top)".to_string());
            return;
        };
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = tokio::process::Command::new(viewer).status().await;
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen);
        let _ = crossterm::terminal::enable_raw_mode();
        self.needs_full_redraw = true;
    }

    /// Open an interactive shell with the project environment activated (US-ENV).
    async fn start_project_shell(&mut self) {
        let Some(project) = self.projects_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        let env_cmd = project.env_cmd.clone().unwrap_or_default();
        let env_cmd = env_cmd.trim().to_string();
        if env_cmd.is_empty() {
            self.status_message = Some(format!(
                "Project '{}' has no environment configured",
                project.name
            ));
            return;
        }
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{env_cmd}; exec {shell}"))
            .status()
            .await;
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen);
        let _ = crossterm::terminal::enable_raw_mode();
        self.needs_full_redraw = true;
        self.status_message = Some(format!("Project shell for '{}' closed", project.name));
    }

    /// Login-screen dev user manager input (US-NF): navigate, delete, reset.
    async fn handle_login_dev_users_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.dev_user_manager = false,
            KeyCode::Up => {
                if self.dev_user_selected > 0 {
                    self.dev_user_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.dev_user_selected + 1 < self.dev_user_list.len() {
                    self.dev_user_selected += 1;
                }
            }
            KeyCode::Char('x') => self.dev_delete_selected_user().await,
            KeyCode::Char('r') => self.dev_reset_selected_password().await,
            _ => {}
        }
    }

    // ── New project workspace creation (US-PROJ, US-ENV) ──────────────────

    /// New project creation form input: name/kind/editor, Ctrl+S creates.
    async fn handle_new_project_key(&mut self, key: KeyEvent) {
        // Template preview popup (US-PLG-15) sits on top of the form
        if self.template_preview.is_some() {
            self.handle_template_preview_key(key);
            return;
        }
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.create_new_project().await;
            return;
        }
        // Ctrl+O: merge into an existing folder (US-PROJ) — offered when
        // creation failed with "already exists"
        if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
            let parent = self
                .new_project_parent
                .clone()
                .unwrap_or_else(|| dirs_home().join("projects"));
            self.create_new_project_in_opts(&parent, true).await;
            return;
        }
        let field = self.new_project_field();
        match key.code {
            KeyCode::Esc => self.new_project_open = false,
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter if field < 3 => {
                self.new_project_field_idx = (self.new_project_field_idx + 1) % 4;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.new_project_field_idx = (self.new_project_field_idx + 3) % 4;
            }
            KeyCode::Left if field == 1 => {
                self.new_project_kind =
                    (self.new_project_kind + project_workspace::ProjectKind::all().len() - 1)
                        % project_workspace::ProjectKind::all().len();
            }
            KeyCode::Right if field == 1 => {
                self.new_project_kind =
                    (self.new_project_kind + 1) % project_workspace::ProjectKind::all().len();
            }
            KeyCode::Left if field == 2 => {
                self.new_project_editor =
                    (self.new_project_editor + project_workspace::ProjectEditor::all().len() - 1)
                        % project_workspace::ProjectEditor::all().len();
            }
            KeyCode::Right if field == 2 => {
                self.new_project_editor =
                    (self.new_project_editor + 1) % project_workspace::ProjectEditor::all().len();
            }
            KeyCode::Left if field == 3 => {
                // Template picker (US-PROJ-08): None first, then each template
                self.new_project_template = match self.new_project_template {
                    None => None, // stay on "none" when only one step back
                    Some(0) => None,
                    Some(i) => Some(i - 1),
                };
            }
            KeyCode::Right if field == 3 => {
                if self.new_project_template.is_none() {
                    if !self.templates.is_empty() {
                        self.new_project_template = Some(0);
                    }
                } else {
                    let idx = self.new_project_template.unwrap();
                    if idx + 1 < self.templates.len() {
                        self.new_project_template = Some(idx + 1);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char('p') if field == 3 => {
                // Preview the selected template before anything is written
                if let Some(idx) = self.new_project_template {
                    if let Some(tpl) = self.templates.get(idx) {
                        self.template_preview = Some(TemplatePreview::new(idx, tpl.clone()));
                    }
                }
            }
            KeyCode::Backspace if field == 0 => {
                self.new_project_name.pop();
            }
            KeyCode::Char(c) if field == 0 => self.new_project_name.push(c),
            _ => {}
        }
    }

    /// Keys for the template preview popup (US-PLG-15): browse files, toggle
    /// inclusion with `x`, edit content inline with `e`, Esc applies the
    /// customizations back to the selected template and closes.
    fn handle_template_preview_key(&mut self, key: KeyEvent) {
        let Some(preview) = self.template_preview.as_mut() else {
            return;
        };
        if preview.editing {
            match key.code {
                KeyCode::Esc => preview.editing = false,
                KeyCode::Enter => {
                    if let Some(file) = preview.template.files.get_mut(preview.selected) {
                        file.content = preview.edit_buf.clone();
                    }
                    preview.editing = false;
                }
                KeyCode::Backspace => {
                    preview.edit_buf.pop();
                }
                KeyCode::Char(c) => preview.edit_buf.push(c),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Esc => {
                // Apply customizations back, then close
                let preview = self.template_preview.take().unwrap();
                let idx = preview.template_idx;
                if let Some(slot) = self.templates.get_mut(idx) {
                    *slot = preview.into_customized();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                preview.selected = preview.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if preview.selected + 1 < preview.template.files.len() {
                    preview.selected += 1;
                }
            }
            KeyCode::Char('x') => {
                if let Some(flag) = preview.included.get_mut(preview.selected) {
                    *flag = !*flag;
                }
            }
            KeyCode::Char('e') => {
                if let Some(file) = preview.template.files.get(preview.selected) {
                    preview.edit_buf = file.content.clone();
                    preview.editing = true;
                }
            }
            _ => {}
        }
    }

    /// The currently focused field of the new project form (0-3).
    fn new_project_field(&self) -> usize {
        self.new_project_field_idx
    }

    /// Register an existing directory as a project (US-PROJ-01): the directory
    /// must exist; its base name becomes the project name and the path is
    /// stored so `O` can open it in an editor.
    async fn register_existing_directory(&mut self, entered: &str) {
        let expanded = expand_tilde(entered);
        let path = std::path::PathBuf::from(expanded);
        if !path.is_dir() {
            self.status_message = Some(format!("Not a directory: {}", path.display()));
            return;
        }
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
            self.status_message = Some("Cannot derive a project name from /".to_string());
            return;
        };
        let req = CreateProject {
            name: name.clone(),
            description: Some(format!("Registered from {}", path.display())),
        };
        match repository::create_project(&*self.pool, &req).await {
            Ok(project) => {
                let _ = repository::set_project_path(
                    &*self.pool,
                    &project.id,
                    Some(&path.to_string_lossy()),
                )
                .await;
                self.fire_project_created(&name, &path).await;
                self.status_message = Some(format!("Registered {} - press O to open it", name));
                let _ = self.fetch_projects().await;
            }
            Err(e) => self.status_message = Some(format!("{}", e)),
        }
    }

    /// Decrypt + offer every ssh_agent-flagged SSH key to ssh-agent (US-SEC).
    /// Passphrase-protected keys are skipped (no interactive prompt here);
    /// use `t` to open an ssh terminal with those.
    async fn load_ssh_agent_keys(&mut self) {
        let user_id = self.current_user_profile_id().await;
        let Ok(all) = repository::list_secrets(&*self.pool, &user_id).await else {
            return;
        };
        let mut added = 0;
        let mut skipped = 0;
        for secret in all
            .iter()
            .filter(|s| s.ssh_agent && s.secret_kind == "ssh_key")
        {
            if secret.passphrase_protected {
                skipped += 1;
                continue;
            }
            match secrets::decrypt_for_user(&*self.pool, &user_id, &secret.value_enc).await {
                Ok(pem) => match secrets::ssh_agent::add_key_to_agent(&secret.name, &pem) {
                    Ok(()) => added += 1,
                    Err(e) => {
                        self.status_message = Some(format!("{}", e));
                    }
                },
                Err(e) => self.status_message = Some(format!("{}", e)),
            }
        }
        self.status_message = Some(format!(
            "ssh-agent: {} key(s) added (passphrase-locked skipped)",
            added
        ));
    }

    /// Open an SSH terminal for the selected secret (US-SEC): uses the stored
    /// SSH key (ssh_agent flag) and connects to `url` as `user@host`.
    async fn open_ssh_terminal(&mut self) {
        let Some(secret) = self.secrets_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        let Some(host) = secret.url.clone().filter(|u| !u.trim().is_empty()) else {
            self.status_message = Some("No host set (edit the secret, fill URL)".to_string());
            return;
        };
        if secret.passphrase_protected {
            self.status_message =
                Some("Secret is passphrase-locked; use S (agent) instead".to_string());
            return;
        }
        let user_id = self.current_user_profile_id().await;
        let pem = match secrets::decrypt_for_user(&*self.pool, &user_id, &secret.value_enc).await {
            Ok(p) => p,
            Err(e) => {
                self.status_message = Some(format!("Decrypt failed: {}", e));
                return;
            }
        };
        // Materialize the private key at a 0600 temp path for the session
        let key_file =
            std::env::temp_dir().join(format!("tui-op-hub-ssh-{}.pem", std::process::id()));
        if std::fs::write(&key_file, &pem).is_err() {
            self.status_message = Some("Failed to write temp key".to_string());
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600));
        }
        let ssh_cmd = format!(
            "ssh -i {} -o StrictHostKeyChecking=accept-new {}",
            key_file.display(),
            host.trim()
        );
        // Suspend the TUI, run the interactive ssh session, restore
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&ssh_cmd)
            .status()
            .await;
        let _ = std::fs::remove_file(&key_file);
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen);
        let _ = crossterm::terminal::enable_raw_mode();
        self.needs_full_redraw = true;
        self.status_message = Some(format!("ssh session ended ({})", host.trim()));
    }

    /// Copy a passphrase-protected secret: unwrap the passphrase layer, then
    /// the user-key layer, and put the plaintext on the clipboard.
    async fn copy_secret_with_passphrase(&mut self, secret_id: &str, passphrase: &str) {
        let Ok(secret) = repository::get_secret(&*self.pool, secret_id).await else {
            self.status_message = Some("Secret not found".to_string());
            return;
        };
        let user_id = self.current_user_profile_id().await;
        let value = match secrets::decrypt_for_user(&*self.pool, &user_id, &secret.value_enc).await
        {
            Ok(v) => v,
            Err(e) => {
                self.status_message = Some(format!("Decrypt failed: {}", e));
                return;
            }
        };
        match secrets::unwrap_with_passphrase(&value, passphrase) {
            Ok(plaintext) => match arboard::Clipboard::new() {
                Ok(mut cb) => {
                    let _ = cb.set_text(plaintext);
                    self.status_message = Some("Copied to clipboard".to_string());
                }
                Err(e) => self.status_message = Some(format!("Clipboard unavailable: {}", e)),
            },
            Err(_) => {
                self.status_message = Some("Wrong passphrase".to_string());
            }
        }
    }

    /// Fire the `project_created` event to all loaded plugins (US-PLG-09).
    /// Payload: {name, path}. Hook errors are logged, never fatal.
    async fn fire_project_created(&self, name: &str, path: &std::path::Path) {
        let payload = serde_json::json!({
            "name": name,
            "path": path.to_string_lossy(),
        });
        let results = self.plugins.emit_event("project_created", &payload).await;
        for (id, result) in results {
            match result {
                Ok(Some(msg)) => {
                    tracing::info!(plugin = %id, message = %msg, "project_created hook ran");
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(plugin = %id, error = %e, "project_created hook failed");
                }
            }
        }
    }

    /// Open the plugin-actions popup for the selected project (US-PLG-13):
    /// only actions from loaded (approved) plugins on the "project" surface.
    async fn open_project_actions(&mut self) {
        let Some(project) = self.projects_list.get_selected().cloned() else {
            self.status_message = Some("No project selected".to_string());
            return;
        };
        let loaded = self.plugins.list_plugins().await;
        let entries: Vec<crate::plugin::PluginActionEntry> = self
            .plugins
            .discover_actions()
            .into_iter()
            .filter(|e| e.action.surface == "project" && loaded.contains(&e.plugin_id))
            .collect();
        if entries.is_empty() {
            self.status_message =
                Some("No plugin actions available (load a plugin with actions)".to_string());
            return;
        }
        self.project_actions = Some(ProjectActionsPanel {
            entries,
            selected: 0,
            project_name: project.name.clone(),
            project_path: project.path.clone().unwrap_or_default(),
            last_result: None,
        });
    }

    /// Keys for the plugin-actions popup (US-PLG-13).
    async fn handle_project_actions_key(&mut self, key: KeyEvent) {
        let Some(panel) = self.project_actions.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.project_actions = None,
            KeyCode::Up | KeyCode::Char('k') => {
                panel.selected = panel.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if panel.selected + 1 < panel.entries.len() {
                    panel.selected += 1;
                }
            }
            KeyCode::Enter => {
                let Some(entry) = panel.entries.get(panel.selected).cloned() else {
                    return;
                };
                let args = vec![panel.project_name.clone(), panel.project_path.clone()];
                let result = self.plugins.run_action(&entry, &args).await;
                let project_actions = self.project_actions.as_mut();
                if let Some(panel) = project_actions {
                    panel.last_result = Some(match &result {
                        Ok(msg) => (true, msg.clone()),
                        Err(e) => (false, format!("{}", e)),
                    });
                }
                self.status_message = Some(match result {
                    Ok(msg) => format!("✓ {}: {}", entry.action.label, msg),
                    Err(e) => format!("✗ {}: {}", entry.action.label, e),
                });
            }
            _ => {}
        }
    }

    /// Create the project directory + git repo + env, save to DB.
    async fn create_new_project(&mut self) {
        let parent = self
            .new_project_parent
            .clone()
            .unwrap_or_else(|| dirs_home().join("projects"));
        self.create_new_project_in_opts(&parent, false).await;
    }

    /// `create_new_project` with an injectable parent directory (tests).
    async fn create_new_project_in(&mut self, parent: &std::path::Path) {
        self.create_new_project_in_opts(parent, false).await;
    }

    /// Create with the "merge into existing folder" option (US-PROJ): when
    /// the target directory already exists, `overwrite = true` reuses it
    /// instead of failing (kind scaffolding and templates never overwrite
    /// existing files).
    async fn create_new_project_in_opts(&mut self, parent: &std::path::Path, overwrite: bool) {
        let name = self.new_project_name.trim().to_string();
        if name.is_empty() {
            self.new_project_error = Some("Name is required".to_string());
            return;
        }
        let kinds = project_workspace::ProjectKind::all();
        let kind = kinds[self.new_project_kind.min(kinds.len() - 1)];
        let existed_before = parent.join(&name).exists();
        match project_workspace::create_project_directory(parent, &name, &kind, overwrite) {
            Ok(created) => {
                // Save to DB, including where the workspace lives (US-PROJ)
                let req = CreateProject {
                    name: name.clone(),
                    description: Some(kind.description().to_string()),
                };
                match repository::create_project(&*self.pool, &req).await {
                    Ok(project) => {
                        let _ = repository::set_project_env(
                            &*self.pool,
                            &project.id,
                            Some(kind.name()),
                            created.env_cmd.as_deref(),
                        )
                        .await;
                        let _ = repository::set_project_path(
                            &*self.pool,
                            &project.id,
                            Some(&created.path.to_string_lossy()),
                        )
                        .await;
                    }
                    Err(e) => {
                        self.new_project_error = Some(format!("DB save failed: {}", e));
                        return;
                    }
                }
                self.new_project_open = false;

                // Apply the selected plugin template, if any (US-PLG-14/15,
                // US-PROJ-08). Runs file writes, git init and post-create
                // commands off the async thread.
                let mut template_note = String::new();
                if let Some(idx) = self.new_project_template {
                    if let Some(tpl) = self.templates.get(idx) {
                        let tpl = tpl.clone();
                        let tpl_name = tpl.name.clone();
                        let target = created.path.clone();
                        let name_clone = name.clone();
                        let applied = tokio::task::spawn_blocking(move || {
                            tpl.instantiate(&name_clone, &target)
                        })
                        .await;
                        match applied {
                            Ok(Ok(report)) => {
                                template_note = format!(
                                    " \u{2014} template '{}': {}",
                                    tpl_name,
                                    report.summary()
                                );
                            }
                            Ok(Err(e)) => {
                                template_note = format!(" \u{2014} template failed: {}", e);
                            }
                            Err(e) => {
                                template_note = format!(" \u{2014} template task failed: {}", e);
                            }
                        }
                    }
                }

                self.fire_project_created(&name, &created.path).await;
                let merged = if overwrite && existed_before {
                    " (merged into existing folder)"
                } else {
                    ""
                };
                self.status_message = Some(format!(
                    "\u{2713} Project '{}' created at {} \u{2014} press O to open it{}{}",
                    name,
                    created.path.display(),
                    merged,
                    template_note
                ));
                let _ = self.fetch_projects().await;
            }
            Err(e) => {
                // Point users at the merge option when the folder exists
                if format!("{}", e).contains("already exists") {
                    self.new_project_error =
                        Some(format!("{} \u{2014} Ctrl+O: use existing folder", e));
                } else {
                    self.new_project_error = Some(format!("{}", e));
                }
            }
        }
    }

    /// Open the selected project in the chosen editor (US-PROJ).
    async fn open_project_in_editor(&mut self) {
        let Some(project) = self.projects_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        let editors = project_workspace::ProjectEditor::all();
        let editor = &editors[self.new_project_editor.min(editors.len() - 1)];
        // Prefer the registered workspace path; fall back to ~/projects/<name>
        // if it exists (older rows have no stored path).
        let fallback = dirs_home().join("projects").join(&project.name);
        let dir = project
            .path
            .clone()
            .filter(|p| std::path::Path::new(p).is_dir())
            .or_else(|| {
                if fallback.is_dir() {
                    Some(fallback.to_string_lossy().to_string())
                } else {
                    None
                }
            });
        let Some(dir) = dir else {
            self.status_message = Some(format!(
                "\u{2717} No directory for '{}' (create a workspace with N)",
                project.name
            ));
            return;
        };
        let cmd = editor.open_command(&dir);
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .status()
            .await;
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen);
        let _ = crossterm::terminal::enable_raw_mode();
        self.needs_full_redraw = true;
        self.status_message = Some(format!("Opened in {}", editor.display_name()));
    }

    /// Render the new project creation form (US-PROJ + US-PROJ-08 template
    /// picker, default none).
    fn render_new_project_form(&self, f: &mut Frame) {
        let area = self.centered_rect(64, 21, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" \u{1f4c1} New Project Workspace ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.success))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Kind
                Constraint::Length(3), // Editor
                Constraint::Length(3), // Template
                Constraint::Length(1), // Help/error
            ])
            .split(inner);

        let field = self.new_project_field();
        self.render_field(
            f,
            chunks[0],
            "Project name",
            &self.new_project_name,
            field == 0,
            false,
        );

        let kinds = project_workspace::ProjectKind::all();
        let kind_name = kinds[self.new_project_kind.min(kinds.len() - 1)].name();
        let kind_text = format!("\u{25c4} {} \u{25ba}", kind_name);
        self.render_field(f, chunks[1], "Environment", &kind_text, field == 1, false);

        let editors = project_workspace::ProjectEditor::all();
        let editor_name = editors[self.new_project_editor.min(editors.len() - 1)].display_name();
        let editor_text = format!("\u{25c4} {} \u{25ba}", editor_name);
        self.render_field(f, chunks[2], "Editor", &editor_text, field == 2, false);

        // Template picker (US-PROJ-08): default none, explicit choice
        let template_text = match self
            .new_project_template
            .and_then(|idx| self.templates.get(idx))
        {
            Some(tpl) => format!("\u{25c4} {} \u{25ba} (Enter: preview)", tpl.name),
            None => "\u{25c4} (none) \u{25ba}".to_string(),
        };
        self.render_field(f, chunks[3], "Template", &template_text, field == 3, false);

        let help = self.form_help_line(
            self.new_project_error.as_ref(),
            "Tab: fields \u{b7} \u{2190}/\u{2192}: cycle kind/editor/template \u{b7} Ctrl+S: create \u{b7} Ctrl+O: merge into existing folder \u{b7} Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[4]);
    }

    /// Template preview popup (US-PLG-15): file list with inclusion toggles,
    /// selected file content, post-create commands. Nothing is written until
    /// the project is created.
    fn render_template_preview(&self, f: &mut Frame) {
        let Some(preview) = &self.template_preview else {
            return;
        };
        let area = self.centered_rect(74, 24, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(format!(
                " \u{1f4c1} Template: {} ({}) ",
                preview.template.name, preview.template.plugin_name
            ))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.success))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // description + git flag
                Constraint::Min(1),    // files + content
                Constraint::Length(1), // help
            ])
            .split(inner);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            format!(
                "{}{}",
                preview
                    .template
                    .description
                    .as_deref()
                    .unwrap_or("Project scaffold"),
                if preview.template.git_init {
                    " \u{b7} git init: yes"
                } else {
                    ""
                }
            ),
            Style::default().fg(self.ui.theme.border),
        )));
        lines.push(Line::from(Span::styled(
            "Files (x: include/exclude):",
            Style::default()
                .fg(self.ui.theme.secondary)
                .add_modifier(Modifier::BOLD),
        )));
        for (i, file) in preview.template.files.iter().enumerate() {
            let focused = i == preview.selected;
            let marker = if focused { "\u{25b6} " } else { "  " };
            let flag = if preview.included[i] {
                "\u{2713}"
            } else {
                "\u{2717}"
            };
            let color = if preview.included[i] {
                self.ui.theme.success
            } else {
                self.ui.theme.error
            };
            let style = if focused {
                Style::default()
                    .fg(self.ui.theme.secondary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui.theme.border)
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{} ", flag), Style::default().fg(color)),
                Span::styled(
                    file.path.clone(),
                    Style::default()
                        .fg(self.ui.theme.fg)
                        .add_modifier(if focused {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]));
        }
        // Selected file content (edit mode shows the buffer instead)
        lines.push(Line::from(Span::styled(
            "Content:",
            Style::default()
                .fg(self.ui.theme.secondary)
                .add_modifier(Modifier::BOLD),
        )));
        let content = if preview.editing {
            format!("{}\u{2588}", preview.edit_buf)
        } else {
            preview
                .template
                .files
                .get(preview.selected)
                .map(|f| f.content.clone())
                .unwrap_or_default()
        };
        let budget = rows[1].height as usize;
        let reserve = preview.template.commands.len() + 3;
        let take = budget.saturating_sub(lines.len() + reserve).max(1);
        for line in content.lines().take(take) {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(self.ui.theme.fg),
            )));
        }
        if !preview.template.commands.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("Post-create: {}", preview.template.commands.join(" ; ")),
                Style::default().fg(self.ui.theme.warning),
            )));
        }
        f.render_widget(Paragraph::new(lines), rows[1]);

        let help_text = if preview.editing {
            "Type to edit \u{b7} Enter: save content \u{b7} Esc: discard"
        } else {
            "\u{2191}\u{2193}: file \u{b7} x: include/exclude \u{b7} e: edit content \u{b7} Esc: done (customizations kept)"
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                help_text,
                Style::default().fg(self.ui.theme.border),
            )),
            rows[2],
        );
    }

    /// Plugin UI actions popup (US-PLG-13): labeled entries contributed by
    /// approved plugins, run against the selected project.
    fn render_project_actions(&self, f: &mut Frame) {
        let Some(panel) = &self.project_actions else {
            return;
        };
        let area = self.centered_rect(64, 16, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(format!(" \u{1f9ee} Actions: {} ", panel.project_name))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.success))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        let mut lines: Vec<Line> = Vec::new();
        for (i, entry) in panel.entries.iter().enumerate() {
            let focused = i == panel.selected;
            let marker = if focused { "\u{25b6} " } else { "  " };
            let style = if focused {
                Style::default()
                    .fg(self.ui.theme.secondary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui.theme.fg)
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(
                    entry.action.label.clone(),
                    Style::default()
                        .fg(self.ui.theme.fg)
                        .add_modifier(if focused {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("  ({})", entry.plugin_name),
                    Style::default().fg(self.ui.theme.border),
                ),
            ]));
        }
        if let Some((ok, msg)) = &panel.last_result {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("{} {}", if *ok { "\u{2713}" } else { "\u{2717}" }, msg),
                Style::default().fg(if *ok {
                    self.ui.theme.success
                } else {
                    self.ui.theme.error
                }),
            )));
        }
        f.render_widget(Paragraph::new(lines), rows[0]);
        f.render_widget(
            Paragraph::new(Span::styled(
                "\u{2191}\u{2193}: action \u{b7} Enter: run \u{b7} Esc: close",
                Style::default().fg(self.ui.theme.border),
            )),
            rows[1],
        );
    }

    /// Keygen form input (US-SEC-01): name/email/passphrase/kind + generate.
    async fn handle_keygen_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.generate_key().await;
            return;
        }
        let field = self.keygen.focused_field;
        match key.code {
            KeyCode::Esc => self.keygen.open = false,
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter if field != 2 => {
                self.keygen.select_next_field();
            }
            KeyCode::Char('2') => {
                self.keygen.select_next_field();
            }
            KeyCode::BackTab | KeyCode::Up | KeyCode::Char('8') => {
                self.keygen.select_previous_field();
            }
            KeyCode::Char('4') if field == 3 => {
                self.keygen.kind = (self.keygen.kind + KEYGEN_KINDS.len() - 1) % KEYGEN_KINDS.len();
            }
            KeyCode::Char('6') if field == 3 => {
                self.keygen.kind = (self.keygen.kind + 1) % KEYGEN_KINDS.len();
            }
            KeyCode::Left if field == 3 => {
                self.keygen.kind = (self.keygen.kind + KEYGEN_KINDS.len() - 1) % KEYGEN_KINDS.len();
            }
            KeyCode::Right if field == 3 => {
                self.keygen.kind = (self.keygen.kind + 1) % KEYGEN_KINDS.len();
            }
            KeyCode::Enter => {
                self.keygen.passphrase.push('\n');
            }
            KeyCode::Backspace => match field {
                0 => {
                    self.keygen.name.pop();
                }
                1 => {
                    self.keygen.email.pop();
                }
                2 => {
                    self.keygen.passphrase.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match field {
                0 => self.keygen.name.push(c),
                1 => self.keygen.email.push(c),
                2 => self.keygen.passphrase.push(c),
                _ => {}
            },
            _ => {}
        }
    }

    /// Generate the SSH/GPG key and store its location + passphrase as
    /// encrypted secrets with tags (US-SEC-01).
    async fn generate_key(&mut self) {
        let name = self.keygen.name.trim().to_string();
        if name.is_empty() {
            self.keygen.error = Some("Name is required".to_string());
            return;
        }
        let user_id = self.current_user_profile_id().await;
        let result = match self.keygen.kind_name() {
            "ssh" => keygen::generate_ssh_key(
                &std::env::temp_dir().join("tui-op-hub-keys"),
                &name,
                &self.keygen.passphrase,
            ),
            "gpg" => keygen::generate_gpg_key(&name, &self.keygen.email, &self.keygen.passphrase),
            _ => unreachable!(),
        };
        let generated = match result {
            Ok(key) => key,
            Err(e) => {
                self.keygen.error = Some(format!("Key generation failed: {}", e));
                return;
            }
        };
        // Store the private key location (requires re-auth to use)
        let enc_path =
            match secrets::encrypt_for_user(&*self.pool, &user_id, &generated.private_path).await {
                Ok(enc) => enc,
                Err(e) => {
                    self.keygen.error = Some(format!("Encryption failed: {}", e));
                    return;
                }
            };
        let _ = repository::create_secret_full(
            &*self.pool,
            &user_id,
            &format!("{name}_private_key"),
            &enc_path,
            &generated.kind,
            true,
        )
        .await;
        // Store the passphrase when one was used
        if !generated.passphrase.is_empty() {
            if let Ok(enc) =
                secrets::encrypt_for_user(&*self.pool, &user_id, &generated.passphrase).await
            {
                let _ = repository::create_secret_full(
                    &*self.pool,
                    &user_id,
                    &format!("{name}_passphrase"),
                    &enc,
                    &generated.kind,
                    true,
                )
                .await;
            }
        }
        self.keygen = KeygenState::default();
        self.status_message = Some(format!(
            "\u{2713} {} key '{}' generated and stored",
            generated.kind, name
        ));
        let _ = self.fetch_secrets().await;
        let _ = self.fetch_stats().await;
    }

    /// Settings input: navigate rows, edit values, capture keybindings, save.
    /// Structured options popup for a command family (US-CMD-01).
    fn render_options_popup(&self, f: &mut Frame) {
        let Some((family, options)) = &self.options_popup else {
            return;
        };
        let height = (options.len() as u16 + 6).clamp(8, 24);
        let area = self.centered_rect(70, height, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(format!(" \u{1f4d6} Options: {} ", family))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.highlight))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        let lines: Vec<Line> = options
            .iter()
            .map(|(flag, description)| {
                Line::from(vec![
                    Span::styled(
                        format!("{:<12}", flag),
                        Style::default()
                            .fg(self.ui.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(description.clone(), Style::default().fg(self.ui.theme.fg)),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), chunks[0]);

        f.render_widget(
            Paragraph::new(Span::styled(
                "Esc: close",
                Style::default().fg(self.ui.theme.border),
            ))
            .alignment(Alignment::Center),
            chunks[1],
        );
    }

    // ── Login-screen dev user manager (US-NF, cargo run only) ──────────────

    /// Open the dev user manager (login screen, dev mode only): `u` lists
    /// users and lets you delete them or reset their password without login.
    fn open_login_user_manager(&mut self) {
        self.dev_user_manager = true;
        self.dev_user_list = Vec::new();
        self.dev_user_selected = 0;
        self.dev_user_error = None;
        self.dev_confirm_wipe = false;
    }

    /// Refresh the user list shown in the dev manager.
    async fn refresh_dev_user_list(&mut self) {
        self.dev_user_list = repository::list_user_profiles(&*self.pool)
            .await
            .unwrap_or_default();
        if self.dev_user_selected >= self.dev_user_list.len() {
            self.dev_user_selected = self.dev_user_list.len().saturating_sub(1);
        }
    }

    /// Delete the selected user (secrets cascade; US-SEC forgotten password).
    async fn dev_delete_selected_user(&mut self) {
        let Some(user) = self.dev_user_list.get(self.dev_user_selected).cloned() else {
            return;
        };
        match repository::delete_user(&*self.pool, &user.id).await {
            Ok(()) => {
                self.dev_user_error = Some(format!(
                    "\u{2713} Deleted '{}' (secrets removed)",
                    user.username
                ));
            }
            Err(e) => self.dev_user_error = Some(format!("\u{2717} {}", e)),
        }
        self.refresh_dev_user_list().await;
    }

    /// Reset the selected user's password to `reset-me` (US-SEC forgotten
    /// password). Existing secrets stay encrypted with the old key and can no
    /// longer be decrypted — documented escape hatch.
    async fn dev_reset_selected_password(&mut self) {
        let Some(user) = self.dev_user_list.get(self.dev_user_selected).cloned() else {
            return;
        };
        let salt = argon2::password_hash::SaltString::generate(
            &mut argon2::password_hash::rand_core::OsRng,
        );
        let argon2 = argon2::Argon2::default();
        use argon2::PasswordHasher;
        let hash = match argon2.hash_password(b"reset-me", &salt) {
            Ok(h) => h.to_string(),
            Err(e) => {
                self.dev_user_error = Some(format!("\u{2717} {}", e));
                return;
            }
        };
        match repository::set_password_hash(&*self.pool, &user.id, &hash, salt.as_str()).await {
            Ok(()) => {
                self.dev_user_error = Some(format!(
                    "\u{2713} '{}' password reset to 'reset-me' (old secrets unrecoverable)",
                    user.username
                ));
            }
            Err(e) => self.dev_user_error = Some(format!("\u{2717} {}", e)),
        }
    }

    /// Render the dev user manager (login screen; cargo run only).
    fn render_login_user_manager(&self, f: &mut Frame) {
        let area = self.centered_rect(60, 16, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" \u{1f527} DEV: User Manager (no login) ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.warning))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        let mut lines: Vec<Line> = Vec::new();
        if self.dev_user_list.is_empty() {
            lines.push(Line::from(Span::raw("No users. Create one via signup.")));
        }
        for (i, user) in self.dev_user_list.iter().enumerate() {
            let selected = i == self.dev_user_selected;
            let marker = if selected { "\u{25b6} " } else { "  " };
            let admin = if user.is_admin { " [admin]" } else { "" };
            let style = if selected {
                Style::default()
                    .fg(self.ui.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui.theme.fg)
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}{}", marker, user.username, admin),
                style,
            )));
        }
        if let Some(err) = &self.dev_user_error {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(self.ui.theme.success),
            )));
        }
        f.render_widget(Paragraph::new(lines), chunks[0]);

        f.render_widget(
            Paragraph::new(Span::styled(
                "\u{2191}\u{2193} select \u{b7} x delete user+secrets \u{b7} r reset pw \u{2192} 'reset-me' \u{b7} Esc back",
                Style::default().fg(self.ui.theme.border),
            ))
            .alignment(Alignment::Center),
            chunks[1],
        );
    }
    fn render_keygen_form(&self, f: &mut Frame) {
        let area = self.centered_rect(60, 16, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" \u{1f511} Generate SSH / GPG key ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.warning))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Name
                Constraint::Length(3), // Email
                Constraint::Length(3), // Passphrase
                Constraint::Length(3), // Kind
                Constraint::Length(1), // Help/error
            ])
            .split(inner);

        let field = self.keygen.focused_field;
        self.render_field(
            f,
            chunks[0],
            "Key name",
            &self.keygen.name,
            field == 0,
            false,
        );
        self.render_field(
            f,
            chunks[1],
            "Email (GPG user id)",
            &self.keygen.email,
            field == 1,
            false,
        );
        self.render_field(
            f,
            chunks[2],
            "Passphrase (stored encrypted)",
            &self.keygen.passphrase,
            field == 2,
            true,
        );
        let kind_text = format!("\u{25c4} {} \u{25ba}", self.keygen.kind_name());
        self.render_field(f, chunks[3], "Kind", &kind_text, field == 3, false);
        let help = self.form_help_line(
            self.keygen.error.as_ref(),
            "Tab/\u{2191}\u{2193}: fields · \u{2190}/\u{2192}: kind · Ctrl+S: generate · Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[4]);
    }

    async fn handle_settings_key(&mut self, key: KeyEvent) {
        // Advanced sub-screen (visual config) sits on top of Settings
        if self.advanced.active {
            self.handle_advanced_key(key).await;
            return;
        }
        // Ctrl+S persists the config to disk from anywhere in Settings
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_settings().await;
            return;
        }

        // A keybinding capture is in progress: the next key becomes the binding
        if let Some(row) = self.settings.capturing_key {
            if key.code == KeyCode::Esc {
                self.settings.capturing_key = None;
                return;
            }
            let action = SETTINGS_ROWS.get(row).copied().unwrap_or("");
            match KeybindingsConfig::keycode_to_string(key.code) {
                Some(text) => {
                    self.config.keybindings.set(action, text.clone());
                    self.settings.capturing_key = None;
                    self.settings.dirty = true;
                    self.settings.error = None;
                    self.status_message = Some(format!("✓ '{}' bound to {}", action, text));
                }
                None => {
                    self.settings.capturing_key = None;
                    self.settings.error = Some("That key cannot be bound".to_string());
                }
            }
            return;
        }

        // A text value (editor / page size) is being edited
        if let Some(row) = self.settings.editing_text {
            match key.code {
                KeyCode::Esc => self.settings.editing_text = None,
                KeyCode::Enter => self.apply_text_setting(row),
                KeyCode::Backspace => {
                    self.settings.buffer.pop();
                }
                KeyCode::Char(c) => self.settings.buffer.push(c),
                _ => {}
            }
            return;
        }

        // Digits 1-9 always switch tabs, even inside Settings
        if self.handle_tab_digit(&key).await {
            return;
        }

        let row = self.settings.selected;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.ui.state = AppState::Dashboard,
            KeyCode::Up | KeyCode::BackTab => self.settings.select_previous(),
            KeyCode::Down | KeyCode::Tab => self.settings.select_next(),
            KeyCode::Char('a') => {
                // Advanced mode: visual theme colors + database/API options
                self.advanced.active = true;
                self.advanced.error = None;
            }
            KeyCode::Char('u') => {
                // Admin: manage users (delete / reset password, US-SEC)
                self.open_users_panel().await;
            }
            KeyCode::Char('D') if crate::auth::dev_mode_enabled() => {
                // DEV: wipe entire database
                self.dev_wipe_database().await;
            }
            KeyCode::Char('A') if crate::auth::dev_mode_enabled() => {
                // DEV: delete all entities in the current tab
                self.delete_all_in_tab().await;
            }
            KeyCode::Char('d') if crate::auth::dev_mode_enabled() => {
                // Developer mode: wipe ALL users without logging in so auth
                // state can be reset while testing (two-step confirm)
                if self.dev_confirm_wipe {
                    self.dev_confirm_wipe = false;
                    match repository::delete_all_users(&*self.pool).await {
                        Ok(n) => {
                            self.status_message = Some(format!(
                                "\u{2713} DEV: deleted {} user(s) \u{2014} next signup is admin",
                                n
                            ));
                        }
                        Err(e) => {
                            self.status_message = Some(format!("\u{2717} DEV wipe failed: {}", e))
                        }
                    }
                    let _ = self.fetch_stats().await;
                } else {
                    self.dev_confirm_wipe = true;
                    self.status_message =
                        Some("DEV: press d again to delete ALL users".to_string());
                }
            }
            KeyCode::Left if row == SETTINGS_THEME_ROW => self.cycle_theme(-1),
            KeyCode::Right if row == SETTINGS_THEME_ROW => self.cycle_theme(1),
            KeyCode::Enter => match row {
                SETTINGS_THEME_ROW => self.cycle_theme(1),
                r if r < SETTINGS_KEYBIND_FIRST_ROW => {
                    // Start text editing with the current value
                    self.settings.editing_text = Some(r);
                    self.settings.buffer = self.setting_value(r);
                }
                r => {
                    // Keybinding rows: capture the next pressed key
                    self.settings.capturing_key = Some(r);
                }
            },
            _ => {}
        }
    }

    /// Current config value of a Settings row (for display and editing).
    fn setting_value(&self, row: usize) -> String {
        let name = SETTINGS_ROWS.get(row).copied().unwrap_or("");
        match name {
            "editor" => self.config.general.editor.clone(),
            "page_size" => self.config.tui.page_size.to_string(),
            "theme" => self.config.theme.name.clone(),
            other => self.config.keybindings.get(other).to_string(),
        }
    }

    /// Commit the text buffer for a text setting (editor or page size).
    fn apply_text_setting(&mut self, row: usize) {
        let name = SETTINGS_ROWS.get(row).copied().unwrap_or("");
        let value = self.settings.buffer.trim().to_string();
        match name {
            "editor" => {
                self.config.general.editor = value;
                self.settings.editing_text = None;
                self.settings.dirty = true;
                self.settings.error = None;
                self.status_message = Some("✓ Editor updated (Ctrl+S to persist)".to_string());
            }
            "page_size" => match value.parse::<usize>() {
                Ok(n) if (1..=100).contains(&n) => {
                    self.config.tui.page_size = n;
                    self.commands_list.page_size = n;
                    self.projects_list.page_size = n;
                    self.workflows_list.page_size = n;
                    self.secrets_list.page_size = n;
                    self.settings.editing_text = None;
                    self.settings.dirty = true;
                    self.settings.error = None;
                    self.status_message =
                        Some("✓ Page size applied (Ctrl+S to persist)".to_string());
                }
                _ => {
                    self.settings.error =
                        Some("Page size must be a number between 1 and 100".to_string());
                }
            },
            _ => {}
        }
    }

    /// Cycle the theme preset (dir = -1 / +1) and apply it immediately (US-APP-01).
    fn cycle_theme(&mut self, dir: i32) {
        let presets = ModernTheme::PRESETS;
        let current = self.setting_value(SETTINGS_THEME_ROW);
        let idx = preset_index(&current);
        let next = (idx as i32 + dir).rem_euclid(presets.len() as i32) as usize;
        let name = presets[next].to_string();
        self.config.theme.name = name.clone();
        self.ui.theme = ModernTheme::from_config(&self.config.theme);
        self.settings.dirty = true;
        self.settings.error = None;
        self.status_message = Some(format!("✓ Theme: {} (Ctrl+S to persist)", name));
    }

    /// Persist the config to disk (US-APP-06).
    async fn save_settings(&mut self) {
        match self.config.save(&self.config_path) {
            Ok(()) => {
                self.settings.dirty = false;
                self.settings.error = None;
                self.status_message = Some(format!(
                    "✓ Settings saved to {}",
                    self.config_path.display()
                ));
            }
            Err(e) => self.settings.error = Some(format!("Save failed: {}", e)),
        }
    }

    // ── Advanced mode: visual config editor (US-APP-01) ────────────────────

    /// Advanced screen input: visual color cycling, text editing, reset.
    async fn handle_advanced_key(&mut self, key: KeyEvent) {
        // Ctrl+S persists from anywhere in Advanced mode
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_settings().await;
            return;
        }

        // A text value is being edited
        if self.advanced.editing {
            match key.code {
                KeyCode::Esc => self.advanced.editing = false,
                KeyCode::Enter => self.apply_advanced_text(),
                KeyCode::Backspace => {
                    self.advanced.buffer.pop();
                }
                KeyCode::Char(c) => self.advanced.buffer.push(c),
                _ => {}
            }
            return;
        }

        // Digits 1-9 always switch tabs (also exits Advanced back to a tab)
        if self.handle_tab_digit(&key).await {
            return;
        }

        let row = self.advanced.selected;
        let name = ADVANCED_ROWS.get(row).copied().unwrap_or("");
        match key.code {
            KeyCode::Esc => self.advanced.active = false,
            KeyCode::Up | KeyCode::BackTab => self.advanced.select_previous(),
            KeyCode::Down | KeyCode::Tab => self.advanced.select_next(),
            KeyCode::Left if is_advanced_color_row(row) => self.cycle_advanced_color(-1),
            KeyCode::Right if is_advanced_color_row(row) => self.cycle_advanced_color(1),
            KeyCode::Enter => {
                // Edit any value inline (hex colors, db path, api bind, timeout)
                self.advanced.editing = true;
                self.advanced.buffer = self.advanced_value(row);
            }
            KeyCode::Backspace | KeyCode::Delete if is_advanced_color_row(row) => {
                self.reset_advanced_color(row);
            }
            KeyCode::Backspace => {
                // Reset non-color advanced option to its default
                self.reset_advanced_option(name);
            }
            _ => {}
        }
    }

    /// Current value of an advanced row (for display and editing).
    fn advanced_value(&self, row: usize) -> String {
        let name = ADVANCED_ROWS.get(row).copied().unwrap_or("");
        match name {
            "fg" => self.config.theme.fg.clone(),
            "bg" => self.config.theme.bg.clone(),
            "accent" => self.config.theme.accent.clone(),
            "status_bg" => self.config.theme.status_bg.clone(),
            "primary" => self.config.theme.primary.clone().unwrap_or_default(),
            "secondary" => self.config.theme.secondary.clone().unwrap_or_default(),
            "success" => self.config.theme.success.clone().unwrap_or_default(),
            "warning" => self.config.theme.warning.clone().unwrap_or_default(),
            "error" => self.config.theme.error.clone().unwrap_or_default(),
            "border" => self.config.theme.border.clone().unwrap_or_default(),
            "highlight" => self.config.theme.highlight.clone().unwrap_or_default(),
            "db_path" => self.config.database.path.clone(),
            "api_bind" => self.config.api.bind_addr.clone(),
            "busy_timeout" => self.config.database.busy_timeout_ms.to_string(),
            _ => String::new(),
        }
    }

    /// Apply the edited text buffer to the advanced row.
    fn apply_advanced_text(&mut self) {
        let name = self.advanced.row_name();
        let value = self.advanced.buffer.trim().to_string();
        match name {
            "busy_timeout" => match value.parse::<u64>() {
                Ok(n) => {
                    self.config.database.busy_timeout_ms = n;
                    self.advanced.editing = false;
                    self.advanced.error = None;
                    self.settings.dirty = true;
                    self.status_message =
                        Some("✓ Busy timeout applied (Ctrl+S to persist)".to_string());
                }
                _ => self.advanced.error = Some("Busy timeout must be a number".to_string()),
            },
            "fg" | "bg" | "accent" | "status_bg" | "primary" | "secondary" | "success"
            | "warning" | "error" | "border" | "highlight" => {
                self.set_advanced_color(name, value.clone());
                self.advanced.editing = false;
                self.advanced.error = None;
                self.settings.dirty = true;
                self.status_message = Some(format!("✓ {} = {} (Ctrl+S to persist)", name, value));
            }
            "db_path" => {
                if value.is_empty() {
                    self.advanced.error = Some("Database path cannot be empty".to_string());
                } else {
                    self.config.database.path = value;
                    self.advanced.editing = false;
                    self.advanced.error = None;
                    self.settings.dirty = true;
                    self.status_message =
                        Some("✓ Database path applied (Ctrl+S to persist)".to_string());
                }
            }
            "api_bind" => {
                if value.is_empty() {
                    self.advanced.error = Some("Bind address cannot be empty".to_string());
                } else {
                    self.config.api.bind_addr = value;
                    self.advanced.editing = false;
                    self.advanced.error = None;
                    self.settings.dirty = true;
                    self.status_message =
                        Some("✓ Bind address applied (Ctrl+S to persist)".to_string());
                }
            }
            _ => {}
        }
    }

    /// Set a theme color and refresh the live theme.
    ///
    /// Base palette fields (`fg`/`bg`/`accent`) are applied directly to the
    /// running theme so the visual editor gives instant feedback even when a
    /// preset is active; optional overrides (`primary`…`highlight`) are
    /// re-derived through `from_config`.
    fn set_advanced_color(&mut self, name: &str, value: String) {
        match name {
            "fg" => {
                self.config.theme.fg = value.clone();
                self.ui.theme.fg = crate::config::parse_color(&value);
            }
            "bg" => {
                self.config.theme.bg = value.clone();
                self.ui.theme.bg = crate::config::parse_color(&value);
            }
            "accent" => {
                self.config.theme.accent = value.clone();
                self.ui.theme.accent = crate::config::parse_color(&value);
            }
            "status_bg" => {
                self.config.theme.status_bg = value.clone();
            }
            "primary" => self.config.theme.primary = Some(value),
            "secondary" => self.config.theme.secondary = Some(value),
            "success" => self.config.theme.success = Some(value),
            "warning" => self.config.theme.warning = Some(value),
            "error" => self.config.theme.error = Some(value),
            "border" => self.config.theme.border = Some(value),
            "highlight" => self.config.theme.highlight = Some(value),
            _ => {}
        }
        if !matches!(name, "fg" | "bg" | "accent" | "status_bg") {
            self.ui.theme = ModernTheme::from_config(&self.config.theme);
        }
        self.settings.dirty = true;
    }

    /// Cycle an advanced color row through [`ADVANCED_PALETTE`] (US-APP-01).
    /// Applied live to the running theme.
    fn cycle_advanced_color(&mut self, dir: i32) {
        let row = self.advanced.selected;
        let name = ADVANCED_ROWS.get(row).copied().unwrap_or("");
        if name.is_empty() {
            return;
        }
        let current = self.advanced_value(row);
        let len = ADVANCED_PALETTE.len() as i32;
        let idx = ADVANCED_PALETTE
            .iter()
            .position(|c| c.eq_ignore_ascii_case(&current))
            .map(|i| i as i32)
            .unwrap_or(0);
        let next = (idx + dir).rem_euclid(len) as usize;
        self.set_advanced_color(name, ADVANCED_PALETTE[next].to_string());
        self.advanced.error = None;
        self.status_message = Some(format!(
            "✓ {} = {} (Ctrl+S to persist)",
            name, ADVANCED_PALETTE[next]
        ));
    }

    /// Clear a theme color override: optional colors go back to the preset,
    /// base palette fields return to their defaults.
    fn reset_advanced_color(&mut self, row: usize) {
        let name = ADVANCED_ROWS.get(row).copied().unwrap_or("");
        match name {
            "primary" => self.config.theme.primary = None,
            "secondary" => self.config.theme.secondary = None,
            "success" => self.config.theme.success = None,
            "warning" => self.config.theme.warning = None,
            "error" => self.config.theme.error = None,
            "border" => self.config.theme.border = None,
            "highlight" => self.config.theme.highlight = None,
            "fg" => self.config.theme.fg = crate::config::default_fg(),
            "bg" => self.config.theme.bg = crate::config::default_bg(),
            "accent" => self.config.theme.accent = crate::config::default_accent(),
            "status_bg" => self.config.theme.status_bg = crate::config::default_status_bg(),
            _ => {}
        }
        self.ui.theme = ModernTheme::from_config(&self.config.theme);
        self.advanced.error = None;
        self.settings.dirty = true;
        self.status_message = Some(format!("✓ {} reset (Ctrl+S to persist)", name));
    }

    /// Reset a non-color advanced option to its built-in default.
    fn reset_advanced_option(&mut self, name: &str) {
        match name {
            "db_path" => self.config.database.path = crate::config::default_db_path(),
            "api_bind" => self.config.api.bind_addr = crate::config::default_api_addr(),
            "busy_timeout" => {
                self.config.database.busy_timeout_ms = crate::config::default_busy_timeout()
            }
            _ => {}
        }
        self.advanced.error = None;
        self.settings.dirty = true;
        self.status_message = Some(format!("✓ {} reset (Ctrl+S to persist)", name));
    }
}

/// Rank items by fuzzy match score against the query (best first). Items that
/// do not match are dropped. `key` extracts (name, description) haystacks.
pub fn fuzzy_rank<T>(items: &[T], query: &str, key: impl Fn(&T) -> (&str, &str)) -> Vec<T>
where
    T: Clone,
{
    let mut scored: Vec<(i64, T)> = items
        .iter()
        .filter_map(|item| {
            let (name, description) = key(item);
            let score = std::cmp::max(
                crate::fuzzy::fuzzy_match(name, query),
                crate::fuzzy::fuzzy_match(description, query),
            )?;
            Some((score, item.clone()))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, item)| item).collect()
}

fn preset_index(name: &str) -> usize {
    ModernTheme::PRESETS
        .iter()
        .position(|p| p.eq_ignore_ascii_case(name))
        .unwrap_or(0)
}

impl ModernApp {
    /// Full keybind helper overlay (`?`): all actions grouped per screen,
    /// config-aware labels (US-TUI-09).
    fn render_users_panel(&self, f: &mut Frame) {
        let Some(panel) = &self.users_panel else {
            return;
        };
        let area = self.centered_rect(52, 60, f);
        f.render_widget(Clear, area);
        let rows: usize = panel.users.len().min(8).max(3);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(rows as u16),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .title(format!(
                " \u{1f465} Users ({}) \u{2014} admin \u{1f451} ",
                panel.users.len()
            ))
            .style(Style::default().bg(self.ui.theme.bg));
        f.render_widget(block, area);
        let items: Vec<Line> = panel
            .users
            .iter()
            .enumerate()
            .map(|(i, u)| {
                let marker = if i == panel.selected { "\u{25b6}" } else { " " };
                let admin = if u.is_admin { " \u{1f451}" } else { "" };
                let confirm = if panel.confirm_delete && i == panel.selected {
                    " \u{26a0} really delete?"
                } else {
                    ""
                };
                let style = if i == panel.selected {
                    Style::default().fg(self.ui.theme.accent)
                } else {
                    Style::default().fg(self.ui.theme.fg)
                };
                Line::from(format!("{} {}{}{}", marker, u.username, admin, confirm)).style(style)
            })
            .collect();
        f.render_widget(
            Paragraph::new(items).style(Style::default().bg(self.ui.theme.bg)),
            chunks[0],
        );
        let hint = panel.message.clone().unwrap_or_else(|| {
            "\u{2191}\u{2193} select \u{b7} d delete \u{b7} Enter reset pw \u{b7} Esc close"
                .to_string()
        });
        f.render_widget(
            Paragraph::new(hint).style(Style::default().fg(self.ui.theme.warning)),
            chunks[1],
        );
    }

    fn render_ssh_panel(&self, f: &mut Frame) {
        let Some(panel) = &self.ssh_panel else { return };
        let area = self.centered_rect(60, 66, f);
        f.render_widget(Clear, area);
        if let Some(form) = &panel.form {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(self.ui.theme.warning))
                .title(if panel.editing_id.is_some() {
                    " \u{1f517} Edit SSH host "
                } else {
                    " \u{2795} New SSH host "
                })
                .style(Style::default().bg(self.ui.theme.bg));
            f.render_widget(block, area);
            let rows: Vec<Line> = SshForm::FIELDS
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let value = match i {
                        0 => &form.name,
                        1 => &form.hostname,
                        2 => &form.port,
                        3 => &form.username,
                        _ => &form.key_path,
                    };
                    let focused = i == form.field;
                    let text = if focused {
                        format!("\u{25b6} {}: {}│", label, value)
                    } else {
                        format!("  {}: {}", label, value)
                    };
                    let style = if focused {
                        Style::default().fg(self.ui.theme.accent)
                    } else {
                        Style::default().fg(self.ui.theme.fg)
                    };
                    Line::from(text).style(style)
                })
                .collect();
            let inner = area.inner(Margin::new(1, 1));
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(6), Constraint::Length(1)])
                .split(inner);
            f.render_widget(
                Paragraph::new(rows).style(Style::default().bg(self.ui.theme.bg)),
                chunks[0],
            );
            f.render_widget(
                Paragraph::new("Tab field \u{b7} Ctrl+S save \u{b7} Esc cancel")
                    .style(Style::default().fg(self.ui.theme.border)),
                chunks[1],
            );
            return;
        }
        let rows: usize = panel.hosts.len().min(8).max(3);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(rows as u16),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .title(format!(" \u{1f517} SSH hosts ({}) ", panel.hosts.len()))
            .style(Style::default().bg(self.ui.theme.bg));
        f.render_widget(block, area);
        let items: Vec<Line> = panel
            .hosts
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let marker = if i == panel.selected { "\u{25b6}" } else { " " };
                let confirm = if panel.confirm_delete && i == panel.selected {
                    " \u{26a0} really delete?"
                } else {
                    ""
                };
                let user = h.username.clone().unwrap_or_default();
                let target = if user.is_empty() {
                    h.hostname.clone()
                } else {
                    format!("{}@{}", user, h.hostname)
                };
                let style = if i == panel.selected {
                    Style::default().fg(self.ui.theme.accent)
                } else {
                    Style::default().fg(self.ui.theme.fg)
                };
                Line::from(format!(
                    "{} {}  {}:{}{}",
                    marker, h.name, target, h.port, confirm
                ))
                .style(style)
            })
            .collect();
        f.render_widget(
            Paragraph::new(items).style(Style::default().bg(self.ui.theme.bg)),
            chunks[0],
        );
        let hint = panel.message.clone().unwrap_or_else(|| {
            if panel.hosts.is_empty() {
                "n new \u{b7} Esc close".to_string()
            } else {
                "\u{2191}\u{2193} select \u{b7} Enter/c connect \u{b7} n new \u{b7} e edit \u{b7} d delete \u{b7} Esc"
                    .to_string()
            }
        });
        f.render_widget(
            Paragraph::new(hint).style(Style::default().fg(self.ui.theme.warning)),
            chunks[1],
        );
    }

    fn render_keybinds_overlay(&self, f: &mut Frame) {
        let area = self.centered_rect(66, 34, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" \u{2328}\u{fe0f}  Keybinds ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.highlight))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let kbd = |name: &str| {
            Span::styled(
                name.to_string(),
                Style::default()
                    .fg(self.ui.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        };
        let txt = |s: &str| Span::raw(format!(" {}", s));
        let row = |k: &str, label: &str| Line::from(vec![kbd(k), txt(label)]);
        let section = |title: &str| {
            Line::from(Span::styled(
                title.to_string(),
                Style::default()
                    .fg(self.ui.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ))
        };

        let mut lines: Vec<Line> = Vec::new();
        lines.push(section("Global"));
        lines.push(row(
            "Tab",
            "Switch tabs (configurable order, Settings last)",
        ));
        lines.push(row(
            "1-7,0",
            "1 Dashboard · 2 Knowledge · 3 Projects · 4 Workflows · 5 Secrets · 6 Configs · 7 Plugins · 0 Settings",
        ));
        lines.push(row(
            "f",
            "Knowledge tab: cycle the type filter (All/Commands/Apps/Scripts)",
        ));
        lines.push(row("`", "Open a new terminal window"));
        lines.push(row("/", "Fuzzy search in the current list"));
        lines.push(row("?", "Toggle this keybind helper"));
        lines.push(row("q", "Quit"));

        lines.push(Line::from(""));
        lines.push(section("Dashboard"));
        lines.push(row("f", "System fetch panel"));
        lines.push(row("p", "Processes (btop/htop/top)"));

        lines.push(Line::from(""));
        lines.push(section("Commands / Apps / Scripts"));
        lines.push(row("n", "New command / script / app (type preset by tab)"));
        lines.push(row("e", "Edit selected"));
        lines.push(row("d", "Delete selected (confirm)"));
        lines.push(row("r", "Run selected"));
        lines.push(row("c", "Copy content to clipboard"));
        lines.push(row("o", "Open in external editor"));
        lines.push(row("i", "Show command options"));
        lines.push(row("m", "Open man page"));
        lines.push(row("x", "Export knowledge base"));
        lines.push(row("I", "Import knowledge base"));
        lines.push(row("R", "Run with sudo/doas/su"));

        lines.push(Line::from(""));
        lines.push(section("Projects"));
        lines.push(row("N", "Create new project workspace (dir + git + env)"));
        lines.push(row("n", "Register an existing directory as a project"));
        lines.push(row("Enter", "Open project detail (entities overview)"));
        lines.push(row("O", "Open project in external editor"));
        lines.push(row("E", "Open a shell in the project environment"));

        lines.push(Line::from(""));
        lines.push(section("Workflows"));
        lines.push(row("v", "Visual builder (pick saved commands)"));
        lines.push(row("r", "Execute workflow (background)"));
        lines.push(row("X", "Cancel the running workflow"));

        lines.push(Line::from(""));
        lines.push(section("Secrets"));
        lines.push(row("k", "Generate SSH / GPG key"));
        lines.push(row("c", "Copy (decrypts) secret value"));
        lines.push(row("H", "SSH host manager (connect / CRUD)"));

        lines.push(Line::from(""));
        lines.push(section("Plugins"));
        lines.push(row("a", "Approve the selected plugin (grant capabilities)"));
        lines.push(row("e", "Enable / disable the selected plugin"));

        lines.push(Line::from(""));
        lines.push(section("Settings"));
        lines.push(row("a", "Advanced mode (visual theme editor)"));
        lines.push(row("u", "Admin: manage users (delete / reset password)"));
        lines.push(row("Ctrl+S", "Save settings to config.conf"));
        lines.push(Line::from(""));
        lines.push(section("Configs"));
        lines.push(row("n", "Register an existing config file"));
        lines.push(row("t", "Add a deploy target to the selected config"));
        lines.push(row("m", "Cycle deploy mode (symlink/hard link/copy)"));
        lines.push(row("l", "Deploy to all targets"));
        lines.push(row("u", "Update: sync source + re-deploy (shows drift)"));
        lines.push(row("g", "Git commit the config store"));

        if crate::auth::dev_mode_enabled() {
            lines.push(Line::from(""));
            lines.push(section("Developer mode"));
            lines.push(row("d", "Wipe ALL users (press twice, no login)"));
        }

        f.render_widget(Paragraph::new(lines), inner);
    }
}

impl ModernApp {
    /// Public hooks for BDD integration tests ([`tests/bdd_scenarios.rs`]).
    /// They expose just enough state to drive and assert the TUI without a
    /// terminal; marked `#[doc(hidden)]` to keep them out of user docs.
    ///
    /// [`tests/bdd_scenarios.rs`]: ../../tests/bdd_scenarios.rs
    #[doc(hidden)]
    pub async fn bdd_press(&mut self, code: KeyCode) {
        self.handle_key(KeyEvent::new(code, KeyModifiers::empty()))
            .await;
    }

    #[doc(hidden)]
    pub fn bdd_open_settings(&mut self) {
        self.ui.state = AppState::Settings;
    }

    #[doc(hidden)]
    #[doc(hidden)]
    pub fn bdd_select_settings_row(&mut self, row: usize) {
        self.settings.selected = row;
        self.advanced.active = false;
    }

    #[doc(hidden)]
    pub fn bdd_settings_selected(&self) -> usize {
        self.settings.selected
    }

    #[doc(hidden)]
    pub fn bdd_open_advanced(&mut self) {
        self.ui.state = AppState::Settings;
        self.advanced.active = true;
    }

    #[doc(hidden)]
    pub fn bdd_advanced_selected(&self) -> usize {
        self.advanced.selected
    }

    #[doc(hidden)]
    pub fn bdd_theme_name(&self) -> String {
        self.config.theme.name.clone()
    }

    #[doc(hidden)]
    pub fn bdd_theme_bg(&self) -> String {
        self.config.theme.bg.clone()
    }

    #[doc(hidden)]
    pub fn bdd_open_keygen(&mut self) {
        self.ui.state = AppState::Secrets;
        self.keygen = KeygenState {
            open: true,
            ..Default::default()
        };
    }

    #[doc(hidden)]
    pub fn bdd_keygen_field(&self) -> usize {
        self.keygen.focused_field
    }

    #[doc(hidden)]
    pub fn bdd_keybinds_open(&self) -> bool {
        self.keybinds_overlay
    }

    #[doc(hidden)]
    pub fn bdd_state(&self) -> AppState {
        self.ui.state.clone()
    }

    #[doc(hidden)]
    pub fn bdd_set_state(&mut self, state: AppState) {
        self.ui.state = state;
    }

    #[doc(hidden)]
    pub fn bdd_keybind_hints(&self) -> Vec<(String, String)> {
        self.keybind_hints()
            .into_iter()
            .map(|(k, l)| (k.to_string(), l.to_string()))
            .collect()
    }

    #[doc(hidden)]
    pub fn bdd_dev_user_list(&self) -> Vec<String> {
        self.dev_user_list
            .iter()
            .map(|u| u.username.clone())
            .collect()
    }

    #[doc(hidden)]
    pub fn bdd_search_active(&self) -> bool {
        self.search_state.active
    }

    #[doc(hidden)]
    pub fn bdd_command_list(&self) -> (Vec<String>, usize) {
        let items = self
            .commands_list
            .items
            .iter()
            .map(|e| e.name.clone())
            .collect();
        (items, self.commands_list.total_count)
    }

    #[doc(hidden)]
    pub async fn bdd_goto_commands(&mut self) {
        self.ui.state = AppState::Knowledge;
        let _ = self.fetch_knowledge().await;
    }
}

/// Run the external editor over `content` using a temp file and return the
/// edited text. Terminal-agnostic (the caller suspends/resumes the TUI).
async fn run_external_editor(editor: &str, content: &str) -> anyhow::Result<String> {
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("tui-op-hub-edit-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, content)?;
    let command = format!("{} \"$TUIOPHUBFILE\"", editor);
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .env("TUIOPHUBFILE", &tmp)
        .status()
        .await?;
    let text = std::fs::read_to_string(&tmp).unwrap_or_else(|_| content.to_string());
    let _ = std::fs::remove_file(&tmp);
    if !status.success() {
        anyhow::bail!("editor exited with status {}", status);
    }
    Ok(text)
}

impl ModernApp {
    /// Open the selected command's content in the external editor (US-CMD-05).
    /// Suspends the TUI while the editor runs, then saves any changes.
    async fn open_in_editor(&mut self) {
        let Some(entity) = self.commands_list.get_selected().cloned() else {
            self.status_message = Some("Nothing selected".to_string());
            return;
        };
        let editor = self.effective_editor();
        let content = entity.content.clone().unwrap_or_default();

        // Suspend the TUI so the editor can take over the terminal
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let result = run_external_editor(&editor, &content).await;
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen);
        let _ = crossterm::terminal::enable_raw_mode();
        self.needs_full_redraw = true;

        let new_content = match result {
            Ok(text) => text,
            Err(e) => {
                self.status_message = Some(format!("✗ Editor failed: {}", e));
                return;
            }
        };
        if new_content == content {
            self.status_message = Some("No changes from editor".to_string());
            return;
        }
        let req = CreateEntity {
            name: entity.name.clone(),
            description: entity.description.clone(),
            content: Some(new_content),
            type_id: entity.type_id.clone(),
            project_id: entity.project_id.clone(),
            tags: None,
            metadata_json: entity.metadata_json.clone(),
        };
        match repository::update_entity(&*self.pool, &entity.id, &req).await {
            Ok(_) => {
                self.status_message = Some("✓ Saved from editor".to_string());
                let _ = self.fetch_knowledge().await;
            }
            Err(e) => self.status_message = Some(format!("✗ Save failed: {}", e)),
        }
    }

    /// Full-screen Settings list (US-APP-01/02/06).
    fn render_settings_screen(&self, f: &mut Frame) {
        let area = f.area();
        let block = Block::default()
            .title(if crate::auth::dev_mode_enabled() {
                " ⚙️ Settings [DEV] "
            } else {
                " ⚙️ Settings "
            })
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.primary))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        let mut lines: Vec<Line> = vec![Line::from(Span::styled(
            "General",
            Style::default()
                .fg(self.ui.theme.highlight)
                .add_modifier(Modifier::BOLD),
        ))];
        if let Some(err) = &self.settings.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default()
                    .fg(self.ui.theme.error)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        for (row, name) in SETTINGS_ROWS.iter().enumerate() {
            let selected = row == self.settings.selected;
            let label = match *name {
                "editor" => "Text editor".to_string(),
                "page_size" => "List page size".to_string(),
                "theme" => "Theme".to_string(),
                other => format!("Key: {}", other),
            };
            let value = if Some(row) == self.settings.editing_text {
                format!("{}|", self.settings.buffer)
            } else if Some(row) == self.settings.capturing_key {
                "press a key… (Esc cancels)".to_string()
            } else if *name == "editor" && self.config.general.editor.is_empty() {
                "(empty = $EDITOR)".to_string()
            } else if *name == "theme" && selected {
                format!("◄ {} ►", self.setting_value(row))
            } else {
                self.setting_value(row)
            };
            let prefix = if selected { "▶ " } else { "  " };
            let style = if selected {
                Style::default()
                    .fg(self.ui.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui.theme.fg)
            };
            lines.push(Line::from(Span::styled(
                format!("{}{:<22} {}", prefix, label, value),
                style,
            )));
        }

        f.render_widget(Paragraph::new(lines), chunks[0]);

        let dev_hint = if crate::auth::dev_mode_enabled() {
            " · d: DEV wipe users"
        } else {
            ""
        };
        let footer = if self.settings.dirty {
            format!(
                "↑↓ navigate · Enter edit · ←/→ theme · a: advanced · Ctrl+S SAVE · Esc back{}  (unsaved)",
                dev_hint
            )
        } else {
            format!(
                "↑↓ navigate · Enter edit · ←/→ theme · a: advanced · Ctrl+S save · Esc back{}",
                dev_hint
            )
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                footer,
                Style::default().fg(self.ui.theme.border),
            ))
            .alignment(Alignment::Center),
            chunks[1],
        );
    }

    /// Full-screen Advanced settings: visual theme colors + system options.
    fn render_advanced_screen(&self, f: &mut Frame) {
        let area = f.area();
        let block = Block::default()
            .title(" ⚙️ Advanced Settings ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.secondary))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        let mut lines: Vec<Line> = vec![Line::from(Span::styled(
            "Theme colors  (←/→ cycle, Backspace resets)",
            Style::default()
                .fg(self.ui.theme.highlight)
                .add_modifier(Modifier::BOLD),
        ))];
        if let Some(err) = &self.advanced.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default()
                    .fg(self.ui.theme.error)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        for (row, name) in ADVANCED_ROWS.iter().enumerate() {
            if row == ADVANCED_COLOR_ROW_COUNT {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "System",
                    Style::default()
                        .fg(self.ui.theme.highlight)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let selected = row == self.advanced.selected;
            let label = match *name {
                "fg" => "Text (fg)".to_string(),
                "bg" => "Background".to_string(),
                "accent" => "Accent".to_string(),
                "status_bg" => "Status bar bg".to_string(),
                "db_path" => "Database path".to_string(),
                "api_bind" => "API bind address".to_string(),
                "busy_timeout" => "DB busy timeout (ms)".to_string(),
                other => format!("{} color", other),
            };
            let raw = self.advanced_value(row);
            let value = if self.advanced.editing && selected {
                format!("{}|", self.advanced.buffer)
            } else if is_advanced_optional_color(row) && raw.is_empty() {
                "(preset)".to_string()
            } else {
                raw.clone()
            };
            let prefix = if selected { "▶ " } else { "  " };
            let style = if selected {
                Style::default()
                    .fg(self.ui.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.ui.theme.fg)
            };
            let mut spans = vec![Span::styled(format!("{}{:<22}", prefix, label), style)];
            if is_advanced_color_row(row) {
                // Visual swatch: shows the actual color next to its name
                let swatch_style = if self.advanced.editing && selected {
                    Style::default().fg(self.ui.theme.fg)
                } else {
                    Style::default().fg(crate::config::parse_color(&raw))
                };
                spans.push(Span::styled("██ ", swatch_style));
            }
            spans.push(Span::styled(value, style));
            lines.push(Line::from(spans));
        }

        f.render_widget(Paragraph::new(lines), chunks[0]);

        f.render_widget(
            Paragraph::new(Span::styled(
                "↑↓ navigate · ←/→ cycle color · Enter edit · Backspace reset · Ctrl+S save · Esc back",
                Style::default().fg(self.ui.theme.border),
            ))
            .alignment(Alignment::Center),
            chunks[1],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Entity;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn ctrl_s() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    }

    async fn test_app() -> ModernApp {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ModernApp::new(
            std::sync::Arc::new(pool),
            crate::config::AppConfig::default(),
        )
    }

    /// Like `test_app`, but with migrations applied so repository calls work.
    async fn test_app_db() -> ModernApp {
        let pool = std::sync::Arc::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        crate::db::run_migrations(&pool).await.unwrap();
        ModernApp::new(pool, crate::config::AppConfig::default())
    }

    // ── Field cycling ───────────────────────────────────────────────────────

    #[test]
    fn cycle_field_wraps_forward_and_backward() {
        assert_eq!(cycle_field(0, 5, true), 1);
        assert_eq!(cycle_field(4, 5, true), 0); // wraps at the end
        assert_eq!(cycle_field(0, 5, false), 4); // wraps at the start
        assert_eq!(cycle_field(2, 5, false), 1);
        assert_eq!(cycle_field(0, 0, true), 0); // degenerate: no fields
    }

    // ── Overlay routing (BDD style, no rendering needed) ────────────────────

    /// Scenario: cancel a new command
    /// Given the command form is open, when Esc is pressed, then the form closes
    /// and nothing is saved.
    #[tokio::test]
    async fn given_command_form_open_when_esc_then_form_closes() {
        let mut app = test_app().await;
        app.ui.state = AppState::Knowledge;
        app.command_form.mode = Some(FormMode::Create);

        app.handle_key(key(KeyCode::Esc)).await;

        assert!(app.command_form.mode.is_none());
    }

    /// Scenario: type into a form field
    /// Given the command form is open on Name, when characters are typed,
    /// then they land in the Name field and the form stays open.
    #[tokio::test]
    async fn given_command_form_open_when_typed_then_goes_into_focused_field() {
        let mut app = test_app().await;
        app.ui.state = AppState::Knowledge;
        app.command_form.mode = Some(FormMode::Create);

        app.handle_key(key(KeyCode::Char('h'))).await;
        app.handle_key(key(KeyCode::Char('i'))).await;

        assert_eq!(app.command_form.name, "hi");
        assert!(
            app.command_form.mode.is_some(),
            "typing must not close the form"
        );
    }

    /// Scenario: Tab moves to the next form field
    /// Given the command form on Name, when Tab is pressed, then Type is focused.
    #[tokio::test]
    async fn given_command_form_on_name_when_tab_then_type_focused() {
        let mut app = test_app().await;
        app.command_form.mode = Some(FormMode::Create);

        app.handle_key(key(KeyCode::Tab)).await;

        assert_eq!(app.command_form.focused_field, 1);
    }

    /// Scenario: cancel with Esc inside the visual builder
    /// Given the visual builder is open, when Esc is pressed, then it closes.
    #[tokio::test]
    async fn given_visual_builder_open_when_esc_then_closes() {
        let mut app = test_app().await;
        app.ui.state = AppState::Workflows;
        app.visual_form = Some(VisualWorkflowState::default());

        app.handle_key(key(KeyCode::Esc)).await;

        assert!(app.visual_form.is_none());
    }

    /// Scenario: add a saved command as a visual step
    /// Given the command picker is open with one command, when Enter is pressed,
    /// then the command becomes a step and the picker closes.
    #[tokio::test]
    async fn given_picker_open_when_enter_then_command_becomes_step() {
        let mut app = test_app().await;
        app.visual_form = Some(VisualWorkflowState {
            focused_field: VisualField::Steps,
            picker: Some(CommandPickerState {
                items: vec![Entity {
                    id: "e1".into(),
                    name: "backup".into(),
                    description: None,
                    content: Some("tar -czf backup.tar.gz ~".into()),
                    type_id: "cmd".into(),
                    project_id: None,
                    metadata_json: None,
                    created_at: String::new(),
                    updated_at: String::new(),
                    parent_id: None,
                }],
                selected: 0,
            }),
            ..Default::default()
        });

        app.handle_key(key(KeyCode::Enter)).await;

        let visual = app.visual_form.as_ref().unwrap();
        assert!(visual.picker.is_none(), "picker closes after picking");
        assert_eq!(visual.steps.len(), 1);
        assert_eq!(visual.steps[0].name, "backup");
        assert_eq!(visual.steps[0].script, "tar -czf backup.tar.gz ~");
    }

    /// Scenario: confirm a delete
    /// Given a delete confirmation popup, when Enter is pressed, then the
    /// confirmation is consumed (a missing id surfaces a status, not a panic).
    #[tokio::test]
    async fn given_confirm_delete_open_when_enter_then_confirmation_consumed() {
        let mut app = test_app().await;
        app.confirm_delete = Some(ConfirmDelete {
            id: "no-such-id".into(),
            label: "ghost".into(),
            kind: DeleteKind::Entity,
            path: None,
            delete_folder: false,
        });

        app.handle_key(key(KeyCode::Enter)).await;

        assert!(app.confirm_delete.is_none());
        assert!(app.status_message.is_some());
    }

    /// Scenario: close a run result popup
    /// Given a run result is shown, when Enter is pressed, then it closes.
    #[tokio::test]
    async fn given_run_result_open_when_enter_then_closes() {
        let mut app = test_app().await;
        app.run_result = Some(RunResult {
            title: "Run: ls".into(),
            success: true,
            text: "ok".into(),
        });

        app.handle_key(key(KeyCode::Enter)).await;

        assert!(app.run_result.is_none());
    }

    /// Scenario: save a visual workflow with no name
    /// Given the visual builder has steps but no name, when Ctrl+S is pressed,
    /// then the builder stays open with an error message.
    #[tokio::test]
    async fn given_visual_without_name_when_ctrl_s_then_error_shown() {
        let mut app = test_app().await;
        app.visual_form = Some(VisualWorkflowState {
            steps: vec![VisualStep::command("e1", "step", "print('hi')")],
            ..Default::default()
        });

        app.handle_key(ctrl_s()).await;

        let visual = app.visual_form.as_ref().unwrap();
        assert_eq!(visual.error_message.as_deref(), Some("Name is required"));
    }

    // ── Settings screen (US-APP-01/02/06) ───────────────────────────────────

    /// Scenario: the selected row in the visual builder shows a cursor
    /// marker, so selection is visible regardless of theme colors (US-APP-01).
    #[tokio::test]
    async fn given_visual_builder_when_rendered_then_selected_step_has_cursor_marker() {
        let mut app = test_app().await;
        app.visual_form = Some(VisualWorkflowState {
            focused_field: VisualField::Steps,
            selected_step: 1,
            steps: vec![
                VisualStep::command("e1", "first", "print('one')"),
                VisualStep::command("e2", "second", "print('two')"),
            ],
            ..Default::default()
        });

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        // The selected step is prefixed with the cursor, the other is not
        assert!(
            text.contains("\u{276f} 2. [CMD] second"),
            "selected row must show the cursor marker"
        );
        assert!(
            !text.contains("\u{276f} 1. [CMD] first"),
            "unselected row must not show the cursor"
        );
    }

    /// Scenario: the node editor shows WHICH kind is active — the active one
    /// is bracketed inside the kind list (US-FUT-07).
    #[tokio::test]
    async fn given_node_editor_when_rendered_then_active_kind_is_bracketed() {
        let mut app = test_app().await;
        let mut steps = vec![VisualStep::command("e1", "build", "print('one')")];
        let mut gate = VisualStep::logic(VisualNodeKind::Or, "gate1");
        gate.inputs = vec!["build".into()];
        steps.push(gate);
        app.visual_form = Some(VisualWorkflowState {
            focused_field: VisualField::Steps,
            selected_step: 1,
            steps,
            node_editor: Some(LogicNodeEditor::load(
                1,
                &VisualStep::logic(VisualNodeKind::Or, "gate1"),
            )),
            ..Default::default()
        });

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 30)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        // The active kind is bracketed, the others are plain
        assert!(text.contains("[OR]"), "active kind must be bracketed");
        assert!(
            text.contains(" AND ") && text.contains(" XOR "),
            "other kinds stay visible as plain labels"
        );
        assert!(
            !text.contains("[AND]") && !text.contains("[XOR]"),
            "only the active kind may be bracketed"
        );
        // The focused Kind row also shows the cursor
        assert!(
            text.contains('\u{276f}'),
            "focused row must show the cursor"
        );
    }

    /// Scenario: navigate the Settings rows
    /// Given the Settings screen, when arrow keys are pressed, then the
    /// selection moves and clamps at the ends.
    #[tokio::test]
    async fn given_settings_open_when_arrows_then_selection_moves_and_clamps() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        assert_eq!(app.settings.selected, 0);

        app.handle_key(key(KeyCode::Up)).await; // clamp at top
        assert_eq!(app.settings.selected, 0);
        app.handle_key(key(KeyCode::Down)).await;
        app.handle_key(key(KeyCode::Down)).await;
        assert_eq!(app.settings.selected, 2);
        app.handle_key(key(KeyCode::Up)).await;
        assert_eq!(app.settings.selected, 1);
    }

    /// Scenario: change the text editor in Settings
    /// Given the editor row, when Enter is pressed and a new value typed,
    /// then the config is updated and marked dirty.
    #[tokio::test]
    async fn given_settings_when_editor_edited_then_config_updated() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        app.settings.selected = 0; // editor row

        app.handle_key(key(KeyCode::Enter)).await; // start editing
        for c in "nvim".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await; // apply

        assert_eq!(app.config.general.editor, "nvim");
        assert!(app.settings.dirty);
        assert!(app.settings.editing_text.is_none());
    }

    /// Scenario: rebind a keybinding in Settings and use it immediately
    /// Given the create row, when a new key is captured, then the binding is
    /// stored and the new key triggers the action on the dashboard.
    #[tokio::test]
    async fn given_capturing_key_when_pressed_then_binding_updates_and_works() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        app.settings.selected = 7; // create row

        app.handle_key(key(KeyCode::Enter)).await; // start capture
        assert!(app.settings.capturing_key.is_some());

        app.handle_key(key(KeyCode::Char('K'))).await; // bind K

        assert_eq!(app.config.keybindings.get("create"), "K");
        assert!(app.settings.capturing_key.is_none());

        // The rebound key opens the create form on the dashboard
        app.ui.state = AppState::Knowledge;
        app.handle_key(key(KeyCode::Char('K'))).await;
        assert!(app.command_form.mode.is_some());
    }

    /// Scenario: cycling the theme applies it live
    /// Given the Theme row, when ←/→ is pressed, then the preset cycles and the
    /// live theme colors change (and back).
    #[tokio::test]
    async fn given_settings_when_theme_cycled_then_applied_live() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        app.settings.selected = SETTINGS_THEME_ROW; // theme row
        assert_eq!(app.config.theme.name, "dark");

        app.handle_key(key(KeyCode::Right)).await; // dark -> light
        assert_eq!(app.config.theme.name, "light");
        assert_ne!(app.ui.theme.bg, ModernTheme::default().bg);

        app.handle_key(key(KeyCode::Left)).await; // back to dark
        assert_eq!(app.config.theme.name, "dark");
        assert_eq!(app.ui.theme.bg, ModernTheme::default().bg);
    }

    /// Scenario: invalid page size is rejected
    #[tokio::test]
    async fn given_invalid_page_size_when_applied_then_error_and_still_editing() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        app.settings.selected = 1; // page_size row

        app.handle_key(key(KeyCode::Enter)).await; // start editing (buffer = "15")
        app.handle_key(key(KeyCode::Char('x'))).await; // "15x"
        app.handle_key(key(KeyCode::Enter)).await; // apply -> rejected

        assert!(
            app.settings.error.is_some(),
            "invalid value must show an error"
        );
        assert!(app.settings.editing_text.is_some(), "stays in edit mode");
        assert_eq!(app.config.tui.page_size, 15, "config unchanged");

        // Esc cancels editing
        app.handle_key(key(KeyCode::Esc)).await;
        assert!(app.settings.editing_text.is_none());
    }

    /// Scenario: Ctrl+S persists Settings to disk
    /// Given a customized config, when Ctrl+S is pressed in Settings, then a
    /// config.toml is written that loads back with the new values.
    #[tokio::test]
    async fn given_customized_config_when_ctrl_s_then_config_persisted() {
        let mut app = test_app().await;
        let path =
            std::env::temp_dir().join(format!("tui-op-hub-savetest-{}.toml", uuid::Uuid::new_v4()));
        app.config_path = path.clone();
        app.config.general.editor = "helix".to_string();
        app.config.tui.page_size = 40;
        app.ui.state = AppState::Settings;

        app.handle_key(ctrl_s()).await;

        assert!(path.exists(), "config file must be written");
        let loaded = crate::config::AppConfig::load(&path).unwrap();
        assert_eq!(loaded.general.editor, "helix");
        assert_eq!(loaded.tui.page_size, 40);
        let _ = std::fs::remove_file(&path);
    }

    // ── Advanced mode (visual config) ───────────────────────────────────────

    /// Scenario: open Advanced mode from Settings
    /// Given the Settings screen, when `a` is pressed, then the advanced
    /// visual config screen opens (and Esc returns to Settings).
    #[tokio::test]
    async fn given_settings_when_a_pressed_then_advanced_opens() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;

        app.handle_key(key(KeyCode::Char('a'))).await;
        assert!(app.advanced.active);

        app.handle_key(key(KeyCode::Esc)).await;
        assert!(!app.advanced.active);
        assert_eq!(app.ui.state, AppState::Settings, "stays in Settings");
    }

    /// Scenario: cycle a color in Advanced mode
    /// Given the bg row in Advanced mode, when → is pressed, then the config
    /// bg color changes and the live theme is refreshed.
    #[tokio::test]
    async fn given_advanced_when_color_cycled_then_config_and_theme_update() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        app.handle_key(key(KeyCode::Char('a'))).await; // open advanced
        app.advanced.selected = 1; // bg row
        let before = app.config.theme.bg.clone();

        app.handle_key(key(KeyCode::Right)).await;

        let after = app.config.theme.bg.clone();
        assert_ne!(before, after, "cycling must change the color");
        assert!(app.settings.dirty);
        // Live theme reflects the new bg
        assert_eq!(app.ui.theme.bg, crate::config::parse_color(&after));
    }

    /// Scenario: set a hex color via text editing in Advanced mode
    /// Given the primary row, when edited to a hex value, then the optional
    /// override is stored and applied to the live theme.
    #[tokio::test]
    async fn given_advanced_when_hex_color_edited_then_override_applies() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        app.handle_key(key(KeyCode::Char('a'))).await;
        app.advanced.selected = 4; // primary row (optional override)

        app.handle_key(key(KeyCode::Backspace)).await; // clear to preset
        assert!(app.config.theme.primary.is_none());

        app.handle_key(key(KeyCode::Enter)).await; // start editing
        for c in "#3366ff".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await; // apply

        assert_eq!(
            app.config.theme.primary.as_deref(),
            Some("#3366ff"),
            "hex override stored"
        );
        assert_eq!(app.ui.theme.primary, crate::config::parse_color("#3366ff"));
    }

    /// Scenario: edit and validate the busy timeout in Advanced mode
    /// Given the busy_timeout row, when a non-number is applied, then an error
    /// shows and the value is unchanged; a valid number applies.
    #[tokio::test]
    async fn given_advanced_when_busy_timeout_edited_then_validated() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        app.handle_key(key(KeyCode::Char('a'))).await;
        app.advanced.selected = 13; // busy_timeout row

        app.handle_key(key(KeyCode::Enter)).await; // edit ("5000")
        app.handle_key(key(KeyCode::Char('x'))).await; // "5000x"
        app.handle_key(key(KeyCode::Enter)).await; // rejected
        assert!(app.advanced.error.is_some());
        assert_eq!(app.config.database.busy_timeout_ms, 5000);

        // Fix the value: backspace removes 'x', then apply
        app.handle_key(key(KeyCode::Backspace)).await;
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(app.advanced.error.is_none());
        assert_eq!(app.config.database.busy_timeout_ms, 5000);
    }

    #[tokio::test]
    async fn which_program_finds_shell_and_rejects_garbage() {
        // `sh` must exist on any Unix build host
        assert!(which_program("sh"));
        assert!(!which_program("definitely-not-a-real-program-xyz-42"));
        // Absolute path form
        assert!(which_program("/bin/sh") || !std::path::Path::new("/bin/sh").is_file());
    }

    #[tokio::test]
    async fn terminal_window_command_returns_valid_shape() {
        match terminal_window_command() {
            Some((prog, args)) => {
                assert!(!prog.is_empty());
                // Last arg must be the shell to run
                let last = args.last().expect("at least shell arg");
                assert!(last.contains("sh"), "last arg should be a shell: {}", last);
            }
            None => {
                // No emulator on this machine is acceptable (headless CI);
                // the TUI shows a status message in that case.
            }
        }
    }
    // ── Entity tabs (Commands / Apps / Scripts) + project detail ────────────

    async fn seed_typed_entities(app: &mut ModernApp) {
        for (name, ty) in [
            ("list-cmd", "cmd"),
            ("list-script", "script"),
            ("list-app", "app"),
        ] {
            repository::create_entity(
                &*app.pool,
                &crate::models::CreateEntity {
                    name: name.to_string(),
                    description: None,
                    content: Some("echo hi".to_string()),
                    type_id: ty.to_string(),
                    project_id: None,
                    tags: None,
                    metadata_json: None,
                },
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn given_entities_of_three_types_when_filter_changed_then_list_matches_filter() {
        let mut app = test_app_db().await;
        seed_typed_entities(&mut app).await;
        app.ui.state = AppState::Knowledge;

        // All: 3 items (cmd + app + script)
        app.kb_filter = KbFilter::All;
        app.fetch_knowledge().await.unwrap();
        assert_eq!(app.commands_list.items.len(), 3);

        // Commands only
        app.kb_filter = KbFilter::Cmd;
        app.fetch_knowledge().await.unwrap();
        assert_eq!(app.commands_list.items.len(), 1);
        assert_eq!(app.commands_list.items[0].type_id, "cmd");

        // Apps only
        app.kb_filter = KbFilter::App;
        app.fetch_knowledge().await.unwrap();
        assert_eq!(app.commands_list.items.len(), 1);
        assert_eq!(app.commands_list.items[0].type_id, "app");

        // Scripts only
        app.kb_filter = KbFilter::Script;
        app.fetch_knowledge().await.unwrap();
        assert_eq!(app.commands_list.items.len(), 1);
        assert_eq!(app.commands_list.items[0].type_id, "script");

        // `f` cycles the filter and refetches (All -> Cmd -> App)
        app.kb_filter = KbFilter::All;
        app.handle_key(key(KeyCode::Char('f'))).await;
        assert_eq!(app.kb_filter, KbFilter::Cmd);
        assert_eq!(app.commands_list.items.len(), 1);
        app.handle_key(key(KeyCode::Char('f'))).await;
        assert_eq!(app.kb_filter, KbFilter::App);
        assert_eq!(app.commands_list.items.len(), 1);
    }

    #[tokio::test]
    async fn given_any_tab_when_digit_pressed_then_correct_state_and_data() {
        let mut app = test_app_db().await;
        seed_typed_entities(&mut app).await;
        app.ui.state = AppState::Dashboard;

        // Mapping: 2 Knowledge, 3 Projects, 4 Workflows, 5 Secrets, 0 Settings
        app.handle_key(key(KeyCode::Char('2'))).await;
        assert_eq!(app.ui.state, AppState::Knowledge);
        assert_eq!(app.commands_list.items.len(), 3, "All filter");

        // Cycle the filter to Commands and re-fetch via f
        app.handle_key(key(KeyCode::Char('f'))).await;
        assert_eq!(app.commands_list.items.len(), 1);
        assert_eq!(app.commands_list.items[0].type_id, "cmd");

        app.handle_key(key(KeyCode::Char('3'))).await;
        assert_eq!(app.ui.state, AppState::Projects);
        app.handle_key(key(KeyCode::Char('4'))).await;
        assert_eq!(app.ui.state, AppState::Workflows);
        app.handle_key(key(KeyCode::Char('5'))).await;
        assert_eq!(app.ui.state, AppState::Secrets);
        app.handle_key(key(KeyCode::Char('0'))).await;
        assert_eq!(app.ui.state, AppState::Settings);
    }

    #[tokio::test]
    async fn given_knowledge_filter_when_creating_then_type_preset_to_filter() {
        let mut app = test_app().await;
        app.ui.state = AppState::Knowledge;

        // All filter -> cmd (index 0)
        app.handle_key(key(KeyCode::Char('n'))).await;
        assert!(app.command_form.mode.is_some());
        assert_eq!(app.command_form.entity_type, 0);
        app.handle_key(key(KeyCode::Esc)).await;

        // App filter -> app (index 2)
        app.kb_filter = KbFilter::App;
        app.handle_key(key(KeyCode::Char('n'))).await;
        assert_eq!(app.command_form.entity_type, 2);
        app.handle_key(key(KeyCode::Esc)).await;

        // Script filter -> script (index 1)
        app.kb_filter = KbFilter::Script;
        app.handle_key(key(KeyCode::Char('n'))).await;
        assert_eq!(app.command_form.entity_type, 1);
    }

    #[tokio::test]
    async fn given_project_with_entities_when_enter_pressed_then_detail_lists_them() {
        let mut app = test_app_db().await;
        let project = repository::create_project(
            &*app.pool,
            &crate::models::CreateProject {
                name: "webapp".to_string(),
                description: Some("demo".to_string()),
            },
        )
        .await
        .unwrap();
        for (name, ty) in [("p-cmd", "cmd"), ("p-script", "script")] {
            repository::create_entity(
                &*app.pool,
                &crate::models::CreateEntity {
                    name: name.to_string(),
                    description: None,
                    content: None,
                    type_id: ty.to_string(),
                    project_id: Some(project.id.clone()),
                    tags: None,
                    metadata_json: None,
                },
            )
            .await
            .unwrap();
        }
        // One unrelated entity that must NOT appear in the detail view
        repository::create_entity(
            &*app.pool,
            &crate::models::CreateEntity {
                name: "other".to_string(),
                description: None,
                content: None,
                type_id: "cmd".to_string(),
                project_id: None,
                tags: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();

        app.ui.state = AppState::Projects;
        app.fetch_projects().await.unwrap();
        app.handle_key(key(KeyCode::Enter)).await;

        let detail = app.project_detail.as_ref().expect("detail should open");
        assert_eq!(detail.project.id, project.id);
        assert_eq!(detail.entities.len(), 2);
        assert!(detail.entities.iter().all(|e| e.project_id.is_some()));

        // Esc closes the detail view
        app.handle_key(key(KeyCode::Esc)).await;
        assert!(app.project_detail.is_none());
    }

    // ── Import popup (path input) ───────────────────────────────────────────

    #[tokio::test]
    async fn given_import_popup_when_path_entered_then_file_imported() {
        let mut app = test_app_db().await;
        app.ui.state = AppState::Knowledge;

        // Write a bare AI-style entity array to a temp file
        let dir = std::env::temp_dir().join(format!("tui-op-hub-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ai-bundle.json");
        std::fs::write(
            &file,
            r#"[{"name": "popup cmd", "type_id": "cmd", "content": "echo hi"}]"#,
        )
        .unwrap();

        // Open the popup (pre-filled with the default path) and type the temp path
        app.handle_key(key(KeyCode::Char('I'))).await;
        assert!(app.import_input.is_some());
        // Clear the pre-filled default so the typed path stands alone
        app.import_input = Some(String::new());
        for c in file.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;

        assert!(app.import_input.is_none(), "popup closed");
        app.fetch_knowledge().await.unwrap();
        assert!(app
            .commands_list
            .items
            .iter()
            .any(|e| e.name == "popup cmd"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn given_path_with_tilde_when_expanded_then_home_substituted() {
        std::env::set_var(
            "HOME",
            std::env::var("HOME").unwrap_or_else(|_| "/home/test".to_string()),
        );
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/x.json"), format!("{home}/x.json"));
        assert_eq!(expand_tilde("/abs/path"), "/abs/path");
        assert_eq!(expand_tilde("relative"), "relative");
    }

    // ── Project creation flow (N workspace / n register) ───────────────────

    #[tokio::test]
    async fn given_new_workspace_form_when_created_then_path_stored_in_db() {
        let mut app = test_app_db().await;
        app.ui.state = AppState::Projects;

        // Open the workspace form (N) and fill the name
        app.handle_key(key(KeyCode::Char('N'))).await;
        assert!(app.new_project_open);
        for c in "flowproj".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        // Use the Generic kind (no venv/cargo scaffolding => fast, offline)
        app.new_project_kind = project_workspace::ProjectKind::all()
            .iter()
            .position(|k| k.name() == "generic")
            .unwrap();

        // Create into a temp parent (injectable parent for tests)
        let parent = std::env::temp_dir().join(format!("tui-op-hub-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        app.create_new_project_in(&parent).await;

        assert!(!app.new_project_open, "form closed on success");
        let created = parent.join("flowproj");
        assert!(created.is_dir(), "workspace directory created");

        // The DB row carries the workspace path
        let projects = repository::list_projects(&*app.pool).await.unwrap();
        let p = projects
            .iter()
            .find(|p| p.name == "flowproj")
            .expect("saved");
        assert_eq!(p.path.as_deref(), Some(created.to_string_lossy().as_ref()));

        let _ = std::fs::remove_dir_all(&parent);
    }

    #[tokio::test]
    async fn given_new_project_form_when_template_picked_then_preview_and_scaffold_applied() {
        // US-PLG-14/15 + US-PROJ-08: plugin ships a template; the picker is
        // optional (none by default); preview + customize before apply.
        let mut app = test_app_db().await;
        app.ui.state = AppState::Projects;

        // Plugin with one template, discoverable via the app's manager
        let plugin_root =
            std::env::temp_dir().join(format!("tuihub-tpl-{}-pick", std::process::id()));
        let _ = std::fs::remove_dir_all(&plugin_root);
        std::fs::create_dir_all(plugin_root.join("py-dev").join("templates")).unwrap();
        std::fs::write(
            plugin_root.join("py-dev").join("plugin.toml"),
            "id = 'py-dev'\nname = 'Python Dev'\nversion = '0.1.0'\nplugin_type = 'lua'\nentry_point = 'main.lua'\nrequired_capabilities = []\n",
        )
        .unwrap();
        std::fs::write(
            plugin_root.join("py-dev").join("templates").join("python.toml"),
            "[template]\nname = \"Python dev\"\ngit_init = false\n\n[[files]]\npath = \"STARTER.md\"\ncontent = \"# {{project_name}}\"\n\n[[files]]\npath = \"notes.md\"\ncontent = \"notes\"\n",
        )
        .unwrap();
        app.plugins = std::sync::Arc::new(crate::plugin::PluginManager::new(
            app.pool.clone(),
            plugin_root.clone(),
        ));

        // Open the form (N) — templates load, selection defaults to none
        app.handle_key(key(KeyCode::Char('N'))).await;
        assert_eq!(app.templates.len(), 1);
        assert!(app.new_project_template.is_none(), "default is none");
        for c in "tplproj".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.new_project_kind = project_workspace::ProjectKind::all()
            .iter()
            .position(|k| k.name() == "generic")
            .unwrap();

        // Field 3: Right selects the first template, Enter opens the preview
        app.new_project_field_idx = 3;
        app.handle_key(key(KeyCode::Right)).await;
        assert_eq!(app.new_project_template, Some(0));
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(app.template_preview.is_some(), "preview opens");
        // Visible in the framebuffer
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Template: Python dev"));

        // Customize: exclude the second file, edit the first one's content
        app.handle_key(key(KeyCode::Down)).await;
        app.handle_key(key(KeyCode::Char('x'))).await;
        app.handle_key(key(KeyCode::Up)).await;
        app.handle_key(key(KeyCode::Char('e'))).await;
        for c in "edited".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await; // save content
        app.handle_key(key(KeyCode::Esc)).await; // close preview, keep edits
        assert!(app.template_preview.is_none());
        // `e` opens the editor with the current content; typing appends
        assert_eq!(
            app.templates[0].files[0].content, "# {{project_name}}edited",
            "content edit must persist"
        );
        assert_eq!(app.templates[0].files.len(), 1, "excluded file dropped");

        // Create — the customized template lands in the new workspace
        let parent = std::env::temp_dir().join(format!("tuihub-tpl-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        app.create_new_project_in(&parent).await;
        let created = parent.join("tplproj");
        assert_eq!(
            std::fs::read_to_string(created.join("STARTER.md")).unwrap(),
            "# tplprojedited",
            "edited template content applied"
        );
        assert!(
            !created.join("notes.md").exists(),
            "excluded file must not be created"
        );
        assert!(
            app.status_message
                .as_deref()
                .unwrap_or_default()
                .contains("template 'Python dev'"),
            "status reports the template application"
        );
        let _ = std::fs::remove_dir_all(&parent);
        let _ = std::fs::remove_dir_all(&plugin_root);
    }

    #[tokio::test]
    async fn given_project_delete_when_folder_toggled_then_workspace_removed() {
        // US-PROJ: DB row deletion never touches the disk unless the user
        // explicitly toggles the folder option with `f` in the confirm popup.
        let mut app = test_app_db().await;
        app.ui.state = AppState::Projects;
        let parent = std::env::temp_dir().join(format!("tuihub-del-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        let ws = parent.join("delproj");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("f.txt"), "x").unwrap();

        let project = repository::create_project(
            &*app.pool,
            &CreateProject {
                name: "delproj".into(),
                description: None,
            },
        )
        .await
        .unwrap();
        repository::set_project_path(&*app.pool, &project.id, Some(&ws.to_string_lossy()))
            .await
            .unwrap();

        app.confirm_delete = Some(ConfirmDelete {
            id: project.id.clone(),
            label: "delproj".into(),
            kind: DeleteKind::Project,
            path: Some(ws.to_string_lossy().to_string()),
            delete_folder: false,
        });

        // Default OFF: plain Enter keeps the folder
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(ws.exists(), "folder must survive without the toggle");
        let projects = repository::list_projects(&*app.pool).await.unwrap();
        assert!(projects.iter().all(|p| p.id != project.id), "row deleted");

        // Second round with the toggle: `f` then Enter removes the folder
        let project2 = repository::create_project(
            &*app.pool,
            &CreateProject {
                name: "delproj2".into(),
                description: None,
            },
        )
        .await
        .unwrap();
        let ws2 = parent.join("delproj2");
        std::fs::create_dir_all(&ws2).unwrap();
        app.confirm_delete = Some(ConfirmDelete {
            id: project2.id.clone(),
            label: "delproj2".into(),
            kind: DeleteKind::Project,
            path: Some(ws2.to_string_lossy().to_string()),
            delete_folder: false,
        });
        app.handle_key(key(KeyCode::Char('f'))).await;
        assert!(app.confirm_delete.as_ref().unwrap().delete_folder);
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(!ws2.exists(), "toggled delete must remove the folder");
        assert!(
            app.status_message
                .as_deref()
                .unwrap_or_default()
                .contains("folder"),
            "status reports the folder removal"
        );
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[tokio::test]
    async fn given_existing_folder_when_ctrl_o_then_project_merges_into_it() {
        let mut app = test_app_db().await;
        app.ui.state = AppState::Projects;
        app.handle_key(key(KeyCode::Char('N'))).await;
        for c in "mergeproj".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.new_project_kind = project_workspace::ProjectKind::all()
            .iter()
            .position(|k| k.name() == "generic")
            .unwrap();

        let parent = std::env::temp_dir().join(format!("tuihub-mrg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        // Inject the temp parent so Ctrl+S/Ctrl+O stay out of $HOME
        app.new_project_parent = Some(parent.clone());
        // The target folder already exists with user content
        std::fs::create_dir_all(parent.join("mergeproj")).unwrap();
        std::fs::write(parent.join("mergeproj").join("keep.txt"), "user data").unwrap();

        // Ctrl+S: fails with a pointer at the merge option
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
            .await;
        assert!(app.new_project_open, "form stays open on conflict");
        assert!(
            app.new_project_error
                .as_deref()
                .unwrap_or_default()
                .contains("Ctrl+O"),
            "error must offer the merge option"
        );

        // Ctrl+O: merges into the existing folder, user content survives
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL))
            .await;
        assert!(!app.new_project_open, "form closed on success");
        assert!(parent.join("mergeproj").join("keep.txt").exists());
        assert!(
            app.status_message
                .as_deref()
                .unwrap_or_default()
                .contains("merged into existing folder"),
            "status reports the merge"
        );
        let projects = repository::list_projects(&*app.pool).await.unwrap();
        assert!(
            projects.iter().any(|p| p.name == "mergeproj"),
            "project row saved"
        );
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[tokio::test]
    async fn given_projects_tab_when_a_pressed_then_plugin_actions_available_and_runnable() {
        // US-PLG-13: an approved plugin's labeled action shows on the project
        // surface and runs against the selected project.
        let mut app = test_app_db().await;
        app.ui.state = AppState::Projects;

        let plugin_root =
            std::env::temp_dir().join(format!("tuihub-act-{}-plug", std::process::id()));
        let _ = std::fs::remove_dir_all(&plugin_root);
        let plugin_dir = plugin_root.join("ui.actions");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            "id = 'ui.actions'\nname = 'UI Actions'\nversion = '1.0.0'\nplugin_type = 'lua'\nentry_point = 'main.lua'\nrequired_capabilities = ['execute_commands', 'filesystem_write']\noptional_capabilities = []\n\n[[actions]]\nid = 'starter-files'\nlabel = 'Create starting files'\ncommand = 'create_starting_files'\nsurface = 'project'\nrequires = ['filesystem_write']\n",
        )
        .unwrap();
        std::fs::write(
            plugin_dir.join("main.lua"),
            "function create_starting_files(args)\n  local out = run_command('echo ' .. args[1] .. ' > ' .. args[2] .. '/starter.txt')\n  assert(out.success)\n  return 'starter.txt created'\nend\n",
        )
        .unwrap();
        app.plugins = std::sync::Arc::new(crate::plugin::PluginManager::new(
            app.pool.clone(),
            plugin_root.clone(),
        ));
        app.plugins
            .approve_plugin("ui.actions", "default")
            .await
            .unwrap();
        app.plugins.load_all_plugins().await.unwrap();

        // A project with a workspace path, selected in the list
        let ws = std::env::temp_dir().join(format!("tuihub-act-{}-ws", std::process::id()));
        std::fs::create_dir_all(&ws).unwrap();
        let project = repository::create_project(
            &*app.pool,
            &CreateProject {
                name: "actproj".into(),
                description: None,
            },
        )
        .await
        .unwrap();
        repository::set_project_path(&*app.pool, &project.id, Some(&ws.to_string_lossy()))
            .await
            .unwrap();
        app.fetch_projects().await.unwrap();
        app.projects_list.selected = 0;

        // `a` opens the actions popup with the plugin's action
        app.handle_key(key(KeyCode::Char('a'))).await;
        let panel = app.project_actions.as_ref().expect("actions panel opens");
        assert_eq!(panel.entries.len(), 1);
        assert_eq!(panel.entries[0].action.label, "Create starting files");

        // Visible in the framebuffer
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Actions: actproj"), "popup must be drawn");

        // Enter runs the action; the Lua fn writes into the project path
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(
            ws.join("starter.txt").exists(),
            "action must create the file in the project folder"
        );
        let panel = app.project_actions.as_ref().unwrap();
        assert!(panel.last_result.as_ref().unwrap().0, "result ok");
        assert!(
            app.status_message
                .as_deref()
                .unwrap_or_default()
                .contains("starter.txt created"),
            "status shows the action result"
        );
        let _ = std::fs::remove_dir_all(&ws);
        let _ = std::fs::remove_dir_all(&plugin_root);
    }

    #[tokio::test]
    async fn given_register_popup_when_path_entered_then_project_registered_with_path() {
        let mut app = test_app_db().await;
        app.ui.state = AppState::Projects;

        // An existing directory to register
        let dir = std::env::temp_dir().join(format!("tui-op-hub-reg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        app.handle_key(key(KeyCode::Char('n'))).await;
        assert!(app.register_input.is_some());
        app.register_input = Some(String::new()); // clear for exact path
        for c in dir.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;

        assert!(app.register_input.is_none(), "popup closed");
        let projects = repository::list_projects(&*app.pool).await.unwrap();
        let expected_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let p = projects
            .iter()
            .find(|p| p.name == expected_name)
            .expect("registered project saved");
        assert_eq!(p.path.as_deref(), Some(dir.to_string_lossy().as_ref()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_settings_when_digit_pressed_then_switches_to_that_tab() {
        let mut app = test_app_db().await;
        app.ui.state = AppState::Dashboard;

        app.handle_key(key(KeyCode::Char('0'))).await;
        assert_eq!(app.ui.state, AppState::Settings);

        // Digits must switch tabs from inside Settings (previously eaten by
        // the numpad-style navigation).
        app.handle_key(key(KeyCode::Char('3'))).await;
        assert_eq!(app.ui.state, AppState::Projects);

        app.handle_key(key(KeyCode::Char('7'))).await;
        assert_eq!(app.ui.state, AppState::Plugins);

        // Back to Settings, then to Workflows
        app.handle_key(key(KeyCode::Char('0'))).await;
        app.handle_key(key(KeyCode::Char('4'))).await;
        assert_eq!(app.ui.state, AppState::Workflows);
    }

    #[tokio::test]
    async fn given_sudo_prompt_open_when_typed_then_chars_stay_in_password() {
        let mut app = test_app_db().await;
        app.ui.state = AppState::Dashboard;

        // Simulate the R-flow: popup open, command pending
        app.sudo_password = Some(String::new());
        app.sudo_pending_command = Some("ls -la".to_string());

        for c in "hunter2".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }

        // All characters landed in the password buffer (not the list handler)
        assert_eq!(app.sudo_password.as_deref(), Some("hunter2"));
        assert_eq!(app.sudo_pending_command.as_deref(), Some("ls -la"));

        // Esc cancels and clears both
        app.handle_key(key(KeyCode::Esc)).await;
        assert!(app.sudo_password.is_none());
        assert!(app.sudo_pending_command.is_none());
    }
}

#[cfg(test)]
mod panel_tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn ctrl_s() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    }

    async fn test_app() -> ModernApp {
        let pool = std::sync::Arc::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        crate::db::run_migrations(&pool).await.unwrap();
        ModernApp::new(pool, crate::config::AppConfig::default())
    }

    // ── US-WF-09: workflow run cancel ────────────────────────────────────────

    #[tokio::test]
    async fn cancel_key_without_running_workflow_sets_status() {
        let mut app = test_app().await;
        app.ui.state = AppState::Workflows;
        app.handle_key(key(KeyCode::Char('X'))).await;
        assert!(app.running_workflow.is_none());
        assert!(app
            .status_message
            .as_deref()
            .unwrap_or("")
            .contains("No workflow"));
    }

    // ── US-SEC: admin user panel ─────────────────────────────────────────────

    #[tokio::test]
    async fn users_panel_requires_admin() {
        let mut app = test_app().await;
        app.ui.state = AppState::Settings;
        // The default profile in a fresh test DB is NOT an admin.
        app.handle_key(key(KeyCode::Char('u'))).await;
        assert!(app.users_panel.is_none());
        assert!(app
            .status_message
            .as_deref()
            .unwrap_or("")
            .contains("admin"));
    }

    #[tokio::test]
    async fn users_panel_admin_can_delete_user() {
        let mut app = test_app().await;
        // Make the current user an admin, add a second user
        let me = app.current_user_profile_id().await;
        repository::set_admin(&*app.pool, &me, true).await.unwrap();
        let other = repository::get_or_create_user(&*app.pool, "bob")
            .await
            .unwrap();

        app.ui.state = AppState::Settings;
        app.handle_key(key(KeyCode::Char('u'))).await;
        assert!(app.users_panel.is_some(), "admin should open the panel");

        // bob sorts before 'default' by username — already selected
        app.handle_key(key(KeyCode::Char('d'))).await; // arm delete
        app.handle_key(key(KeyCode::Enter)).await; // confirm
        assert!(!repository::list_user_profiles(&*app.pool)
            .await
            .unwrap()
            .iter()
            .any(|u| u.id == other.id));
    }

    // ── US-SSH-01..05: SSH host manager panel ────────────────────────────────

    #[tokio::test]
    async fn ssh_panel_create_connect_and_delete() {
        let mut app = test_app().await;
        app.ui.state = AppState::Secrets;
        app.handle_key(key(KeyCode::Char('H'))).await;
        assert!(app.ssh_panel.is_some());

        // Create a host through the form (n → type → Ctrl+S)
        app.handle_key(key(KeyCode::Char('n'))).await;
        for (field, text) in [
            (0usize, "prod"),
            (1, "10.1.1.9"),
            (2, "2222"),
            (3, "gerard"),
            (4, "~/.ssh/id_ed25519"),
        ] {
            if field == 2 {
                // Port starts pre-filled with 22 — clear it first
                app.handle_key(key(KeyCode::Backspace)).await;
                app.handle_key(key(KeyCode::Backspace)).await;
            }
            for ch in text.chars() {
                app.handle_key(key(KeyCode::Char(ch))).await;
            }
            if field < 4 {
                app.handle_key(key(KeyCode::Tab)).await;
            }
        }
        app.handle_key(ctrl_s()).await;

        let hosts = repository::list_ssh_hosts(&*app.pool).await.unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "prod");
        assert_eq!(hosts[0].port, 2222);
        assert_eq!(hosts[0].username.as_deref(), Some("gerard"));

        // Connect: sets the new-terminal command and closes the panel
        app.handle_key(key(KeyCode::Enter)).await;
        let (cmd, _cwd) = app
            .wants_terminal_cmd
            .take()
            .expect("connect should arm ssh");
        assert!(cmd.starts_with("ssh -p 2222"), "unexpected: {}", cmd);
        assert!(cmd.ends_with("gerard@10.1.1.9"));
        assert!(cmd.contains("-i ~/.ssh/id_ed25519"));
        assert!(app.ssh_panel.is_none());

        // Delete the host
        app.ui.state = AppState::Secrets;
        app.handle_key(key(KeyCode::Char('H'))).await;
        app.handle_key(key(KeyCode::Char('d'))).await;
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(repository::list_ssh_hosts(&*app.pool)
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ssh_form_connect_command_shapes() {
        let mut f = SshForm::new();
        assert!(f.connect_command().is_none(), "no hostname → no command");
        f.hostname = "host.example.com".into();
        assert_eq!(f.connect_command().unwrap(), "ssh -p 22 host.example.com");
        f.username = "root".into();
        f.port = "2200".into();
        f.key_path = "/keys/k".into();
        assert_eq!(
            f.connect_command().unwrap(),
            "ssh -i /keys/k -p 2200 root@host.example.com"
        );
    }
}

#[cfg(test)]
mod knowledge_nav_tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    async fn test_app() -> ModernApp {
        let pool = std::sync::Arc::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        crate::db::run_migrations(&pool).await.unwrap();
        ModernApp::new(pool, crate::config::AppConfig::default())
    }

    #[tokio::test]
    async fn default_tab_cycle_has_knowledge_and_settings_last() {
        let app = ModernApp::new(
            std::sync::Arc::new(
                sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_lazy("sqlite::memory:")
                    .unwrap(),
            ),
            crate::config::AppConfig::default(),
        );
        let cycle = app.tab_cycle();
        assert_eq!(cycle.len(), 8);
        assert_eq!(cycle[0], AppState::Dashboard);
        assert_eq!(cycle[1], AppState::Knowledge);
        assert_eq!(cycle[5], AppState::Configs);
        assert_eq!(cycle[cycle.len() - 1], AppState::Settings, "last tab");
    }

    #[tokio::test]
    async fn given_custom_tab_order_when_tab_pressed_then_follows_config() {
        let mut app = test_app().await;
        app.config.tui.tab_order = Some("dash,sec,set".to_string());

        app.ui.state = AppState::Dashboard;
        app.handle_key(key(KeyCode::Tab)).await;
        assert_eq!(app.ui.state, AppState::Secrets);
        app.handle_key(key(KeyCode::Tab)).await;
        assert_eq!(app.ui.state, AppState::Settings);
        // Inside Settings, Tab moves the settings row cursor (screen-local);
        // digits still jump tabs, so go back via a digit and verify wrap there
        app.handle_key(key(KeyCode::Char('1'))).await;
        assert_eq!(app.ui.state, AppState::Dashboard);
    }

    #[tokio::test]
    async fn given_garbage_tab_order_then_falls_back_to_default() {
        let mut app = test_app().await;
        app.config.tui.tab_order = Some("bogus,,nope".to_string());
        let cycle = app.tab_cycle();
        assert_eq!(cycle.len(), 8, "falls back to the default order");
        assert_eq!(cycle[cycle.len() - 1], AppState::Settings);
    }

    #[tokio::test]
    async fn given_any_tab_when_digit_pressed_then_correct_state() {
        let mut app = test_app().await;
        app.ui.state = AppState::Dashboard;

        // Mapping (US-TUI-11/12): 2 kb, 3 proj, 4 wf, 5 sec, 6 plug, 0 set
        app.handle_key(key(KeyCode::Char('2'))).await;
        assert_eq!(app.ui.state, AppState::Knowledge);
        app.handle_key(key(KeyCode::Char('3'))).await;
        assert_eq!(app.ui.state, AppState::Projects);
        app.handle_key(key(KeyCode::Char('4'))).await;
        assert_eq!(app.ui.state, AppState::Workflows);
        app.handle_key(key(KeyCode::Char('5'))).await;
        assert_eq!(app.ui.state, AppState::Secrets);
        app.handle_key(key(KeyCode::Char('6'))).await;
        assert_eq!(app.ui.state, AppState::Configs);
        // From Configs, 7 is numpad-home; leave with 0 first
        app.handle_key(key(KeyCode::Char('0'))).await;
        assert_eq!(app.ui.state, AppState::Settings);
        app.handle_key(key(KeyCode::Char('7'))).await;
        assert_eq!(app.ui.state, AppState::Plugins);
        app.handle_key(key(KeyCode::Char('1'))).await;
        assert_eq!(app.ui.state, AppState::Dashboard);
        // Unmapped digits do nothing
        app.handle_key(key(KeyCode::Char('9'))).await;
        assert_eq!(app.ui.state, AppState::Dashboard);
    }
}

#[cfg(test)]
mod knowledge_picker_tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    async fn test_app() -> ModernApp {
        let pool = std::sync::Arc::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        crate::db::run_migrations(&pool).await.unwrap();
        ModernApp::new(pool, crate::config::AppConfig::default())
    }

    #[tokio::test]
    async fn given_knowledge_tab_when_n_pressed_then_form_preset_to_filter_type() {
        let mut app = test_app().await;
        app.ui.state = AppState::Dashboard;
        app.handle_key(key(KeyCode::Char('2'))).await; // Knowledge tab

        // Default filter All -> form type cmd (index 0)
        app.handle_key(key(KeyCode::Char('n'))).await;
        assert_eq!(app.command_form.entity_type, 0);
        app.command_form.mode = None;

        // Filter to Apps (All -> Cmd -> App), then `n` presets app (index 2)
        app.handle_key(key(KeyCode::Char('f'))).await;
        app.handle_key(key(KeyCode::Char('f'))).await;
        assert_eq!(app.kb_filter, KbFilter::App);
        app.handle_key(key(KeyCode::Char('n'))).await;
        assert_eq!(app.command_form.entity_type, 2);
    }

    #[test]
    fn kb_filter_cycle_and_mappings_are_consistent() {
        assert_eq!(KbFilter::All.next(), KbFilter::Cmd);
        assert_eq!(KbFilter::Cmd.next(), KbFilter::App);
        assert_eq!(KbFilter::App.next(), KbFilter::Script);
        assert_eq!(KbFilter::Script.next(), KbFilter::All);
        assert_eq!(KbFilter::All.form_type_index(), 0);
        assert_eq!(KbFilter::App.form_type_index(), 2);
        assert_eq!(KbFilter::Script.form_type_index(), 1);
        assert_eq!(KbFilter::All.type_ids().len(), 3);
    }

    #[test]
    fn entity_display_shows_type_icon() {
        let e = crate::models::Entity {
            id: "1".into(),
            name: "docker ps".into(),
            description: None,
            content: None,
            type_id: "cmd".into(),
            project_id: None,
            parent_id: None,
            metadata_json: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert!(e.to_string().starts_with("💻 "), "cmd gets the icon");

        let app_entity = crate::models::Entity {
            type_id: "app".into(),
            ..e.clone()
        };
        assert!(app_entity.to_string().starts_with("🚀 "));
    }
}

#[cfg(test)]
mod configs_tab_tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn ctrl_s() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    }

    fn ctrl_o() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
    }

    async fn test_app() -> ModernApp {
        let pool = std::sync::Arc::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        crate::db::run_migrations(&pool).await.unwrap();
        ModernApp::new(pool, crate::config::AppConfig::default())
    }

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("tuihub-cfgtab-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn given_register_form_when_ctrl_o_then_builtin_browser_opens() {
        let mut app = test_app().await;
        let dir = tmp("browser");
        app.config_store_dir = dir.join("store");
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();

        app.handle_key(key(KeyCode::Char('n'))).await;
        for c in dir.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(ctrl_o()).await;

        assert!(app.config_input.is_some(), "form must stay open");
        let fb = app.file_browser.as_ref().expect("browser must open");
        assert_eq!(fb.cwd, dir);
        assert!(!fb.pick_dir);
        // Visible in the framebuffer
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Select file"), "browser must be drawn");
        // Esc closes only the browser, the form stays
        app.handle_key(key(KeyCode::Esc)).await;
        assert!(app.file_browser.is_none());
        assert!(app.config_input.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_file_browser_when_new_file_then_created_picked_into_form() {
        let mut app = test_app().await;
        let dir = tmp("newfile");
        app.ui.state = AppState::Configs;
        app.config_input = Some(ConfigRegisterForm::new());
        app.file_browser = Some(FileBrowser::new(
            false,
            BrowserDest::RegisterPath,
            dir.clone(),
        ));

        // `a` = new file, type the name, Enter creates it
        app.handle_key(key(KeyCode::Char('a'))).await;
        for c in "brand-new.conf".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(dir.join("brand-new.conf").exists(), "file must be created");
        let fb = app.file_browser.as_ref().unwrap();
        assert_eq!(fb.current().unwrap().name, "brand-new.conf");

        // Space picks it -> form path filled, name auto-derived, browser closed
        app.handle_key(key(KeyCode::Char(' '))).await;
        assert!(app.file_browser.is_none());
        let form = app.config_input.as_ref().expect("form open");
        assert_eq!(
            form.path,
            dir.join("brand-new.conf").to_string_lossy().to_string()
        );
        assert_eq!(form.name, "brand-new.conf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_folder_browser_when_new_folder_then_created_and_pickable() {
        let mut app = test_app().await;
        let dir = tmp("newdir");
        app.ui.state = AppState::Configs;
        app.config_target_input = Some(String::new());
        app.file_browser = Some(FileBrowser::new(true, BrowserDest::TargetPath, dir.clone()));

        // `A` = new folder, type the name, Enter creates it
        app.handle_key(key(KeyCode::Char('A'))).await;
        for c in "deploy-here".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(dir.join("deploy-here").is_dir(), "folder must be created");

        // Space picks the folder under the cursor for the target field
        app.handle_key(key(KeyCode::Char(' '))).await;
        assert!(app.file_browser.is_none());
        assert_eq!(
            app.config_target_input.as_deref(),
            Some(dir.join("deploy-here").to_string_lossy().as_ref())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_file_browser_when_navigating_then_descend_parent_and_pick_work() {
        let mut app = test_app().await;
        let dir = tmp("nav2");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("x.conf"), "k=v").unwrap();
        app.ui.state = AppState::Configs;
        app.config_input = Some(ConfigRegisterForm::new());
        app.file_browser = Some(FileBrowser::new(
            false,
            BrowserDest::RegisterPath,
            dir.clone(),
        ));

        // entries: "..", "sub/"; Down -> subdir, Right -> descend
        app.handle_key(key(KeyCode::Down)).await;
        app.handle_key(key(KeyCode::Right)).await;
        assert_eq!(app.file_browser.as_ref().unwrap().cwd, dir.join("sub"));
        // ".." is selected after descending; Down -> x.conf, Enter picks it
        app.handle_key(key(KeyCode::Down)).await;
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(app.file_browser.is_none());
        let form = app.config_input.as_ref().unwrap();
        assert_eq!(
            form.path,
            dir.join("sub").join("x.conf").to_string_lossy().to_string()
        );
        assert_eq!(form.name, "x.conf");

        // Parent navigation: open again, go up with Left
        app.file_browser = Some(FileBrowser::new(
            false,
            BrowserDest::RegisterPath,
            dir.join("sub"),
        ));
        app.handle_key(key(KeyCode::Left)).await;
        assert_eq!(app.file_browser.as_ref().unwrap().cwd, dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_file_browser_when_hidden_toggled_then_dotfiles_shown() {
        let mut app = test_app().await;
        let dir = tmp("hidden");
        std::fs::write(dir.join(".secret"), "x").unwrap();
        app.file_browser = Some(FileBrowser::new(
            false,
            BrowserDest::RegisterPath,
            dir.clone(),
        ));

        assert!(!app
            .file_browser
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|r| r.name == ".secret"));
        app.handle_key(key(KeyCode::Char('.'))).await;
        assert!(app
            .file_browser
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|r| r.name == ".secret"));
        app.handle_key(key(KeyCode::Char('.'))).await;
        assert!(!app
            .file_browser
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|r| r.name == ".secret"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_configs_tab_when_registered_via_popup_then_entry_listed() {
        let mut app = test_app().await;
        let dir = tmp("reg");
        app.config_store_dir = dir.join("store");
        let src = dir.join("hypr.conf");
        std::fs::write(&src, "monitor=,1920x1080").unwrap();

        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();
        assert!(app.configs_list.items.is_empty());

        // `n` opens the source-path popup; type the path; Enter registers
        app.handle_key(key(KeyCode::Char('n'))).await;
        assert!(app.config_input.is_some());
        for c in src.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;

        assert!(app.config_input.is_none());
        assert_eq!(app.configs_list.items.len(), 1);
        assert_eq!(app.configs_list.items[0].name, "hypr.conf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn given_config_when_target_added_and_deployed_then_symlink_created() {
        let mut app = test_app().await;
        let dir = tmp("dep");
        app.config_store_dir = dir.join("store");
        let src = dir.join("kitty.conf");
        std::fs::write(&src, "font_size 12").unwrap();

        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();
        app.handle_key(key(KeyCode::Char('n'))).await;
        for c in src.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;

        // `t` opens the target popup; Enter adds the target
        let target = dir.join("deployed").to_string_lossy().to_string();
        app.handle_key(key(KeyCode::Char('t'))).await;
        assert!(app.config_target_input.is_some());
        for c in target.chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;
        assert_eq!(app.configs_list.items[0].targets.len(), 1);

        // `m` cycles the mode: symlink -> hard link (starts at symlink)
        app.handle_key(key(KeyCode::Char('m'))).await;
        eprintln!(
            "DEBUG status={:?} mode={:?}",
            app.status_message, app.configs_list.items[0].deploy_mode
        );
        // Cycle back to symlink and deploy with `l`
        app.handle_key(key(KeyCode::Char('m'))).await;
        app.handle_key(key(KeyCode::Char('m'))).await;
        app.handle_key(key(KeyCode::Char('l'))).await;

        let link = std::fs::symlink_metadata(&target).unwrap();
        assert!(link.file_type().is_symlink());

        // `d` + Enter deletes the managed config
        app.handle_key(key(KeyCode::Char('d'))).await;
        app.handle_key(key(KeyCode::Enter)).await;
        assert!(app.configs_list.items.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_dashboard_when_six_then_n_then_escape_then_seven_then_zero_then_keys_work() {
        let mut app = test_app().await;
        app.ui.state = AppState::Dashboard;

        // User presses 6 to reach the Configs page
        app.handle_key(key(KeyCode::Char('6'))).await;
        assert_eq!(app.ui.state, AppState::Configs);

        // q must NOT quit?? (quit arm) - check Esc does not break
        app.handle_key(key(KeyCode::Esc)).await;

        // Press 2 (Knowledge), then 6 back — digits must work FROM configs
        app.handle_key(key(KeyCode::Char('2'))).await;
        assert_eq!(app.ui.state, AppState::Knowledge);
        app.handle_key(key(KeyCode::Char('6'))).await;
        assert_eq!(app.ui.state, AppState::Configs);

        // `n` opens the register popup; Esc closes it
        app.handle_key(key(KeyCode::Char('n'))).await;
        assert!(app.config_input.is_some(), "n must open the register popup");
        app.handle_key(key(KeyCode::Esc)).await;
        assert!(app.config_input.is_none());

        // `t` on empty list -> status, not hang
        app.handle_key(key(KeyCode::Char('t'))).await;

        // 0 -> Settings from Configs (7 is numpad-home here now)
        app.handle_key(key(KeyCode::Char('0'))).await;
        assert_eq!(app.ui.state, AppState::Settings);
    }

    #[tokio::test]
    async fn given_configs_tab_when_numpad_digits_pressed_then_navigate_and_cycle() {
        let mut app = test_app().await;
        let dir = tmp("numpad");
        app.config_store_dir = dir.join("store");
        for (i, name) in ["a.conf", "b.conf", "c.conf"].iter().enumerate() {
            let src = dir.join(name);
            std::fs::write(&src, format!("{}", i)).unwrap();
            app.ui.state = AppState::Configs;
            app.fetch_configs().await.unwrap();
            app.handle_key(key(KeyCode::Char('n'))).await;
            for c in src.to_string_lossy().chars() {
                app.handle_key(key(KeyCode::Char(c))).await;
            }
            app.handle_key(key(KeyCode::Enter)).await;
        }
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();

        // Numpad 2 = down, 8 = up (NumLock on)
        app.handle_key(key(KeyCode::Char('2'))).await;
        assert_eq!(app.configs_list.selected, 1);
        app.handle_key(key(KeyCode::Char('8'))).await;
        assert_eq!(app.configs_list.selected, 0);
        // Numpad 9 = end, 7 = home
        app.handle_key(key(KeyCode::Char('9'))).await;
        assert_eq!(app.configs_list.selected, 2);
        app.handle_key(key(KeyCode::Char('7'))).await;
        assert_eq!(app.configs_list.selected, 0);
        // Numpad 4 = previous mode in the cycle (symlink <- copy)
        app.configs_list.items[0].deploy_mode = crate::config_manager::DeployMode::Copy;
        app.handle_key(key(KeyCode::Char('4'))).await;
        assert_eq!(
            app.configs_list.items[0].deploy_mode,
            crate::config_manager::DeployMode::HardLink
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_configs_tab_when_n_pressed_then_register_popup_is_actually_visible() {
        // Regression: the config popups were nested inside the unrelated
        // register_input render block, so `n` set the state but drew nothing.
        // This test renders into a TestBackend and asserts the popup text
        // appears in the framebuffer (US-CFG-09).
        let mut app = test_app().await;
        let dir = tmp("vis");
        app.config_store_dir = dir.join("store");
        let src = dir.join("kitty.conf");
        std::fs::write(&src, "font_size 12").unwrap();
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();
        // Register one config so the list has a selection for the `t` case
        app.handle_key(key(KeyCode::Char('n'))).await;
        for c in src.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();

        app.handle_key(key(KeyCode::Char('n'))).await;
        assert!(app.config_input.is_some(), "state must open");

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            text.contains("Register existing config"),
            "register popup must be drawn after `n`"
        );

        // Same for the deploy-target popup (t with a selection)
        app.handle_key(key(KeyCode::Esc)).await;
        app.config_input = None;
        app.handle_key(key(KeyCode::Char('t'))).await;
        terminal.draw(|f| app.render(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            text.contains("Add deploy target"),
            "target popup must be drawn after `t`"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_register_form_when_fields_filled_then_metadata_is_stored() {
        // More fields (US-CFG-09): path, name, description, tags, deploy mode
        let mut app = test_app().await;
        let dir = tmp("form");
        app.config_store_dir = dir.join("store");
        let src = dir.join("hypr.conf");
        std::fs::write(&src, "monitor=eDP-1,1920x1080").unwrap();
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();

        app.handle_key(key(KeyCode::Char('n'))).await;
        for c in src.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        // Name auto-filled from the file name
        {
            let form = app.config_input.as_ref().unwrap();
            assert_eq!(form.name, "hypr.conf", "name must auto-fill from path");
        }
        // Tab -> Name, override it
        app.handle_key(key(KeyCode::Tab)).await;
        for c in "my-hypr".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        // Tab -> Description
        app.handle_key(key(KeyCode::Tab)).await;
        for c in "Hyprland monitor setup".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        // Tab -> Tags
        app.handle_key(key(KeyCode::Tab)).await;
        for c in "hyprland, waybar".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        // Tab -> Deploy to (leave empty), Tab -> Deploy mode; Right cycles
        // symlink -> hardlink
        app.handle_key(key(KeyCode::Tab)).await;
        app.handle_key(key(KeyCode::Tab)).await;
        app.handle_key(key(KeyCode::Right)).await;
        app.handle_key(key(KeyCode::Enter)).await;

        assert!(app.config_input.is_none(), "valid form must close");
        let entries = app.config_manager().load_registry();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my-hypr");
        assert_eq!(
            entries[0].description.as_deref(),
            Some("Hyprland monitor setup")
        );
        assert_eq!(
            entries[0].tags,
            vec!["hyprland".to_string(), "waybar".to_string()]
        );
        assert!(entries[0].targets.is_empty());
        assert_eq!(
            entries[0].deploy_mode,
            crate::config_manager::DeployMode::HardLink
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_register_form_when_deploy_to_set_then_targets_stored_and_deploy_works() {
        // The saved path and the "should be" path are different (US-CFG-10):
        // targets may live anywhere and need not exist yet.
        let mut app = test_app().await;
        let dir = tmp("targets");
        app.config_store_dir = dir.join("store");
        let src = dir.join("waybar.conf");
        std::fs::write(&src, "clock { format = %H:%M }").unwrap();
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();

        app.handle_key(key(KeyCode::Char('n'))).await;
        for c in src.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        // Tab x4 -> Deploy to; two comma-separated destinations, one inside
        // a not-yet-existing folder
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Tab)).await;
        }
        let t1 = dir
            .join("live")
            .join("waybar.conf")
            .to_string_lossy()
            .to_string();
        let t2 = dir
            .join("backup")
            .join("waybar.conf")
            .to_string_lossy()
            .to_string();
        for c in format!("{}, {}", t1, t2).chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;

        assert!(app.config_input.is_none());
        let entries = app.config_manager().load_registry();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].targets,
            vec![t1.clone(), t2.clone()],
            "deploy targets must be stored at registration"
        );

        // Deploying materializes the file at both targets
        let results = app.config_manager().deploy(&entries[0]).unwrap();
        assert!(results.iter().all(|r| r.ok), "deploy must succeed");
        assert!(dir.join("live").join("waybar.conf").exists());
        assert!(dir.join("backup").join("waybar.conf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_deploy_to_field_when_ctrl_o_then_picked_folder_appends() {
        let mut app = test_app().await;
        let dir = tmp("append");
        std::fs::create_dir_all(dir.join("out")).unwrap();
        app.ui.state = AppState::Configs;
        app.config_input = Some(ConfigRegisterForm::new());
        // Simulate Tab x4 to the Deploy-to field, pre-filled so the browser
        // opens in the temp dir
        let mut form = app.config_input.take().unwrap();
        form.focus = 4;
        form.deploy_to = dir.to_string_lossy().to_string();
        app.config_input = Some(form);
        app.handle_key(ctrl_o()).await;
        let fb = app.file_browser.as_ref().expect("browser opens");
        assert_eq!(fb.dest, BrowserDest::DeployTo);
        assert!(fb.pick_dir);
        // entries: "..", "out/"; Down -> "out", Space picks it
        app.handle_key(key(KeyCode::Down)).await;
        app.handle_key(key(KeyCode::Char(' '))).await;
        assert!(app.file_browser.is_none());
        let form = app.config_input.as_ref().unwrap();
        assert_eq!(
            form.deploy_to,
            format!(
                "{}, {}",
                dir.to_string_lossy(),
                dir.join("out").to_string_lossy()
            ),
            "picked folder must be appended to the existing Deploy-to list"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_register_form_when_path_empty_then_error_shown_and_form_stays() {
        let mut app = test_app().await;
        let dir = tmp("formerr");
        app.config_store_dir = dir.join("store");
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();

        app.handle_key(key(KeyCode::Char('n'))).await;
        app.handle_key(key(KeyCode::Enter)).await;
        let form = app.config_input.as_ref().expect("form must stay open");
        assert_eq!(form.error.as_deref(), Some("Path is required"));

        // Nonexistent path -> file-not-found error
        let mut form = app.config_input.take().unwrap();
        form.path = "/definitely/not/here.conf".to_string();
        app.config_input = Some(form);
        app.handle_key(key(KeyCode::Enter)).await;
        let form = app.config_input.as_ref().expect("form must stay open");
        assert!(
            form.error
                .as_deref()
                .unwrap_or_default()
                .starts_with("File not found"),
            "missing file must surface an inline error"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_configs_tab_when_arrows_pressed_then_selection_moves() {
        let mut app = test_app().await;
        let dir = tmp("nav");
        app.config_store_dir = dir.join("store");
        for (i, name) in ["a.conf", "b.conf", "c.conf"].iter().enumerate() {
            let src = dir.join(name);
            std::fs::write(&src, format!("{}", i)).unwrap();
            app.ui.state = AppState::Configs;
            app.fetch_configs().await.unwrap();
            app.handle_key(key(KeyCode::Char('n'))).await;
            for c in src.to_string_lossy().chars() {
                app.handle_key(key(KeyCode::Char(c))).await;
            }
            app.handle_key(key(KeyCode::Enter)).await;
        }
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();
        assert_eq!(app.configs_list.items.len(), 3);
        assert_eq!(app.configs_list.selected, 0);

        app.handle_key(key(KeyCode::Down)).await;
        app.handle_key(key(KeyCode::Down)).await;
        assert_eq!(app.configs_list.selected, 2);
        app.handle_key(key(KeyCode::Up)).await;
        assert_eq!(app.configs_list.selected, 1);
        app.handle_key(key(KeyCode::Home)).await;
        assert_eq!(app.configs_list.selected, 0);
        app.handle_key(key(KeyCode::End)).await;
        assert_eq!(app.configs_list.selected, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_configs_tab_when_search_applied_then_list_filtered() {
        let mut app = test_app().await;
        let dir = tmp("search");
        app.config_store_dir = dir.join("store");
        for name in ["alpha.conf", "beta.conf"] {
            let src = dir.join(name);
            std::fs::write(&src, "x").unwrap();
            app.ui.state = AppState::Configs;
            app.fetch_configs().await.unwrap();
            app.handle_key(key(KeyCode::Char('n'))).await;
            for c in src.to_string_lossy().chars() {
                app.handle_key(key(KeyCode::Char(c))).await;
            }
            app.handle_key(key(KeyCode::Enter)).await;
        }
        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();
        assert_eq!(app.configs_list.items.len(), 2);

        // `/` search: type "alp", apply with Enter
        app.handle_key(key(KeyCode::Char('/'))).await;
        assert!(app.search_state.active);
        for c in "alp".chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;
        assert_eq!(app.configs_list.items.len(), 1);
        assert_eq!(app.configs_list.items[0].name, "alpha.conf");

        // Esc restores the full list
        app.handle_key(key(KeyCode::Char('/'))).await;
        app.handle_key(key(KeyCode::Esc)).await;
        assert_eq!(app.configs_list.items.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn given_config_when_u_pressed_then_source_synced_into_master() {
        let mut app = test_app().await;
        let dir = tmp("upd");
        app.config_store_dir = dir.join("store");
        let src = dir.join("zshrc");
        std::fs::write(&src, "export A=1").unwrap();

        app.ui.state = AppState::Configs;
        app.fetch_configs().await.unwrap();
        app.handle_key(key(KeyCode::Char('n'))).await;
        for c in src.to_string_lossy().chars() {
            app.handle_key(key(KeyCode::Char(c))).await;
        }
        app.handle_key(key(KeyCode::Enter)).await;
        assert_eq!(app.configs_list.items[0].version, 1);

        // Change the source, then `u` updates the master (version bump)
        std::fs::write(&src, "export A=2").unwrap();
        app.handle_key(key(KeyCode::Char('u'))).await;
        assert_eq!(app.configs_list.items[0].version, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

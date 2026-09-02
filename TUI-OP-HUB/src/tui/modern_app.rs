//! Modern Application Runner with Login/Signup Integration
//!
//! This module integrates the modern UI with authentication and provides
//! a complete login/signup flow before accessing the main application.

use super::list_state::*;
use super::modern_ui::{AppState, LoginField, LoginState, ModernTheme, ModernUI};
use crate::auth::AuthManager;
use crate::config::{AppConfig, KeybindingsConfig};
use crate::keygen;
use crate::models::{CreateEntity, CreateProject};
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
    layout::{Alignment, Constraint, Direction, Layout, Rect},
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DeleteKind {
    Entity,
    Project,
    Secret,
}

/// Result popup after running a command or workflow
struct RunResult {
    title: String,
    success: bool,
    text: String,
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
    // Structured options popup for a command family (parent name, options)
    options_popup: Option<(String, Vec<(String, String)>)>,
    // Keybind helper overlay (? key; US-TUI-09)
    keybinds_overlay: bool,
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

impl ModernApp {
    pub fn new(pool: Arc<SqlitePool>, config: AppConfig) -> Self {
        let page_size = config.tui.page_size.max(1);
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
            workflow_form: WorkflowFormState::default(),
            secret_form: SecretFormState::default(),
            // Initialize search state
            search_state: SearchState::default(),
            settings: SettingsState::default(),
            advanced: AdvancedState::default(),
            keygen: KeygenState::default(),
            dev_confirm_wipe: false,
            options_popup: None,
            keybinds_overlay: false,
            // Overlays start closed
            visual_form: None,
            confirm_delete: None,
            run_result: None,
            status_message: None,
        }
    }

    /// `KeyCode` bound to a keybinding action (US-APP-02).
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
    async fn fetch_commands(&mut self) -> anyhow::Result<()> {
        let entities = repository::list_entities(&*self.pool, Some("cmd"), None).await?;
        let total = entities.len();
        self.commands_list.set_items(entities, total);
        Ok(())
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
            AppState::Commands => self.fetch_commands().await?,
            AppState::Projects => self.fetch_projects().await?,
            AppState::Workflows => self.fetch_workflows().await?,
            AppState::Secrets => self.fetch_secrets().await?,
            AppState::Dashboard => self.fetch_stats().await?,
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
        if self.keybinds_overlay {
            self.render_keybinds_overlay(f);
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
            AppState::Commands => {
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
            AppState::Settings => {
                // Dedicated settings screen (US-APP-01/02/06)
                if self.advanced.active {
                    self.render_advanced_screen(f);
                } else {
                    self.render_settings_screen(f);
                }
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

        // Render dashboard content with real stats
        let card_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .margin(1)
            .split(chunks[1]);

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

        // Footer: keybind hints for this screen
        self.render_keybind_footer(f, chunks[2], &self.keybind_hints());
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
        // Overlays take priority over normal tab handling (top of the input stack)
        if self.keybinds_overlay {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter) {
                self.keybinds_overlay = false;
            }
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
                if self.auth_mode == AuthMode::Login {
                    self.handle_login_key(key).await;
                } else {
                    self.handle_signup_key(key).await;
                }
            }
            AppState::Dashboard
            | AppState::Commands
            | AppState::Projects
            | AppState::Workflows
            | AppState::Secrets => {
                // Handle dashboard and other app state keys
                self.handle_dashboard_key(key).await;
            }
            AppState::Settings => {
                // Settings screen has its own key handling (US-APP-01/02/06)
                self.handle_settings_key(key).await;
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
            // Navigation - Number keys
            KeyCode::Char('1') => {
                self.ui.state = AppState::Dashboard;
                let _ = self.fetch_stats().await;
            }
            KeyCode::Char('2') => {
                self.ui.state = AppState::Commands;
                let _ = self.fetch_commands().await;
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
                self.ui.state = AppState::Settings;
            }
            // Tab - Cycle through states
            KeyCode::Tab => {
                self.ui.state = match self.ui.state {
                    AppState::Dashboard => AppState::Commands,
                    AppState::Commands => AppState::Projects,
                    AppState::Projects => AppState::Workflows,
                    AppState::Workflows => AppState::Secrets,
                    AppState::Secrets => AppState::Settings,
                    AppState::Settings => AppState::Dashboard,
                    _ => AppState::Dashboard,
                };
            }
            // Action keybindings
            k if Some(k) == kb_create => {
                // Create new item (context-dependent)
                match self.ui.state {
                    AppState::Commands => {
                        self.command_form = CommandFormState {
                            mode: Some(FormMode::Create),
                            ..Default::default()
                        };
                    }
                    AppState::Projects => {
                        self.project_form = ProjectFormState {
                            mode: Some(FormMode::Create),
                            ..Default::default()
                        };
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
                    AppState::Commands => {
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
                    _ => {}
                }
            }
            k if Some(k) == kb_delete => {
                // Delete selected item — ask for confirmation first
                match self.ui.state {
                    AppState::Commands => {
                        if let Some(entity) = self.commands_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: entity.id.clone(),
                                label: entity.name.clone(),
                                kind: DeleteKind::Entity,
                            });
                        }
                    }
                    AppState::Projects => {
                        if let Some(project) = self.projects_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: project.id.clone(),
                                label: project.name.clone(),
                                kind: DeleteKind::Project,
                            });
                        }
                    }
                    AppState::Workflows => {
                        if let Some(entity) = self.workflows_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: entity.id.clone(),
                                label: entity.name.clone(),
                                kind: DeleteKind::Entity,
                            });
                        }
                    }
                    AppState::Secrets => {
                        if let Some(secret) = self.secrets_list.get_selected() {
                            self.confirm_delete = Some(ConfirmDelete {
                                id: secret.id.clone(),
                                label: secret.name.clone(),
                                kind: DeleteKind::Secret,
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
                if self.ui.state == AppState::Commands {
                    self.open_in_editor().await;
                }
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
            KeyCode::Char('m') => {
                // Man page for the selected command (graceful when missing)
                if self.ui.state == AppState::Commands {
                    self.show_man_page().await;
                }
            }
            KeyCode::Char('i') => {
                // Structured options of the selected command family (US-CMD-01)
                if self.ui.state == AppState::Commands {
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
            k if Some(k) == kb_search => {
                // Start search mode
                self.search_state.active = true;
                self.search_state.query.clear();
            }
            k if Some(k) == kb_filter => {
                // Toggle filter mode
                match self.ui.state {
                    AppState::Commands => {
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
                AppState::Commands => self.commands_list.select_previous(),
                AppState::Projects => self.projects_list.select_previous(),
                AppState::Workflows => self.workflows_list.select_previous(),
                AppState::Secrets => self.secrets_list.select_previous(),
                _ => {}
            },
            KeyCode::Down => match self.ui.state {
                AppState::Commands => self.commands_list.select_next(),
                AppState::Projects => self.projects_list.select_next(),
                AppState::Workflows => self.workflows_list.select_next(),
                AppState::Secrets => self.secrets_list.select_next(),
                _ => {}
            },
            KeyCode::PageUp => match self.ui.state {
                AppState::Commands => self.commands_list.previous_page(),
                AppState::Projects => self.projects_list.previous_page(),
                AppState::Workflows => self.workflows_list.previous_page(),
                AppState::Secrets => self.secrets_list.previous_page(),
                _ => {}
            },
            KeyCode::PageDown => match self.ui.state {
                AppState::Commands => self.commands_list.next_page(),
                AppState::Projects => self.projects_list.next_page(),
                AppState::Workflows => self.workflows_list.next_page(),
                AppState::Secrets => self.secrets_list.next_page(),
                _ => {}
            },
            KeyCode::Home => match self.ui.state {
                AppState::Commands => self.commands_list.selected = 0,
                AppState::Projects => self.projects_list.selected = 0,
                AppState::Workflows => self.workflows_list.selected = 0,
                AppState::Secrets => self.secrets_list.selected = 0,
                _ => {}
            },
            KeyCode::End => match self.ui.state {
                AppState::Commands => {
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

                // Fetch dashboard statistics after successful login
                if let Err(e) = self.fetch_stats().await {
                    eprintln!("Warning: Failed to fetch stats: {}", e);
                }

                self.ui.state = AppState::Dashboard;
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
        self.render_list(
            f,
            "📝 Commands",
            &self.commands_list.items,
            self.commands_list.selected,
            self.ui.theme.primary,
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

        let header_text = Line::from(vec![
            Span::styled("🎛️ ", Style::default().fg(self.ui.theme.accent)),
            Span::styled(
                "TUI-OP-HUB",
                Style::default()
                    .fg(self.ui.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" › "),
            Span::styled(title, Style::default().fg(color)),
        ]);

        let header = Paragraph::new(header_text).alignment(Alignment::Center);
        f.render_widget(header, inner);

        // List content
        let list_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(color))
            .title(format!(" {} ({} items) ", title, items.len()))
            .style(Style::default().bg(self.ui.theme.bg));

        if items.is_empty() {
            let empty_text = Paragraph::new("No items found. Press 'n' to create a new one.")
                .style(Style::default().fg(self.ui.theme.fg))
                .alignment(Alignment::Center)
                .block(list_block);
            f.render_widget(empty_text, chunks[1]);
        } else {
            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let content = format!("{}", item);
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

        // Footer: keybind hints for this screen
        self.render_keybind_footer(f, chunks[2], &self.keybind_hints());
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
                ("1-6", "Tabs"),
                ("f", "Fetch"),
                ("p", "Processes"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Commands => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("n", "New"),
                ("e", "Edit"),
                ("d", "Delete"),
                ("r", "Run"),
                ("c", "Copy"),
                ("o", "Editor"),
                ("i", "Options"),
                ("m", "Man"),
                ("/", "Find"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Projects => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("n", "New"),
                ("e", "Edit"),
                ("d", "Delete"),
                ("E", "Shell"),
                ("/", "Find"),
                ("?", "Keybinds"),
                ("q", "Quit"),
            ],
            AppState::Workflows => vec![
                ("\u{2191}\u{2193}", "Navigate"),
                ("n", "New"),
                ("e", "Edit"),
                ("d", "Delete"),
                ("r", "Run"),
                ("c", "Copy"),
                ("v", "Visual"),
                ("/", "Find"),
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

    /// Delete confirmation popup: Enter confirms, Esc cancels.
    async fn handle_confirm_key(&mut self, key: KeyEvent) {
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
                    };
                    match result {
                        Ok(()) => {
                            self.status_message = Some(format!("✓ Deleted '{}'", confirm.label));
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
        let q = self.search_state.query.to_lowercase();
        match self.ui.state {
            AppState::Commands => {
                let _ = self.fetch_commands().await;
                self.commands_list.items.retain(|e| {
                    e.name.to_lowercase().contains(&q)
                        || e.description
                            .as_ref()
                            .map_or(false, |d| d.to_lowercase().contains(&q))
                });
                self.commands_list.selected = 0;
            }
            AppState::Projects => {
                let _ = self.fetch_projects().await;
                self.projects_list.items.retain(|p| {
                    p.name.to_lowercase().contains(&q)
                        || p.description
                            .as_ref()
                            .map_or(false, |d| d.to_lowercase().contains(&q))
                });
                self.projects_list.selected = 0;
            }
            AppState::Workflows => {
                let _ = self.fetch_workflows().await;
                self.workflows_list.items.retain(|e| {
                    e.name.to_lowercase().contains(&q)
                        || e.description
                            .as_ref()
                            .map_or(false, |d| d.to_lowercase().contains(&q))
                });
                self.workflows_list.selected = 0;
            }
            AppState::Secrets => {
                let _ = self.fetch_secrets().await;
                self.secrets_list
                    .items
                    .retain(|s| s.name.to_lowercase().contains(&q));
                self.secrets_list.selected = 0;
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
                let _ = self.fetch_commands().await;
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
            KeyCode::Backspace => match field {
                0 => {
                    self.secret_form.name.pop();
                }
                1 => {
                    self.secret_form.value.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match field {
                0 => self.secret_form.name.push(c),
                1 => self.secret_form.value.push(c),
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
        let value_enc =
            match secrets::encrypt_for_user(&*self.pool, &user_id, &self.secret_form.value).await {
                Ok(enc) => enc,
                Err(e) => {
                    self.secret_form.error_message = Some(format!("Encryption failed: {}", e));
                    return;
                }
            };
        let result = match self.secret_form.editing_id.clone() {
            Some(id) => repository::update_secret(&*self.pool, &id, &value_enc).await,
            None => repository::create_secret(&*self.pool, &user_id, &name, &value_enc).await,
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
                        .map(|s| VisualStep {
                            entity_id: String::new(),
                            name: s.name.clone(),
                            script: s.script.clone(),
                        })
                        .collect();
                }
            }
        }
        self.visual_form = Some(visual);
    }

    /// Visual workflow builder input (US-WF-03, US-WF-10).
    async fn handle_visual_key(&mut self, key: KeyEvent) {
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
                        v.steps.push(VisualStep {
                            entity_id: entity.id.clone(),
                            name: entity.name.clone(),
                            script: entity.content.clone().unwrap_or_default(),
                        });
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
            AppState::Commands => self
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
                        match secrets::decrypt_for_user_id(&*self.pool, &user_id, &secret.value_enc)
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
            AppState::Commands => {
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
                let selected = self.workflows_list.get_selected().cloned();
                let Some(entity) = selected else {
                    self.status_message = Some("Nothing selected".to_string());
                    return;
                };
                let user_id = self.current_user_profile_id().await;
                let result = workflow::execute_workflow_by_id_for_user(
                    self.pool.clone(),
                    &entity.id,
                    None,
                    Some(user_id),
                )
                .await;
                match result {
                    Ok(result) => {
                        // Persist run history (US-WF-08)
                        let run = crate::models::WorkflowRun {
                            run_id: result.run_id.clone(),
                            workflow_id: entity.id.clone(),
                            success: result.success,
                            output: Some(result.output.clone()),
                            error: result.error.clone(),
                            duration_ms: Some(result.duration_ms as i64),
                            steps_completed: Some(result.steps_completed as i32),
                            created_at: chrono::Utc::now().to_rfc3339(),
                        };
                        let _ = repository::insert_workflow_run(&*self.pool, &run).await;
                        self.run_result = Some(RunResult {
                            title: format!("Workflow: {}", entity.name),
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
                    Err(e) => {
                        self.run_result = Some(RunResult {
                            title: format!("Workflow: {}", entity.name),
                            success: false,
                            text: format!("Execution failed: {}", e),
                        });
                    }
                }
            }
            _ => {
                self.status_message =
                    Some("Run is only available for commands and workflows".to_string());
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
        let area = self.centered_rect(60, 12, f);
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
        let help = self.form_help_line(
            self.secret_form.error_message.as_ref(),
            "Encrypted with XChaCha20Poly1305 before storage · Ctrl+S: save · Esc: cancel",
        );
        f.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[2]);
    }

    fn render_confirm_delete(&self, f: &mut Frame, confirm: &ConfirmDelete) {
        let area = self.centered_rect(56, 9, f);
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" ⚠ Confirm Delete ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.ui.theme.error))
            .style(Style::default().bg(self.ui.theme.bg));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let text = vec![
            Line::from(Span::raw("Delete permanently?")),
            Line::from(Span::styled(
                confirm.label.clone(),
                Style::default()
                    .fg(self.ui.theme.warning)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
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
            ]),
        ];
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
            let empty = Paragraph::new("No steps yet — press Enter or 'a' to pick a saved command")
                .style(Style::default().fg(self.ui.theme.border))
                .alignment(Alignment::Center);
            f.render_widget(empty, steps_inner);
        } else {
            let lines: Vec<Line> = v
                .steps
                .iter()
                .enumerate()
                .map(|(i, step)| {
                    let first_line = step.script.lines().next().unwrap_or("").to_string();
                    let content = format!("{}. {}  →  {}", i + 1, step.name, first_line);
                    let style = if v.focused_field == VisualField::Steps && i == v.selected_step {
                        Style::default()
                            .fg(self.ui.theme.accent)
                            .add_modifier(Modifier::BOLD)
                            .bg(self.ui.theme.highlight)
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
            "Tab: Name/Desc/Steps · Enter/a: pick command · d: remove step · ←/→: move step · Ctrl+S: save · Esc: cancel",
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
                    let content =
                        format!("[{}] {}  →  {}", entity.type_id, entity.name, first_line);
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

    /// Show the man page for the selected command (US-CMD-05). Suspends the
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
        self.status_message = Some(format!("Project shell for '{}' closed", project.name));
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

    /// Keygen form renderer (US-SEC-01).
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

        let row = self.settings.selected;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.ui.state = AppState::Dashboard,
            KeyCode::Up | KeyCode::BackTab => self.settings.select_previous(),
            KeyCode::Down | KeyCode::Tab => self.settings.select_next(),
            KeyCode::Char('8') => self.settings.select_previous(), // numpad 8
            KeyCode::Char('2') => self.settings.select_next(),     // numpad 2
            KeyCode::Char('7') | KeyCode::Char('9') => self.settings.selected = 0, // numpad home
            KeyCode::Char('1') | KeyCode::Char('3') => {
                self.settings.selected = SETTINGS_ROWS.len().saturating_sub(1) // numpad end
            }
            KeyCode::Char('4') if row == SETTINGS_THEME_ROW => self.cycle_theme(-1), // numpad left
            KeyCode::Char('6') if row == SETTINGS_THEME_ROW => self.cycle_theme(1),  // numpad right
            KeyCode::Char('a') => {
                // Advanced mode: visual theme colors + database/API options
                self.advanced.active = true;
                self.advanced.error = None;
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

        let row = self.advanced.selected;
        let name = ADVANCED_ROWS.get(row).copied().unwrap_or("");
        match key.code {
            KeyCode::Esc => self.advanced.active = false,
            KeyCode::Up | KeyCode::BackTab => self.advanced.select_previous(),
            KeyCode::Down | KeyCode::Tab => self.advanced.select_next(),
            KeyCode::Char('8') => self.advanced.select_previous(), // numpad 8
            KeyCode::Char('2') => self.advanced.select_next(),     // numpad 2
            KeyCode::Char('7') | KeyCode::Char('9') => self.advanced.selected = 0, // numpad home
            KeyCode::Char('1') | KeyCode::Char('3') => {
                self.advanced.selected = ADVANCED_ROWS.len().saturating_sub(1) // numpad end
            }
            KeyCode::Char('4') if is_advanced_color_row(row) => self.cycle_advanced_color(-1),
            KeyCode::Char('6') if is_advanced_color_row(row) => self.cycle_advanced_color(1),
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

/// Index of a theme preset name (unknown names map to the first preset).
fn preset_index(name: &str) -> usize {
    ModernTheme::PRESETS
        .iter()
        .position(|p| p.eq_ignore_ascii_case(name))
        .unwrap_or(0)
}

impl ModernApp {
    /// Full keybind helper overlay (`?`): all actions grouped per screen,
    /// config-aware labels (US-TUI-09).
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
        lines.push(row("Tab", "Switch tabs"));
        lines.push(row(
            "1-6",
            "Dashboard / Commands / Projects / Workflows / Secrets / Settings",
        ));
        lines.push(row("/", "Fuzzy search in the current list"));
        lines.push(row("?", "Toggle this keybind helper"));
        lines.push(row("q", "Quit"));

        lines.push(Line::from(""));
        lines.push(section("Dashboard"));
        lines.push(row("f", "System fetch panel"));
        lines.push(row("p", "Processes (btop/htop/top)"));

        lines.push(Line::from(""));
        lines.push(section("Commands"));
        lines.push(row("n", "New command / script / app"));
        lines.push(row("e", "Edit selected"));
        lines.push(row("d", "Delete selected (confirm)"));
        lines.push(row("r", "Run selected"));
        lines.push(row("c", "Copy content to clipboard"));
        lines.push(row("o", "Open in external editor"));
        lines.push(row("i", "Show command options"));
        lines.push(row("m", "Open man page"));

        lines.push(Line::from(""));
        lines.push(section("Projects"));
        lines.push(row("E", "Open a shell in the project environment"));

        lines.push(Line::from(""));
        lines.push(section("Workflows"));
        lines.push(row("v", "Visual builder (pick saved commands)"));
        lines.push(row("r", "Execute workflow"));

        lines.push(Line::from(""));
        lines.push(section("Secrets"));
        lines.push(row("k", "Generate SSH / GPG key"));
        lines.push(row("c", "Copy (decrypts) secret value"));

        lines.push(Line::from(""));
        lines.push(section("Settings"));
        lines.push(row("a", "Advanced mode (visual theme editor)"));
        lines.push(row("Ctrl+S", "Save settings to config.conf"));

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
                let _ = self.fetch_commands().await;
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
        app.ui.state = AppState::Commands;
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
        app.ui.state = AppState::Commands;
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
            steps: vec![VisualStep {
                entity_id: "e1".into(),
                name: "step".into(),
                script: "print('hi')".into(),
            }],
            ..Default::default()
        });

        app.handle_key(ctrl_s()).await;

        let visual = app.visual_form.as_ref().unwrap();
        assert_eq!(visual.error_message.as_deref(), Some("Name is required"));
    }

    // ── Settings screen (US-APP-01/02/06) ───────────────────────────────────

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
        app.ui.state = AppState::Commands;
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
}

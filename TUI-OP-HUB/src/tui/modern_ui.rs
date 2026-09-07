//! Modern TUI Design with Login Screen
//!
//! Features:
//! - Beautiful centered login screen
//! - Modern dashboard with cards
//! - Improved navigation and breadcrumbs
//! - Professional color scheme
//! - Smooth state transitions

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Padding, Paragraph},
    Frame,
};
use std::time::Instant;

/// Modern color palette
pub struct ModernTheme {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub bg: Color,
    pub fg: Color,
    pub border: Color,
    pub highlight: Color,
}

impl Default for ModernTheme {
    fn default() -> Self {
        Self {
            primary: Color::Rgb(99, 102, 241),    // Indigo
            secondary: Color::Rgb(139, 92, 246),  // Purple
            accent: Color::Rgb(236, 72, 153),     // Pink
            success: Color::Rgb(34, 197, 94),     // Green
            warning: Color::Rgb(251, 191, 36),    // Amber
            error: Color::Rgb(239, 68, 68),       // Red
            bg: Color::Rgb(17, 24, 39),           // Dark gray
            fg: Color::Rgb(243, 244, 246),        // Light gray
            border: Color::Rgb(75, 85, 99),       // Medium gray
            highlight: Color::Rgb(147, 197, 253), // Light blue
        }
    }
}

impl ModernTheme {
    /// Theme preset names selectable in the Settings screen (US-APP-01).
    pub const PRESETS: [&'static str; 5] = ["dark", "light", "nord", "dracula", "gruvbox"];

    /// Look up a theme preset by name.
    pub fn preset(name: &str) -> Option<ModernTheme> {
        match name.to_lowercase().as_str() {
            "dark" => Some(Self::default()),
            "light" => Some(Self {
                primary: Color::Rgb(59, 73, 223),
                secondary: Color::Rgb(124, 58, 237),
                accent: Color::Rgb(219, 39, 119),
                success: Color::Rgb(22, 163, 74),
                warning: Color::Rgb(217, 119, 6),
                error: Color::Rgb(220, 38, 38),
                bg: Color::Rgb(248, 248, 252),
                fg: Color::Rgb(30, 30, 40),
                border: Color::Rgb(203, 203, 213),
                highlight: Color::Rgb(2, 132, 199),
            }),
            "nord" => Some(Self {
                primary: Color::Rgb(94, 129, 172),
                secondary: Color::Rgb(129, 161, 193),
                accent: Color::Rgb(191, 97, 106),
                success: Color::Rgb(163, 190, 140),
                warning: Color::Rgb(235, 203, 139),
                error: Color::Rgb(191, 97, 106),
                bg: Color::Rgb(46, 52, 64),
                fg: Color::Rgb(216, 222, 233),
                border: Color::Rgb(76, 86, 106),
                highlight: Color::Rgb(136, 192, 208),
            }),
            "dracula" => Some(Self {
                primary: Color::Rgb(189, 147, 249),
                secondary: Color::Rgb(139, 143, 190),
                accent: Color::Rgb(255, 121, 198),
                success: Color::Rgb(80, 250, 123),
                warning: Color::Rgb(241, 250, 140),
                error: Color::Rgb(255, 85, 85),
                bg: Color::Rgb(40, 42, 54),
                fg: Color::Rgb(248, 248, 242),
                border: Color::Rgb(68, 71, 90),
                highlight: Color::Rgb(139, 233, 253),
            }),
            "gruvbox" => Some(Self {
                primary: Color::Rgb(250, 189, 47),
                secondary: Color::Rgb(184, 187, 38),
                accent: Color::Rgb(254, 128, 62),
                success: Color::Rgb(184, 187, 38),
                warning: Color::Rgb(250, 189, 47),
                error: Color::Rgb(251, 73, 52),
                bg: Color::Rgb(40, 40, 40),
                fg: Color::Rgb(235, 219, 178),
                border: Color::Rgb(146, 131, 116),
                highlight: Color::Rgb(131, 165, 152),
            }),
            _ => None,
        }
    }

    /// Build the TUI theme from the config file (US-APP-01).
    ///
    /// - A known preset name gives the preset palette.
    /// - Optional per-color overrides (`primary`…`highlight` in the `theme`
    ///   section) are applied on top of the preset/default palette — this is
    ///   how users create their own theme.
    /// - `fg`/`bg`/`accent`/`status_bg` apply when the name is not a preset
    ///   (i.e. a fully custom theme); presets keep their palette otherwise.
    /// - Unknown preset names without any colors fall back to the default.
    pub fn from_config(cfg: &crate::config::ThemeConfig) -> Self {
        let is_preset = Self::preset(&cfg.name).is_some();
        let mut theme = Self::preset(&cfg.name).unwrap_or_default();

        // Per-color overrides (custom theming, US-APP-01)
        if let Some(c) = &cfg.primary {
            theme.primary = crate::config::parse_color(c);
        }
        if let Some(c) = &cfg.secondary {
            theme.secondary = crate::config::parse_color(c);
        }
        if let Some(c) = &cfg.success {
            theme.success = crate::config::parse_color(c);
        }
        if let Some(c) = &cfg.warning {
            theme.warning = crate::config::parse_color(c);
        }
        if let Some(c) = &cfg.error {
            theme.error = crate::config::parse_color(c);
        }
        if let Some(c) = &cfg.border {
            theme.border = crate::config::parse_color(c);
        }
        if let Some(c) = &cfg.highlight {
            theme.highlight = crate::config::parse_color(c);
        }

        // Base palette fields apply to fully custom themes only
        if !is_preset {
            theme.fg = cfg.fg_color();
            theme.bg = cfg.bg_color();
            theme.accent = cfg.accent_color();
        }
        theme
    }
}

/// Application state
#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Login,
    Dashboard,
    Commands,
    Apps,
    Scripts,
    Projects,
    Workflows,
    Secrets,
    Settings,
    Plugins,
    Help,
}

/// Login form state
#[derive(Debug, Clone)]
pub struct LoginState {
    pub username: String,
    pub password: String,
    pub focused_field: LoginField,
    pub error_message: Option<String>,
    pub is_authenticating: bool,
    pub auth_start: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoginField {
    Username,
    Password,
}

impl Default for LoginState {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            focused_field: LoginField::Username,
            error_message: None,
            is_authenticating: false,
            auth_start: None,
        }
    }
}

impl LoginState {
    pub fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            LoginField::Username => LoginField::Password,
            LoginField::Password => LoginField::Username,
        };
    }

    pub fn start_auth(&mut self) {
        self.is_authenticating = true;
        self.auth_start = Some(Instant::now());
        self.error_message = None;
    }

    pub fn auth_success(&mut self) {
        self.is_authenticating = false;
        self.auth_start = None;
        self.password.clear(); // Clear password from memory
    }

    pub fn auth_failed(&mut self, message: String) {
        self.is_authenticating = false;
        self.auth_start = None;
        self.error_message = Some(message);
        self.password.clear(); // Clear password from memory
    }
}

/// Modern UI renderer
pub struct ModernUI {
    pub theme: ModernTheme,
    pub state: AppState,
    pub login_state: LoginState,
}

impl ModernUI {
    pub fn new() -> Self {
        Self {
            theme: ModernTheme::default(),
            state: AppState::Login,
            login_state: LoginState::default(),
        }
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) {
        use crossterm::event::KeyCode;

        match key {
            KeyCode::Char('1') => self.state = AppState::Dashboard,
            KeyCode::Char('2') => self.state = AppState::Commands,
            KeyCode::Char('3') => self.state = AppState::Workflows,
            KeyCode::Char('4') => self.state = AppState::Secrets,
            KeyCode::Char('5') => self.state = AppState::Settings,
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.state = if self.state == AppState::Help {
                    AppState::Dashboard
                } else {
                    AppState::Help
                };
            }
            _ => {}
        }
    }

    /// Render the appropriate screen based on state
    pub fn render(&self, f: &mut Frame) {
        match self.state {
            AppState::Login => self.render_login(f),
            AppState::Dashboard => self.render_dashboard(f),
            AppState::Commands => self.render_commands(f),
            AppState::Workflows => self.render_workflows(f),
            AppState::Secrets => self.render_secrets(f),
            AppState::Settings => self.render_settings(f),
            AppState::Help => self.render_help(f),
            _ => self.render_placeholder(f),
        }
    }

    /// Render modern login screen
    fn render_login(&self, f: &mut Frame) {
        let area = f.area();

        // Create centered login box
        let login_width = 60.min(area.width - 4);
        let login_height = 20.min(area.height - 4);

        let vertical_margin = (area.height.saturating_sub(login_height)) / 2;
        let horizontal_margin = (area.width.saturating_sub(login_width)) / 2;

        let login_area = Rect {
            x: horizontal_margin,
            y: vertical_margin,
            width: login_width,
            height: login_height,
        };

        // Clear background
        f.render_widget(Clear, login_area);

        // Main login container
        let login_block = Block::default()
            .title(" 🔐 TUI-OP-HUB Login ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.primary))
            .padding(Padding::uniform(2))
            .style(Style::default().bg(self.theme.bg));

        let inner = login_block.inner(login_area);
        f.render_widget(login_block, login_area);

        // Split into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Welcome message
                Constraint::Length(3), // Username field
                Constraint::Length(3), // Password field
                Constraint::Length(2), // Error message
                Constraint::Length(1), // Progress bar
                Constraint::Min(1),    // Help text
            ])
            .split(inner);

        // Welcome message
        let welcome = Paragraph::new(vec![
            Line::from(Span::styled(
                "Welcome to TUI-OP-HUB",
                Style::default()
                    .fg(self.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Please enter your credentials",
                Style::default().fg(self.theme.fg),
            )),
        ])
        .alignment(Alignment::Center);
        f.render_widget(welcome, chunks[0]);

        // Username field
        let username_focused = self.login_state.focused_field == LoginField::Username;
        let username_style = if username_focused {
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.border)
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

        let username_text = Paragraph::new(self.login_state.username.as_str())
            .block(username_block)
            .style(Style::default().fg(self.theme.fg));
        f.render_widget(username_text, chunks[1]);

        // Password field
        let password_focused = self.login_state.focused_field == LoginField::Password;
        let password_style = if password_focused {
            Style::default()
                .fg(self.theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.border)
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

        let masked_password = "•".repeat(self.login_state.password.len());
        let password_text = Paragraph::new(masked_password.as_str())
            .block(password_block)
            .style(Style::default().fg(self.theme.fg));
        f.render_widget(password_text, chunks[2]);

        // Error message
        if let Some(ref error) = self.login_state.error_message {
            let error_text = Paragraph::new(error.as_str())
                .style(
                    Style::default()
                        .fg(self.theme.error)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center);
            f.render_widget(error_text, chunks[3]);
        }

        // Progress bar during authentication
        if self.login_state.is_authenticating {
            if let Some(start) = self.login_state.auth_start {
                let elapsed = start.elapsed().as_secs_f64();
                let progress = ((elapsed * 20.0) % 100.0) as u16;

                let gauge = Gauge::default()
                    .block(Block::default())
                    .gauge_style(Style::default().fg(self.theme.primary).bg(self.theme.bg))
                    .percent(progress)
                    .label("Authenticating...");
                f.render_widget(gauge, chunks[4]);
            }
        }

        // Help text
        let help_lines = vec![Line::from(vec![
            Span::styled(
                "Tab",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" / "),
            Span::styled(
                "↑↓",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Switch fields  "),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(self.theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Login  "),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Quit"),
        ])];
        let help = Paragraph::new(help_lines).alignment(Alignment::Center);
        f.render_widget(help, chunks[5]);
    }

    /// Render modern dashboard (placeholder - actual rendering done in ModernApp)
    fn render_dashboard(&self, f: &mut Frame) {
        let area = f.area();
        let text = Paragraph::new("Dashboard loading...")
            .style(Style::default().fg(self.theme.fg))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(self.theme.border)),
            );
        f.render_widget(text, area);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let header_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.primary))
            .style(Style::default().bg(self.theme.bg));

        let inner = header_block.inner(area);
        f.render_widget(header_block, area);

        let header_text = Line::from(vec![
            Span::styled("🎛️ ", Style::default().fg(self.theme.accent)),
            Span::styled(
                "TUI-OP-HUB",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" › "),
            Span::styled("Dashboard", Style::default().fg(self.theme.highlight)),
        ]);

        let header = Paragraph::new(header_text).alignment(Alignment::Center);
        f.render_widget(header, inner);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let footer_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.border))
            .style(Style::default().bg(self.theme.bg));

        let inner = footer_block.inner(area);
        f.render_widget(footer_block, area);

        let footer_text = Line::from(vec![
            Span::styled(
                "Tab",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Navigate  "),
            Span::styled(
                "?",
                Style::default()
                    .fg(self.theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Help  "),
            Span::styled(
                "q",
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Quit"),
        ]);

        let footer = Paragraph::new(footer_text).alignment(Alignment::Center);
        f.render_widget(footer, inner);
    }

    fn render_help(&self, f: &mut Frame) {
        let area = f.area();

        // Create centered help box
        let help_width = 70.min(area.width - 4);
        let help_height = 30.min(area.height - 4);

        let vertical_margin = (area.height.saturating_sub(help_height)) / 2;
        let horizontal_margin = (area.width.saturating_sub(help_width)) / 2;

        let help_area = Rect {
            x: horizontal_margin,
            y: vertical_margin,
            width: help_width,
            height: help_height,
        };

        f.render_widget(Clear, help_area);

        let help_block = Block::default()
            .title(" ❓ TUI-OP-HUB Keybindings ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.success))
            .padding(Padding::uniform(2))
            .style(Style::default().bg(self.theme.bg));

        let inner = help_block.inner(help_area);
        f.render_widget(help_block, help_area);

        let help_text = vec![
            Line::from(Span::styled(
                "═══════════════════════════════════════════════════════════════",
                Style::default().fg(self.theme.border),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Navigation:",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Tab / Left/Right",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  — Switch tabs"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Up/Down",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("           — Navigate list items"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("             — Select item / Switch project"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "?",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Toggle this help"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "q",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Quit (in Normal mode)"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Creating & Editing:",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "n",
                    Style::default()
                        .fg(self.theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Create new entity/project"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "e",
                    Style::default()
                        .fg(self.theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Edit entity (in detail view)"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "d",
                    Style::default()
                        .fg(self.theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Delete entity/project (with confirmation)"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Search & Filter:",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "/",
                    Style::default()
                        .fg(self.theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Start search (on Search tab)"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "f",
                    Style::default()
                        .fg(self.theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Filter by tags"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Detail View:",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "c",
                    Style::default()
                        .fg(self.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Copy content to clipboard"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "r",
                    Style::default()
                        .fg(self.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Run command/script"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "e",
                    Style::default()
                        .fg(self.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Edit entity"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "d",
                    Style::default()
                        .fg(self.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Delete entity (with confirmation)"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(self.theme.secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("               — Back to list"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Projects:",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(self.theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("             — Switch active project"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "d",
                    Style::default()
                        .fg(self.theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Delete project"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "n",
                    Style::default()
                        .fg(self.theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("                 — Create new project"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Forms:",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Tab/Up/Down",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("       — Switch fields"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("             — Submit form"),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("               — Cancel"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press Esc or ? to close this help.",
                Style::default()
                    .fg(self.theme.fg)
                    .add_modifier(Modifier::ITALIC),
            )),
        ];

        let help_paragraph = Paragraph::new(help_text).alignment(Alignment::Left);
        f.render_widget(help_paragraph, inner);
    }

    fn render_commands(&self, f: &mut Frame) {
        let area = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        self.render_header(f, chunks[0]);

        let content_block = Block::default()
            .title(" 📝 Commands ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.primary))
            .padding(Padding::uniform(2));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);

        let text = Paragraph::new(vec![
            Line::from(Span::styled(
                "Commands View",
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Press 'n' to create a new command"),
            Line::from("Press '1' to return to dashboard"),
        ])
        .alignment(Alignment::Center);
        f.render_widget(text, inner);

        self.render_footer(f, chunks[2]);
    }

    fn render_workflows(&self, f: &mut Frame) {
        let area = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        self.render_header(f, chunks[0]);

        let content_block = Block::default()
            .title(" ⚙️ Workflows ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.accent))
            .padding(Padding::uniform(2));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);

        let text = Paragraph::new(vec![
            Line::from(Span::styled(
                "Workflows View",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Press 'n' to create a new workflow"),
            Line::from("Press '1' to return to dashboard"),
        ])
        .alignment(Alignment::Center);
        f.render_widget(text, inner);

        self.render_footer(f, chunks[2]);
    }

    fn render_secrets(&self, f: &mut Frame) {
        let area = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        self.render_header(f, chunks[0]);

        let content_block = Block::default()
            .title(" 🔐 Secrets ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.warning))
            .padding(Padding::uniform(2));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);

        let text = Paragraph::new(vec![
            Line::from(Span::styled(
                "Secrets View",
                Style::default()
                    .fg(self.theme.warning)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Press 'n' to create a new secret"),
            Line::from("Press '1' to return to dashboard"),
        ])
        .alignment(Alignment::Center);
        f.render_widget(text, inner);

        self.render_footer(f, chunks[2]);
    }

    fn render_settings(&self, f: &mut Frame) {
        let area = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        self.render_header(f, chunks[0]);

        let content_block = Block::default()
            .title(" ⚙️ Settings ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.secondary))
            .padding(Padding::uniform(2));

        let inner = content_block.inner(chunks[1]);
        f.render_widget(content_block, chunks[1]);

        let text = Paragraph::new(vec![
            Line::from(Span::styled(
                "Settings View",
                Style::default()
                    .fg(self.theme.secondary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Configuration options will appear here"),
            Line::from("Press '1' to return to dashboard"),
        ])
        .alignment(Alignment::Center);
        f.render_widget(text, inner);

        self.render_footer(f, chunks[2]);
    }

    fn render_placeholder(&self, f: &mut Frame) {
        let area = f.area();
        let text = Paragraph::new("Coming soon...")
            .style(Style::default().fg(self.theme.fg))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(self.theme.border)),
            );
        f.render_widget(text, area);
    }
}

impl Default for ModernUI {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_state_field_navigation() {
        let mut state = LoginState::default();
        assert_eq!(state.focused_field, LoginField::Username);

        state.next_field();
        assert_eq!(state.focused_field, LoginField::Password);

        state.next_field();
        assert_eq!(state.focused_field, LoginField::Username);
    }

    #[test]
    fn test_login_state_auth_flow() {
        let mut state = LoginState::default();
        state.password = "test123".to_string();

        state.start_auth();
        assert!(state.is_authenticating);
        assert!(state.auth_start.is_some());
        assert!(state.error_message.is_none());

        state.auth_success();
        assert!(!state.is_authenticating);
        assert!(state.password.is_empty()); // Password cleared
    }

    #[test]
    fn test_modern_theme_colors() {
        let theme = ModernTheme::default();
        assert_eq!(theme.primary, Color::Rgb(99, 102, 241));
        assert_eq!(theme.success, Color::Rgb(34, 197, 94));
    }
}

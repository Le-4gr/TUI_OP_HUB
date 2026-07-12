//! Application configuration (US-APP-01, US-APP-02, US-APP-06).
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    /// Configurable keybindings (US-APP-02).
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
    /// Current user for multi-user encryption
    #[serde(default = "default_user")]
    pub current_user: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            api: ApiConfig::default(),
            tui: TuiConfig::default(),
            theme: ThemeConfig::default(),
            keybindings: KeybindingsConfig::default(),
            current_user: default_user(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
    #[serde(default = "default_busy_timeout")]
    pub busy_timeout_ms: u64,
}
impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
            busy_timeout_ms: default_busy_timeout(),
        }
    }
}
fn default_db_path() -> String {
    "tuihub.db".to_string()
}
fn default_busy_timeout() -> u64 {
    5000
}
fn default_user() -> String {
    std::env::var("TUI_OP_HUB_USER").unwrap_or_else(|_| "default".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_api_addr")]
    pub bind_addr: String,
}
impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_api_addr(),
        }
    }
}
fn default_api_addr() -> String {
    "127.0.0.1:0".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_tui_enabled")]
    pub enabled: bool,
}
impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            enabled: default_tui_enabled(),
        }
    }
}
fn default_tui_enabled() -> bool {
    true
}

/// Theme configuration (US-APP-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme_name")]
    pub name: String,
    #[serde(default = "default_fg")]
    pub fg: String,
    #[serde(default = "default_bg")]
    pub bg: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_status_bg")]
    pub status_bg: String,
}
impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
            fg: default_fg(),
            bg: default_bg(),
            accent: default_accent(),
            status_bg: default_status_bg(),
        }
    }
}
fn default_theme_name() -> String {
    "dark".to_string()
}
fn default_fg() -> String {
    "white".to_string()
}
fn default_bg() -> String {
    "black".to_string()
}
fn default_accent() -> String {
    "yellow".to_string()
}
fn default_status_bg() -> String {
    "blue".to_string()
}

impl ThemeConfig {
    pub fn fg_color(&self) -> ratatui::style::Color {
        parse_color(&self.fg)
    }
    pub fn bg_color(&self) -> ratatui::style::Color {
        parse_color(&self.bg)
    }
    pub fn accent_color(&self) -> ratatui::style::Color {
        parse_color(&self.accent)
    }
    pub fn status_bg_color(&self) -> ratatui::style::Color {
        parse_color(&self.status_bg)
    }
}

fn parse_color(s: &str) -> ratatui::style::Color {
    match s.to_lowercase().as_str() {
        "white" => ratatui::style::Color::White,
        "black" => ratatui::style::Color::Black,
        "red" => ratatui::style::Color::Red,
        "green" => ratatui::style::Color::Green,
        "yellow" => ratatui::style::Color::Yellow,
        "blue" => ratatui::style::Color::Blue,
        "magenta" => ratatui::style::Color::Magenta,
        "cyan" => ratatui::style::Color::Cyan,
        "gray" => ratatui::style::Color::Gray,
        "darkgray" => ratatui::style::Color::DarkGray,
        _ => ratatui::style::Color::White,
    }
}

/// Configurable keybindings for the TUI (US-APP-02).
///
/// Each field maps to a single character or key name. Users can override
/// these in their `config.toml` under the `[keybindings]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    #[serde(default = "default_kb_quit")]
    pub quit: String,
    #[serde(default = "default_kb_help")]
    pub help: String,
    #[serde(default = "default_kb_search")]
    pub search: String,
    #[serde(default = "default_kb_filter")]
    pub filter: String,
    #[serde(default = "default_kb_create")]
    pub create: String,
    #[serde(default = "default_kb_edit")]
    pub edit: String,
    #[serde(default = "default_kb_delete")]
    pub delete: String,
    #[serde(default = "default_kb_copy")]
    pub copy: String,
    #[serde(default = "default_kb_run")]
    pub run: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            quit: default_kb_quit(),
            help: default_kb_help(),
            search: default_kb_search(),
            filter: default_kb_filter(),
            create: default_kb_create(),
            edit: default_kb_edit(),
            delete: default_kb_delete(),
            copy: default_kb_copy(),
            run: default_kb_run(),
        }
    }
}

fn default_kb_quit() -> String {
    "q".to_string()
}
fn default_kb_help() -> String {
    "?".to_string()
}
fn default_kb_search() -> String {
    "/".to_string()
}
fn default_kb_filter() -> String {
    "f".to_string()
}
fn default_kb_create() -> String {
    "n".to_string()
}
fn default_kb_edit() -> String {
    "e".to_string()
}
fn default_kb_delete() -> String {
    "d".to_string()
}
fn default_kb_copy() -> String {
    "c".to_string()
}
fn default_kb_run() -> String {
    "r".to_string()
}

impl KeybindingsConfig {
    /// Parse a keybinding string into a `KeyCode`.
    /// Supports single characters and special keys: "tab", "enter", "esc", "up", "down", "left", "right".
    pub fn to_keycode(s: &str) -> Option<crossterm::event::KeyCode> {
        use crossterm::event::KeyCode;
        match s.to_lowercase().as_str() {
            "tab" => Some(KeyCode::Tab),
            "enter" => Some(KeyCode::Enter),
            "esc" => Some(KeyCode::Esc),
            "up" => Some(KeyCode::Up),
            "down" => Some(KeyCode::Down),
            "left" => Some(KeyCode::Left),
            "right" => Some(KeyCode::Right),
            "backspace" => Some(KeyCode::Backspace),
            "space" => Some(KeyCode::Char(' ')),
            s if s.len() == 1 => s.chars().next().map(KeyCode::Char),
            _ => None,
        }
    }

    pub fn quit_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.quit).unwrap_or(crossterm::event::KeyCode::Char('q'))
    }
    pub fn help_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.help).unwrap_or(crossterm::event::KeyCode::Char('?'))
    }
    pub fn search_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.search).unwrap_or(crossterm::event::KeyCode::Char('/'))
    }
    pub fn filter_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.filter).unwrap_or(crossterm::event::KeyCode::Char('f'))
    }
    pub fn create_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.create).unwrap_or(crossterm::event::KeyCode::Char('n'))
    }
    pub fn edit_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.edit).unwrap_or(crossterm::event::KeyCode::Char('e'))
    }
    pub fn delete_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.delete).unwrap_or(crossterm::event::KeyCode::Char('d'))
    }
    pub fn copy_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.copy).unwrap_or(crossterm::event::KeyCode::Char('c'))
    }
    pub fn run_key(&self) -> crossterm::event::KeyCode {
        Self::to_keycode(&self.run).unwrap_or(crossterm::event::KeyCode::Char('r'))
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> AppResult<Self> {
        if !path.exists() {
            tracing::info!(path = %path.display(), "config file not found, using defaults");
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("failed to read config: {e}")))?;
        toml::from_str(&contents)
            .map_err(|e| AppError::Config(format!("failed to parse TOML: {e}")))
    }
    pub fn default_path() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("tui-op-hub").join("config.toml");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("tui-op-hub")
                .join("config.toml");
        }
        PathBuf::from("config.toml")
    }
}

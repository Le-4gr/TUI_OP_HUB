//! Application configuration (US-APP-01, US-APP-02, US-APP-06).
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::{AppError, AppResult};

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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            api: ApiConfig::default(),
            tui: TuiConfig::default(),
            theme: ThemeConfig::default(),
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
    fn default() -> Self { Self { path: default_db_path(), busy_timeout_ms: default_busy_timeout() } }
}
fn default_db_path() -> String { "tuihub.db".to_string() }
fn default_busy_timeout() -> u64 { 5000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_api_addr")]
    pub bind_addr: String,
}
impl Default for ApiConfig {
    fn default() -> Self { Self { bind_addr: default_api_addr() } }
}
fn default_api_addr() -> String { "127.0.0.1:0".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_tui_enabled")]
    pub enabled: bool,
}
impl Default for TuiConfig {
    fn default() -> Self { Self { enabled: default_tui_enabled() } }
}
fn default_tui_enabled() -> bool { true }

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
fn default_theme_name() -> String { "dark".to_string() }
fn default_fg() -> String { "white".to_string() }
fn default_bg() -> String { "black".to_string() }
fn default_accent() -> String { "yellow".to_string() }
fn default_status_bg() -> String { "blue".to_string() }

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
            return PathBuf::from(home).join(".config").join("tui-op-hub").join("config.toml");
        }
        PathBuf::from("config.toml")
    }
}

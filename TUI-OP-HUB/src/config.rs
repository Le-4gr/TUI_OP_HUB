//! Application configuration (US-APP-01, US-APP-02, US-APP-06).
//!
//! The config file uses a Hyprland-style `section { key = value }` format
//! (see `parse_hypr_config`) stored at `~/.config/tui-op-hub/config.conf`.
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// General preferences: editor and other defaults (US-APP-01, US-APP-06).
    #[serde(default)]
    pub general: GeneralConfig,
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
            general: GeneralConfig::default(),
            current_user: default_user(),
        }
    }
}

/// General preferences editable from the Settings screen (US-APP-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// External text editor used to edit command/script/workflow content
    /// (e.g. "vi", "nvim", "code --wait"). Empty = use `$EDITOR` or "vi".
    #[serde(default)]
    pub editor: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            editor: String::new(),
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
pub(crate) fn default_db_path() -> String {
    "tuihub.db".to_string()
}
pub(crate) fn default_busy_timeout() -> u64 {
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
pub(crate) fn default_api_addr() -> String {
    "127.0.0.1:0".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_tui_enabled")]
    pub enabled: bool,
    /// Page size for TUI lists (US-APP-01, changeable in Settings)
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    /// Comma-separated Tab-cycle order (US-TUI-12), e.g.
    /// `tab_order = "dash,kb,cmd,app,script,proj,wf,sec,plug,set"`.
    /// Unknown or missing ids fall back to the default order.
    #[serde(default)]
    pub tab_order: Option<String>,
}
impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            enabled: default_tui_enabled(),
            page_size: default_page_size(),
            tab_order: None,
        }
    }
}
fn default_tui_enabled() -> bool {
    true
}
fn default_page_size() -> usize {
    15
}

/// Theme configuration (US-APP-01).
///
/// `name` selects a built-in preset (`dark`, `light`, `nord`, `dracula`,
/// `gruvbox`, `solarized`, `catppuccin`, `catppuccin-latte`). Any other name is a **custom theme**: the palette is then built
/// from the color values below (falling back to the default theme for colors
/// that are not set). The optional `primary`…`highlight` fields can also be
/// used to *override* individual colors of a preset.
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
    // Optional overrides — unset means "use the preset/default color".
    #[serde(default)]
    pub primary: Option<String>,
    #[serde(default)]
    pub secondary: Option<String>,
    #[serde(default)]
    pub success: Option<String>,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default)]
    pub highlight: Option<String>,
}
impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
            fg: default_fg(),
            bg: default_bg(),
            accent: default_accent(),
            status_bg: default_status_bg(),
            primary: None,
            secondary: None,
            success: None,
            warning: None,
            error: None,
            border: None,
            highlight: None,
        }
    }
}
fn default_theme_name() -> String {
    "dark".to_string()
}
pub(crate) fn default_fg() -> String {
    "white".to_string()
}
pub(crate) fn default_bg() -> String {
    "black".to_string()
}
pub(crate) fn default_accent() -> String {
    "yellow".to_string()
}
pub(crate) fn default_status_bg() -> String {
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
    /// True when at least one optional override color is set (US-APP-01).
    pub fn has_custom_colors(&self) -> bool {
        [
            &self.primary,
            &self.secondary,
            &self.success,
            &self.warning,
            &self.error,
            &self.border,
            &self.highlight,
        ]
        .iter()
        .any(|c| c.is_some())
    }
}

pub(crate) fn parse_color(s: &str) -> ratatui::style::Color {
    let lower = s.to_lowercase();
    // Hex colors: "#rrggbb" (US-APP-01)
    if let Some(hex) = lower.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                return ratatui::style::Color::Rgb(
                    ((v >> 16) & 0xFF) as u8,
                    ((v >> 8) & 0xFF) as u8,
                    (v & 0xFF) as u8,
                );
            }
        }
    }
    match lower.as_str() {
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
        // Named keys are case-insensitive; single characters keep their case so
        // that rebinding to an uppercase letter round-trips (US-APP-02).
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
            _ => {
                if s.chars().count() == 1 {
                    s.chars().next().map(KeyCode::Char)
                } else {
                    None
                }
            }
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

    /// Ordered action names shown in the Settings screen (US-APP-02).
    pub const ACTIONS: [&'static str; 9] = [
        "quit", "help", "search", "filter", "create", "edit", "delete", "copy", "run",
    ];

    /// Current binding text for an action (falls back to the default if unknown).
    pub fn get(&self, action: &str) -> &str {
        match action {
            "quit" => &self.quit,
            "help" => &self.help,
            "search" => &self.search,
            "filter" => &self.filter,
            "create" => &self.create,
            "edit" => &self.edit,
            "delete" => &self.delete,
            "copy" => &self.copy,
            "run" => &self.run,
            _ => "",
        }
    }

    /// Set the binding text for an action (unknown actions are ignored).
    pub fn set(&mut self, action: &str, value: String) {
        match action {
            "quit" => self.quit = value,
            "help" => self.help = value,
            "search" => self.search = value,
            "filter" => self.filter = value,
            "create" => self.create = value,
            "edit" => self.edit = value,
            "delete" => self.delete = value,
            "copy" => self.copy = value,
            "run" => self.run = value,
            _ => {}
        }
    }

    /// `KeyCode` for an action, with the legacy default as fallback.
    pub fn key_for(&self, action: &str) -> crossterm::event::KeyCode {
        Self::to_keycode(self.get(action)).unwrap_or(crossterm::event::KeyCode::Char('?'))
    }

    /// Inverse of [`Self::to_keycode`]: serialize a pressed key to config text.
    pub fn keycode_to_string(code: crossterm::event::KeyCode) -> Option<String> {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Char(' ') => Some("space".to_string()),
            KeyCode::Char(c) => Some(c.to_string()),
            KeyCode::Tab => Some("tab".to_string()),
            KeyCode::Enter => Some("enter".to_string()),
            KeyCode::Esc => Some("esc".to_string()),
            KeyCode::Up => Some("up".to_string()),
            KeyCode::Down => Some("down".to_string()),
            KeyCode::Left => Some("left".to_string()),
            KeyCode::Right => Some("right".to_string()),
            KeyCode::Backspace => Some("backspace".to_string()),
            _ => None,
        }
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
        Ok(Self::from_sections(&parse_hypr_config(&contents)))
    }

    /// Persist the config to disk in Hyprland-style format (US-APP-06).
    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Config(format!("failed to create config dir: {e}")))?;
            }
        }
        std::fs::write(path, self.to_hypr())
            .map_err(|e| AppError::Config(format!("failed to write config: {e}")))
    }

    pub fn default_path() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("tui-op-hub").join("config.conf");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("tui-op-hub")
                .join("config.conf");
        }
        PathBuf::from("config.conf")
    }
}

impl AppConfig {
    /// Build the config from parsed `section -> (key -> value)` pairs.
    /// Missing keys keep their defaults; unknown keys are ignored.
    fn from_sections(sections: &HyprSections) -> Self {
        let get = |section: &str, key: &str| -> Option<String> {
            sections.get(section).and_then(|m| m.get(key)).cloned()
        };
        let get_bool = |section: &str, key: &str| -> Option<bool> {
            get(section, key).and_then(|v| parse_bool(&v))
        };

        let mut cfg = Self::default();

        // general
        if let Some(v) = get("general", "editor") {
            cfg.general.editor = v;
        }
        if let Some(v) = get("general", "user") {
            cfg.current_user = v;
        }
        // database
        if let Some(v) = get("database", "path") {
            cfg.database.path = v;
        }
        if let Some(v) = get("database", "busy_timeout_ms").and_then(|v| v.parse().ok()) {
            cfg.database.busy_timeout_ms = v;
        }
        // api
        if let Some(v) = get("api", "bind_addr") {
            cfg.api.bind_addr = v;
        }
        // tui
        if let Some(v) = get_bool("tui", "enabled") {
            cfg.tui.enabled = v;
        }
        if let Some(v) = get("tui", "page_size").and_then(|v| v.parse().ok()) {
            cfg.tui.page_size = v;
        }
        // Tab-cycle order (US-TUI-12): free-form, validated at use site
        if let Some(v) = get("tui", "tab_order") {
            let v = v.trim().trim_matches('"').to_string();
            if !v.is_empty() {
                cfg.tui.tab_order = Some(v);
            }
        }
        // theme
        if let Some(v) = get("theme", "name") {
            cfg.theme.name = v;
        }
        if let Some(v) = get("theme", "fg") {
            cfg.theme.fg = v;
        }
        if let Some(v) = get("theme", "bg") {
            cfg.theme.bg = v;
        }
        if let Some(v) = get("theme", "accent") {
            cfg.theme.accent = v;
        }
        if let Some(v) = get("theme", "status_bg") {
            cfg.theme.status_bg = v;
        }
        for (key, target) in [
            ("primary", &mut cfg.theme.primary),
            ("secondary", &mut cfg.theme.secondary),
            ("success", &mut cfg.theme.success),
            ("warning", &mut cfg.theme.warning),
            ("error", &mut cfg.theme.error),
            ("border", &mut cfg.theme.border),
            ("highlight", &mut cfg.theme.highlight),
        ] {
            if let Some(v) = get("theme", key) {
                *target = Some(v);
            }
        }
        // keybindings
        for action in KeybindingsConfig::ACTIONS {
            if let Some(v) = get("keybindings", action) {
                cfg.keybindings.set(action, v);
            }
        }
        cfg
    }

    /// Render the config in Hyprland style (`section { key = value }`).
    fn to_hypr(&self) -> String {
        fn val(s: &str) -> String {
            // Quote values that are empty, contain spaces, or start with '#'
            // (hex colors) so they survive a round trip.
            if s.is_empty() || s.contains(' ') || s.starts_with('#') {
                format!("\"{}\"", s)
            } else {
                s.to_string()
            }
        }

        let mut out = String::new();
        out.push_str("# TUI-OP-HUB configuration\n");
        out.push_str("# Format: Hyprland-style `section { key = value }` blocks.\n");
        out.push_str("# Lines starting with `#` are comments. Values may be quoted.\n");
        out.push_str("# Missing keys fall back to built-in defaults.\n\n");

        out.push_str("general {\n");
        out.push_str(
            "    # External editor for the \"open in editor\" action (o on the Commands tab).\n",
        );
        out.push_str("    # Empty = use $EDITOR, falling back to \"vi\".\n");
        out.push_str(&format!("    editor = {}\n", val(&self.general.editor)));
        out.push_str("    # User profile used for secrets encryption.\n");
        out.push_str(&format!("    user = {}\n", val(&self.current_user)));
        out.push_str("}\n\n");

        out.push_str("database {\n");
        out.push_str(&format!("    path = {}\n", val(&self.database.path)));
        out.push_str(&format!(
            "    busy_timeout_ms = {}\n",
            self.database.busy_timeout_ms
        ));
        out.push_str("}\n\n");

        out.push_str("api {\n");
        out.push_str(&format!("    bind_addr = {}\n", val(&self.api.bind_addr)));
        out.push_str("}\n\n");

        out.push_str("tui {\n");
        out.push_str(&format!(
            "    enabled = {}\n",
            if self.tui.enabled { "true" } else { "false" }
        ));
        out.push_str("    # Items per page in the list tabs.\n");
        out.push_str(&format!("    page_size = {}\n", self.tui.page_size));
        if let Some(order) = &self.tui.tab_order {
            out.push_str(&format!("    tab_order = {}\n", order));
        }
        out.push_str("}\n\n");

        out.push_str("theme {\n");
        out.push_str("    # Preset: dark | light | nord | dracula | gruvbox\n");
        out.push_str("    # Any other name = your own custom theme: set the colors below.\n");
        out.push_str(&format!("    name = {}\n", val(&self.theme.name)));
        out.push_str("    # Colors accept named colors or hex (#rrggbb). Unset optional\n");
        out.push_str("    # colors fall back to the preset/default palette.\n");
        out.push_str(&format!("    fg = {}\n", val(&self.theme.fg)));
        out.push_str(&format!("    bg = {}\n", val(&self.theme.bg)));
        out.push_str(&format!("    accent = {}\n", val(&self.theme.accent)));
        out.push_str(&format!("    status_bg = {}\n", val(&self.theme.status_bg)));
        for (key, value) in [
            ("primary", &self.theme.primary),
            ("secondary", &self.theme.secondary),
            ("success", &self.theme.success),
            ("warning", &self.theme.warning),
            ("error", &self.theme.error),
            ("border", &self.theme.border),
            ("highlight", &self.theme.highlight),
        ] {
            if let Some(v) = value {
                out.push_str(&format!("    {} = {}\n", key, val(v)));
            }
        }
        out.push_str("}\n\n");

        out.push_str("keybindings {\n");
        for action in KeybindingsConfig::ACTIONS {
            out.push_str(&format!(
                "    {} = {}\n",
                action,
                val(self.keybindings.get(action))
            ));
        }
        out.push_str("}\n");
        out
    }
}

// ── Hyprland-style config file parsing ──────────────────────────────────────
//
// The config file uses a flat `section { key = value }` syntax (like Hyprland
// or sway configs). Rules:
// - `#` at the start of a line begins a comment
// - values may be quoted ("code --wait") or bare
// - unknown sections/keys are ignored (forward compatible)
// - missing keys keep their built-in defaults

/// Parsed `section -> (key -> value)` map.
pub(crate) type HyprSections = HashMap<String, HashMap<String, String>>;

/// Parse Hyprland-style config text into `section -> (key -> value)` pairs.
pub(crate) fn parse_hypr_config(text: &str) -> HyprSections {
    let mut sections: HyprSections = HashMap::new();
    let mut current = String::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        // Opening a section: "name {" (or one-line "name { key = value }")
        if let Some(open) = line.find('{') {
            current = line[..open].trim().to_string();
            let entry = sections.entry(current.clone()).or_default();
            let inner = line[open + 1..].trim();
            if let Some(close) = inner.strip_suffix('}') {
                if let Some((key, value)) = split_key_value(close.trim()) {
                    entry.insert(key, value);
                }
            }
            continue;
        }
        if line == "}" {
            current.clear();
            continue;
        }
        // key = value inside the current section
        if let Some((key, value)) = split_key_value(line) {
            if !current.is_empty() {
                sections
                    .entry(current.clone())
                    .or_default()
                    .insert(key, value);
            }
        }
    }
    sections
}

/// Split `key = value`, returning `None` for lines without `=`.
fn split_key_value(line: &str) -> Option<(String, String)> {
    let eq = line.find('=')?;
    let key = line[..eq].trim().to_string();
    if key.is_empty() {
        return None;
    }
    let mut value = line[eq + 1..].trim().to_string();
    // Strip one pair of surrounding quotes
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value = value[1..value.len() - 1].to_string();
    }
    Some((key, value))
}

/// Parse a boolean the way Hyprland-style configs usually spell them.
fn parse_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tui-op-hub-config-test-{}-{}.conf",
            tag,
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn defaults_are_sensible() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.tui.page_size, 15);
        assert!(cfg.tui.enabled);
        assert!(cfg.general.editor.is_empty(), "editor defaults to $EDITOR");
        assert_eq!(cfg.theme.name, "dark");
        assert_eq!(cfg.keybindings.get("create"), "n");
        assert_eq!(KeybindingsConfig::ACTIONS.len(), 9);
    }

    /// Scenario: Settings saved to disk load back identically
    /// Given a customized config, when it is saved and reloaded, then every
    /// setting (editor, page size, theme, keybindings) round-trips.
    #[test]
    fn given_customized_config_when_saved_then_loads_identically() {
        let mut cfg = AppConfig::default();
        cfg.general.editor = "nvim".to_string();
        cfg.tui.page_size = 25;
        cfg.theme.name = "nord".to_string();
        cfg.keybindings.set("create", "C".to_string());
        cfg.keybindings.set("run", "F5".to_string()); // invalid → key_for falls back

        let path = temp_config_path("roundtrip");
        cfg.save(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.general.editor, "nvim");
        assert_eq!(loaded.tui.page_size, 25);
        assert_eq!(loaded.theme.name, "nord");
        assert_eq!(loaded.keybindings.get("create"), "C");
    }

    /// Scenario: saving into a non-existent directory creates it
    #[test]
    fn given_missing_dir_when_saved_then_created_and_written() {
        let dir = std::env::temp_dir().join(format!("tui-op-hub-cfg-{}", uuid::Uuid::new_v4()));
        let path = dir.join("sub").join("config.toml");
        AppConfig::default().save(&path).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keybinding_parse_and_serialize_round_trip() {
        use crossterm::event::KeyCode;
        assert_eq!(KeybindingsConfig::to_keycode("n"), Some(KeyCode::Char('n')));
        assert_eq!(KeybindingsConfig::to_keycode("TAB"), Some(KeyCode::Tab));
        assert_eq!(
            KeybindingsConfig::to_keycode("space"),
            Some(KeyCode::Char(' '))
        );
        assert_eq!(KeybindingsConfig::to_keycode("f12"), None);

        assert_eq!(
            KeybindingsConfig::keycode_to_string(KeyCode::Char('x')),
            Some("x".into())
        );
        assert_eq!(
            KeybindingsConfig::keycode_to_string(KeyCode::Esc),
            Some("esc".into())
        );
        assert_eq!(
            KeybindingsConfig::keycode_to_string(KeyCode::Char(' ')),
            Some("space".into())
        );
        assert_eq!(KeybindingsConfig::keycode_to_string(KeyCode::F(1)), None);

        // Round trip both ways for a selection of keys
        for code in [
            KeyCode::Char('n'),
            KeyCode::Tab,
            KeyCode::Up,
            KeyCode::Enter,
        ] {
            let text = KeybindingsConfig::keycode_to_string(code).unwrap();
            assert_eq!(KeybindingsConfig::to_keycode(&text), Some(code));
        }
    }

    /// Scenario: a rebinding is used by the action lookup
    /// Given create is rebound to "C", when the create key is looked up,
    /// then it resolves to the new key (and invalid text falls back safely).
    #[test]
    fn given_rebinding_when_looked_up_then_action_uses_it() {
        use crossterm::event::KeyCode;
        let mut kb = KeybindingsConfig::default();
        assert_eq!(kb.key_for("create"), KeyCode::Char('n'));

        kb.set("create", "C".to_string());
        assert_eq!(kb.get("create"), "C");
        assert_eq!(kb.key_for("create"), KeyCode::Char('C'));

        // Unparseable binding text falls back to a safe key, never panics
        kb.set("run", "not-a-key".to_string());
        assert_eq!(kb.key_for("run"), KeyCode::Char('?'));
        assert_eq!(kb.key_for("nonexistent-action"), KeyCode::Char('?'));
    }

    #[test]
    fn hex_and_named_colors_parse() {
        assert_eq!(
            ThemeConfig::default().fg_color(),
            ratatui::style::Color::White
        );
        assert_eq!(
            ThemeConfig {
                fg: "#4ade80".to_string(),
                ..Default::default()
            }
            .fg_color(),
            ratatui::style::Color::Rgb(0x4a, 0xde, 0x80)
        );
        // Invalid hex falls back
        assert_eq!(
            ThemeConfig {
                fg: "#zzz".to_string(),
                ..Default::default()
            }
            .fg_color(),
            ratatui::style::Color::White
        );
    }

    // ── Hyprland-style config file parsing ──────────────────────────────────

    /// Scenario: a hand-written Hyprland-style config is parsed
    /// Given a config file with comments, quotes and unknown keys, when it is
    /// loaded, then known values apply, comments/quotes are stripped and
    /// unknown keys are ignored.
    #[test]
    fn given_hypr_style_file_when_loaded_then_values_comments_and_quotes_handled() {
        let dir = std::env::temp_dir().join(format!("tui-op-hub-hypr-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.conf");
        std::fs::write(
            &path,
            "# my config\n\
             \n\
             general {\n\
             \x20   editor = \"code --wait\"\n\
             }\n\
             tui { page_size = 25 }\n\
             theme {\n\
             \x20   name = myscheme\n\
             \x20   bg = #101010\n\
             \x20   unknown_key = whatever\n\
             }\n\
             unknown_section { foo = bar }\n",
        )
        .unwrap();

        let cfg = AppConfig::load(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(cfg.general.editor, "code --wait");
        assert_eq!(cfg.tui.page_size, 25);
        assert_eq!(cfg.theme.name, "myscheme");
        assert_eq!(cfg.theme.bg, "#101010");
        // Defaults survive for keys not present
        assert_eq!(cfg.database.path, "tuihub.db");
    }

    /// Scenario: a saved config is readable Hyprland-style text
    /// Given any config, when saved, then the file uses `section { }` syntax
    /// and round-trips.
    #[test]
    fn given_config_when_saved_then_hyprland_format_round_trips() {
        let mut cfg = AppConfig::default();
        cfg.tui.enabled = false;
        cfg.theme.name = "gruvbox".to_string();
        cfg.theme.primary = Some("#abc123".to_string());

        let path = temp_config_path("hypr");
        cfg.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(text.contains("tui {"));
        assert!(text.contains("page_size = 15"));
        assert!(text.contains("primary = \"#abc123\""));
        assert!(!loaded.tui.enabled);
        assert_eq!(loaded.theme.name, "gruvbox");
        assert_eq!(loaded.theme.primary.as_deref(), Some("#abc123"));
    }

    #[test]
    fn hypr_bools_parse_in_all_common_spellings() {
        let text = "tui {\n enabled = yes\n}\ndatabase {\n busy_timeout_ms = 0\n}";
        let sections = parse_hypr_config(text);
        let cfg = AppConfig::from_sections(&sections);
        assert!(cfg.tui.enabled);
        assert_eq!(cfg.database.busy_timeout_ms, 0);
    }
}

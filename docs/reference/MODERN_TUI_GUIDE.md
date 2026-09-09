# 🎨 Modern TUI Design Guide

## Overview

TUI-OP-HUB features a **modern, polished terminal user interface** with:

- ✨ **Beautiful Login Screen** - Centered, professional authentication
- 🎯 **Modern Dashboard** - Card-based layout with statistics
- 🎨 **Professional Color Scheme** - Indigo, purple, and pink accents
- 🔄 **Smooth State Transitions** - Between login and main application
- 📱 **Responsive Design** - Adapts to terminal size
- ⌨️ **Intuitive Navigation** - Keyboard-first interface

---

## 🚀 Quick Start

### Using the Modern UI

```rust
use tui_op_hub::tui::modern_ui::{ModernUI, AppState, LoginState};
use ratatui::{backend::CrosstermBackend, Terminal};
use crossterm::{
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create modern UI
    let mut ui = ModernUI::new();
    
    // Render
    terminal.draw(|f| ui.render(f))?;
    
    Ok(())
}
```

---

## 🎨 Color Palette

The modern theme uses a carefully selected color palette:

| Color | RGB | Usage |
|-------|-----|-------|
| **Primary** | `(99, 102, 241)` | Indigo - Main UI elements, borders |
| **Secondary** | `(139, 92, 246)` | Purple - Secondary elements |
| **Accent** | `(236, 72, 153)` | Pink - Highlights, active states |
| **Success** | `(34, 197, 94)` | Green - Success messages, confirmations |
| **Warning** | `(251, 191, 36)` | Amber - Warnings |
| **Error** | `(239, 68, 68)` | Red - Errors, destructive actions |
| **Background** | `(17, 24, 39)` | Dark Gray - Main background |
| **Foreground** | `(243, 244, 246)` | Light Gray - Text |
| **Border** | `(75, 85, 99)` | Medium Gray - Inactive borders |
| **Highlight** | `(147, 197, 253)` | Light Blue - Hover states |

---

## 🔐 Login Screen

### Features

- **Centered Design** - Automatically centers on any terminal size
- **Field Navigation** - Tab, Up/Down arrows to switch fields
- **Password Masking** - Displays bullets (•) instead of characters
- **Visual Feedback** - Active field highlighted with ▶ indicator
- **Error Display** - Shows authentication errors in red
- **Progress Indicator** - Animated gauge during authentication
- **Help Text** - Always visible keyboard shortcuts

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` / `↑↓` | Switch between username and password fields |
| `Enter` | Submit login |
| `Esc` | Quit application |
| `Backspace` | Delete character |
| `Any char` | Type in active field |

### Login Flow

```rust
// 1. User enters username
login_state.username = "admin".to_string();

// 2. User tabs to password field
login_state.next_field();

// 3. User enters password (masked)
login_state.password = "secret123".to_string();

// 4. User presses Enter - start authentication
login_state.start_auth();

// 5. Authentication succeeds
login_state.auth_success();
// State transitions to Dashboard

// OR authentication fails
login_state.auth_failed("Invalid credentials".to_string());
// Error message displayed, password cleared
```

---

## 📊 Dashboard

### Layout

```
┌─────────────────────────────────────────────────┐
│  🎛️ TUI-OP-HUB › Dashboard                     │
├─────────────────────────────────────────────────┤
│                                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐     │
│  │📝Commands│  │📁Projects│  │⚙️Workflows│     │
│  │    42    │  │    8     │  │    15    │     │
│  └──────────┘  └──────────┘  └──────────┘     │
│                                                 │
├─────────────────────────────────────────────────┤
│  Tab Navigate  ?  Help  q Quit                 │
└─────────────────────────────────────────────────┘
```

### Card Components

Each card displays:
- **Icon** - Visual identifier (📝, 📁, ⚙️)
- **Title** - Card name
- **Count** - Number of items
- **Color** - Unique accent color per card

---

## 🎯 Application States

```rust
pub enum AppState {
    Login,      // Login screen
    Dashboard,  // Main dashboard with cards
    Commands,   // Command list view
    Projects,   // Project management
    Workflows,  // Workflow execution
    Secrets,    // Secret management
    Settings,   // Application settings
}
```

### State Transitions

```
Login ──[auth success]──> Dashboard
  │
  └──[Esc]──> Exit

Dashboard ──[Tab]──> Commands/Projects/etc.
  │
  └──[q]──> Exit
```

---

## 🎨 UI Components

### Rounded Borders

All containers use rounded borders (`BorderType::Rounded`) for a modern look:

```
╭─────────────╮
│   Content   │
╰─────────────╯
```

### Centered Layouts

Login screen and modals are automatically centered:

```rust
let login_width = 60.min(area.width - 4);
let login_height = 20.min(area.height - 4);

let vertical_margin = (area.height.saturating_sub(login_height)) / 2;
let horizontal_margin = (area.width.saturating_sub(login_width)) / 2;
```

### Progress Indicators

Animated progress bars during async operations:

```rust
let gauge = Gauge::default()
    .gauge_style(Style::default().fg(theme.primary))
    .percent(progress)
    .label("Authenticating...");
```

---

## 🔧 Integration Guide

### Step 1: Add to Your Application

```rust
use tui_op_hub::tui::modern_ui::{ModernUI, AppState};

let mut ui = ModernUI::new();
```

### Step 2: Handle Events

```rust
use crossterm::event::{self, Event, KeyCode};

loop {
    terminal.draw(|f| ui.render(f))?;
    
    if let Event::Key(key) = event::read()? {
        match (ui.state, key.code) {
            (AppState::Login, KeyCode::Tab) => {
                ui.login_state.next_field();
            }
            (AppState::Login, KeyCode::Enter) => {
                // Trigger authentication
                ui.login_state.start_auth();
                // ... perform auth ...
                ui.login_state.auth_success();
                ui.state = AppState::Dashboard;
            }
            (AppState::Login, KeyCode::Char(c)) => {
                // Add character to active field
                match ui.login_state.focused_field {
                    LoginField::Username => ui.login_state.username.push(c),
                    LoginField::Password => ui.login_state.password.push(c),
                }
            }
            _ => {}
        }
    }
}
```

### Step 3: Connect to Authentication

```rust
use tui_op_hub::auth::AuthManager;

// When user submits login
if ui.login_state.is_authenticating {
    let auth_manager = AuthManager::new(pool.clone());
    
    match auth_manager.verify_password(
        &ui.login_state.username,
        &ui.login_state.password
    ).await {
        Ok(true) => {
            ui.login_state.auth_success();
            ui.state = AppState::Dashboard;
        }
        Ok(false) => {
            ui.login_state.auth_failed("Invalid credentials".to_string());
        }
        Err(e) => {
            ui.login_state.auth_failed(format!("Error: {}", e));
        }
    }
}
```

---

## 📝 Customization

### Custom Theme

```rust
use tui_op_hub::tui::modern_ui::ModernTheme;

let custom_theme = ModernTheme {
    primary: Color::Rgb(59, 130, 246),    // Blue
    secondary: Color::Rgb(168, 85, 247),  // Purple
    accent: Color::Rgb(251, 146, 60),     // Orange
    // ... other colors
    ..Default::default()
};

let mut ui = ModernUI {
    theme: custom_theme,
    ..Default::default()
};
```

### Custom States

Extend the `AppState` enum for your needs:

```rust
pub enum AppState {
    Login,
    Dashboard,
    // Your custom states
    UserManagement,
    Analytics,
    Reports,
}
```

---

## 🧪 Testing

The modern UI includes comprehensive tests:

```bash
cargo test --lib tui::modern_ui
```

### Test Coverage

- ✅ Login state field navigation
- ✅ Authentication flow (start, success, failure)
- ✅ Password clearing on auth completion
- ✅ Theme color values
- ✅ State transitions

---

## 🎯 Best Practices

### 1. **Always Clear Sensitive Data**

```rust
// After authentication (success or failure)
login_state.password.clear();
```

### 2. **Provide Visual Feedback**

```rust
// Show loading state
login_state.start_auth();

// Show errors clearly
login_state.auth_failed("Invalid credentials".to_string());
```

### 3. **Handle Terminal Resize**

The UI automatically adapts, but test with:

```bash
# Resize terminal while app is running
# UI should remain centered and functional
```

### 4. **Use Consistent Colors**

```rust
// Use theme colors for consistency
Style::default().fg(theme.primary)  // For primary elements
Style::default().fg(theme.error)    // For errors
Style::default().fg(theme.success)  // For success
```

---

## 🚀 Future Enhancements

Status (updated):

- [x] **Themes** - 8 presets (dark, light, nord, dracula, gruvbox, solarized, catppuccin, catppuccin-latte) + custom themes + visual color editor (Settings > a)
- [x] **Modal Dialogs** - delete confirm (with project-folder toggle), sudo password, cron input, import/export path, project detail, run result, keybind overlay, SSH panel, plugin actions, config register/target forms, file browser, template preview, logic-node editor
- [x] **Tabs** - 8 main tabs (Dashboard, Knowledge Base, Projects, Workflows, Secrets, Configs, Plugins, Settings), keys 1-9/0 from any screen; Knowledge Base holds commands/scripts/apps with a type filter; selected rows show a `❯` cursor marker
- [x] **Charts** - Dashboard mini-btop: CPU per-core bars, RAM/swap gauges, network rates, temperatures, GPU stats (auto-refresh 2s)
- [x] **Search** - per-tab fuzzy search with a visible search bar (`/`)
- [ ] **Animations** - Smooth transitions between states
- [ ] **Notifications** - Toast-style messages (status bar used instead)
- [ ] **Command Palette** - Quick action launcher

---

## 📚 Examples

### Complete Login Example

See `examples/modern_login.rs`:

```bash
cargo run --example modern_login
```

### Dashboard Example

See `examples/modern_dashboard.rs`:

```bash
cargo run --example modern_dashboard
```

---

## 🐛 Troubleshooting

### Login Screen Not Centered

**Problem**: Login box appears in corner

**Solution**: Ensure terminal size is at least 64x24

```bash
# Check terminal size
tput cols  # Should be >= 64
tput lines # Should be >= 24
```

### Colors Not Showing

**Problem**: Colors appear as gray

**Solution**: Enable true color support

```bash
export COLORTERM=truecolor
```

### Password Not Masking

**Problem**: Password shows as plain text

**Solution**: Check that password field is rendering bullets:

```rust
let masked_password = "•".repeat(self.login_state.password.len());
```

---

## 📖 API Reference

### `ModernUI`

Main UI controller

```rust
pub struct ModernUI {
    pub theme: ModernTheme,
    pub state: AppState,
    pub login_state: LoginState,
}

impl ModernUI {
    pub fn new() -> Self
    pub fn render<B: Backend>(&self, f: &mut Frame<B>)
}
```

### `LoginState`

Login form state management

```rust
pub struct LoginState {
    pub username: String,
    pub password: String,
    pub focused_field: LoginField,
    pub error_message: Option<String>,
    pub is_authenticating: bool,
}

impl LoginState {
    pub fn next_field(&mut self)
    pub fn start_auth(&mut self)
    pub fn auth_success(&mut self)
    pub fn auth_failed(&mut self, message: String)
}
```

### `ModernTheme`

Color palette configuration

```rust
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
```

---

## 🤝 Contributing

To contribute to the modern UI:

1. Follow the existing color scheme
2. Use rounded borders for consistency
3. Add tests for new components
4. Update this guide with new features
5. Ensure accessibility (keyboard navigation)

---

## 📄 License

Same as TUI-OP-HUB project license.

---

**Built with ❤️ using Ratatui and Crossterm**

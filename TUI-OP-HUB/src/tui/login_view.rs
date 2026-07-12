use crate::auth::{AuthManager, EncryptionKey};
use crate::error::AppResult;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Login view for password-based authentication
pub struct LoginView {
    pub username: String,
    pub password: String,
    pub cursor_field: usize, // 0 = username, 1 = password
    pub error_message: Option<String>,
    pub is_loading: bool,
}

impl LoginView {
    pub fn new() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            cursor_field: 0,
            error_message: None,
            is_loading: false,
        }
    }

    /// Handle key input
    pub fn handle_key(&mut self, key: KeyEvent) -> LoginAction {
        if self.is_loading {
            return LoginAction::None;
        }

        match key.code {
            KeyCode::Char(c) => {
                if self.cursor_field == 0 {
                    self.username.push(c);
                } else {
                    self.password.push(c);
                }
                self.error_message = None;
                LoginAction::None
            }
            KeyCode::Backspace => {
                if self.cursor_field == 0 {
                    self.username.pop();
                } else {
                    self.password.pop();
                }
                self.error_message = None;
                LoginAction::None
            }
            KeyCode::Tab | KeyCode::Down => {
                self.cursor_field = (self.cursor_field + 1) % 2;
                LoginAction::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.cursor_field = if self.cursor_field == 0 { 1 } else { 0 };
                LoginAction::None
            }
            KeyCode::Enter => {
                if self.username.is_empty() {
                    self.error_message = Some("Username cannot be empty".to_string());
                    LoginAction::None
                } else if self.password.is_empty() {
                    self.error_message = Some("Password cannot be empty".to_string());
                    LoginAction::None
                } else {
                    self.is_loading = true;
                    LoginAction::AttemptLogin
                }
            }
            KeyCode::Esc => LoginAction::Quit,
            _ => LoginAction::None,
        }
    }

    /// Attempt to login with the provided credentials
    pub async fn attempt_login(
        &mut self,
        pool: Arc<SqlitePool>,
    ) -> AppResult<Option<EncryptionKey>> {
        let auth_manager = AuthManager::new(pool.clone());

        // Verify password
        let is_valid = auth_manager
            .verify_password(&self.username, &self.password)
            .await?;

        if !is_valid {
            self.error_message = Some("Invalid username or password".to_string());
            self.is_loading = false;
            self.password.clear();
            return Ok(None);
        }

        // Derive encryption key from password
        let salt = auth_manager.get_salt(&self.username).await?;
        let encryption_key = EncryptionKey::from_password(&self.password, &salt)?;

        // Record successful login
        auth_manager.record_login(&self.username, true).await?;

        self.is_loading = false;
        Ok(Some(encryption_key))
    }

    /// Render the login screen
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        // Create centered layout
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Length(15),
                Constraint::Percentage(30),
            ])
            .split(area);

        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(50),
                Constraint::Percentage(25),
            ])
            .split(vertical_chunks[1]);

        let login_area = horizontal_chunks[1];

        // Clear the area
        frame.render_widget(Clear, login_area);

        // Create login box
        let block = Block::default()
            .title(" TUI-OP-HUB Login ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        frame.render_widget(block, login_area);

        // Inner area for content
        let inner_area = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(1), // Title
                Constraint::Length(1), // Spacing
                Constraint::Length(3), // Username field
                Constraint::Length(1), // Spacing
                Constraint::Length(3), // Password field
                Constraint::Length(1), // Spacing
                Constraint::Length(2), // Error message
                Constraint::Length(1), // Spacing
                Constraint::Length(1), // Help text
            ])
            .split(login_area);

        // Title
        let title = Paragraph::new("Please enter your credentials")
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center);
        frame.render_widget(title, inner_area[0]);

        // Username field
        let username_block = Block::default()
            .title(" Username ")
            .borders(Borders::ALL)
            .border_style(if self.cursor_field == 0 {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            });

        let username_text = Paragraph::new(self.username.as_str())
            .block(username_block)
            .style(Style::default().fg(Color::White));
        frame.render_widget(username_text, inner_area[2]);

        // Password field
        let password_block = Block::default()
            .title(" Password ")
            .borders(Borders::ALL)
            .border_style(if self.cursor_field == 1 {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            });

        let masked_password = "*".repeat(self.password.len());
        let password_text = Paragraph::new(masked_password.as_str())
            .block(password_block)
            .style(Style::default().fg(Color::White));
        frame.render_widget(password_text, inner_area[4]);

        // Error message
        if let Some(ref error) = self.error_message {
            let error_text = Paragraph::new(error.as_str())
                .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center);
            frame.render_widget(error_text, inner_area[6]);
        }

        // Loading indicator
        if self.is_loading {
            let loading_text = Paragraph::new("Authenticating...")
                .style(Style::default().fg(Color::Yellow))
                .alignment(Alignment::Center);
            frame.render_widget(loading_text, inner_area[6]);
        }

        // Help text
        let help_spans = vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(": Switch field | "),
            Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(": Login | "),
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(": Quit"),
        ];
        let help_text = Paragraph::new(Line::from(help_spans))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(help_text, inner_area[8]);
    }
}

impl Default for LoginView {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions that can be triggered from the login view
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginAction {
    None,
    AttemptLogin,
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_view_creation() {
        let view = LoginView::new();
        assert_eq!(view.username, "");
        assert_eq!(view.password, "");
        assert_eq!(view.cursor_field, 0);
        assert!(view.error_message.is_none());
        assert!(!view.is_loading);
    }

    #[test]
    fn test_handle_key_input() {
        let mut view = LoginView::new();

        // Type username
        view.handle_key(KeyEvent::from(KeyCode::Char('u')));
        view.handle_key(KeyEvent::from(KeyCode::Char('s')));
        view.handle_key(KeyEvent::from(KeyCode::Char('e')));
        view.handle_key(KeyEvent::from(KeyCode::Char('r')));
        assert_eq!(view.username, "user");

        // Switch to password field
        view.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(view.cursor_field, 1);

        // Type password
        view.handle_key(KeyEvent::from(KeyCode::Char('p')));
        view.handle_key(KeyEvent::from(KeyCode::Char('a')));
        view.handle_key(KeyEvent::from(KeyCode::Char('s')));
        view.handle_key(KeyEvent::from(KeyCode::Char('s')));
        assert_eq!(view.password, "pass");

        // Backspace
        view.handle_key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(view.password, "pas");
    }

    #[test]
    fn test_validation() {
        let mut view = LoginView::new();

        // Try to login with empty username
        let action = view.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(action, LoginAction::None);
        assert!(view.error_message.is_some());

        // Add username but no password
        view.username = "user".to_string();
        view.error_message = None;
        let action = view.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(action, LoginAction::None);
        assert!(view.error_message.is_some());

        // Add password
        view.password = "pass".to_string();
        view.error_message = None;
        let action = view.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(action, LoginAction::AttemptLogin);
        assert!(view.is_loading);
    }
}

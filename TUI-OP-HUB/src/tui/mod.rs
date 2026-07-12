//! TUI layer (US-TUI-01..10, US-CMD-07..09, US-PROJ-02..06, US-NF-10, US-DEP-02).
use crossterm::{event::{self, Event, KeyCode, KeyEventKind}, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}, execute};
use ratatui::{backend::CrosstermBackend, layout::{Constraint, Direction, Layout}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap}, Terminal, Frame};
use std::io;
use sqlx::SqlitePool;
use std::sync::Arc;
use crate::repository;
use crate::config::ThemeConfig;
use crate::models::Entity;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Commands, Projects, Tags, Search,
}

impl Tab {
    fn all() -> [Tab; 4] { [Tab::Commands, Tab::Projects, Tab::Tags, Tab::Search] }
    fn title(self) -> &'static str {
        match self { Tab::Commands => "Commands", Tab::Projects => "Projects", Tab::Tags => "Tags", Tab::Search => "Search" }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Normal,
    Search,
    Filter,
    Detail,
    ConfirmDelete,
    Help,
}

pub struct App {
    pub tab: Tab,
    pub input: String,
    pub filter_input: String,
    pub items: Vec<String>,
    pub entities: Vec<Entity>,
    pub selected: usize,
    pub db_path: String,
    pub active_project: String,
    pub active_project_id: Option<String>,
    pub mode: Mode,
    pub theme: ThemeConfig,
    pub message: String,
    pub projects: Vec<crate::models::Project>,
}

impl App {
    pub fn new(db_path: &str, theme: ThemeConfig) -> Self {
        Self {
            tab: Tab::Commands,
            input: String::new(),
            filter_input: String::new(),
            items: Vec::new(),
            entities: Vec::new(),
            selected: 0,
            db_path: db_path.to_string(),
            active_project: "all".to_string(),
            active_project_id: None,
            mode: Mode::Normal,
            theme,
            message: String::new(),
            projects: Vec::new(),
        }
    }

    fn selected_entity(&self) -> Option<&Entity> {
        self.entities.get(self.selected)
    }

    fn move_selection(&mut self, delta: i32) {
        if self.entities.is_empty() { return; }
        let len = self.entities.len();
        let new_idx = (self.selected as i32 + delta).rem_euclid(len as i32) as usize;
        self.selected = new_idx;
    }
}

pub async fn run(app: &mut App, pool: Arc<SqlitePool>) -> anyhow::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        // Refresh data
        app.projects = repository::list_projects(&pool).await.unwrap_or_default();
        app.entities = match app.tab {
            Tab::Commands => {
                if app.filter_input.is_empty() {
                    repository::list_entities(&pool, None, app.active_project_id.as_deref()).await.unwrap_or_default()
                } else {
                    let tags: Vec<String> = app.filter_input.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    repository::filter_by_tags(&pool, &tags).await.unwrap_or_default()
                }
            }
            Tab::Projects => { Vec::new() }
            Tab::Tags => { Vec::new() }
            Tab::Search => {
                if app.input.is_empty() { Vec::new() }
                else {
                    let results = repository::search_entities(&pool, &app.input).await.unwrap_or_default();
                    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
                    let mut entities = Vec::new();
                    for id in ids {
                        if let Ok(e) = repository::get_entity(&pool, &id).await {
                            entities.push(e);
                        }
                    }
                    entities
                }
            }
        };

        app.items = match app.tab {
            Tab::Commands | Tab::Search => {
                app.entities.iter().map(|e| {
                    let desc = e.description.as_deref().unwrap_or("");
                    format!("[{}] {} \u{2014} {}", e.type_id, e.name, desc)
                }).collect()
            }
            Tab::Projects => {
                app.projects.iter().map(|p| {
                    let marker = if Some(&p.id) == app.active_project_id.as_ref() { "* " } else { "  " };
                    format!("{}{} \u{2014} {}", marker, p.name, p.description.as_deref().unwrap_or(""))
                }).collect()
            }
            Tab::Tags => {
                let tags = repository::list_tags(&pool).await.unwrap_or_default();
                tags.iter().map(|t| t.name.clone()).collect()
            }
        };

        terminal.draw(|f| draw(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }
                let should_quit = app.mode == Mode::Normal && key.code == KeyCode::Char('q');
                handle_key(app, key.code, &pool).await;
                if should_quit {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyCode, pool: &SqlitePool) {
    // Handle mode-specific keys first
    match app.mode {
        Mode::Search => {
            match key {
                KeyCode::Esc => { app.mode = Mode::Normal; app.input.clear(); }
                KeyCode::Backspace => { app.input.pop(); }
                KeyCode::Char(c) => { app.input.push(c); }
                _ => {}
            }
            return;
        }
        Mode::Filter => {
            match key {
                KeyCode::Esc => { app.mode = Mode::Normal; app.filter_input.clear(); }
                KeyCode::Backspace => { app.filter_input.pop(); }
                KeyCode::Char(c) => { app.filter_input.push(c); }
                KeyCode::Enter => { app.mode = Mode::Normal; }
                _ => {}
            }
            return;
        }
        Mode::Detail => {
            match key {
                KeyCode::Esc | KeyCode::Char('q') => { app.mode = Mode::Normal; }
                KeyCode::Char('c') => {
                    // Copy to clipboard (US-CMD-08)
                    if let Some(e) = app.selected_entity() {
                        let text = e.content.as_deref().unwrap_or(&e.name);
                        match arboard::Clipboard::new() {
                            Ok(mut cb) => {
                                match cb.set_text(text.to_string()) {
                                    Ok(_) => app.message = "Copied to clipboard!".to_string(),
                                    Err(_) => app.message = "Failed to copy".to_string(),
                                }
                            }
                            Err(_) => app.message = "Clipboard unavailable".to_string(),
                        }
                    }
                }
                KeyCode::Char('r') => {
                    // Run command (US-CMD-09)
                    let run_info = app.selected_entity().and_then(|e| {
                        e.content.as_ref().map(|c| (e.name.clone(), c.clone()))
                    });
                    if let Some((name, content)) = run_info {
                        app.mode = Mode::Normal;
                        app.message = format!("Running: {}...", name);
                        // Suspend TUI, run command, resume
                        disable_raw_mode().ok();
                        execute!(io::stdout(), LeaveAlternateScreen).ok();
                        let shell = std::env::var("SHELL").unwrap_or("bash".to_string());
                        let result = std::process::Command::new(&shell)
                            .arg("-c")
                            .arg(&content)
                            .status();
                        enable_raw_mode().ok();
                        execute!(io::stdout(), EnterAlternateScreen).ok();
                        match result {
                            Ok(status) => {
                                app.message = if status.success() {
                                    format!("{} completed successfully", name)
                                } else {
                                    format!("{} exited with error", name)
                                };
                            }
                            Err(_) => app.message = "Failed to execute".to_string(),
                        }
                    }
                }
                KeyCode::Char('d') => {
                    // Delete with confirmation (US-NF-10)
                    app.mode = Mode::ConfirmDelete;
                }
                _ => {}
            }
            return;
        }
        Mode::ConfirmDelete => {
            match key {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(e) = app.selected_entity() {
                        let id = e.id.clone();
                        match repository::delete_entity(pool, &id).await {
                            Ok(_) => {
                                app.message = "Deleted successfully".to_string();
                                if app.selected > 0 { app.selected -= 1; }
                            }
                            Err(e) => app.message = format!("Delete failed: {}", e),
                        }
                    }
                    app.mode = Mode::Normal;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    app.mode = Mode::Normal;
                    app.message = "Delete cancelled".to_string();
                }
                _ => {}
            }
            return;
        }
        Mode::Help => {
            if key == KeyCode::Esc || key == KeyCode::Char('q') || key == KeyCode::Char('?') {
                app.mode = Mode::Normal;
            }
            return;
        }
        Mode::Normal => {}
    }

    // Normal mode key handling
    match key {
        KeyCode::Tab => {
            let tabs = Tab::all();
            let idx = tabs.iter().position(|t| *t == app.tab).unwrap();
            app.tab = tabs[(idx + 1) % tabs.len()];
            app.selected = 0;
        }
        KeyCode::Left => {
            let tabs = Tab::all();
            let idx = tabs.iter().position(|t| *t == app.tab).unwrap();
            app.tab = tabs[(idx + tabs.len() - 1) % tabs.len()];
            app.selected = 0;
        }
        KeyCode::Right => {
            let tabs = Tab::all();
            let idx = tabs.iter().position(|t| *t == app.tab).unwrap();
            app.tab = tabs[(idx + 1) % tabs.len()];
            app.selected = 0;
        }
        KeyCode::Up => { app.move_selection(-1); }
        KeyCode::Down => { app.move_selection(1); }
        KeyCode::Char('?') => { app.mode = Mode::Help; }
        KeyCode::Char('/') => {
            if app.tab == Tab::Search {
                app.mode = Mode::Search;
                app.input.clear();
            }
        }
        KeyCode::Char('f') => {
            app.mode = Mode::Filter;
            app.filter_input.clear();
        }
        KeyCode::Enter => {
            match app.tab {
                Tab::Commands | Tab::Search => {
                    if app.selected_entity().is_some() {
                        app.mode = Mode::Detail;
                    }
                }
                Tab::Projects => {
                    // Switch project (US-PROJ-04)
                    if let Some(p) = app.projects.get(app.selected) {
                        if app.active_project_id.as_ref() == Some(&p.id) {
                            // Deselect
                            app.active_project_id = None;
                            app.active_project = "all".to_string();
                            app.message = "Switched to all projects".to_string();
                        } else {
                            app.active_project_id = Some(p.id.clone());
                            app.active_project = p.name.clone();
                            app.message = format!("Switched to project: {}", p.name);
                        }
                    }
                }
                _ => {}
            }
        }
        KeyCode::Char('d') if app.tab == Tab::Projects => {
            // Delete project (US-PROJ-06, US-NF-10)
            if let Some(p) = app.projects.get(app.selected) {
                let pid = p.id.clone();
                let pname = p.name.clone();
                app.message = format!("Deleting project '{}'...", pname);
                match repository::delete_project(pool, &pid).await {
                    Ok(_) => {
                        app.message = format!("Project '{}' deleted", pname);
                        if app.active_project_id.as_ref() == Some(&pid) {
                            app.active_project_id = None;
                            app.active_project = "all".to_string();
                        }
                    }
                    Err(e) => app.message = format!("Delete failed: {}", e),
                }
            }
        }
        _ => {}
    }
}

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tabs
            Constraint::Min(1),     // Main content
            Constraint::Length(3),  // Input/Help
            Constraint::Length(1),  // Status bar
        ])
        .split(f.area());

    // Tabs
    let titles = Tab::all().into_iter().map(|t| {
        Span::styled(t.title(), Style::default().add_modifier(Modifier::BOLD))
    });
    let tabs = Tabs::new(titles.collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title("TUI-OP-HUB"))
        .select(app.tab as usize);
    f.render_widget(tabs, chunks[0]);

    // Main content - list with selection highlight
    let items: Vec<ListItem> = app.items.iter().enumerate().map(|(i, s)| {
        let style = if i == app.selected && (app.tab == Tab::Commands || app.tab == Tab::Search || app.tab == Tab::Projects) {
            Style::default().fg(app.theme.accent_color()).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(Line::from(Span::styled(s.as_str(), style)))
    }).collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!("{} ({})", app.tab.title(), app.items.len())));
    f.render_widget(list, chunks[1]);

    // Bottom panel
    match app.mode {
        Mode::Search => {
            let input = Paragraph::new(app.input.as_str())
                .style(Style::default().fg(app.theme.accent_color()))
                .block(Block::default().borders(Borders::ALL).title("Search (Esc to cancel)"));
            f.render_widget(input, chunks[2]);
        }
        Mode::Filter => {
            let input = Paragraph::new(app.filter_input.as_str())
                .style(Style::default().fg(Color::Cyan))
                .block(Block::default().borders(Borders::ALL).title("Filter by tags (comma-separated, Esc to cancel)"));
            f.render_widget(input, chunks[2]);
        }
        Mode::Detail => {
            if let Some(e) = app.selected_entity() {
                let detail = format!(
                    "Name: {}\nType: {}\nDescription: {}\n\nContent:\n{}\n\n[c] Copy to clipboard  [r] Run  [d] Delete  [Esc] Back",
                    e.name,
                    e.type_id,
                    e.description.as_deref().unwrap_or("(none)"),
                    e.content.as_deref().unwrap_or("(none)"),
                );
                let para = Paragraph::new(detail)
                    .wrap(Wrap { trim: true })
                    .block(Block::default().borders(Borders::ALL).title(format!("Entity: {}", e.name)));
                f.render_widget(para, chunks[2]);
            }
        }
        Mode::ConfirmDelete => {
            let msg = "\u{26a0} CONFIRM DELETE \u{2014} Press Y to confirm, N to cancel";
            let para = Paragraph::new(msg)
                .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL).title("Confirmation"));
            f.render_widget(para, chunks[2]);
        }
        Mode::Help => {
            let help_text = "\
TUI-OP-HUB Keybindings (US-TUI-09):
  Tab / Left/Right  \u{2014} Switch tabs
  Up/Down           \u{2014} Navigate list
  Enter             \u{2014} Select item / Switch project
  /                 \u{2014} Start search (on Search tab)
  f                 \u{2014} Filter by tags
  ?                 \u{2014} Toggle this help
  q                 \u{2014} Quit (in Normal mode)

Detail View:
  c                 \u{2014} Copy content to clipboard (US-CMD-08)
  r                 \u{2014} Run command/script (US-CMD-09)
  d                 \u{2014} Delete entity (with confirmation) (US-NF-10)
  Esc               \u{2014} Back to list

Projects:
  Enter             \u{2014} Switch active project (US-PROJ-04)
  d                 \u{2014} Delete project (US-PROJ-06)

Press Esc or ? to close this help.";
            let para = Paragraph::new(help_text)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title("Help (US-TUI-09)"));
            f.render_widget(Clear, chunks[1]);
            f.render_widget(para, chunks[1]);
        }
        _ => {
            let help_text = if app.message.is_empty() {
                match app.tab {
                    Tab::Search => "Type / to search | Up/Down: navigate | Enter: detail | ?: help | q: quit",
                    Tab::Projects => "Up/Down: navigate | Enter: switch project | d: delete project | ?: help | q: quit",
                    _ => "Up/Down: navigate | Enter: detail | f: filter by tags | ?: help | q: quit",
                }
            } else {
                &app.message.as_str()
            };
            let help = Paragraph::new(help_text)
                .block(Block::default().borders(Borders::ALL).title("Help"));
            f.render_widget(help, chunks[2]);
        }
    }

    // Status bar (US-TUI-08)
    let status = format!(
        " DB: {} | Project: {} | Tab: {} | Mode: {:?} ",
        app.db_path,
        app.active_project,
        app.tab.title(),
        app.mode,
    );
    let status_bar = Paragraph::new(status)
        .style(Style::default().bg(app.theme.status_bg_color()).fg(Color::White));
    f.render_widget(status_bar, chunks[3]);
}

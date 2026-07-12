//! TUI layer — full rewrite with create/edit/delete, dashboard, better UX.
//!
//! Covers: US-TUI-01..10, US-CMD-01..09, US-PROJ-01..07, US-SRCH-01..04,
//! US-NF-10, US-DEP-02, US-APP-01..02.

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};
use std::io;
use sqlx::SqlitePool;
use std::sync::Arc;
use crate::repository;
use crate::config::ThemeConfig;
use crate::models::{CreateEntity, CreateProject, Entity, EntityType, Project, Tag};

// ─── Tabs ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tab {
    Dashboard,
    Commands,
    Projects,
    Tags,
    Search,
}

impl Tab {
    fn all() -> [Tab; 5] {
        [Tab::Dashboard, Tab::Commands, Tab::Projects, Tab::Tags, Tab::Search]
    }
    fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Commands => "Commands",
            Tab::Projects => "Projects",
            Tab::Tags => "Tags",
            Tab::Search => "Search",
        }
    }
    fn next(self) -> Tab {
        let tabs = Self::all();
        let idx = tabs.iter().position(|t| *t == self).unwrap();
        tabs[(idx + 1) % tabs.len()]
    }
    fn prev(self) -> Tab {
        let tabs = Self::all();
        let idx = tabs.iter().position(|t| *t == self).unwrap();
        tabs[(idx + tabs.len() - 1) % tabs.len()]
    }
}

// ─── Modes ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Search,
    Filter,
    Detail,
    CreateEntity,
    EditEntity,
    CreateProject,
    ConfirmDelete,
    Help,
}

// ─── Form state ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
struct EntityForm {
    name: String,
    description: String,
    content: String,
    type_id: String,
    project_id: String,
    tags: String,
    field_index: usize,
}

impl EntityForm {
    const FIELDS: [&'static str; 6] = [
        "Name",
        "Description",
        "Content",
        "Type (cmd/script/app/wf/env/cfg/sec)",
        "Project ID (optional)",
        "Tags (comma-sep)",
    ];

    fn from_entity(e: &Entity) -> Self {
        Self {
            name: e.name.clone(),
            description: e.description.clone().unwrap_or_default(),
            content: e.content.clone().unwrap_or_default(),
            type_id: e.type_id.clone(),
            project_id: e.project_id.clone().unwrap_or_default(),
            tags: String::new(),
            field_index: 0,
        }
    }

    fn to_create(&self) -> CreateEntity {
        CreateEntity {
            name: self.name.clone(),
            description: if self.description.is_empty() { None } else { Some(self.description.clone()) },
            content: if self.content.is_empty() { None } else { Some(self.content.clone()) },
            type_id: if self.type_id.is_empty() { "cmd".to_string() } else { self.type_id.clone() },
            project_id: if self.project_id.is_empty() { None } else { Some(self.project_id.clone()) },
            tags: if self.tags.is_empty() {
                None
            } else {
                Some(self.tags.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            },
            metadata_json: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ProjectForm {
    name: String,
    description: String,
    field_index: usize,
}

impl ProjectForm {
    const FIELDS: [&'static str; 2] = ["Name", "Description"];
}

// ─── Delete target ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum DeleteTarget {
    Entity(String),
    Project(String),
}

// ─── App state ──────────────────────────────────────────────────────────────

pub struct App {
    pub tab: Tab,
    pub mode: Mode,
    pub input: String,
    pub filter_input: String,
    pub items: Vec<String>,
    pub entities: Vec<Entity>,
    pub projects: Vec<Project>,
    pub tags: Vec<Tag>,
    pub types: Vec<EntityType>,
    pub selected: usize,
    pub db_path: String,
    pub active_project: String,
    pub active_project_id: Option<String>,
    pub theme: ThemeConfig,
    pub message: String,
    pub entity_form: EntityForm,
    pub project_form: ProjectForm,
    pub editing_id: Option<String>,
    pub delete_target: DeleteTarget,
}

impl App {
    pub fn new(db_path: &str, theme: ThemeConfig) -> Self {
        Self {
            tab: Tab::Dashboard,
            mode: Mode::Normal,
            input: String::new(),
            filter_input: String::new(),
            items: Vec::new(),
            entities: Vec::new(),
            projects: Vec::new(),
            tags: Vec::new(),
            types: Vec::new(),
            selected: 0,
            db_path: db_path.to_string(),
            active_project: "all".to_string(),
            active_project_id: None,
            theme,
            message: String::new(),
            entity_form: EntityForm::default(),
            project_form: ProjectForm::default(),
            editing_id: None,
            delete_target: DeleteTarget::Entity(String::new()),
        }
    }

    fn selected_entity(&self) -> Option<&Entity> {
        self.entities.get(self.selected)
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.items.len();
        if len == 0 {
            return;
        }
        let new_idx = (self.selected as i32 + delta).rem_euclid(len as i32) as usize;
        self.selected = new_idx;
    }

    fn set_message(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
    }
}

// ─── Main run loop ───────────────────────────────────────────────────────────

pub async fn run(app: &mut App, pool: Arc<SqlitePool>) -> anyhow::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        refresh_data(app, &pool).await;
        terminal.draw(|f| draw(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
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

async fn refresh_data(app: &mut App, pool: &SqlitePool) {
    app.projects = repository::list_projects(pool).await.unwrap_or_default();
    app.tags = repository::list_tags(pool).await.unwrap_or_default();
    app.types = repository::list_types(pool).await.unwrap_or_default();

    match app.tab {
        Tab::Dashboard => {
            app.entities = repository::list_entities(pool, None, None).await.unwrap_or_default();
            app.items = Vec::new();
        }
        Tab::Commands => {
            app.entities = if app.filter_input.is_empty() {
                repository::list_entities(pool, None, app.active_project_id.as_deref())
                    .await
                    .unwrap_or_default()
            } else {
                let tags: Vec<String> = app
                    .filter_input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                repository::filter_by_tags(pool, &tags).await.unwrap_or_default()
            };
            app.items = app
                .entities
                .iter()
                .map(|e| {
                    let desc = e.description.as_deref().unwrap_or("");
                    format!("[{}] {} — {}", e.type_id, e.name, desc)
                })
                .collect();
        }
        Tab::Projects => {
            app.entities = Vec::new();
            app.items = app
                .projects
                .iter()
                .map(|p| {
                    let marker = if Some(&p.id) == app.active_project_id.as_ref() {
                        "* "
                    } else {
                        "  "
                    };
                    format!("{}{} — {}", marker, p.name, p.description.as_deref().unwrap_or(""))
                })
                .collect();
        }
        Tab::Tags => {
            app.entities = Vec::new();
            app.items = app.tags.iter().map(|t| t.name.clone()).collect();
        }
        Tab::Search => {
            app.entities = if app.input.is_empty() {
                Vec::new()
            } else {
                let results = repository::search_entities(pool, &app.input).await.unwrap_or_default();
                let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
                let mut entities = Vec::new();
                for id in ids {
                    if let Ok(e) = repository::get_entity(pool, &id).await {
                        entities.push(e);
                    }
                }
                entities
            };
            app.items = app
                .entities
                .iter()
                .map(|e| {
                    let desc = e.description.as_deref().unwrap_or("");
                    format!("[{}] {} — {}", e.type_id, e.name, desc)
                })
                .collect();
        }
    }

    if app.selected >= app.items.len() && !app.items.is_empty() {
        app.selected = app.items.len() - 1;
    } else if app.items.is_empty() {
        app.selected = 0;
    }
}

// ─── Key handling ───────────────────────────────────────────────────────────

async fn handle_key(app: &mut App, key: KeyCode, pool: &SqlitePool) {
    match app.mode {
        Mode::Search => {
            match key {
                KeyCode::Esc => {
                    app.mode = Mode::Normal;
                    app.input.clear();
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Char(c) => {
                    app.input.push(c);
                }
                _ => {}
            }
            return;
        }
        Mode::Filter => {
            match key {
                KeyCode::Esc => {
                    app.mode = Mode::Normal;
                    app.filter_input.clear();
                }
                KeyCode::Backspace => {
                    app.filter_input.pop();
                }
                KeyCode::Char(c) => {
                    app.filter_input.push(c);
                }
                KeyCode::Enter => {
                    app.mode = Mode::Normal;
                }
                _ => {}
            }
            return;
        }
        Mode::CreateEntity | Mode::EditEntity => {
            handle_entity_form(app, key, pool).await;
            return;
        }
        Mode::CreateProject => {
            handle_project_form(app, key, pool).await;
            return;
        }
        Mode::Detail => {
            match key {
                KeyCode::Esc => {
                    app.mode = Mode::Normal;
                }
                KeyCode::Char('c') => {
                    if let Some(e) = app.selected_entity() {
                        let text = e.content.as_deref().unwrap_or(&e.name);
                        match arboard::Clipboard::new() {
                            Ok(mut cb) => match cb.set_text(text.to_string()) {
                                Ok(_) => app.set_message("✓ Copied to clipboard!"),
                                Err(_) => app.set_message("✗ Failed to copy"),
                            },
                            Err(_) => app.set_message("✗ Clipboard unavailable"),
                        }
                    }
                }
                KeyCode::Char('r') => {
                    let run_info = app
                        .selected_entity()
                        .and_then(|e| e.content.as_ref().map(|c| (e.name.clone(), c.clone())));
                    if let Some((name, content)) = run_info {
                        app.mode = Mode::Normal;
                        app.set_message(format!("Running: {}...", name));
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
                                app.set_message(if status.success() {
                                    format!("✓ {} completed successfully", name)
                                } else {
                                    format!("✗ {} exited with error", name)
                                });
                            }
                            Err(_) => app.set_message("✗ Failed to execute"),
                        }
                    }
                }
                KeyCode::Char('e') => {
                    // Clone to avoid borrow conflict with app.entity_form
                    if let Some(e) = app.selected_entity().cloned() {
                        app.entity_form = EntityForm::from_entity(&e);
                        app.editing_id = Some(e.id.clone());
                        app.mode = Mode::EditEntity;
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(e) = app.selected_entity() {
                        app.delete_target = DeleteTarget::Entity(e.id.clone());
                        app.mode = Mode::ConfirmDelete;
                    }
                }
                _ => {}
            }
            return;
        }
        Mode::ConfirmDelete => {
            match key {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    // Clone to avoid borrow conflict with mutable app
                    let target = app.delete_target.clone();
                    match &target {
                        DeleteTarget::Entity(id) => {
                            match repository::delete_entity(pool, id).await {
                                Ok(_) => {
                                    app.set_message("✓ Deleted successfully");
                                    if app.selected > 0 {
                                        app.selected -= 1;
                                    }
                                }
                                Err(e) => app.set_message(format!("✗ Delete failed: {}", e)),
                            }
                        }
                        DeleteTarget::Project(id) => {
                            let id = id.clone();
                            match repository::delete_project(pool, &id).await {
                                Ok(_) => {
                                    app.set_message("✓ Project deleted");
                                    if app.active_project_id.as_ref() == Some(&id) {
                                        app.active_project_id = None;
                                        app.active_project = "all".to_string();
                                    }
                                }
                                Err(e) => app.set_message(format!("✗ Delete failed: {}", e)),
                            }
                        }
                    }
                    app.mode = Mode::Normal;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    app.mode = Mode::Normal;
                    app.set_message("Delete cancelled");
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

    // Normal mode
    match key {
        KeyCode::Tab => {
            app.tab = app.tab.next();
            app.selected = 0;
        }
        KeyCode::Left => {
            app.tab = app.tab.prev();
            app.selected = 0;
        }
        KeyCode::Right => {
            app.tab = app.tab.next();
            app.selected = 0;
        }
        KeyCode::Up => {
            app.move_selection(-1);
        }
        KeyCode::Down => {
            app.move_selection(1);
        }
        KeyCode::Char('?') => {
            app.mode = Mode::Help;
        }
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
        KeyCode::Char('n') => match app.tab {
            Tab::Commands | Tab::Search => {
                app.entity_form = EntityForm {
                    type_id: "cmd".to_string(),
                    ..Default::default()
                };
                app.editing_id = None;
                app.mode = Mode::CreateEntity;
            }
            Tab::Projects => {
                app.project_form = ProjectForm::default();
                app.mode = Mode::CreateProject;
            }
            _ => {}
        },
        KeyCode::Enter => match app.tab {
            Tab::Commands | Tab::Search => {
                if app.selected_entity().is_some() {
                    app.mode = Mode::Detail;
                }
            }
            Tab::Projects => {
                if let Some(p) = app.projects.get(app.selected) {
                    if app.active_project_id.as_ref() == Some(&p.id) {
                        app.active_project_id = None;
                        app.active_project = "all".to_string();
                        app.set_message("Switched to all projects");
                    } else {
                        app.active_project_id = Some(p.id.clone());
                        app.active_project = p.name.clone();
                        app.set_message(format!("Switched to project: {}", p.name));
                    }
                }
            }
            _ => {}
        },
        KeyCode::Char('d') if app.tab == Tab::Projects => {
            if let Some(p) = app.projects.get(app.selected) {
                app.delete_target = DeleteTarget::Project(p.id.clone());
                app.mode = Mode::ConfirmDelete;
            }
        }
        _ => {}
    }
}

// ─── Entity form handling ───────────────────────────────────────────────────

async fn handle_entity_form(app: &mut App, key: KeyCode, pool: &SqlitePool) {
    let form = &mut app.entity_form;
    match key {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Tab => {
            form.field_index = (form.field_index + 1) % EntityForm::FIELDS.len();
        }
        KeyCode::BackTab => {
            form.field_index = (form.field_index + EntityForm::FIELDS.len() - 1) % EntityForm::FIELDS.len();
        }
        KeyCode::Up => {
            form.field_index = (form.field_index + EntityForm::FIELDS.len() - 1) % EntityForm::FIELDS.len();
        }
        KeyCode::Down => {
            form.field_index = (form.field_index + 1) % EntityForm::FIELDS.len();
        }
        KeyCode::Enter => {
            let req = form.to_create();
            if form.name.is_empty() {
                app.set_message("✗ Name is required");
                return;
            }
            match &app.editing_id {
                Some(id) => match repository::update_entity(pool, id, &req).await {
                    Ok(e) => {
                        app.set_message(format!("✓ Updated: {}", e.name));
                        app.mode = Mode::Normal;
                    }
                    Err(e) => app.set_message(format!("✗ Update failed: {}", e)),
                },
                None => match repository::create_entity(pool, &req).await {
                    Ok(e) => {
                        app.set_message(format!("✓ Created: {}", e.name));
                        app.mode = Mode::Normal;
                    }
                    Err(e) => app.set_message(format!("✗ Create failed: {}", e)),
                },
            }
        }
        KeyCode::Backspace => match form.field_index {
            0 => {
                form.name.pop();
            }
            1 => {
                form.description.pop();
            }
            2 => {
                form.content.pop();
            }
            3 => {
                form.type_id.pop();
            }
            4 => {
                form.project_id.pop();
            }
            5 => {
                form.tags.pop();
            }
            _ => {}
        },
        KeyCode::Char(c) => match form.field_index {
            0 => {
                form.name.push(c);
            }
            1 => {
                form.description.push(c);
            }
            2 => {
                form.content.push(c);
            }
            3 => {
                form.type_id.push(c);
            }
            4 => {
                form.project_id.push(c);
            }
            5 => {
                form.tags.push(c);
            }
            _ => {}
        },
        _ => {}
    }
}

// ─── Project form handling ──────────────────────────────────────────────────

async fn handle_project_form(app: &mut App, key: KeyCode, pool: &SqlitePool) {
    let form = &mut app.project_form;
    match key {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Tab | KeyCode::Down => {
            form.field_index = (form.field_index + 1) % ProjectForm::FIELDS.len();
        }
        KeyCode::Up => {
            form.field_index = (form.field_index + ProjectForm::FIELDS.len() - 1) % ProjectForm::FIELDS.len();
        }
        KeyCode::Enter => {
            if form.name.is_empty() {
                app.set_message("✗ Name is required");
                return;
            }
            let req = CreateProject {
                name: form.name.clone(),
                description: if form.description.is_empty() {
                    None
                } else {
                    Some(form.description.clone())
                },
            };
            match repository::create_project(pool, &req).await {
                Ok(p) => {
                    app.set_message(format!("✓ Project created: {}", p.name));
                    app.mode = Mode::Normal;
                }
                Err(e) => app.set_message(format!("✗ Create failed: {}", e)),
            }
        }
        KeyCode::Backspace => match form.field_index {
            0 => {
                form.name.pop();
            }
            1 => {
                form.description.pop();
            }
            _ => {}
        },
        KeyCode::Char(c) => match form.field_index {
            0 => {
                form.name.push(c);
            }
            1 => {
                form.description.push(c);
            }
            _ => {}
        },
        _ => {}
    }
}

// ─── Drawing ────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tabs
            Constraint::Min(1),     // Main content
            Constraint::Length(3),  // Input/Help/Message
            Constraint::Length(1),  // Status bar
        ])
        .split(f.area());

    draw_tabs(f, app, chunks[0]);
    draw_content(f, app, chunks[1]);
    draw_bottom(f, app, chunks[2]);
    draw_status_bar(f, app, chunks[3]);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Span> = Tab::all()
        .into_iter()
        .map(|t| Span::styled(t.title(), Style::default().add_modifier(Modifier::BOLD)))
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("TUI-OP-HUB"))
        .select(app.tab as usize);
    f.render_widget(tabs, area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        Mode::Help => draw_help(f, app, area),
        Mode::CreateEntity | Mode::EditEntity => draw_entity_form(f, app, area),
        Mode::CreateProject => draw_project_form(f, app, area),
        Mode::ConfirmDelete => draw_confirm_delete(f, app, area),
        _ => match app.tab {
            Tab::Dashboard => draw_dashboard(f, app, area),
            _ => draw_list(f, app, area),
        },
    }
}

fn draw_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(1)])
        .split(area);

    let stats = format!(
        "Commands: {}  |  Projects: {}  |  Tags: {}  |  Types: {}\n\
         Active Project: {}\n\
         Database: {}\n\
         \n\
         Quick Actions:\n\
           n — Create new entity/command    f — Filter by tags\n\
           / — Search (on Search tab)      ? — Help\n\
           Tab — Switch tabs                q — Quit",
        app.entities.len(),
        app.projects.len(),
        app.tags.len(),
        app.types.len(),
        app.active_project,
        app.db_path,
    );
    let stats_panel = Paragraph::new(stats)
        .block(Block::default().borders(Borders::ALL).title("Dashboard"))
        .wrap(Wrap { trim: true });
    f.render_widget(stats_panel, chunks[0]);

    let items: Vec<ListItem> = app
        .entities
        .iter()
        .take(20)
        .map(|e| {
            ListItem::new(Line::from(Span::raw(format!(
                "[{}] {} — {}",
                e.type_id,
                e.name,
                e.description.as_deref().unwrap_or("")
            ))))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Recent Entities"));
    f.render_widget(list, chunks[1]);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let show_selection = matches!(app.tab, Tab::Commands | Tab::Search | Tab::Projects);
    let items: Vec<ListItem> = app
        .items
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let style = if i == app.selected && show_selection {
                Style::default()
                    .fg(app.theme.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(s.as_str(), style)))
        })
        .collect();
    let title = format!("{} ({})", app.tab.title(), app.items.len());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}

fn draw_entity_form(f: &mut Frame, app: &App, area: Rect) {
    let form = &app.entity_form;
    let title = if app.mode == Mode::EditEntity {
        "Edit Entity"
    } else {
        "Create Entity"
    };
    let values = [
        &form.name,
        &form.description,
        &form.content,
        &form.type_id,
        &form.project_id,
        &form.tags,
    ];
    let lines: Vec<Line> = EntityForm::FIELDS
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let value = values[i].as_str();
            let prefix = if i == form.field_index { "▶ " } else { "  " };
            let style = if i == form.field_index {
                Style::default()
                    .fg(app.theme.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(Span::styled(
                format!("{}{}: {}", prefix, label, value),
                style,
            ))
        })
        .collect();

    let help = "\n\nTab/Up/Down: switch fields | Enter: submit | Esc: cancel";
    let content = format!(
        "{}\n{}",
        lines
            .into_iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        help
    );

    let para = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_project_form(f: &mut Frame, app: &App, area: Rect) {
    let form = &app.project_form;
    let values = [&form.name, &form.description];
    let lines: Vec<Line> = ProjectForm::FIELDS
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let value = values[i].as_str();
            let prefix = if i == form.field_index { "▶ " } else { "  " };
            let style = if i == form.field_index {
                Style::default()
                    .fg(app.theme.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(Span::styled(
                format!("{}{}: {}", prefix, label, value),
                style,
            ))
        })
        .collect();

    let help = "\n\nTab/Up/Down: switch fields | Enter: submit | Esc: cancel";
    let content = format!(
        "{}\n{}",
        lines
            .into_iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        help
    );

    let para = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Create Project"))
        .wrap(Wrap { trim: true });
    f.render_widget(para, area);
}

fn draw_confirm_delete(f: &mut Frame, app: &App, area: Rect) {
    let msg = match &app.delete_target {
        DeleteTarget::Entity(_) => "⚠ CONFIRM DELETE ENTITY — Press Y to confirm, N to cancel",
        DeleteTarget::Project(_) => "⚠ CONFIRM DELETE PROJECT — Press Y to confirm, N to cancel",
    };
    let para = Paragraph::new(msg)
        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Confirmation"));
    f.render_widget(Clear, area);
    f.render_widget(para, area);
}

fn draw_help(f: &mut Frame, _app: &App, area: Rect) {
    let help_text = "\
TUI-OP-HUB Keybindings
═══════════════════════════════════════════════════

  Navigation:
    Tab / Left/Right  — Switch tabs
    Up/Down           — Navigate list items
    Enter             — Select item / Switch project
    ?                 — Toggle this help
    q                 — Quit (in Normal mode)

  Creating & Editing:
    n                 — Create new entity/project
    e                 — Edit entity (in detail view)
    d                 — Delete entity/project (with confirmation)

  Search & Filter:
    /                 — Start search (on Search tab)
    f                 — Filter by tags

  Detail View:
    c                 — Copy content to clipboard
    r                 — Run command/script
    e                 — Edit entity
    d                 — Delete entity (with confirmation)
    Esc               — Back to list

  Projects:
    Enter             — Switch active project
    d                 — Delete project
    n                 — Create new project

  Forms:
    Tab/Up/Down       — Switch fields
    Enter             — Submit form
    Esc               — Cancel

Press Esc or ? to close this help.";
    let para = Paragraph::new(help_text)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(Clear, area);
    f.render_widget(para, area);
}

fn draw_bottom(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        Mode::Search => {
            let input = Paragraph::new(app.input.as_str())
                .style(Style::default().fg(app.theme.accent_color()))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Search (Esc to cancel)"),
                );
            f.render_widget(input, area);
        }
        Mode::Filter => {
            let input = Paragraph::new(app.filter_input.as_str())
                .style(Style::default().fg(Color::Cyan))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Filter by tags (comma-separated, Esc to cancel)"),
                );
            f.render_widget(input, area);
        }
        Mode::Detail => {
            if let Some(e) = app.selected_entity() {
                let detail = format!(
                    "Name: {}  |  Type: {}  |  Description: {}\n\
                     Content: {}\n\n\
                     [c] Copy  [r] Run  [e] Edit  [d] Delete  [Esc] Back",
                    e.name,
                    e.type_id,
                    e.description.as_deref().unwrap_or("(none)"),
                    e.content.as_deref().unwrap_or("(none)"),
                );
                let para = Paragraph::new(detail)
                    .wrap(Wrap { trim: true })
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("Entity: {}", e.name)),
                    );
                f.render_widget(para, area);
            }
        }
        _ => {
            let help_text = if app.message.is_empty() {
                match app.tab {
                    Tab::Dashboard => "n: create | f: filter | ?: help | q: quit",
                    Tab::Search => {
                        "Type / to search | Up/Down: navigate | Enter: detail | n: create | ?: help | q: quit"
                    }
                    Tab::Projects => {
                        "Up/Down: navigate | Enter: switch project | n: create | d: delete | ?: help | q: quit"
                    }
                    _ => {
                        "Up/Down: navigate | Enter: detail | n: create | f: filter | d: delete | ?: help | q: quit"
                    }
                }
            } else {
                app.message.as_str()
            };
            let help = Paragraph::new(help_text)
                .block(Block::default().borders(Borders::ALL).title("Help"));
            f.render_widget(help, area);
        }
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let status = format!(
        " DB: {} | Project: {} | Tab: {} | Mode: {:?} | Items: {} ",
        app.db_path,
        app.active_project,
        app.tab.title(),
        app.mode,
        app.items.len(),
    );
    let status_bar = Paragraph::new(status)
        .style(Style::default().bg(app.theme.status_bg_color()).fg(Color::White));
    f.render_widget(status_bar, area);
}
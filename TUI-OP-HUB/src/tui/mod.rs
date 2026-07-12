//! TUI layer (US-TUI-01, US-TUI-02, US-TUI-03, US-TUI-08, US-TUI-10, US-DEP-02).
use crossterm::{event::{self, Event, KeyCode, KeyEventKind}, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}, execute};
use ratatui::{backend::CrosstermBackend, layout::{Constraint, Direction, Layout}, style::{Color, Modifier, Style}, text::Span, widgets::{Block, Borders, List, ListItem, Paragraph, Tabs}, Terminal, Frame};
use std::io;
use sqlx::SqlitePool;
use std::sync::Arc;
use crate::repository;

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

pub struct App {
    pub tab: Tab,
    pub input: String,
    pub items: Vec<String>,
    pub db_path: String,
    pub active_project: String,
}

impl App {
    pub fn new(db_path: &str) -> Self {
        Self { tab: Tab::Commands, input: String::new(), items: Vec::new(), db_path: db_path.to_string(), active_project: "default".to_string() }
    }
}

pub async fn run(app: &mut App, pool: Arc<SqlitePool>) -> anyhow::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        app.items = match app.tab {
            Tab::Commands => {
                let entities = repository::list_entities(&pool, None, None).await.unwrap_or_default();
                entities.iter().map(|e| format!("{} | {}", e.name, e.type_id)).collect()
            }
            Tab::Projects => {
                let projects = repository::list_projects(&pool).await.unwrap_or_default();
                projects.iter().map(|p| p.name.clone()).collect()
            }
            Tab::Tags => {
                let tags = repository::list_tags(&pool).await.unwrap_or_default();
                tags.iter().map(|t| t.name.clone()).collect()
            }
            Tab::Search => {
                if app.input.is_empty() { Vec::new() }
                else {
                    let results = repository::search_entities(&pool, &app.input).await.unwrap_or_default();
                    results.iter().map(|r| format!("{} | {}", r.name, r.type_id)).collect()
                }
            }
        };

        terminal.draw(|f| draw(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => {
                        let tabs = Tab::all();
                        let idx = tabs.iter().position(|t| *t == app.tab).unwrap();
                        app.tab = tabs[(idx + 1) % tabs.len()];
                    }
                    KeyCode::Char(c) if app.tab == Tab::Search => { app.input.push(c); }
                    KeyCode::Backspace if app.tab == Tab::Search => { app.input.pop(); }
                    KeyCode::Left => {
                        let tabs = Tab::all();
                        let idx = tabs.iter().position(|t| *t == app.tab).unwrap();
                        app.tab = tabs[(idx + tabs.len() - 1) % tabs.len()];
                    }
                    KeyCode::Right => {
                        let tabs = Tab::all();
                        let idx = tabs.iter().position(|t| *t == app.tab).unwrap();
                        app.tab = tabs[(idx + 1) % tabs.len()];
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3), Constraint::Length(1)])
        .split(f.area());

    let titles = Tab::all().into_iter().map(|t| Span::styled(t.title(), Style::default().add_modifier(Modifier::BOLD)));
    let tabs = Tabs::new(titles.collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title("TUI-OP-HUB"))
        .select(app.tab as usize);
    f.render_widget(tabs, chunks[0]);

    let items: Vec<ListItem> = app.items.iter().map(|s| ListItem::new(s.as_str())).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(app.tab.title()));
    f.render_widget(list, chunks[1]);

    if app.tab == Tab::Search {
        let input = Paragraph::new(app.input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title("Search (type to search, q to quit)"));
        f.render_widget(input, chunks[2]);
    } else {
        let help = Paragraph::new("Tab/Left/Right: switch tabs | q: quit")
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help, chunks[2]);
    }

    let status = format!(" DB: {} | Project: {} | Tab: {} ", app.db_path, app.active_project, app.tab.title());
    let status_bar = Paragraph::new(status).style(Style::default().bg(Color::Blue).fg(Color::White));
    f.render_widget(status_bar, chunks[3]);
}

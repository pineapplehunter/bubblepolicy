use color_eyre::{Result, eyre::WrapErr};
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        self,
        event::{self, Event, KeyCode},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
        tty::IsTty,
    },
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, Borders, Paragraph},
};
use std::collections::HashMap;
use std::fs;
use std::io;

use crate::common::{entries_to_string, parse_entries, Access, PolicyEntry};
use crate::tree_widget::{Tree, TreeItem, TreeState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AllowState {
    Deny,
    RO,
    RW,
    Tmp,
    Partial,
}

pub struct App {
    items: Vec<TreeItem<'static, String>>,
    state: TreeState<String>,
    allow_states: HashMap<String, AllowState>,
    dirty: bool,
    filename: String,
    show_debug: bool,
}

impl App {
    pub fn from_entries(entries: Vec<PolicyEntry>, filename: String) -> Self {
        let mut allow_states = HashMap::new();

        let mut path_map: HashMap<String, (AllowState, Vec<String>)> = HashMap::new();

        for entry in entries {
            let full_path = if entry.path == "/" {
                "/".to_string()
            } else {
                entry.path.clone()
            };

            let allow_state = match entry.access {
                Access::Deny => AllowState::Deny,
                Access::ReadOnly => AllowState::RO,
                Access::ReadWrite => AllowState::RW,
                Access::Tmpfs => AllowState::Tmp,
            };
            allow_states.insert(full_path.clone(), allow_state);

            let parts: Vec<&str> = entry.path.split('/').filter(|s| !s.is_empty()).collect();
            let path_parts: Vec<String> = parts.iter().map(|s| s.to_string()).collect();

            path_map.insert(full_path, (allow_state, path_parts));
        }

        let root_children = build_tree_from_paths(&path_map, "/");

        App {
            items: root_children,
            state: TreeState::default(),
            allow_states,
            dirty: false,
            filename,
            show_debug: false,
        }
    }

    fn set_state(&mut self, state: AllowState) {
        let selected_paths = self.state.selected_cloned();
        if let Some(path) = selected_paths.last() {
            let icon = state_icon(state);
            self.allow_states.insert(path.clone(), state);
            self.dirty = true;
            update_item_text_in_tree(&mut self.items, path, icon);
        }
    }

    fn get_item_state(&self, identifier: &str) -> AllowState {
        self.allow_states.get(identifier).copied().unwrap_or(AllowState::Partial)
    }

    fn to_entries(&self) -> Vec<PolicyEntry> {
        let mut entries = Vec::new();
        self.collect_entries_recursive(&self.items, &mut entries);
        entries
    }

    fn collect_entries_recursive(&self, items: &[TreeItem<'static, String>], entries: &mut Vec<PolicyEntry>) {
        for item in items {
            let path = item.identifier();
            if let Some(state) = self.allow_states.get(path)
                && *state != AllowState::Partial
            {
                let access = match state {
                    AllowState::Deny => Access::Deny,
                    AllowState::RO => Access::ReadOnly,
                    AllowState::RW => Access::ReadWrite,
                    AllowState::Tmp => Access::Tmpfs,
                    AllowState::Partial => continue,
                };
                entries.push(PolicyEntry {
                    path: path.clone(),
                    access,
                });
            }
            self.collect_entries_recursive(item.children(), entries);
        }
    }

    pub fn select_first(&mut self) {
        self.state.select_first_item(&self.items);
    }
}

fn build_tree_from_paths(path_map: &HashMap<String, (AllowState, Vec<String>)>, parent_path: &str) -> Vec<TreeItem<'static, String>> {
    let parent_parts: Vec<&str> = if parent_path == "/" {
        Vec::new()
    } else {
        parent_path.split('/').filter(|s| !s.is_empty()).collect()
    };

    let mut child_names: Vec<String> = path_map
        .keys()
        .filter_map(|p| {
            let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
            if parts.is_empty() {
                return None;
            }
            if parent_parts.is_empty() {
                Some(parts[0].to_string())
            } else if parts.len() > parent_parts.len() 
                && parts[..parent_parts.len()].iter().eq(parent_parts.iter()) 
            {
                Some(parts[parent_parts.len()].to_string())
            } else {
                None
            }
        })
        .collect();

    child_names.sort();
    child_names.dedup();

    let mut result = Vec::new();
    for child_name in child_names {
        let child_path = if parent_path == "/" {
            format!("/{}", child_name)
        } else {
            format!("{}/{}", parent_path, child_name)
        };

        let child_children = build_tree_from_paths(path_map, &child_path);

        let state = path_map
            .get(&child_path)
            .map(|(s, _)| *s)
            .unwrap_or(AllowState::Partial);
        let icon = state_icon(state);
        let item_text = format!("{} {}", icon, child_name);

        let item = TreeItem::new(
            child_path.clone(),
            item_text,
            child_children,
        );

        if let Ok(item) = item {
            result.push(item);
        }
    }

    result.sort_by(|a, b| a.identifier().cmp(b.identifier()));
    result
}

fn state_icon(state: AllowState) -> &'static str {
    match state {
        AllowState::Deny => "✗",
        AllowState::Partial => "○",
        AllowState::RO => "◐",
        AllowState::RW => "●",
        AllowState::Tmp => "◆",
    }
}

fn update_item_text_in_tree(items: &mut [TreeItem<'static, String>], path: &str, icon: &str) -> bool {
    for item in items.iter_mut() {
        let id = item.identifier().clone();
        if id == path {
            let name = id.rsplit('/').next().unwrap_or(&id);
            *item.text_mut() = Text::from(format!("{} {}", icon, name));
            return true;
        }
    }
    for item in items.iter_mut() {
        if update_item_text_in_tree(item.children_mut(), path, icon) {
            return true;
        }
    }
    false
}

pub fn run(filename: &str) -> Result<()> {
    let data = fs::read_to_string(filename)
        .with_context(|| format!("Failed to read file: {}", filename))?;

    let entries = parse_entries(&data);
    let mut app = App::from_entries(entries, filename.to_string());

    if !std::io::stdout().is_tty() {
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let size = ratatui::layout::Rect::new(0, 0, 80, 24);
    terminal.resize(size)?;

    terminal.draw(|f| ui(f, &mut app))?;

    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    } else if app.dirty {
        let entries = app.to_entries();
        let output = entries_to_string(&entries);
        fs::write(&app.filename, output)
            .with_context(|| format!("Failed to write file: {}", app.filename))?;
        eprintln!("Updated: {}", app.filename);
    }

    Ok(())
}

fn run_app<W: io::Write>(
    terminal: &mut ratatui::Terminal<CrosstermBackend<W>>,
    app: &mut App,
) -> Result<()> {
    app.state.select_first_item(&app.items);

    loop {
        terminal.draw(|f| ui(f, app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('d') => app.set_state(AllowState::Deny),
                KeyCode::Char('D') => app.show_debug = !app.show_debug,
                KeyCode::Char('r') => app.set_state(AllowState::RO),
                KeyCode::Char('w') => app.set_state(AllowState::RW),
                KeyCode::Char('t') => app.set_state(AllowState::Tmp),
                KeyCode::Char('p') => app.set_state(AllowState::Partial),
                KeyCode::Char(' ') => {
                    app.state.toggle_selected();
                }
                KeyCode::Up => {
                    app.state.key_up();
                }
                KeyCode::Down => {
                    app.state.key_down();
                }
                KeyCode::Left => {
                    app.state.key_left();
                }
                KeyCode::Right => {
                    app.state.key_right();
                }
                KeyCode::Char('h') => {
                    app.state.key_left();
                }
                KeyCode::Char('j') => {
                    app.state.key_down();
                }
                KeyCode::Char('k') => {
                    app.state.key_up();
                }
                KeyCode::Char('l') => {
                    app.state.key_right();
                }
                _ => {}
            }
        }
    }

    Ok(())
}

pub fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(if app.show_debug { 10 } else { 0 }),
        ])
        .split(f.area());

    let selected = app.state.selected();
    let highlight_symbol = if let Some(path) = selected.first() {
        state_icon(app.get_item_state(path)).to_string()
    } else {
        String::new()
    };

    let tree_widget = Tree::new(&app.items)
        .expect("all item identifiers are unique")
        .block(
            Block::default()
                .title("File Tree (d=✗ Deny r=◐ RO w=● RW t=◆ Tmp p=○ Partial)")
                .borders(Borders::ALL),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(&highlight_symbol)
        .node_closed_symbol("▶ ")
        .node_open_symbol("▼ ")
        .node_no_children_symbol("  ");

    f.render_stateful_widget(tree_widget, chunks[0], &mut app.state);

    let help_text = "d=✗ Deny r=◐ RO w=● RW t=◆ Tmp p=○ Partial | Space=expand | hkjl/←→=navigate | q=quit | D=debug";
    let help_widget = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    f.render_widget(help_widget, chunks[1]);
}

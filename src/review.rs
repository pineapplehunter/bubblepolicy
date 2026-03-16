use color_eyre::{eyre::WrapErr, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use serde::{Deserialize, Serialize};
use std::io;
use std::{fs, io::Read};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    pub path: String,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct TraceOutput {
    files: Vec<FileAccess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub path: String,
    pub allowed: bool,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Policy {
    pub entries: Vec<PolicyEntry>,
}

fn print_or_write(json: &str, output: Option<&str>, _default_filename: &str) -> Result<()> {
    if let Some(path) = output {
        fs::write(path, json).with_context(|| format!("Failed to write output file: {}", path))?;
        eprintln!("Output written to: {}", path);
    } else {
        println!("{}", json);
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct TreeNode {
    path: String,
    display_name: String,
    allowed: bool,
    expanded: bool,
    children: Vec<usize>, // indices into the flat tree
    level: usize,
    is_file: bool,
    read: bool,
    write: bool,
}

struct App {
    tree: Vec<TreeNode>,
    #[allow(dead_code)]
    file_access_map: std::collections::BTreeMap<String, FileAccess>,
    selected_idx: usize,
    scroll_offset: usize,
    dirty: bool,
}

impl App {
    fn from_trace_output(output: TraceOutput) -> Self {
        let mut tree = Vec::new();
        let mut file_access_map: std::collections::BTreeMap<String, FileAccess> =
            std::collections::BTreeMap::new();

        // Build access map
        for file in &output.files {
            file_access_map.insert(file.path.clone(), file.clone());
        }

        let mut paths: Vec<String> = output.files.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        paths.dedup();

        // Build tree structure
        for path in paths {
            let parts: Vec<&str> = path.split('/').collect();
            let mut current_path = String::new();

            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() && i > 0 {
                    continue;
                }

                current_path = if i == 0 && part.is_empty() {
                    "/".to_string()
                } else if current_path == "/" {
                    format!("{}{}", current_path, part)
                } else {
                    format!("{}/{}", current_path, part)
                };

                // Check if node already exists
                let exists = tree.iter().any(|n: &TreeNode| n.path == current_path);
                if !exists {
                    let display_name = if current_path == "/" {
                        "/".to_string()
                    } else {
                        part.to_string()
                    };

                    let is_file = i == parts.len() - 1;
                    let (read, write) = if is_file {
                        if let Some(access) = file_access_map.get(&current_path) {
                            (access.read, access.write)
                        } else {
                            (false, false)
                        }
                    } else {
                        (false, false)
                    };

                    tree.push(TreeNode {
                        path: current_path.clone(),
                        display_name,
                        allowed: true,
                        expanded: current_path == "/",
                        children: Vec::new(),
                        level: i,
                        is_file,
                        read,
                        write,
                    });
                }
            }
        }

        // Build parent-child relationships
        for i in 0..tree.len() {
            let current = tree[i].clone();
            for j in (i + 1)..tree.len() {
                let other = &tree[j];
                if other.path.starts_with(&current.path)
                    && other.path.len() > current.path.len()
                    && other.level == current.level + 1
                {
                    tree[i].children.push(j);
                }
            }
        }

        App {
            tree,
            file_access_map,
            selected_idx: 0,
            scroll_offset: 0,
            dirty: false,
        }
    }

    fn toggle_selected(&mut self) {
        if self.selected_idx < self.tree.len() {
            self.tree[self.selected_idx].allowed = !self.tree[self.selected_idx].allowed;
            self.dirty = true;
        }
    }

    fn toggle_expanded(&mut self) {
        if self.selected_idx < self.tree.len() {
            self.tree[self.selected_idx].expanded = !self.tree[self.selected_idx].expanded;
        }
    }

    fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
        }
    }

    fn move_down(&mut self) {
        if self.selected_idx < self.tree.len() - 1 {
            self.selected_idx += 1;
        }
    }

    fn render_tree(&self, max_height: u16) -> Vec<String> {
        let mut lines = Vec::new();
        self.render_tree_recursive(0, max_height as usize, &mut lines);
        lines
    }

    fn render_tree_recursive(&self, idx: usize, max_height: usize, lines: &mut Vec<String>) {
        if idx >= self.tree.len() || lines.len() >= max_height {
            return;
        }

        let node = &self.tree[idx];

        // Build the display string
        let indent = "  ".repeat(node.level);
        let prefix = if node.children.is_empty() {
            "  "
        } else if node.expanded {
            "▼ "
        } else {
            "▶ "
        };

        let allowed_icon = if node.allowed { "✓" } else { "✗" };
        let line = format!(
            "{}{}{} [{}] {}",
            indent, prefix, allowed_icon, node.path, node.display_name
        );

        lines.push(line);

        // Render children if expanded
        if node.expanded {
            for &child_idx in &node.children {
                if lines.len() >= max_height {
                    break;
                }
                self.render_tree_recursive(child_idx, max_height, lines);
            }
        }
    }

    fn get_policy(&self) -> Policy {
        Policy {
            entries: self
                .tree
                .iter()
                .filter(|node| node.is_file)
                .map(|node| PolicyEntry {
                    path: node.path.clone(),
                    allowed: node.allowed,
                    read: node.read,
                    write: node.write,
                })
                .collect(),
        }
    }
}

pub fn run(paths: &[String], generate_policy: bool, output: Option<&str>) -> Result<()> {
    // Load and merge trace data from multiple files
    let mut all_files = Vec::new();

    if paths.is_empty() || (paths.len() == 1 && paths[0] == ".") {
        // Read from stdin
        let stdin = io::stdin();
        let mut buffer = String::new();
        stdin.lock().read_to_string(&mut buffer)?;

        if buffer.is_empty() {
            color_eyre::eyre::bail!(
                "No trace data provided. Usage:\n\
                 - Provide trace file(s): myjail review <trace1.json> [trace2.json] ...\n\
                 - Or pipe JSON: myjail trace -- <command> | myjail review"
            );
        }

        let trace_output: TraceOutput =
            serde_json::from_str(&buffer).context("Failed to parse trace JSON from stdin")?;
        all_files.extend(trace_output.files);
    } else {
        // Read from multiple files
        for path in paths {
            let trace_data = fs::read_to_string(path)
                .with_context(|| format!("Failed to read trace file: {}", path))?;

            let trace_output: TraceOutput = serde_json::from_str(&trace_data)
                .with_context(|| format!("Failed to parse trace JSON from file: {}", path))?;
            all_files.extend(trace_output.files);
        }
    }

    // Deduplicate and merge files
    let mut merged_files: std::collections::BTreeMap<String, FileAccess> =
        std::collections::BTreeMap::new();

    for file in all_files {
        merged_files
            .entry(file.path.clone())
            .and_modify(|f| {
                f.read = f.read || file.read;
                f.write = f.write || file.write;
                f.execute = f.execute || file.execute;
            })
            .or_insert(file);
    }

    let trace_output = TraceOutput {
        files: merged_files.into_values().collect(),
    };

    if generate_policy {
        // Generate policy without TUI (all entries allowed)
        let policy = Policy {
            entries: trace_output
                .files
                .iter()
                .map(|f| PolicyEntry {
                    path: f.path.clone(),
                    allowed: true,
                    read: f.read,
                    write: f.write,
                })
                .collect(),
        };

        let json = serde_json::to_string_pretty(&policy)?;
        print_or_write(&json, output, "policy.json")?;
        return Ok(());
    }

    let mut app = App::from_trace_output(trace_output);

    // Try to setup terminal for interactive mode
    // If not connected to a TTY, skip TUI and just output policy
    if !atty::is(atty::Stream::Stdout) {
        let policy = app.get_policy();
        let json = serde_json::to_string_pretty(&policy)?;
        println!("{}", json);
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Run app
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    } else if app.dirty {
        let policy = app.get_policy();
        let json = serde_json::to_string_pretty(&policy)?;
        print_or_write(&json, output, "policy.json")?;
    }

    Ok(())
}

fn run_app<W: io::Write>(
    terminal: &mut ratatui::Terminal<CrosstermBackend<W>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Up => app.move_up(),
                    KeyCode::Down => app.move_down(),
                    KeyCode::Char(' ') => app.toggle_selected(),
                    KeyCode::Right | KeyCode::Char('e') => app.toggle_expanded(),
                    KeyCode::Left => {
                        if app.tree[app.selected_idx].expanded {
                            app.toggle_expanded();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(f.size());

    // Render tree
    let tree_lines = app.render_tree(chunks[0].height);
    let tree_items: Vec<ListItem> = tree_lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let style = if idx == app.selected_idx {
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(line.clone()).style(style)
        })
        .collect();

    let tree_widget = List::new(tree_items).block(
        Block::default()
            .title("File Tree (↑/↓ navigate, SPACE toggle, e expand/collapse, q quit)")
            .borders(Borders::ALL),
    );

    f.render_widget(tree_widget, chunks[0]);

    // Render help text
    let help_text = "SPACE: Allow/Deny | e: Expand/Collapse | ↑↓: Navigate | q: Quit & Save";
    let help_widget = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    f.render_widget(help_widget, chunks[1]);
}

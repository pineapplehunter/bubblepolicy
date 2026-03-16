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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Access {
    Deny,
    ReadOnly,
    ReadWrite,
    Tmpfs,
}

impl Access {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Access::Deny)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub path: String,
    pub access: Access,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Policy {
    pub entries: Vec<PolicyEntry>,
}

fn write_policy(json: &str, output: &str) -> Result<()> {
    fs::write(output, json).with_context(|| format!("Failed to write output file: {}", output))?;
    eprintln!("Policy written to: {}", output);
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
enum AllowState {
    Deny,    // 1 - Deny access
    RO,      // 2 - Read-only access
    RW,      // 3 - Read-write access
    Tmp,     // 4 - Tmpfs mount (temporary filesystem)
    Partial, // 5 - Partial/inherited state
}

#[derive(Debug, Clone)]
struct TreeNode {
    path: String,
    display_name: String,
    allow_state: AllowState,
    expanded: bool,
    level: usize,
    is_file: bool,
    read: bool,
    write: bool,
    children: Vec<TreeNode>,
}

struct App {
    root: TreeNode,
    path: Vec<usize>,
    dirty: bool,
}

impl App {
    fn from_trace_output(output: TraceOutput) -> Self {
        let mut root = TreeNode {
            path: "/".to_string(),
            display_name: "/".to_string(),
            allow_state: AllowState::Partial,
            expanded: true,
            level: 0,
            is_file: false,
            read: false,
            write: false,
            children: Vec::new(),
        };

        let mut paths: Vec<String> = output.files.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        paths.dedup();

        for path in paths {
            Self::insert_path(&mut root, &path, None);
        }

        App {
            root,
            path: vec![],
            dirty: false,
        }
    }

    fn insert_path(parent: &mut TreeNode, path: &str, access: Option<&FileAccess>) {
        if path == parent.path || path.is_empty() {
            return;
        }

        let path_obj = std::path::Path::new(path);
        let relative = path_obj
            .strip_prefix(&parent.path)
            .unwrap_or(std::path::Path::new(path));
        let parts: Vec<String> = relative
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        let parts_refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();

        if parts_refs.is_empty() || (parts_refs.len() == 1 && parts_refs[0].is_empty()) {
            return;
        }

        let part = parts_refs[0];
        let is_file = parts_refs.len() == 1;

        let child_path = if parent.path == "/" {
            format!("/{}", part)
        } else {
            format!("{}/{}", parent.path, part)
        };

        if let Some(child) = parent.children.iter_mut().find(|c| c.display_name == part) {
            Self::insert_path(child, path, access);
        } else {
            let (read, write) = if is_file {
                access
                    .map(|a| (a.read, a.write))
                    .unwrap_or_else(|| (false, false))
            } else {
                (false, false)
            };

            let mut new_child = TreeNode {
                path: child_path.clone(),
                display_name: part.to_string(),
                allow_state: if is_file {
                    AllowState::RO
                } else {
                    AllowState::Partial
                },
                expanded: false,
                level: parent.level + 1,
                is_file,
                read,
                write,
                children: Vec::new(),
            };

            if !is_file {
                Self::insert_path(&mut new_child, path, access);
            }

            parent.children.push(new_child);
        }
    }

    #[allow(clippy::if_same_then_else)]
    fn calculate_state_from_children(node: &TreeNode) -> AllowState {
        if node.children.is_empty() {
            return AllowState::Partial;
        }

        let mut deny_count = 0;
        let mut ro_count = 0;
        let mut rw_count = 0;
        let mut tmp_count = 0;

        for child in &node.children {
            match child.allow_state {
                AllowState::Deny => deny_count += 1,
                AllowState::RO => ro_count += 1,
                AllowState::RW => rw_count += 1,
                AllowState::Tmp => tmp_count += 1,
                AllowState::Partial => {}
            }
        }

        let total = deny_count + ro_count + rw_count + tmp_count;
        if total == 0 {
            AllowState::Partial
        } else if deny_count == total {
            AllowState::Deny
        } else if rw_count == total {
            AllowState::RW
        } else if ro_count == total {
            AllowState::RO
        } else if tmp_count > 0 {
            AllowState::Tmp
        } else if ro_count > 0 {
            AllowState::Partial
        } else {
            AllowState::Partial
        }
    }

    fn get_node_at_path(&self, path: &[usize]) -> Option<&TreeNode> {
        let mut node = &self.root;
        for &idx in path {
            if idx >= node.children.len() {
                return None;
            }
            node = &node.children[idx];
        }
        Some(node)
    }

    fn get_node_at_path_mut(&mut self, path: &[usize]) -> Option<&mut TreeNode> {
        let mut node = &mut self.root;
        for &idx in path {
            if idx >= node.children.len() {
                return None;
            }
            node = &mut node.children[idx];
        }
        Some(node)
    }

    fn set_state(&mut self, state: AllowState) {
        let path = self.path.clone();
        let is_file = if let Some(node) = self.get_node_at_path_mut(&path) {
            node.allow_state = state.clone();
            node.is_file
        } else {
            return;
        };

        self.dirty = true;

        if !is_file {
            if let Some(node) = self.get_node_at_path_mut(&path) {
                Self::update_children_recursive(node, state);
            }
        } else {
            self.update_parent_states();
        }
    }

    fn update_children_recursive(node: &mut TreeNode, state: AllowState) {
        for child in &mut node.children {
            child.allow_state = state.clone();
            if !child.is_file {
                Self::update_children_recursive(child, state.clone());
            }
        }
    }

    fn update_parent_states(&mut self) {
        let mut path = self.path.clone();
        while path.pop().is_some() {
            if let Some(parent) = self.get_node_at_path_mut(&path) {
                parent.allow_state = Self::calculate_state_from_children(parent);
            }
        }
    }

    fn toggle_expanded(&mut self) {
        let path = self.path.clone();
        if let Some(node) = self.get_node_at_path_mut(&path) {
            node.expanded = !node.expanded;
        }
    }

    fn move_up(&mut self) {
        if self.path.is_empty() {
            return;
        }

        if let Some(_node) = self.get_node_at_path(&self.path) {
            // Try to move to previous sibling
            if let Some(parent_path) = self.get_parent_path() {
                if let Some(_parent) = self.get_node_at_path(&parent_path) {
                    let current_idx = *self.path.last().unwrap();
                    if current_idx > 0 {
                        // Move to previous sibling
                        self.path.pop();
                        self.path.push(current_idx - 1);
                        // Then go to last visible descendant
                        self.go_to_last_visible_descendant();
                        return;
                    }
                }
            }

            // No previous sibling, move to parent
            if !self.path.is_empty() {
                self.path.pop();
            }
        }
    }

    fn move_down(&mut self) {
        if let Some(node) = self.get_node_at_path(&self.path) {
            // If expanded and has children, go to first child
            if node.expanded && !node.children.is_empty() {
                self.path.push(0);
                return;
            }

            // Try to move to next sibling
            if let Some(parent_path) = self.get_parent_path() {
                if let Some(parent) = self.get_node_at_path(&parent_path) {
                    let current_idx = *self.path.last().unwrap();
                    if current_idx + 1 < parent.children.len() {
                        self.path.pop();
                        self.path.push(current_idx + 1);
                        return;
                    }
                }
            }

            // No next sibling, find next visible ancestor's sibling
            let mut search_path = self.path.clone();
            while !search_path.is_empty() {
                search_path.pop();
                if let Some(parent) = self.get_node_at_path(&search_path) {
                    let current_idx = if search_path.len() < self.path.len() {
                        self.path.get(search_path.len()).copied().unwrap_or(0)
                    } else {
                        0
                    };

                    if current_idx + 1 < parent.children.len() {
                        let new_idx = current_idx + 1;
                        if search_path.len() < self.path.len() {
                            self.path.truncate(search_path.len());
                        }
                        self.path.push(new_idx);
                        return;
                    }
                }
                if search_path.is_empty() {
                    break;
                }
            }
        }
    }

    fn get_parent_path(&self) -> Option<Vec<usize>> {
        if self.path.is_empty() {
            None
        } else {
            let mut parent = self.path.clone();
            parent.pop();
            Some(parent)
        }
    }

    fn go_to_last_visible_descendant(&mut self) {
        while let Some(node) = self.get_node_at_path(&self.path) {
            if node.expanded && !node.children.is_empty() {
                self.path.push(node.children.len() - 1);
            } else {
                break;
            }
        }
    }

    fn get_current_visible_position(&self) -> usize {
        if self.path.is_empty() {
            return 0;
        }
        let mut pos = 0;
        self.find_position_recursive(&self.root, &self.path, 0, &mut pos);
        pos
    }

    fn find_position_recursive(
        &self,
        node: &TreeNode,
        target_path: &[usize],
        depth: usize,
        pos: &mut usize,
    ) -> bool {
        let target_idx = target_path[depth];

        for (i, child) in node.children.iter().enumerate() {
            if i == target_idx {
                *pos += 1;
                if depth == target_path.len() - 1 {
                    return true;
                }
                return self.find_position_recursive(child, target_path, depth + 1, pos);
            }
            // Count this child
            *pos += 1;
            // If expanded, count its visible descendants too
            if child.expanded && self.count_visible_descendants(child, pos) {
                return true;
            }
        }
        false
    }

    fn count_visible_descendants(&self, node: &TreeNode, pos: &mut usize) -> bool {
        for child in &node.children {
            *pos += 1;
            if child.expanded && self.count_visible_descendants(child, pos) {
                return true;
            }
        }
        false
    }

    fn render_tree(&self, max_height: u16) -> Vec<String> {
        let mut lines = Vec::new();
        self.render_tree_recursive(&self.root, max_height as usize, &mut lines);
        lines
    }

    fn render_tree_recursive(&self, node: &TreeNode, max_height: usize, lines: &mut Vec<String>) {
        if lines.len() >= max_height {
            return;
        }

        let indent = "  ".repeat(node.level);
        let prefix = if node.children.is_empty() {
            "  "
        } else if node.expanded {
            "▼ "
        } else {
            "▶ "
        };

        let state_icon = match &node.allow_state {
            AllowState::Deny => "✗",
            AllowState::Partial => "○",
            AllowState::RO => "◐",
            AllowState::RW => "●",
            AllowState::Tmp => "◆",
        };

        let line = format!(
            "{}{}{} [{}] {}",
            indent, prefix, state_icon, node.path, node.display_name
        );

        lines.push(line);

        if node.expanded {
            for child in &node.children {
                self.render_tree_recursive(child, max_height, lines);
            }
        }
    }

    fn get_policy(&self) -> Policy {
        let mut entries = Vec::new();
        Self::collect_policy_recursive(&self.root, &self.root, &mut entries);
        Policy { entries }
    }

    fn collect_policy_recursive(root: &TreeNode, node: &TreeNode, entries: &mut Vec<PolicyEntry>) {
        if node.is_file {
            let access = Self::check_file_access(root, node);
            entries.push(PolicyEntry {
                path: node.path.clone(),
                access,
            });
        }

        for child in &node.children {
            Self::collect_policy_recursive(root, child, entries);
        }
    }

    fn find_node_recursive<'a>(node: &'a TreeNode, path: &str) -> Option<&'a TreeNode> {
        if node.path == path {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = Self::find_node_recursive(child, path) {
                return Some(found);
            }
        }
        None
    }

    fn check_file_access(root: &TreeNode, file_node: &TreeNode) -> Access {
        let parts: Vec<&str> = file_node
            .path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for i in 0..parts.len() {
            let parent_path = format!("/{}", parts[0..=i].join("/"));

            if let Some(parent) = Self::find_node_recursive(root, &parent_path) {
                match parent.allow_state {
                    AllowState::Deny => return Access::Deny,
                    AllowState::RO => return Access::ReadOnly,
                    AllowState::RW => return Access::ReadWrite,
                    AllowState::Tmp => return Access::Tmpfs,
                    AllowState::Partial => continue,
                }
            }
        }

        // Default based on original access
        if file_node.write {
            Access::ReadWrite
        } else if file_node.read {
            Access::ReadOnly
        } else {
            Access::Deny
        }
    }
}

pub fn run(paths: &[String], generate_policy: bool, output: &str) -> Result<()> {
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

        // Parse as trace format
        let trace_output: TraceOutput =
            serde_json::from_str(&buffer).context("Failed to parse JSON from stdin")?;
        all_files.extend(trace_output.files);
    } else {
        // Read from multiple files
        for path in paths {
            let data = fs::read_to_string(path)
                .with_context(|| format!("Failed to read file: {}", path))?;

            // Parse as trace format
            let trace_output: TraceOutput = serde_json::from_str(&data)
                .with_context(|| format!("Failed to parse JSON from file: {}", path))?;
            all_files.extend(trace_output.files);
        }
    }

    // Build tree from trace files
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
                    access: if f.write {
                        Access::ReadWrite
                    } else if f.read {
                        Access::ReadOnly
                    } else {
                        Access::Deny
                    },
                })
                .collect(),
        };

        let json = serde_json::to_string_pretty(&policy)?;
        write_policy(&json, output)?;
        return Ok(());
    }

    let mut app = App::from_trace_output(trace_output);

    // Try to setup terminal for interactive mode
    // If not connected to a TTY, skip TUI and just output policy
    if !atty::is(atty::Stream::Stdout) {
        let policy = app.get_policy();
        let json = serde_json::to_string_pretty(&policy)?;
        write_policy(&json, output)?;
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
        write_policy(&json, output)?;
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
                    KeyCode::Char(' ') => {
                        if let Some(node) = app.get_node_at_path(&app.path) {
                            if node.is_file {
                                if node.allow_state == AllowState::RO {
                                    app.set_state(AllowState::RW);
                                } else {
                                    app.set_state(AllowState::RO);
                                }
                            } else {
                                if node.allow_state == AllowState::Partial {
                                    app.set_state(AllowState::Deny);
                                } else {
                                    app.set_state(AllowState::Partial);
                                }
                            }
                        }
                    }
                    KeyCode::Char('d') => app.set_state(AllowState::Deny),
                    KeyCode::Char('r') => app.set_state(AllowState::RO),
                    KeyCode::Char('w') => app.set_state(AllowState::RW),
                    KeyCode::Char('t') => app.set_state(AllowState::Tmp),
                    KeyCode::Char('p') => app.set_state(AllowState::Partial),
                    KeyCode::Right | KeyCode::Char('e') => app.toggle_expanded(),
                    KeyCode::Left => {
                        if let Some(node) = app.get_node_at_path(&app.path) {
                            if node.expanded {
                                app.toggle_expanded();
                            }
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
    let current_pos = app.get_current_visible_position();
    let tree_items: Vec<ListItem> = tree_lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let style = if idx == current_pos {
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
            .title("File Tree (d=✗ Deny r=◐ RO w=● RW t=◆ Tmp p=○ Partial)")
            .borders(Borders::ALL),
    );

    f.render_widget(tree_widget, chunks[0]);

    // Render help text
    let help_text = "d=✗ Deny r=◐ RO w=● RW t=◆ Tmp p=○ Partial | SPACE=toggle | e=expand | ←→=collapse | ↑↓=navigate | q=quit";
    let help_widget = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    f.render_widget(help_widget, chunks[1]);
}

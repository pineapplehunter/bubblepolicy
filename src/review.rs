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
                        allow_state: if is_file {
                            AllowState::RO
                        } else {
                            AllowState::Partial
                        },
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

    fn from_policy(entries: &[PolicyEntry]) -> Self {
        let mut tree = Vec::new();
        let mut file_access_map: std::collections::BTreeMap<String, FileAccess> =
            std::collections::BTreeMap::new();

        // Build access map from policy entries
        for entry in entries {
            file_access_map.insert(
                entry.path.clone(),
                FileAccess {
                    path: entry.path.clone(),
                    read: entry.read,
                    write: entry.write,
                    execute: false,
                },
            );
        }

        let mut paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
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

                    // Determine state from policy entry
                    let allow_state = if is_file {
                        if let Some(entry) = entries.iter().find(|e| e.path == current_path) {
                            if !entry.allowed {
                                AllowState::Deny
                            } else if entry.write {
                                AllowState::RW
                            } else {
                                AllowState::RO
                            }
                        } else {
                            AllowState::RO
                        }
                    } else {
                        AllowState::Partial
                    };

                    tree.push(TreeNode {
                        path: current_path.clone(),
                        display_name,
                        allow_state,
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

        // Update parent states based on children
        for i in (0..tree.len()).rev() {
            if !tree[i].is_file {
                let state = App::calculate_parent_state_from_tree(&tree, i);
                tree[i].allow_state = state;
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

    fn calculate_parent_state_from_tree(tree: &[TreeNode], parent_idx: usize) -> AllowState {
        let children = &tree[parent_idx].children;
        if children.is_empty() {
            return AllowState::Partial;
        }

        let mut ro_count = 0;
        let mut rw_count = 0;
        let mut tmp_count = 0;
        let mut deny_count = 0;
        let mut partial_count = 0;

        for &child_idx in children {
            match tree[child_idx].allow_state {
                AllowState::RO => ro_count += 1,
                AllowState::RW => rw_count += 1,
                AllowState::Tmp => tmp_count += 1,
                AllowState::Deny => deny_count += 1,
                AllowState::Partial => partial_count += 1,
            }
        }

        if partial_count > 0 {
            AllowState::Partial
        } else if deny_count > 0 && ro_count == 0 && rw_count == 0 && tmp_count == 0 {
            AllowState::Deny
        } else if rw_count > 0 {
            AllowState::RW
        } else if tmp_count > 0 {
            AllowState::Tmp
        } else if ro_count > 0 {
            AllowState::RO
        } else {
            AllowState::Partial
        }
    }

    fn flat_index(&self) -> Option<usize> {
        if self.tree.is_empty() {
            return None;
        }
        let mut current_visible = 0;
        for i in 0..self.tree.len() {
            if self.is_visible(i) {
                if current_visible == self.selected_idx {
                    return Some(i);
                }
                current_visible += 1;
            }
        }
        None
    }

    fn visible_count(&self) -> usize {
        (0..self.tree.len()).filter(|&i| self.is_visible(i)).count()
    }

    fn set_state(&mut self, state: AllowState) {
        let flat_idx = match self.flat_index() {
            Some(idx) => idx,
            None => return,
        };

        // Set the selected node's state
        self.tree[flat_idx].allow_state = state.clone();
        self.dirty = true;

        // If this is a directory (not a file), update all children recursively
        let is_file = self.tree[flat_idx].is_file;
        if !is_file {
            self.update_children_state(flat_idx, state);
        } else {
            // If it's a file, update parent directory states
            self.update_parent_states(flat_idx);
        }
    }

    fn update_children_state(&mut self, parent_idx: usize, state: AllowState) {
        let children = self.tree[parent_idx].children.clone();
        for child_idx in children {
            self.tree[child_idx].allow_state = state.clone();
            // Recursively update grandchildren
            if !self.tree[child_idx].is_file {
                self.update_children_state(child_idx, state.clone());
            }
        }
    }

    fn update_parent_states(&mut self, child_idx: usize) {
        let child_path = self.tree[child_idx].path.clone();
        let child_level = self.tree[child_idx].level;

        // Walk up the tree and update each parent
        for i in (0..child_idx).rev() {
            if self.tree[i].level < child_level {
                let parent_path = &self.tree[i].path;
                if child_path.starts_with(parent_path) {
                    let parent_state = self.calculate_parent_state(i);
                    self.tree[i].allow_state = parent_state;
                } else {
                    // No more parents in this branch
                    break;
                }
            }
        }
    }

    fn calculate_parent_state(&self, parent_idx: usize) -> AllowState {
        Self::calculate_parent_state_from_tree(&self.tree, parent_idx)
    }

    fn toggle_expanded(&mut self) {
        let flat_idx = match self.flat_index() {
            Some(idx) => idx,
            None => return,
        };
        self.tree[flat_idx].expanded = !self.tree[flat_idx].expanded;
    }

    fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected_idx < self.visible_count() - 1 {
            self.selected_idx += 1;
        }
    }

    fn is_visible(&self, idx: usize) -> bool {
        // Check if all ancestors of idx are expanded
        let node_level = self.tree[idx].level;
        let node_path = &self.tree[idx].path;

        // Walk up to find parent at each level
        for level in (0..node_level).rev() {
            // Find the parent at this level
            for i in (0..idx).rev() {
                if self.tree[i].level == level {
                    let parent_path = &self.tree[i].path;
                    if node_path.starts_with(parent_path) {
                        // Found the parent at this level
                        if !self.tree[i].expanded {
                            return false; // Parent is collapsed
                        }
                        break; // Continue checking next level up
                    }
                }
            }
        }
        true
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
        let mut entries = Vec::new();

        // Process each file in the tree
        for node in &self.tree {
            if !node.is_file {
                continue;
            }

            // Check if this file is allowed based on parent directories
            let (allowed, read, write) = self.is_file_allowed(node);

            entries.push(PolicyEntry {
                path: node.path.clone(),
                allowed,
                read,
                write,
            });
        }

        Policy { entries }
    }

    fn is_file_allowed(&self, file_node: &TreeNode) -> (bool, bool, bool) {
        // Returns (allowed, read, write) for a file
        // Check if any parent directory explicitly denies this file
        // or if all parent directories allow it

        let parts: Vec<&str> = file_node
            .path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        // Check each parent directory level
        for i in 0..parts.len() {
            let parent_path = format!("/{}", parts[0..=i].join("/"));

            // Find the parent node in the tree
            for node in &self.tree {
                if node.path == parent_path {
                    match node.allow_state {
                        AllowState::Deny => return (false, false, false), // Parent explicitly denies
                        AllowState::RO => {
                            // Parent allows read-only
                            return (true, true, false);
                        }
                        AllowState::RW => {
                            // Parent allows read-write
                            return (true, true, true);
                        }
                        AllowState::Tmp => {
                            // Parent has tmpfs - allow with tmpfs semantics
                            // For tmpfs, we treat it as read-write but it's a tmpfs mount
                            return (true, true, true);
                        }
                        AllowState::Partial => continue, // Continue checking up the tree
                    }
                }
            }
        }

        // Default to allowed with original read/write if no explicit deny
        (true, file_node.read, file_node.write)
    }
}

pub fn run(paths: &[String], generate_policy: bool, output: &str) -> Result<()> {
    // Load and merge trace data or policy data from multiple files
    let mut all_files = Vec::new();
    let mut is_policy = false;
    let mut policy_entries: Vec<PolicyEntry> = Vec::new();

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

        // Try to parse as policy first
        if let Ok(policy) = serde_json::from_str::<Policy>(&buffer) {
            is_policy = true;
            policy_entries = policy.entries;
        } else {
            // Fall back to trace format
            let trace_output: TraceOutput =
                serde_json::from_str(&buffer).context("Failed to parse JSON from stdin")?;
            all_files.extend(trace_output.files);
        }
    } else {
        // Read from multiple files
        for path in paths {
            let data = fs::read_to_string(path)
                .with_context(|| format!("Failed to read file: {}", path))?;

            // Try to parse as policy first
            if let Ok(policy) = serde_json::from_str::<Policy>(&data) {
                is_policy = true;
                policy_entries.extend(policy.entries);
            } else {
                // Fall back to trace format
                let trace_output: TraceOutput = serde_json::from_str(&data)
                    .with_context(|| format!("Failed to parse JSON from file: {}", path))?;
                all_files.extend(trace_output.files);
            }
        }
    }

    // If we loaded a policy file, convert it to the tree structure
    if is_policy {
        let mut app = App::from_policy(&policy_entries);

        // Try to setup terminal for interactive mode
        if !atty::is(atty::Stream::Stdout) {
            // Non-interactive mode - just output the policy as-is
            let policy = Policy {
                entries: policy_entries,
            };
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
            println!("{}", json);
        }

        return Ok(());
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
        println!("{}", json);
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
                        // Space toggles between RO and RW for files, or Partial/Deny for dirs
                        let node = &app.tree[app.selected_idx];
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
                    KeyCode::Char('d') => app.set_state(AllowState::Deny),
                    KeyCode::Char('r') => app.set_state(AllowState::RO),
                    KeyCode::Char('w') => app.set_state(AllowState::RW),
                    KeyCode::Char('t') => app.set_state(AllowState::Tmp),
                    KeyCode::Char('p') => app.set_state(AllowState::Partial),
                    KeyCode::Right | KeyCode::Char('e') => app.toggle_expanded(),
                    KeyCode::Left => {
                        if let Some(flat_idx) = app.flat_index() {
                            if app.tree[flat_idx].expanded {
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

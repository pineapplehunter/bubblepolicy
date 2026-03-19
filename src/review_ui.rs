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
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::fs;
use std::io;

use crate::common::{Access, PolicyNode, PolicyTree};

#[derive(Debug, Clone, PartialEq)]
enum AllowState {
    Deny,
    RO,
    RW,
    Tmp,
    Partial,
}

#[derive(Debug, Clone)]
struct TreeNode {
    path: String,
    display_name: String,
    allow_state: AllowState,
    expanded: bool,
    level: usize,
    children: Vec<TreeNode>,
}

#[allow(dead_code)]
struct App {
    root: TreeNode,
    path: Vec<usize>,
    dirty: bool,
    filename: String,
}

impl App {
    fn from_policy_trees(trees: Vec<PolicyTree>, filename: String) -> Self {
        let mut root = TreeNode {
            path: "/".to_string(),
            display_name: "/".to_string(),
            allow_state: AllowState::Partial,
            expanded: true,
            level: 0,
            children: Vec::new(),
        };

        // Add each tree's entries directly to root
        for tree in trees.into_iter() {
            for entry in tree.entries {
                // Handle root entry specially
                if entry.path == "/" {
                    // Set root's access if it has no children
                    if entry.children.is_empty() {
                        root.allow_state = match entry.access {
                            Access::Deny => AllowState::Deny,
                            Access::ReadOnly => AllowState::RO,
                            Access::ReadWrite => AllowState::RW,
                            Access::Tmpfs => AllowState::Tmp,
                        };
                    }
                    // Add root's children as children of UI root
                    for child in entry.children {
                        Self::insert_node(&mut root, child);
                    }
                } else {
                    Self::insert_node(&mut root, entry);
                }
            }
        }

        App {
            root,
            path: vec![],
            dirty: false,
            filename,
        }
    }

    #[allow(clippy::double_ended_iterator_last)]
    fn insert_node(parent: &mut TreeNode, node: PolicyNode) {
        let display_name = node.path.split('/').next_back().unwrap_or("/").to_string();
        let child_path = node.path.clone();

        let allow_state = match node.access {
            Access::Deny => AllowState::Deny,
            Access::ReadOnly => AllowState::RO,
            Access::ReadWrite => AllowState::RW,
            Access::Tmpfs => AllowState::Tmp,
        };

        let mut new_node = TreeNode {
            path: child_path,
            display_name,
            allow_state,
            expanded: false,
            level: parent.level + 1,
            children: Vec::new(),
        };

        for child in node.children {
            Self::insert_node(&mut new_node, child);
        }

        parent.children.push(new_node);
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
        if let Some(node) = self.get_node_at_path_mut(&path) {
            node.allow_state = state;
            self.dirty = true;
        }
    }

    fn move_up(&mut self) {
        if self.path.is_empty() {
            return;
        }

        // Try to go to previous sibling first
        let current_idx = *self.path.last().unwrap();

        if current_idx > 0 {
            // Go to previous sibling
            self.path.pop();
            self.path.push(current_idx - 1);

            // If the previous sibling is expanded, go to its last visible descendant
            self.go_to_last_visible_descendant();
        } else {
            // Already at first sibling, just go up
            self.path.pop();
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

    fn move_down(&mut self) {
        if let Some(node) = self.get_node_at_path(&self.path) {
            if node.expanded && !node.children.is_empty() {
                self.path.push(0);
                return;
            }
        }

        // Try to find next sibling at current level or higher
        loop {
            if self.path.is_empty() {
                return;
            }

            let current_idx = *self.path.last().unwrap();
            let parent_path_len = self.path.len() - 1;

            // Get parent node
            let parent_node = if parent_path_len == 0 {
                Some(&self.root)
            } else {
                self.get_node_at_path(&self.path[..parent_path_len])
            };

            if let Some(parent) = parent_node {
                // Try to go to next sibling
                if current_idx + 1 < parent.children.len() {
                    self.path.pop();
                    self.path.push(current_idx + 1);
                    return;
                }
            }

            // No more siblings at this level, go up one level and try again
            self.path.pop();
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

    fn toggle_expanded(&mut self) {
        let path = self.path.clone();
        if let Some(node) = self.get_node_at_path_mut(&path) {
            node.expanded = !node.expanded;
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
        if depth == target_path.len() {
            return true;
        }
        let target_idx = target_path[depth];
        for (i, child) in node.children.iter().enumerate() {
            if i == target_idx {
                *pos += 1;
                return self.find_position_recursive(child, target_path, depth + 1, pos);
            }
            *pos += 1;
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

        let state_icon = match node.allow_state {
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

    fn to_policy_trees(&self) -> Vec<PolicyTree> {
        // All entries are directly under root, so return as single tree
        let entries: Vec<PolicyNode> = self
            .root
            .children
            .iter()
            .map(|child| self.node_to_policy_node(child))
            .collect();

        vec![PolicyTree { entries }]
    }

    fn node_to_policy_node(&self, node: &TreeNode) -> PolicyNode {
        let access = match node.allow_state {
            AllowState::Deny => Access::Deny,
            AllowState::RO => Access::ReadOnly,
            AllowState::RW => Access::ReadWrite,
            AllowState::Tmp => Access::Tmpfs,
            AllowState::Partial => Access::ReadOnly,
        };

        PolicyNode {
            path: node.path.clone(),
            access,
            children: node
                .children
                .iter()
                .map(|c| self.node_to_policy_node(c))
                .collect(),
        }
    }
}

pub fn run(filename: &str) -> Result<()> {
    let data = fs::read_to_string(filename)
        .with_context(|| format!("Failed to read file: {}", filename))?;

    let trees: Vec<PolicyTree> = serde_json::from_str(&data).context("Failed to parse JSON")?;

    let mut app = App::from_policy_trees(trees, filename.to_string());

    if !std::io::stdout().is_tty() {
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    } else if app.dirty {
        let trees = app.to_policy_trees();
        let json = serde_json::to_string_pretty(&trees)?;
        fs::write(&app.filename, json)
            .with_context(|| format!("Failed to write file: {}", app.filename))?;
        eprintln!("Updated: {}", app.filename);
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
        .split(f.area());

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

    let help_text = "d=✗ Deny r=◐ RO w=● RW t=◆ Tmp p=○ Partial | e=expand | ←→=collapse | ↑↓=navigate | q=quit";
    let help_widget = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    f.render_widget(help_widget, chunks[1]);
}

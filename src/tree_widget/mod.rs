use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Text;
use ratatui::widgets::{Block, StatefulWidget, Widget};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct TreeItem<'a, Identifier> {
    pub(crate) identifier: Identifier,
    pub(crate) text: Text<'a>,
    pub(crate) children: Vec<TreeItem<'a, Identifier>>,
}

impl<'a, Identifier> TreeItem<'a, Identifier>
where
    Identifier: Clone + PartialEq + Eq + std::hash::Hash,
{
    pub fn new_leaf<T: Into<Text<'a>>>(identifier: Identifier, text: T) -> Self {
        Self {
            identifier,
            text: text.into(),
            children: Vec::new(),
        }
    }

    pub fn new<T: Into<Text<'a>>>(
        identifier: Identifier,
        text: T,
        children: Vec<Self>,
    ) -> std::io::Result<Self> {
        let identifiers: HashSet<_> = children.iter().map(|item| &item.identifier).collect();
        if identifiers.len() != children.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "duplicate identifiers",
            ));
        }
        Ok(Self {
            identifier,
            text: text.into(),
            children,
        })
    }

    pub fn identifier(&self) -> &Identifier {
        &self.identifier
    }

    pub fn text(&self) -> &Text<'a> {
        &self.text
    }

    pub fn text_mut(&mut self) -> &mut Text<'a> {
        &mut self.text
    }

    pub fn children(&self) -> &[Self] {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut Vec<Self> {
        &mut self.children
    }

    pub fn height(&self) -> usize {
        self.text.height()
    }
}

#[derive(Debug, Clone)]
pub struct Flattened<'a, Identifier> {
    pub identifier: Vec<Identifier>,
    pub item: &'a TreeItem<'a, Identifier>,
}

impl<Identifier> Flattened<'_, Identifier> {
    pub fn depth(&self) -> usize {
        self.identifier.len().saturating_sub(1)
    }
}

fn flatten_helper<'a, Identifier>(
    open_identifiers: &HashSet<Vec<Identifier>>,
    items: &'a [TreeItem<'a, Identifier>],
    current: &[Identifier],
) -> Vec<Flattened<'a, Identifier>>
where
    Identifier: Clone + PartialEq + Eq + std::hash::Hash,
{
    let mut result = Vec::new();
    for item in items {
        let mut child_identifier = current.to_vec();
        child_identifier.push(item.identifier.clone());

        let is_open = open_identifiers.contains(&child_identifier);

        result.push(Flattened {
            identifier: child_identifier.clone(),
            item,
        });

        if is_open {
            let child_result = flatten_helper(open_identifiers, &item.children, &child_identifier);
            result.extend(child_result);
        }
    }
    result
}

#[derive(Debug, Default)]
pub struct TreeState<Identifier> {
    pub(crate) offset: usize,
    pub(crate) opened: HashSet<Vec<Identifier>>,
    pub(crate) selected: Vec<Identifier>,
    pub(crate) ensure_selected_in_view: bool,

    pub(crate) last_area: Rect,
    pub(crate) last_biggest_index: usize,
    pub(crate) last_identifiers: Vec<Vec<Identifier>>,
    pub(crate) last_rendered_identifiers: Vec<(u16, Vec<Identifier>)>,
}

impl<Identifier> TreeState<Identifier>
where
    Identifier: Clone + PartialEq + Eq + std::hash::Hash,
{
    pub fn selected(&self) -> &[Identifier] {
        &self.selected
    }

    pub fn selected_cloned(&self) -> Vec<Identifier> {
        self.selected.clone()
    }

    pub fn opened(&self) -> &HashSet<Vec<Identifier>> {
        &self.opened
    }

    pub fn flatten<'a>(
        &self,
        items: &'a [TreeItem<'a, Identifier>],
    ) -> Vec<Flattened<'a, Identifier>> {
        flatten_helper(&self.opened, items, &[])
    }

    pub fn select(&mut self, identifier: Vec<Identifier>) -> bool {
        self.ensure_selected_in_view = true;
        let changed = self.selected != identifier;
        self.selected = identifier;
        changed
    }

    pub fn open(&mut self, identifier: Vec<Identifier>) -> bool {
        if identifier.is_empty() {
            false
        } else {
            self.opened.insert(identifier)
        }
    }

    pub fn close(&mut self, identifier: &[Identifier]) -> bool {
        self.opened.remove(identifier)
    }

    pub fn toggle(&mut self, identifier: Vec<Identifier>) -> bool {
        if identifier.is_empty() {
            false
        } else if self.opened.contains(&identifier) {
            self.close(&identifier)
        } else {
            self.open(identifier)
        }
    }

    pub fn toggle_selected(&mut self) -> bool {
        if self.selected.is_empty() {
            return false;
        }
        self.ensure_selected_in_view = true;
        let was_open = self.opened.remove(&self.selected);
        if was_open {
            return true;
        }
        self.open(self.selected.clone())
    }

    pub fn close_all(&mut self) -> bool {
        if self.opened.is_empty() {
            false
        } else {
            self.opened.clear();
            true
        }
    }

    pub fn select_first(&mut self) -> bool {
        let identifier = self.last_identifiers.first().cloned();
        if let Some(identifier) = identifier {
            self.select(identifier)
        } else {
            false
        }
    }

    pub fn select_first_item(&mut self, items: &[TreeItem<'_, Identifier>]) -> bool {
        if let Some(first) = items.first() {
            self.select(vec![first.identifier.clone()])
        } else {
            false
        }
    }

    pub fn select_last(&mut self) -> bool {
        let identifier = self.last_identifiers.last().cloned().unwrap_or_default();
        self.select(identifier)
    }

    fn select_relative<F>(&mut self, change_fn: F) -> bool
    where
        F: FnOnce(Option<usize>) -> usize,
    {
        let identifiers = &self.last_identifiers;
        let current_identifier = &self.selected;
        let current_index = identifiers.iter().position(|id| id == current_identifier);
        let new_index = change_fn(current_index).min(self.last_biggest_index);
        let new_identifier = identifiers.get(new_index).cloned().unwrap_or_default();
        self.select(new_identifier)
    }

    pub fn key_up(&mut self) -> bool {
        self.select_relative(|current| current.map_or(usize::MAX, |c| c.saturating_sub(1)))
    }

    pub fn key_down(&mut self) -> bool {
        self.select_relative(|current| current.map_or(0, |c| c.saturating_add(1)))
    }

    pub fn key_left(&mut self) -> bool {
        self.ensure_selected_in_view = true;
        let mut changed = self.opened.remove(&self.selected);
        if !changed {
            let popped = self.selected.pop();
            changed = popped.is_some();
        }
        changed
    }

    pub fn key_right(&mut self) -> bool {
        if self.selected.is_empty() {
            false
        } else {
            self.ensure_selected_in_view = true;
            self.open(self.selected.clone())
        }
    }
}

pub struct Tree<'a, Identifier> {
    items: &'a [TreeItem<'a, Identifier>],
    block: Option<Block<'a>>,
    style: Style,
    highlight_style: Style,
    highlight_symbol: &'a str,
    node_closed_symbol: &'a str,
    node_open_symbol: &'a str,
    node_no_children_symbol: &'a str,
}

impl<'a, Identifier> Tree<'a, Identifier>
where
    Identifier: Clone + PartialEq + Eq + std::hash::Hash,
{
    pub fn new(items: &'a [TreeItem<'a, Identifier>]) -> std::io::Result<Self> {
        let identifiers: HashSet<_> = items.iter().map(|item| &item.identifier).collect();
        if identifiers.len() != items.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "duplicate identifiers",
            ));
        }
        Ok(Self {
            items,
            block: None,
            style: Style::new(),
            highlight_style: Style::new(),
            highlight_symbol: "",
            node_closed_symbol: "▶ ",
            node_open_symbol: "▼ ",
            node_no_children_symbol: "  ",
        })
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    pub fn highlight_symbol(mut self, symbol: &'a str) -> Self {
        self.highlight_symbol = symbol;
        self
    }

    pub fn node_closed_symbol(mut self, symbol: &'a str) -> Self {
        self.node_closed_symbol = symbol;
        self
    }

    pub fn node_open_symbol(mut self, symbol: &'a str) -> Self {
        self.node_open_symbol = symbol;
        self
    }

    pub fn node_no_children_symbol(mut self, symbol: &'a str) -> Self {
        self.node_no_children_symbol = symbol;
        self
    }
}

impl<Identifier> StatefulWidget for Tree<'_, Identifier>
where
    Identifier: Clone + PartialEq + Eq + std::hash::Hash,
{
    type State = TreeState<Identifier>;

    fn render(self, full_area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        buf.set_style(full_area, self.style);

        let area = if let Some(block) = self.block {
            let inner = block.inner(full_area);
            block.render(full_area, buf);
            inner
        } else {
            full_area
        };

        if area.width < 1 || area.height < 1 {
            return;
        }

        state.last_area = area;
        state.last_rendered_identifiers.clear();

        let visible = state.flatten(self.items);
        state.last_biggest_index = visible.len().saturating_sub(1);

        if visible.is_empty() {
            return;
        }

        let available_height = area.height as usize;

        let mut start = state.offset.min(state.last_biggest_index);

        if state.ensure_selected_in_view && !state.selected.is_empty()
            && let Some(idx) = visible.iter().position(|f| f.identifier == state.selected)
        {
            start = start.min(idx);
        }

        let mut end = start;
        let mut height = 0;
        for flattened in visible.iter().skip(start) {
            if height + flattened.item.height() > available_height {
                break;
            }
            height += flattened.item.height();
            end += 1;
        }

        if state.ensure_selected_in_view && !state.selected.is_empty() {
            while let Some(idx) = visible.iter().position(|f| f.identifier == state.selected) {
                if idx >= end {
                    height += visible[end].item.height();
                    end += 1;
                    while height > available_height && start < end {
                        height = height.saturating_sub(visible[start].item.height());
                        start += 1;
                    }
                } else {
                    break;
                }
            }
        }

        state.offset = start;
        state.ensure_selected_in_view = false;

        let blank_symbol = " ".repeat(self.highlight_symbol.width());
        let mut current_height = 0;
        let has_selection = !state.selected.is_empty();

        for flattened in visible.iter().skip(state.offset).take(end - start) {
            let x = area.x;
            let y = area.y + current_height;
            let item_height = flattened.item.height() as u16;
            current_height += item_height;

            let item_area = Rect {
                x,
                y,
                width: area.width,
                height: item_height,
            };

            let is_selected = state.selected == flattened.identifier;
            let mut after_x = x;

            if has_selection {
                let symbol = if is_selected {
                    self.highlight_symbol
                } else {
                    &blank_symbol
                };
                let (new_x, _) = buf.set_stringn(x, y, symbol, area.width as usize, Style::new());
                after_x = new_x;
            }

            let indent_width = flattened.depth() * 2;
            let (after_indent_x, _) = buf.set_stringn(
                after_x,
                y,
                " ".repeat(indent_width),
                indent_width,
                Style::new(),
            );

            let symbol = if flattened.item.children.is_empty() {
                self.node_no_children_symbol
            } else if state.opened.contains(&flattened.identifier) {
                self.node_open_symbol
            } else {
                self.node_closed_symbol
            };
            let max_width = area.width.saturating_sub(after_indent_x - x);
            let (after_symbol_x, _) =
                buf.set_stringn(after_indent_x, y, symbol, max_width as usize, Style::new());

            let text_area = Rect {
                x: after_symbol_x,
                width: area.width.saturating_sub(after_symbol_x - x),
                height: item_height,
                y,
            };
            flattened.item.text.clone().render(text_area, buf);

            if is_selected {
                buf.set_style(item_area, self.highlight_style);
            }

            state
                .last_rendered_identifiers
                .push((item_area.y, flattened.identifier.clone()));
        }

        state.last_identifiers = visible.into_iter().map(|f| f.identifier).collect();
    }
}

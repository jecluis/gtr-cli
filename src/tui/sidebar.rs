// SPDX-License-Identifier: AGPL-3.0-or-later
// gtr - CLI client for Getting Things Rusty
// Copyright (C) 2026 Joao Eduardo Luis <joao@abysmo.tech>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Sidebar widget showing project and namespace trees.
//!
//! The sidebar renders two collapsible sections (Projects, Namespaces)
//! as flat lists with indentation to represent hierarchy.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, StatefulWidget};

use super::theme::Theme;
use crate::cache::{CachedNamespace, CachedProject, TaskCache};

/// Nil UUID constant for the meta-root project.
const META_ROOT_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// The kind of entity a sidebar item represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeItemKind {
    Dashboard,
    SectionHeader,
    Project,
    Namespace,
    /// Non-selectable horizontal divider.
    Divider,
}

/// A flattened, renderable tree item.
#[derive(Debug, Clone)]
struct TreeItem {
    id: String,
    name: String,
    depth: u16,
    kind: TreeItemKind,
    is_section_header: bool,
}

impl TreeItem {
    fn divider() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            depth: 0,
            kind: TreeItemKind::Divider,
            is_section_header: false,
        }
    }
}

/// Which section of the sidebar is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Projects,
    Namespaces,
}

/// Describes which sidebar item corresponds to the current main view.
pub enum ActiveItem<'a> {
    Dashboard,
    /// All projects (the "Projects" section header was selected).
    AllProjects,
    /// A specific project by ID.
    Project(&'a str),
    /// All namespaces (the "Namespaces" section header was selected).
    AllNamespaces,
    /// A specific namespace by ID.
    Namespace(&'a str),
}

/// State for the sidebar widget.
pub struct SidebarState {
    /// Flattened list of renderable items.
    items: Vec<TreeItem>,
    /// Index of the currently selected item.
    pub selected: usize,
    /// Ratatui list state for scroll offset and selection tracking.
    list_state: ListState,
}

impl SidebarState {
    /// Build sidebar state from cached data.
    pub fn from_cache(cache: &TaskCache) -> crate::Result<Self> {
        let projects = cache.list_projects().unwrap_or_default();
        let namespaces = cache.list_namespaces().unwrap_or_default();

        let mut items = Vec::new();

        // Dashboard entry at the top.
        items.push(TreeItem {
            id: String::new(),
            name: "Dashboard".to_string(),
            depth: 0,
            kind: TreeItemKind::Dashboard,
            is_section_header: false,
        });

        items.push(TreeItem::divider());

        // Projects section
        items.push(TreeItem {
            id: String::new(),
            name: "Projects".to_string(),
            depth: 0,
            kind: TreeItemKind::SectionHeader,
            is_section_header: true,
        });
        flatten_projects(&projects, Some(META_ROOT_UUID), 1, &mut items);

        items.push(TreeItem::divider());

        // Namespaces section
        items.push(TreeItem {
            id: String::new(),
            name: "Namespaces".to_string(),
            depth: 0,
            kind: TreeItemKind::SectionHeader,
            is_section_header: true,
        });
        flatten_namespaces(&namespaces, None, 1, &mut items);

        Ok(Self {
            items,
            selected: 0,
            list_state: ListState::default().with_selected(Some(0)),
        })
    }

    /// Set selection to the given index, updating both fields.
    fn set_selected(&mut self, index: usize) {
        self.selected = index;
        self.list_state.select(Some(index));
    }

    /// Move selection up, skipping dividers.
    pub fn select_prev(&mut self) {
        let mut target = self.selected;
        while target > 0 {
            target -= 1;
            if self.items[target].kind != TreeItemKind::Divider {
                self.set_selected(target);
                return;
            }
        }
    }

    /// Move selection down, skipping dividers.
    pub fn select_next(&mut self) {
        let mut target = self.selected;
        while target + 1 < self.items.len() {
            target += 1;
            if self.items[target].kind != TreeItemKind::Divider {
                self.set_selected(target);
                return;
            }
        }
    }

    /// Snap the selection to the item matching the given active view.
    pub fn select_active(&mut self, active: &ActiveItem<'_>) {
        for (i, item) in self.items.iter().enumerate() {
            if item_matches_active(item, active) {
                self.set_selected(i);
                return;
            }
        }
    }

    /// Get the ID of the currently selected item (empty for headers).
    pub fn selected_id(&self) -> &str {
        self.items
            .get(self.selected)
            .map(|i| i.id.as_str())
            .unwrap_or("")
    }

    /// Get the kind of the currently selected item.
    pub fn selected_kind(&self) -> Option<TreeItemKind> {
        self.items.get(self.selected).map(|i| i.kind)
    }

    /// Get the display name of the currently selected item.
    pub fn selected_name(&self) -> &str {
        self.items
            .get(self.selected)
            .map(|i| i.name.as_str())
            .unwrap_or("")
    }

    /// Render the sidebar into the given area.
    pub fn render(
        &mut self,
        theme: &Theme,
        focused: bool,
        active: &ActiveItem<'_>,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let block = Block::bordered()
            .title(" sidebar ")
            .border_style(border_style);

        let selected = self.selected;
        let list_items: Vec<ListItem<'_>> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let is_active = item_matches_active(item, active);
                render_item(item, theme, is_active, i == selected)
            })
            .collect();

        // Only set background so item foreground colors are preserved.
        let highlight_style = Style::default().bg(Color::DarkGray);

        let list = List::new(list_items)
            .block(block)
            .highlight_style(highlight_style);

        StatefulWidget::render(list, area, buf, &mut self.list_state);
    }
}

/// Check whether a tree item matches the currently active main view.
fn item_matches_active(item: &TreeItem, active: &ActiveItem<'_>) -> bool {
    match (active, item.kind) {
        (ActiveItem::Dashboard, TreeItemKind::Dashboard) => true,
        (ActiveItem::AllProjects, TreeItemKind::SectionHeader) => item.name == "Projects",
        (ActiveItem::Project(id), TreeItemKind::Project) => item.id == *id,
        (ActiveItem::AllNamespaces, TreeItemKind::SectionHeader) => item.name == "Namespaces",
        (ActiveItem::Namespace(id), TreeItemKind::Namespace) => item.id == *id,
        _ => false,
    }
}

/// Render a single tree item as a ListItem.
fn render_item<'a>(item: &TreeItem, theme: &Theme, active: bool, selected: bool) -> ListItem<'a> {
    if item.kind == TreeItemKind::Divider {
        // Dividers: a long horizontal rule (will be clipped to width).
        let line = " \u{2500}".repeat(60);
        return ListItem::new(Line::from(Span::styled(
            line,
            Style::default().fg(Color::Gray),
        )));
    }

    // Base indent (2 cols) + tree depth; active items get ">" marker.
    let depth_indent = "  ".repeat(item.depth as usize);
    let prefix = if active {
        format!("> {depth_indent}")
    } else {
        format!("  {depth_indent}")
    };

    let style = if active && selected {
        // Slightly darker, redder tone for contrast on the gray highlight row.
        Style::default()
            .fg(Color::Rgb(200, 100, 30))
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::default()
            .fg(Color::Rgb(220, 150, 50))
            .add_modifier(Modifier::BOLD)
    } else if item.is_section_header || item.kind == TreeItemKind::Dashboard {
        theme.emphasis
    } else {
        Style::default()
    };

    ListItem::new(Line::from(Span::styled(
        format!("{prefix}{}", item.name),
        style,
    )))
}

/// Recursively flatten projects into the tree item list.
fn flatten_projects(
    all: &[CachedProject],
    parent_id: Option<&str>,
    depth: u16,
    out: &mut Vec<TreeItem>,
) {
    let children: Vec<_> = all
        .iter()
        .filter(|p| p.parent_id.as_deref() == parent_id)
        .collect();

    for proj in children {
        out.push(TreeItem {
            id: proj.id.clone(),
            name: proj.name.clone(),
            depth,
            kind: TreeItemKind::Project,
            is_section_header: false,
        });
        flatten_projects(all, Some(&proj.id), depth + 1, out);
    }
}

/// Recursively flatten namespaces into the tree item list.
fn flatten_namespaces(
    all: &[CachedNamespace],
    parent_id: Option<&str>,
    depth: u16,
    out: &mut Vec<TreeItem>,
) {
    let children: Vec<_> = all
        .iter()
        .filter(|n| n.parent_id.as_deref() == parent_id)
        .collect();

    for ns in children {
        out.push(TreeItem {
            id: ns.id.clone(),
            name: ns.name.clone(),
            depth,
            kind: TreeItemKind::Namespace,
            is_section_header: false,
        });
        flatten_namespaces(all, Some(&ns.id), depth + 1, out);
    }
}

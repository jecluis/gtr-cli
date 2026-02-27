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
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use super::theme::Theme;
use crate::cache::{CachedNamespace, CachedProject, TaskCache};

/// Nil UUID constant for the meta-root project.
const META_ROOT_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// The kind of entity a sidebar item represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeItemKind {
    SectionHeader,
    Project,
    Namespace,
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

/// Which section of the sidebar is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Projects,
    Namespaces,
}

/// State for the sidebar widget.
pub struct SidebarState {
    /// Flattened list of renderable items.
    items: Vec<TreeItem>,
    /// Index of the currently selected item.
    pub selected: usize,
}

impl SidebarState {
    /// Build sidebar state from cached data.
    pub fn from_cache(cache: &TaskCache) -> crate::Result<Self> {
        let projects = cache.list_projects().unwrap_or_default();
        let namespaces = cache.list_namespaces().unwrap_or_default();

        let mut items = Vec::new();

        // Projects section
        items.push(TreeItem {
            id: String::new(),
            name: "Projects".to_string(),
            depth: 0,
            kind: TreeItemKind::SectionHeader,
            is_section_header: true,
        });
        flatten_projects(&projects, Some(META_ROOT_UUID), 1, &mut items);

        // Namespaces section
        items.push(TreeItem {
            id: String::new(),
            name: "Namespaces".to_string(),
            depth: 0,
            kind: TreeItemKind::SectionHeader,
            is_section_header: true,
        });
        flatten_namespaces(&namespaces, None, 1, &mut items);

        Ok(Self { items, selected: 0 })
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
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
    pub fn render(&self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let block = ratatui::widgets::Block::bordered()
            .title(" sidebar ")
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        for (i, item) in self.items.iter().enumerate() {
            let y = i as u16;
            if y >= inner.height {
                break;
            }

            let is_selected = i == self.selected && focused;
            let line = render_item(item, theme, is_selected);
            let row = Rect::new(inner.x, inner.y + y, inner.width, 1);
            line.render(row, buf);
        }
    }
}

/// Render a single tree item as a styled Line.
fn render_item<'a>(item: &TreeItem, theme: &Theme, selected: bool) -> Line<'a> {
    let indent = "  ".repeat(item.depth as usize);

    let base_style = if selected {
        theme.selected
    } else {
        Style::default()
    };

    if item.is_section_header {
        let header_style = base_style
            .patch(theme.emphasis)
            .add_modifier(Modifier::UNDERLINED);
        Line::from(Span::styled(format!("{indent}{}", item.name), header_style))
    } else {
        let name_style = base_style.patch(theme.accent);
        Line::from(Span::styled(format!("{indent}{}", item.name), name_style))
    }
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

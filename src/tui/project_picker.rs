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

//! Project picker popup for the task update form.
//!
//! A search-as-you-type overlay listing all projects with hierarchy
//! paths, modelled after the wiki-link reference picker.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Padding, StatefulWidget, Widget};

use crate::cache::TaskCache;
use crate::tui::theme::Theme;

/// Maximum visible height (including border + input line).
const MAX_POPUP_HEIGHT: u16 = 14;

/// Minimum popup width.
const MIN_POPUP_WIDTH: u16 = 40;

/// A single project entry in the picker.
#[derive(Debug, Clone)]
pub struct ProjectPickerEntry {
    /// Project UUID.
    pub id: String,
    /// Project name (leaf segment).
    pub name: String,
    /// Full hierarchy path segments (e.g. `["home", "dev"]`).
    pub path: Vec<String>,
}

/// State for the project picker popup.
pub struct ProjectPickerState {
    /// Current search query.
    query: String,
    /// Byte offset of the cursor within `query`.
    cursor: usize,
    /// All non-deleted projects (loaded once on open).
    all_entries: Vec<ProjectPickerEntry>,
    /// Filtered entries matching the current query.
    filtered: Vec<ProjectPickerEntry>,
    /// List widget state for selection tracking.
    list_state: ListState,
}

impl ProjectPickerState {
    /// Create a new picker, loading all projects from the cache.
    pub fn new(cache: &TaskCache) -> Self {
        let projects = cache.list_projects().unwrap_or_default();
        let mut entries: Vec<ProjectPickerEntry> = projects
            .iter()
            .map(|p| {
                let path = cache.get_project_path(&p.id).unwrap_or_default();
                ProjectPickerEntry {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    path,
                }
            })
            .collect();

        // Sort by display path for a stable, readable order.
        entries.sort_by(|a, b| {
            a.path
                .join(" > ")
                .to_ascii_lowercase()
                .cmp(&b.path.join(" > ").to_ascii_lowercase())
        });

        let filtered = entries.clone();
        let mut state = Self {
            query: String::new(),
            cursor: 0,
            all_entries: entries,
            filtered,
            list_state: ListState::default(),
        };
        if !state.filtered.is_empty() {
            state.list_state.select(Some(0));
        }
        state
    }

    /// Insert a character at the cursor position and re-filter.
    pub fn char_input(&mut self, c: char) {
        self.query.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.update_filter();
    }

    /// Delete the character before the cursor and re-filter.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.query[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.query.remove(prev);
        self.cursor = prev;
        self.update_filter();
    }

    /// Move selection to the next entry (wraps around).
    pub fn select_next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let next = if current + 1 >= self.filtered.len() {
            0
        } else {
            current + 1
        };
        self.list_state.select(Some(next));
    }

    /// Move selection to the previous entry (wraps around).
    pub fn select_prev(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let next = if current == 0 {
            self.filtered.len() - 1
        } else {
            current - 1
        };
        self.list_state.select(Some(next));
    }

    /// Get the currently selected entry.
    pub fn selected(&self) -> Option<&ProjectPickerEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered.get(i))
    }

    /// Re-filter entries based on the current query.
    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = self.all_entries.clone();
        } else {
            let q = self.query.to_ascii_lowercase();
            self.filtered = self
                .all_entries
                .iter()
                .filter(|e| {
                    let display = e.path.join(" > ").to_ascii_lowercase();
                    display.contains(&q) || e.name.to_ascii_lowercase().contains(&q)
                })
                .cloned()
                .collect();
        }

        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    /// Render the picker popup at the bottom centre of the given area.
    pub fn render(&mut self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let width = (area.width * 2 / 3).max(MIN_POPUP_WIDTH).min(area.width);
        // border(2) + input(1) + results
        let content_rows = self.filtered.len() as u16 + 3;
        let height = content_rows.min(MAX_POPUP_HEIGHT).min(area.height);

        let popup = bottom_center_rect(area, width, height);
        Clear.render(popup, buf);

        let block = Block::bordered()
            .title(" Project ")
            .border_style(theme.border_focused)
            .padding(Padding::horizontal(1));
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);

        render_input(&self.query, self.cursor, theme, rows[0], buf);

        if self.filtered.is_empty() {
            let msg = if self.query.is_empty() {
                "  no projects"
            } else {
                "  no matches"
            };
            Line::from(Span::styled(msg, Style::default().fg(Color::Gray))).render(rows[1], buf);
        } else {
            let items: Vec<ListItem<'_>> = self.filtered.iter().map(|e| render_entry(e)).collect();

            let highlight_style = Style::default().bg(Color::DarkGray);
            let list = List::new(items).highlight_style(highlight_style);
            StatefulWidget::render(list, rows[1], buf, &mut self.list_state);
        }
    }
}

/// Render the search input line with a block cursor.
fn render_input(query: &str, cursor: usize, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let mut spans = vec![Span::styled(
        ">> ",
        theme.accent.add_modifier(Modifier::BOLD),
    )];

    if !query.is_empty() {
        let (before, after) = query.split_at(cursor);
        spans.push(Span::raw(before.to_string()));
        if after.is_empty() {
            spans.push(Span::styled("\u{2588}", theme.selected));
        } else {
            let first_char = after.chars().next().unwrap();
            spans.push(Span::styled(first_char.to_string(), theme.selected));
            if after.len() > first_char.len_utf8() {
                spans.push(Span::raw(after[first_char.len_utf8()..].to_string()));
            }
        }
    } else {
        spans.push(Span::styled(
            "type to filter...",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled("\u{2588}", theme.selected));
    }

    Line::from(spans).render(area, buf);
}

/// Format a single project entry as a list item.
fn render_entry(entry: &ProjectPickerEntry) -> ListItem<'static> {
    let display = entry.path.join(" > ");
    ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(display, Style::default()),
    ]))
}

/// Compute a popup rectangle positioned at the bottom centre of the
/// given area.
fn bottom_center_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[1]);
    horizontal[0]
}

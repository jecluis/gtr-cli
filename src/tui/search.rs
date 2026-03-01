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

//! Full-screen search overlay with live-filtering by title.
//!
//! Searches tasks and/or documents from the SQLite cache (title only).
//! Activated via `:search <query>` or the `s` compound keybinding group.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Padding, StatefulWidget, Widget};

use crate::cache::TaskCache;
use crate::tui::theme::{ENTITY_PALETTE, Theme};

/// Maximum number of results to show.
const MAX_RESULTS: usize = 50;

/// Which entity types the search includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFilter {
    /// Tasks and documents.
    All,
    /// Tasks only.
    Tasks,
    /// Documents only.
    Documents,
}

/// A single search result entry.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub kind: SearchResultKind,
    /// Hierarchy path segments (e.g., ["work", "clyso", "ces"]).
    pub context_path: Vec<String>,
    /// Colour for the context text.
    pub context_color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResultKind {
    Task,
    Document,
}

/// State for the search overlay.
pub struct SearchOverlayState {
    /// Current search input.
    input: String,
    cursor_pos: usize,
    /// Active entity filter.
    pub filter: SearchFilter,
    /// Current result set.
    results: Vec<SearchResult>,
    /// List widget state for selection tracking.
    list_state: ListState,
}

impl SearchOverlayState {
    /// Create a new search overlay, optionally pre-filled with a query.
    pub fn new(filter: SearchFilter, initial_query: &str) -> Self {
        let cursor_pos = initial_query.len();
        Self {
            input: initial_query.to_string(),
            cursor_pos,
            filter,
            results: Vec::new(),
            list_state: ListState::default(),
        }
    }

    /// Current input text.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Add a character at the cursor.
    pub fn char_input(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self.input[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev);
            self.cursor_pos = prev;
        }
    }

    /// Whether the input is empty.
    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let next = if current == 0 {
            self.results.len() - 1
        } else {
            current - 1
        };
        self.list_state.select(Some(next));
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let next = if current + 1 >= self.results.len() {
            0
        } else {
            current + 1
        };
        self.list_state.select(Some(next));
    }

    /// Get the currently selected result.
    pub fn selected_result(&self) -> Option<&SearchResult> {
        self.list_state.selected().and_then(|i| self.results.get(i))
    }

    /// Re-run the search against the cache and update results.
    pub fn update_results(&mut self, cache: &TaskCache) {
        self.results.clear();

        let query = self.input.trim();
        if query.is_empty() {
            self.list_state.select(None);
            return;
        }

        // Track unique container IDs to assign stable palette colours.
        let mut color_map: HashMap<String, Color> = HashMap::new();
        let mut color_idx: usize = 0;

        if matches!(self.filter, SearchFilter::All | SearchFilter::Tasks)
            && let Ok(tasks) = cache.search_tasks_by_title(query, MAX_RESULTS)
        {
            for task in tasks {
                let context_path = cache.get_project_path(&task.project_id).unwrap_or_default();
                let context_color =
                    *color_map.entry(task.project_id.clone()).or_insert_with(|| {
                        let c = ENTITY_PALETTE[color_idx % ENTITY_PALETTE.len()];
                        color_idx += 1;
                        c
                    });
                self.results.push(SearchResult {
                    id: task.id,
                    title: task.title,
                    kind: SearchResultKind::Task,
                    context_path,
                    context_color,
                });
            }
        }

        if matches!(self.filter, SearchFilter::All | SearchFilter::Documents)
            && let Ok(docs) = cache.search_documents_by_title(query, MAX_RESULTS)
        {
            for doc in docs {
                let path_ids = cache
                    .get_namespace_path(&doc.namespace_id)
                    .unwrap_or_default();
                let context_path: Vec<String> = path_ids
                    .iter()
                    .map(|id| {
                        cache
                            .get_namespace(id)
                            .ok()
                            .flatten()
                            .map(|n| n.name)
                            .unwrap_or_else(|| id[..8.min(id.len())].to_string())
                    })
                    .collect();
                let context_color =
                    *color_map
                        .entry(doc.namespace_id.clone())
                        .or_insert_with(|| {
                            let c = ENTITY_PALETTE[color_idx % ENTITY_PALETTE.len()];
                            color_idx += 1;
                            c
                        });
                self.results.push(SearchResult {
                    id: doc.id,
                    title: doc.title,
                    kind: SearchResultKind::Document,
                    context_path,
                    context_color,
                });
            }
        }

        // Select the first result if any.
        if !self.results.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    /// Render the search overlay centred on the screen.
    pub fn render(&mut self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup = centered_rect(area, 70, 80);
        Clear.render(popup, buf);

        let filter_label = match self.filter {
            SearchFilter::All => "all",
            SearchFilter::Tasks => "tasks",
            SearchFilter::Documents => "docs",
        };

        let block = Block::bordered()
            .title(format!(" search ({filter_label}) "))
            .border_style(theme.border_focused)
            .padding(Padding::horizontal(1));
        let inner = block.inner(popup);
        block.render(popup, buf);

        // Layout: input line at top, results below.
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);

        // Render input line with cursor.
        render_search_input(&self.input, self.cursor_pos, theme, rows[0], buf);

        // Render results list.
        if self.results.is_empty() && !self.input.trim().is_empty() {
            let msg = Line::from(Span::styled("  no results", theme.muted));
            msg.render(rows[1], buf);
        } else {
            let items: Vec<ListItem<'_>> = self
                .results
                .iter()
                .map(|r| render_result_item(r, theme))
                .collect();

            let highlight_style = Style::default().bg(Color::DarkGray);
            let list = List::new(items).highlight_style(highlight_style);
            StatefulWidget::render(list, rows[1], buf, &mut self.list_state);
        }
    }
}

/// Render the search input line with a block cursor.
fn render_search_input(
    input: &str,
    cursor_pos: usize,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let mut spans = vec![Span::styled("/", theme.accent.add_modifier(Modifier::BOLD))];

    if !input.is_empty() {
        let (before, after) = input.split_at(cursor_pos);
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
        spans.push(Span::styled("\u{2588}", theme.selected));
    }

    Line::from(spans).render(area, buf);
}

/// Render a single search result as a list item.
fn render_result_item<'a>(result: &SearchResult, _theme: &Theme) -> ListItem<'a> {
    let kind_label = match result.kind {
        SearchResultKind::Task => Span::styled(" T ", Style::default().fg(Color::Cyan)),
        SearchResultKind::Document => Span::styled(" D ", Style::default().fg(Color::Magenta)),
    };

    let title = Span::raw(result.title.clone());

    let mut spans = vec![kind_label, title];

    if !result.context_path.is_empty() {
        let path_str = result.context_path.join(" > ");
        spans.push(Span::styled(
            format!("  {path_str}"),
            Style::default().fg(result.context_color),
        ));
    }

    ListItem::new(Line::from(spans))
}

/// Compute a centred popup rectangle as a percentage of the outer area.
fn centered_rect(area: Rect, percent_width: u16, percent_height: u16) -> Rect {
    let w = (area.width as u32 * percent_width as u32 / 100) as u16;
    let h = (area.height as u32 * percent_height as u32 / 100) as u16;

    let vertical = Layout::vertical([Constraint::Length(h)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(w)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

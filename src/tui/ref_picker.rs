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

//! Wiki-link reference picker popup.
//!
//! Triggered when `[[` is typed in the document editor, this popup shows
//! matching tasks, documents, and namespaces from the cache. Results can
//! be filtered by type prefix (`task://`, `doc://`, `ns://`).

use std::collections::HashMap;
use std::ops::Not;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Padding, StatefulWidget, Widget};

use crate::cache::TaskCache;
use crate::tui::theme::{ENTITY_PALETTE, Theme};

/// Maximum total results shown in the picker.
const MAX_RESULTS: usize = 20;

/// Maximum visible height (including border + input line).
const MAX_POPUP_HEIGHT: u16 = 12;

/// Minimum popup width.
const MIN_POPUP_WIDTH: u16 = 30;

/// Which entity types the picker includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerFilter {
    /// All entity types.
    All,
    /// Tasks only (prefix `task://`).
    Tasks,
    /// Documents only (prefix `doc://`).
    Documents,
    /// Namespaces only (prefix `ns://`).
    Namespaces,
}

/// The kind of entity a picker result represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerResultKind {
    Task,
    Document,
    Namespace,
}

/// A single result entry in the reference picker.
#[derive(Debug, Clone)]
pub struct PickerResult {
    /// Display title.
    pub title: String,
    /// Entity kind.
    pub kind: PickerResultKind,
    /// Pre-formatted reference text to insert (without `[[ ]]`).
    pub insert_text: String,
    /// Hierarchy context segments (e.g. namespace or project path).
    pub context_path: Vec<String>,
    /// Colour for the context path text.
    pub context_color: Color,
}

/// State for the `[[` wiki-link reference picker popup.
pub struct RefPickerState {
    /// Current query text.
    query: String,
    /// Byte offset of the cursor within `query`.
    cursor: usize,
    /// Active entity filter (derived from type prefix).
    filter: PickerFilter,
    /// Current result set.
    results: Vec<PickerResult>,
    /// List widget state for selection tracking.
    list_state: ListState,
    /// Namespace ID of the source document (for same-namespace detection).
    namespace_id: String,
}

impl RefPickerState {
    /// Create a new picker with an empty query and `All` filter.
    pub fn new(namespace_id: String) -> Self {
        Self {
            query: String::new(),
            cursor: 0,
            filter: PickerFilter::All,
            results: Vec::new(),
            list_state: ListState::default(),
            namespace_id,
        }
    }

    /// Insert a character at the cursor position.
    pub fn char_input(&mut self, c: char) {
        self.query.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.detect_filter();
    }

    /// Delete the character before the cursor. Returns `false` if the
    /// query was already empty, signalling the caller to dismiss the
    /// picker.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return self.query.is_empty().not();
        }
        let prev = self.query[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.query.remove(prev);
        self.cursor = prev;
        self.detect_filter();
        true
    }

    /// Whether the query is empty.
    pub fn is_empty(&self) -> bool {
        self.query.is_empty()
    }

    /// Move selection to the next result (wraps around).
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

    /// Move selection to the previous result (wraps around).
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

    /// Get the currently selected result.
    pub fn selected_result(&self) -> Option<&PickerResult> {
        self.list_state.selected().and_then(|i| self.results.get(i))
    }

    /// Detect a type-prefix filter from the query text and update the
    /// active filter accordingly.
    pub fn detect_filter(&mut self) {
        let q = self.query.to_ascii_lowercase();
        if q.starts_with("task://") {
            self.filter = PickerFilter::Tasks;
        } else if q.starts_with("doc://") {
            self.filter = PickerFilter::Documents;
        } else if q.starts_with("ns://") {
            self.filter = PickerFilter::Namespaces;
        } else {
            self.filter = PickerFilter::All;
        }
    }

    /// Return the query with any type prefix stripped.
    pub fn search_query(&self) -> &str {
        let q = &self.query;
        let lower = q.to_ascii_lowercase();
        if lower.starts_with("task://") {
            &q[7..]
        } else if lower.starts_with("doc://") {
            &q[6..]
        } else if lower.starts_with("ns://") {
            &q[5..]
        } else {
            q
        }
    }

    /// Search the cache and populate results based on the current query
    /// and filter.
    pub fn update_results(&mut self, cache: &TaskCache) {
        self.results.clear();

        let query = self.search_query().trim().to_owned();
        if query.is_empty() {
            self.list_state.select(None);
            return;
        }

        let mut color_map: HashMap<String, Color> = HashMap::new();
        let mut color_idx: usize = 0;

        // Documents.
        if matches!(self.filter, PickerFilter::All | PickerFilter::Documents)
            && let Ok(docs) = cache.search_documents_by_title(&query, MAX_RESULTS)
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

                let insert_text = if doc.namespace_id == self.namespace_id {
                    format!("doc://{}", doc.slug)
                } else {
                    let ns_name = cache
                        .get_namespace(&doc.namespace_id)
                        .ok()
                        .flatten()
                        .map(|n| n.name)
                        .unwrap_or_else(|| {
                            doc.namespace_id[..8.min(doc.namespace_id.len())].to_string()
                        });
                    format!("doc://{}:{}", ns_name, doc.slug)
                };

                let context_color =
                    *color_map
                        .entry(doc.namespace_id.clone())
                        .or_insert_with(|| {
                            let c = ENTITY_PALETTE[color_idx % ENTITY_PALETTE.len()];
                            color_idx += 1;
                            c
                        });

                self.results.push(PickerResult {
                    title: doc.title,
                    kind: PickerResultKind::Document,
                    insert_text,
                    context_path,
                    context_color,
                });
            }
        }

        // Tasks.
        if matches!(self.filter, PickerFilter::All | PickerFilter::Tasks)
            && let Ok(tasks) = cache.search_tasks_by_title(&query, MAX_RESULTS)
        {
            for task in tasks {
                let context_path = cache.get_project_path(&task.project_id).unwrap_or_default();
                let prefix = &task.id[..8.min(task.id.len())];
                let insert_text = format!("task://{prefix}");
                let context_color =
                    *color_map.entry(task.project_id.clone()).or_insert_with(|| {
                        let c = ENTITY_PALETTE[color_idx % ENTITY_PALETTE.len()];
                        color_idx += 1;
                        c
                    });

                self.results.push(PickerResult {
                    title: task.title,
                    kind: PickerResultKind::Task,
                    insert_text,
                    context_path,
                    context_color,
                });
            }
        }

        // Namespaces.
        if matches!(self.filter, PickerFilter::All | PickerFilter::Namespaces)
            && let Ok(namespaces) = cache.list_namespaces()
        {
            let query_lower = query.to_ascii_lowercase();
            for ns in namespaces {
                if !ns.name.to_ascii_lowercase().contains(&query_lower) {
                    continue;
                }
                let path_ids = cache.get_namespace_path(&ns.id).unwrap_or_default();
                let path_names: Vec<String> = path_ids
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

                let insert_text = format!("ns://{}", path_names.join("/"));

                let context_color = *color_map.entry(ns.id.clone()).or_insert_with(|| {
                    let c = ENTITY_PALETTE[color_idx % ENTITY_PALETTE.len()];
                    color_idx += 1;
                    c
                });

                self.results.push(PickerResult {
                    title: ns.name,
                    kind: PickerResultKind::Namespace,
                    insert_text,
                    context_path: path_names,
                    context_color,
                });
            }
        }

        // Truncate to overall limit.
        self.results.truncate(MAX_RESULTS);

        if !self.results.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    /// Render the picker popup at the bottom centre of the given area.
    pub fn render(&mut self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let width = (area.width / 2).max(MIN_POPUP_WIDTH).min(area.width);
        let content_rows = self.results.len() as u16 + 3; // border(2) + input(1)
        let height = content_rows.min(MAX_POPUP_HEIGHT).min(area.height);

        let popup = bottom_center_rect(area, width, height);
        Clear.render(popup, buf);

        let filter_label = match self.filter {
            PickerFilter::All => "all",
            PickerFilter::Tasks => "tasks",
            PickerFilter::Documents => "docs",
            PickerFilter::Namespaces => "ns",
        };

        let block = Block::bordered()
            .title(format!(" [[ {filter_label} "))
            .border_style(theme.border_focused)
            .padding(Padding::horizontal(1));
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);

        render_picker_input(&self.query, self.cursor, theme, rows[0], buf);

        if self.results.is_empty() && !self.query.trim().is_empty() {
            let msg = Line::from(Span::styled("  no results", theme.muted));
            msg.render(rows[1], buf);
        } else {
            let items: Vec<ListItem<'_>> = self
                .results
                .iter()
                .map(|r| render_picker_item(r, theme))
                .collect();

            let highlight_style = Style::default().bg(Color::DarkGray);
            let list = List::new(items).highlight_style(highlight_style);
            StatefulWidget::render(list, rows[1], buf, &mut self.list_state);
        }
    }
}

/// Render the picker input line with a block cursor.
fn render_picker_input(query: &str, cursor: usize, theme: &Theme, area: Rect, buf: &mut Buffer) {
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
        spans.push(Span::styled("\u{2588}", theme.selected));
    }

    Line::from(spans).render(area, buf);
}

/// Format a single picker result as a list item.
fn render_picker_item<'a>(result: &PickerResult, _theme: &Theme) -> ListItem<'a> {
    let (letter, color) = match result.kind {
        PickerResultKind::Document => ("D", Color::Cyan),
        PickerResultKind::Task => ("T", Color::Green),
        PickerResultKind::Namespace => ("N", Color::Yellow),
    };

    let kind_span = Span::styled(format!(" {letter} "), Style::default().fg(color));
    let title_span = Span::raw(result.title.clone());

    let mut spans = vec![kind_span, title_span];

    if !result.context_path.is_empty() {
        let path_str = result.context_path.join(" > ");
        spans.push(Span::styled(
            format!("  {path_str}"),
            Style::default().fg(result.context_color),
        ));
    }

    ListItem::new(Line::from(spans))
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

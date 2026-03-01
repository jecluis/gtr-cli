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

//! Document list view showing a namespace's documents in a table.
//!
//! Displays documents sorted as a depth-first tree (matching the CLI's
//! `print_documents_as_tree`), with parent-child hierarchy shown via
//! tree connectors. Supports filtering by title.

use std::collections::HashMap;

use chrono::DateTime;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
    Table, TableState, Widget,
};

use super::theme::{LABEL_PALETTE, Theme};
use crate::cache::{CachedDocument, TaskCache};
use crate::config::Config;
use crate::display::{self, LabelColorIndex};
use crate::output::compute_min_prefix_len;

/// State for the document list view.
pub struct DocumentListState {
    /// Namespace ID whose documents are being shown.
    pub namespace_id: String,
    /// Display name for the namespace (shown in title).
    pub namespace_name: String,
    /// Breadcrumb trail for the title bar.
    breadcrumb: String,
    /// Documents in tree-flattened order.
    documents: Vec<CachedDocument>,
    /// Tree depth per document index (for indentation).
    depth_map: Vec<u16>,
    /// Whether each document is the last child at its level.
    is_last_child: Vec<bool>,
    /// Currently selected visible row index.
    selected: usize,
    /// Minimum unique ID prefix length.
    prefix_len: usize,
    /// Label colour assignments.
    label_color_map: HashMap<String, LabelColorIndex>,
    /// Optional filter text.
    filter: Option<String>,
    /// Indices into `documents` that pass the current filter.
    filtered_indices: Vec<usize>,
    /// Table widget state (tracks scroll offset).
    table_state: TableState,
}

impl DocumentListState {
    /// Build the document list from cache, tree-flattening by parent-child
    /// hierarchy (same algorithm as CLI's `print_documents_as_tree`).
    pub fn from_cache(
        cache: &TaskCache,
        namespace_id: &str,
        namespace_name: &str,
        config: &Config,
    ) -> crate::Result<Self> {
        let _ = config; // reserved for future use

        let docs = cache.list_documents(namespace_id, false)?;

        // Compute prefix length across all documents.
        let all_ids = cache.all_document_ids().unwrap_or_default();
        let prefix_len = compute_min_prefix_len(&all_ids);

        // Build namespace breadcrumb.
        let ns_path = cache.get_namespace_path(namespace_id).unwrap_or_default();
        let breadcrumb = if ns_path.len() > 1 {
            // Resolve names for ancestor namespaces.
            let names: Vec<String> = ns_path
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
            names.join(" > ")
        } else {
            namespace_name.to_string()
        };

        // Tree-flatten documents (depth-first, alphabetical within each level).
        let (flattened, depths, last_child_flags) = tree_flatten(&docs);

        // Assign label colours by first appearance in display order.
        let label_color_map =
            display::assign_label_colors(flattened.iter().map(|d| d.labels.as_slice()))
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();

        let filtered_indices = (0..flattened.len()).collect();
        let mut table_state = TableState::default();
        if !flattened.is_empty() {
            table_state.select(Some(0));
        }

        Ok(Self {
            namespace_id: namespace_id.to_string(),
            namespace_name: namespace_name.to_string(),
            breadcrumb,
            documents: flattened,
            depth_map: depths,
            is_last_child: last_child_flags,
            selected: 0,
            prefix_len,
            label_color_map,
            filter: None,
            filtered_indices,
            table_state,
        })
    }

    /// Reload documents from cache, preserving the current selection by ID.
    pub fn refresh(&mut self, cache: &TaskCache, config: &Config) {
        let prev_id = self.selected_id().map(String::from);

        if let Ok(new) = Self::from_cache(cache, &self.namespace_id, &self.namespace_name, config) {
            *self = new;
        }

        // Restore selection by ID if possible.
        if let Some(ref prev) = prev_id
            && let Some(pos) = self
                .filtered_indices
                .iter()
                .position(|&idx| self.documents[idx].id == *prev)
        {
            self.selected = pos;
            self.table_state.select(Some(pos));
        }
    }

    /// Get the ID of the currently selected document.
    pub fn selected_id(&self) -> Option<&str> {
        self.filtered_indices
            .get(self.selected)
            .and_then(|&idx| self.documents.get(idx))
            .map(|d| d.id.as_str())
    }

    /// Move selection up by one row.
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.table_state.select(Some(self.selected));
        }
    }

    /// Move selection down by one row.
    pub fn select_next(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected + 1 < self.filtered_indices.len() {
            self.selected += 1;
            self.table_state.select(Some(self.selected));
        }
    }

    /// Move selection up by a page.
    pub fn select_page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
        self.table_state.select(Some(self.selected));
    }

    /// Move selection down by a page.
    pub fn select_page_down(&mut self, page_size: usize) {
        if !self.filtered_indices.is_empty() {
            self.selected = (self.selected + page_size).min(self.filtered_indices.len() - 1);
            self.table_state.select(Some(self.selected));
        }
    }

    /// Enter filter mode.
    pub fn start_filter(&mut self) {
        self.filter = Some(String::new());
    }

    /// Cancel the filter and show all documents.
    pub fn cancel_filter(&mut self) {
        self.filter = None;
        self.filtered_indices = (0..self.documents.len()).collect();
        self.selected = 0;
        self.table_state.select(Some(0));
    }

    /// Append a character to the filter query.
    pub fn filter_push(&mut self, c: char) {
        if let Some(ref mut f) = self.filter {
            f.push(c);
        }
        self.recompute_filter();
    }

    /// Remove the last character from the filter query.
    pub fn filter_pop(&mut self) {
        if let Some(ref mut f) = self.filter {
            f.pop();
        }
        self.recompute_filter();
    }

    /// Whether filter mode is active.
    pub fn is_filtering(&self) -> bool {
        self.filter.is_some()
    }

    /// Recompute filtered indices based on the current filter text.
    fn recompute_filter(&mut self) {
        let query = self.filter.as_deref().unwrap_or("").to_lowercase();

        if query.is_empty() {
            self.filtered_indices = (0..self.documents.len()).collect();
        } else {
            self.filtered_indices = self
                .documents
                .iter()
                .enumerate()
                .filter(|(_, d)| d.title.to_lowercase().contains(&query))
                .map(|(i, _)| i)
                .collect();
        }

        self.selected = 0;
        self.table_state
            .select(if self.filtered_indices.is_empty() {
                None
            } else {
                Some(0)
            });
    }

    /// Render the document list table.
    pub fn render(&mut self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let title = if let Some(ref q) = self.filter {
            format!(" {} \u{2502} /{q}\u{2588} ", self.breadcrumb)
        } else {
            format!(" {} ", self.breadcrumb)
        };

        let block = Block::bordered().title(title).border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        if self.filtered_indices.is_empty() {
            let msg = if self.filter.is_some() {
                "  No matching documents"
            } else {
                "  No documents"
            };
            Paragraph::new(Line::from(Span::styled(msg, theme.muted))).render(inner, buf);
            return;
        }

        // Column widths: ID, Title (fill), Labels, Modified.
        let widths = [
            Constraint::Length(13),
            Constraint::Fill(1),
            Constraint::Length(20),
            Constraint::Length(10),
        ];

        // Build header.
        let header = Row::new([
            Cell::from("  ID"),
            Cell::from("Title"),
            Cell::from("Labels"),
            Cell::from("Modified"),
        ])
        .style(theme.emphasis)
        .bottom_margin(0);

        // Resolve the Labels column's actual width for wrapping.
        let labels_width = {
            let col_rects = ratatui::layout::Layout::horizontal(widths)
                .spacing(2)
                .split(inner);
            col_rects[2].width as usize
        };

        // Build rows.
        let mut rows: Vec<Row<'_>> = Vec::new();
        for (vis_row, &doc_idx) in self.filtered_indices.iter().enumerate() {
            let doc = &self.documents[doc_idx];
            let is_selected = vis_row == self.selected && focused;
            let row = self.render_row(doc, doc_idx, theme, is_selected, labels_width);

            let mut row = if is_selected {
                row.style(theme.selected)
            } else if vis_row % 2 == 1 {
                row.style(Style::default().bg(theme.row_alt_bg))
            } else {
                row
            };
            row = row.bottom_margin(0);
            rows.push(row);
        }

        let table = Table::new(rows, widths).header(header).column_spacing(2);
        StatefulWidget::render(table, inner, buf, &mut self.table_state);

        // Scrollbar when content overflows.
        let viewport_height = inner.height.saturating_sub(1); // minus header
        if self.filtered_indices.len() as u16 > viewport_height {
            use ratatui::layout::Margin;
            let scrollbar_area = area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            });
            let mut scrollbar_state = ScrollbarState::new(self.filtered_indices.len())
                .position(self.table_state.offset())
                .viewport_content_length(viewport_height as usize);
            StatefulWidget::render(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None),
                scrollbar_area,
                buf,
                &mut scrollbar_state,
            );
        }

        // Footer: document count.
        let count_text = if self.filter.is_some() {
            format!(
                " {}/{} documents ",
                self.filtered_indices.len(),
                self.documents.len()
            )
        } else {
            format!(" {} documents ", self.documents.len())
        };
        let footer_area = Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        );
        Line::from(Span::styled(count_text, theme.muted)).render(footer_area, buf);
    }

    /// Render a single document row.
    fn render_row<'a>(
        &self,
        doc: &CachedDocument,
        doc_idx: usize,
        theme: &Theme,
        selected: bool,
        labels_width: usize,
    ) -> Row<'a> {
        let base = if selected {
            theme.selected
        } else {
            Style::default()
        };

        // ID cell: cyan prefix | gray suffix (no DIM to avoid black bg).
        let (prefix, suffix) = display::split_id(&doc.id, self.prefix_len);
        let id_cell = Cell::from(Line::from(vec![
            Span::styled(format!("  {prefix}\u{2502}"), base.patch(theme.accent)),
            Span::styled(suffix.to_string(), base.fg(Color::Gray)),
        ]));

        // Title cell: tree connector + title.
        let depth = self.depth_map.get(doc_idx).copied().unwrap_or(0);
        let title_cell = if depth > 0 {
            let is_last = self.is_last_child.get(doc_idx).copied().unwrap_or(true);
            let connector = if is_last {
                "\u{2514}\u{2500}\u{2500} "
            } else {
                "\u{251c}\u{2500}\u{2500} "
            };
            let indent = "    ".repeat((depth - 1) as usize);
            Cell::from(Line::from(vec![
                Span::styled(format!("{indent}{connector}"), base.patch(theme.muted)),
                Span::styled(doc.title.clone(), base),
            ]))
        } else {
            Cell::from(Line::styled(doc.title.clone(), base))
        };

        // Labels cell (multi-line if labels exceed column width).
        let labels_cell = if doc.labels.is_empty() {
            Cell::from(Text::default())
        } else {
            let mut label_lines: Vec<Line<'_>> = Vec::new();
            let mut current_spans: Vec<Span<'_>> = Vec::new();
            let mut current_width: usize = 0;

            for (i, label) in doc.labels.iter().enumerate() {
                let sep_width = if i > 0 { 2 } else { 0 }; // ", "
                let needed = sep_width + label.len();

                // Wrap to next line if this label would overflow.
                if current_width > 0 && current_width + needed > labels_width {
                    label_lines.push(Line::from(std::mem::take(&mut current_spans)));
                    current_width = 0;
                }

                if current_width > 0 {
                    current_spans.push(Span::styled(", ", base));
                    current_width += 2;
                }

                let idx = self
                    .label_color_map
                    .get(label.as_str())
                    .copied()
                    .unwrap_or(0);
                let color = LABEL_PALETTE[idx];
                current_spans.push(Span::styled(label.clone(), base.fg(color)));
                current_width += label.len();
            }

            if !current_spans.is_empty() {
                label_lines.push(Line::from(current_spans));
            }

            Cell::from(Text::from(label_lines))
        };

        // Modified cell: relative time (plain text, no dim).
        let modified_text = format_relative_time_str(&doc.modified);
        let modified_cell = Cell::from(Line::styled(modified_text, base));

        Row::new([id_cell, title_cell, labels_cell, modified_cell])
    }
}

/// Tree-flatten documents depth-first, alphabetically within each level.
///
/// Returns `(flattened_docs, depths, is_last_child_flags)`.
fn tree_flatten(docs: &[CachedDocument]) -> (Vec<CachedDocument>, Vec<u16>, Vec<bool>) {
    let doc_ids: std::collections::HashSet<&str> = docs.iter().map(|d| d.id.as_str()).collect();

    let mut children_map: HashMap<Option<&str>, Vec<&CachedDocument>> = HashMap::new();
    for doc in docs {
        let key = match doc.parent_id.as_deref() {
            Some(pid) if doc_ids.contains(pid) => Some(pid),
            _ => None,
        };
        children_map.entry(key).or_default().push(doc);
    }
    // Sort children alphabetically by title within each group.
    for group in children_map.values_mut() {
        group.sort_by(|a, b| a.title.cmp(&b.title));
    }

    let mut flattened = Vec::new();
    let mut depths = Vec::new();
    let mut last_child_flags = Vec::new();

    let roots = children_map.get(&None).cloned().unwrap_or_default();
    for (i, doc) in roots.iter().enumerate() {
        let is_last = i == roots.len() - 1;
        flatten_recurse(
            doc,
            0,
            is_last,
            &children_map,
            &mut flattened,
            &mut depths,
            &mut last_child_flags,
        );
    }

    (flattened, depths, last_child_flags)
}

/// Recursive helper for tree-flattening.
fn flatten_recurse(
    doc: &CachedDocument,
    depth: u16,
    is_last: bool,
    children_map: &HashMap<Option<&str>, Vec<&CachedDocument>>,
    out: &mut Vec<CachedDocument>,
    depths: &mut Vec<u16>,
    last_child_flags: &mut Vec<bool>,
) {
    out.push(doc.clone());
    depths.push(depth);
    last_child_flags.push(is_last);

    let children = children_map
        .get(&Some(doc.id.as_str()))
        .cloned()
        .unwrap_or_default();

    for (i, child) in children.iter().enumerate() {
        let child_is_last = i == children.len() - 1;
        flatten_recurse(
            child,
            depth + 1,
            child_is_last,
            children_map,
            out,
            depths,
            last_child_flags,
        );
    }
}

/// Format an RFC 3339 timestamp as a relative time string (e.g. "2d ago").
fn format_relative_time_str(rfc3339: &str) -> String {
    DateTime::parse_from_rfc3339(rfc3339)
        .map(|dt| {
            let now = chrono::Utc::now();
            let duration = now.signed_duration_since(dt);
            let minutes = duration.num_minutes();
            let hours = duration.num_hours();
            let days = duration.num_days();

            if minutes < 1 {
                "just now".to_string()
            } else if minutes < 60 {
                format!("{minutes}m ago")
            } else if hours < 24 {
                format!("{hours}h ago")
            } else {
                format!("{days}d ago")
            }
        })
        .unwrap_or_else(|_| "-".to_string())
}

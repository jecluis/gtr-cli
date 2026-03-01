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

use super::theme::{ENTITY_PALETTE, LABEL_PALETTE, Theme};
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
    /// Per-document guide rails: for each ancestor depth, whether to
    /// draw a `│` continuation line (true = ancestor has more siblings).
    guide_rails: Vec<Vec<bool>>,
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
    /// Whether recursive listing is active (shows descendant namespaces).
    recursive: bool,
    /// Maps namespace_id → ancestor path names (populated in recursive mode).
    namespace_paths: HashMap<String, Vec<String>>,
    /// Maps namespace_id → palette colour (populated in recursive mode).
    namespace_colors: HashMap<String, Color>,
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
        let (flattened, depths, last_child_flags, rails) = tree_flatten(&docs);

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
            guide_rails: rails,
            selected: 0,
            prefix_len,
            label_color_map,
            filter: None,
            filtered_indices,
            recursive: false,
            namespace_paths: HashMap::new(),
            namespace_colors: HashMap::new(),
            table_state,
        })
    }

    /// Build a document list showing all documents across all namespaces.
    pub fn from_all_namespaces(cache: &TaskCache, config: &Config) -> crate::Result<Self> {
        let _ = config;

        let namespaces = cache.list_namespaces()?;

        // Load documents from every namespace.
        let mut all_docs = Vec::new();
        for ns in &namespaces {
            if let Ok(docs) = cache.list_documents(&ns.id, false) {
                all_docs.extend(docs);
            }
        }

        let all_ids = cache.all_document_ids().unwrap_or_default();
        let prefix_len = compute_min_prefix_len(&all_ids);

        let (flattened, depths, last_child_flags, rails) = tree_flatten(&all_docs);

        let label_color_map =
            display::assign_label_colors(flattened.iter().map(|d| d.labels.as_slice()))
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();

        // Build namespace path and colour lookups.
        let mut namespace_paths = HashMap::new();
        let mut namespace_colors = HashMap::new();
        for (idx, ns) in namespaces.iter().enumerate() {
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
            namespace_paths.insert(ns.id.clone(), path_names);
            namespace_colors.insert(ns.id.clone(), ENTITY_PALETTE[idx % ENTITY_PALETTE.len()]);
        }

        let filtered_indices = (0..flattened.len()).collect();
        let mut table_state = TableState::default();
        if !flattened.is_empty() {
            table_state.select(Some(0));
        }

        Ok(Self {
            namespace_id: String::new(),
            namespace_name: "All Namespaces".to_string(),
            breadcrumb: "All Namespaces".to_string(),
            documents: flattened,
            depth_map: depths,
            is_last_child: last_child_flags,
            guide_rails: rails,
            selected: 0,
            prefix_len,
            label_color_map,
            filter: None,
            filtered_indices,
            recursive: true,
            namespace_paths,
            namespace_colors,
            table_state,
        })
    }

    /// Reload documents from cache, preserving the current selection by ID.
    pub fn refresh(&mut self, cache: &TaskCache, config: &Config) {
        let prev_id = self.selected_id().map(String::from);
        let was_recursive = self.recursive;

        if self.namespace_id.is_empty() {
            // "All Namespaces" mode — reload from all.
            if let Ok(new) = Self::from_all_namespaces(cache, config) {
                *self = new;
            }
        } else if let Ok(new) =
            Self::from_cache(cache, &self.namespace_id, &self.namespace_name, config)
        {
            *self = new;
            // Restore recursive mode if it was active.
            if was_recursive {
                self.toggle_recursive(cache);
            }
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

    /// Toggle recursive mode (show documents from descendant namespaces).
    pub fn toggle_recursive(&mut self, cache: &TaskCache) {
        self.recursive = !self.recursive;

        let docs = if self.recursive {
            // Gather all descendant namespace IDs.
            let descendants = cache
                .get_namespace_descendants(&self.namespace_id)
                .unwrap_or_default();

            let mut all_ns_ids = vec![self.namespace_id.clone()];
            all_ns_ids.extend(descendants);

            // Build namespace path and colour lookups.
            self.namespace_paths.clear();
            self.namespace_colors.clear();
            for (idx, nsid) in all_ns_ids.iter().enumerate() {
                let path_ids = cache.get_namespace_path(nsid).unwrap_or_default();
                // Trim to start from the recursion root namespace.
                let start = path_ids
                    .iter()
                    .position(|id| id == &self.namespace_id)
                    .unwrap_or(0);
                let path_names: Vec<String> = path_ids[start..]
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
                self.namespace_paths.insert(nsid.clone(), path_names);
                self.namespace_colors
                    .insert(nsid.clone(), ENTITY_PALETTE[idx % ENTITY_PALETTE.len()]);
            }

            // Load documents from all namespaces.
            let mut all_docs = Vec::new();
            for nsid in &all_ns_ids {
                if let Ok(list) = cache.list_documents(nsid, false) {
                    all_docs.extend(list);
                }
            }
            all_docs
        } else {
            self.namespace_paths.clear();
            self.namespace_colors.clear();
            cache
                .list_documents(&self.namespace_id, false)
                .unwrap_or_default()
        };

        // Re-flatten and rebuild state.
        let (flattened, depths, last_child_flags, rails) = tree_flatten(&docs);

        self.label_color_map =
            display::assign_label_colors(flattened.iter().map(|d| d.labels.as_slice()))
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();

        self.documents = flattened;
        self.depth_map = depths;
        self.is_last_child = last_child_flags;
        self.guide_rails = rails;
        self.filter = None;
        self.filtered_indices = (0..self.documents.len()).collect();
        self.selected = 0;
        self.table_state.select(if self.documents.is_empty() {
            None
        } else {
            Some(0)
        });
    }

    /// Build the namespace cell for recursive mode.
    ///
    /// Returns `(Cell, line_count)`.
    fn build_namespace_cell<'a>(&self, namespace_id: &str, base: Style) -> (Cell<'a>, usize) {
        let color = self
            .namespace_colors
            .get(namespace_id)
            .copied()
            .unwrap_or(Color::White);

        let path = self.namespace_paths.get(namespace_id);

        match path {
            Some(p) if p.len() > 1 => {
                let mut lines: Vec<Line<'_>> = Vec::new();
                for (i, segment) in p.iter().enumerate() {
                    let is_last = i == p.len() - 1;
                    if i == 0 {
                        lines.push(Line::styled(segment.clone(), base.fg(color)));
                    } else {
                        let connector = if is_last { "\u{2514} " } else { "\u{251c} " };
                        let indent = "  ".repeat(i.saturating_sub(1));
                        lines.push(Line::from(vec![
                            Span::styled(indent, base),
                            Span::styled(connector.to_string(), base.fg(color)),
                            Span::styled(segment.clone(), base.fg(color)),
                        ]));
                    }
                }
                let count = lines.len();
                (Cell::from(Text::from(lines)), count)
            }
            Some(p) if !p.is_empty() => {
                let line = Line::styled(p[0].clone(), base.fg(color));
                (Cell::from(line), 1)
            }
            _ => {
                let short = &namespace_id[..8.min(namespace_id.len())];
                let line = Line::styled(short.to_string(), base.fg(color));
                (Cell::from(line), 1)
            }
        }
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

        // Column widths: ID, Title (fill), [Namespace if recursive], Labels, Modified.
        let mut widths: Vec<Constraint> = vec![Constraint::Length(13), Constraint::Fill(1)];
        if self.recursive {
            widths.push(Constraint::Length(14));
        }
        widths.extend([Constraint::Length(20), Constraint::Length(10)]);

        // Build header.
        let mut header_cells: Vec<Cell<'_>> = vec![Cell::from("  ID"), Cell::from("Title")];
        if self.recursive {
            header_cells.push(Cell::from("Namespace"));
        }
        header_cells.extend([Cell::from("Labels"), Cell::from("Modified")]);
        let header = Row::new(header_cells)
            .style(theme.emphasis)
            .bottom_margin(0);

        // Resolve the Labels column's actual width for wrapping.
        let labels_col_idx = if self.recursive { 3 } else { 2 };
        let labels_width = {
            let col_rects = ratatui::layout::Layout::horizontal(&widths)
                .spacing(2)
                .split(inner);
            col_rects[labels_col_idx].width as usize
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

        let table = Table::new(rows, &widths).header(header).column_spacing(2);
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

        // Compute namespace cell first so we know the row height.
        let mut max_lines: usize = 1;
        let ns_cell = if self.recursive {
            let (cell, ns_lines) = self.build_namespace_cell(&doc.namespace_id, base);
            max_lines = max_lines.max(ns_lines);
            Some(cell)
        } else {
            None
        };

        // Title cell: tree connector + title, with continuation lines
        // to fill multi-line rows.
        let depth = self.depth_map.get(doc_idx).copied().unwrap_or(0);
        let is_last = self.is_last_child.get(doc_idx).copied().unwrap_or(true);
        // Check if this document has children (next in flattened order is deeper).
        let has_children = self
            .depth_map
            .get(doc_idx + 1)
            .is_some_and(|&next_d| next_d > depth);

        let title_cell = if depth > 0 {
            let connector = if is_last {
                "\u{2514}\u{2500}\u{2500} "
            } else {
                "\u{251c}\u{2500}\u{2500} "
            };
            let indent = "    ".repeat((depth - 1) as usize);
            let mut lines = vec![Line::from(vec![
                Span::styled(format!("{indent}{connector}"), base.patch(theme.muted)),
                Span::styled(doc.title.clone(), base),
            ])];

            // Add continuation lines with │ guide rails for multi-line rows.
            if max_lines > 1 {
                let rails = self.guide_rails.get(doc_idx).cloned().unwrap_or_default();
                let continuation =
                    build_continuation_line(&rails, depth, is_last, has_children, base, theme);
                for _ in 1..max_lines {
                    lines.push(continuation.clone());
                }
            }

            Cell::from(Text::from(lines))
        } else if max_lines > 1 {
            // Depth-0 document: no tree connector, but may need │ if it
            // has children that will draw connectors below.
            let mut lines = vec![Line::styled(doc.title.clone(), base)];
            let continuation = if has_children {
                Line::styled("\u{2502}   ", base.patch(theme.muted))
            } else {
                Line::from("")
            };
            for _ in 1..max_lines {
                lines.push(continuation.clone());
            }
            Cell::from(Text::from(lines))
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

        let mut cells: Vec<Cell<'a>> = vec![id_cell, title_cell];
        if let Some(cell) = ns_cell {
            cells.push(cell);
        }
        cells.extend([labels_cell, modified_cell]);
        Row::new(cells).height(max_lines as u16)
    }
}

/// Build a continuation line for a multi-line row's title cell.
///
/// Draws `│` at each ancestor depth where the ancestor has more siblings,
/// `│` at the node's own depth if it is not the last child, and `│` below
/// the connector if the node has children.
fn build_continuation_line<'a>(
    rails: &[bool],
    depth: u16,
    is_last: bool,
    has_children: bool,
    base: Style,
    theme: &Theme,
) -> Line<'a> {
    let mut s = String::new();
    // Ancestor levels: draw │ where the ancestor has more siblings.
    // Skip the immediate parent level (depth-1) when this node is the
    // last child — the └── connector already terminates that line.
    for d in 0..depth as usize {
        let suppressed = is_last && d + 1 == depth as usize;
        let has_rail = !suppressed && rails.get(d).copied().unwrap_or(false);
        if has_rail {
            s.push_str("\u{2502}   "); // │ + 3 spaces (matches "    " indent)
        } else {
            s.push_str("    ");
        }
    }
    // Node's own level: draw │ if not last child (more siblings follow).
    if depth > 0 && !is_last {
        s.push_str("\u{2502}   ");
    }
    // One level deeper: draw │ if node has children (connector comes next row).
    if has_children {
        s.push_str("\u{2502}   ");
    }
    Line::styled(s, base.patch(theme.muted))
}

/// Accumulator for tree-flattening output.
struct TreeFlattenOut {
    docs: Vec<CachedDocument>,
    depths: Vec<u16>,
    is_last_child: Vec<bool>,
    guide_rails: Vec<Vec<bool>>,
}

/// Tree-flatten documents depth-first, alphabetically within each level.
///
/// Returns `(flattened_docs, depths, is_last_child_flags, guide_rails)`.
fn tree_flatten(
    docs: &[CachedDocument],
) -> (Vec<CachedDocument>, Vec<u16>, Vec<bool>, Vec<Vec<bool>>) {
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

    let mut out = TreeFlattenOut {
        docs: Vec::new(),
        depths: Vec::new(),
        is_last_child: Vec::new(),
        guide_rails: Vec::new(),
    };

    let roots = children_map.get(&None).cloned().unwrap_or_default();
    for (i, doc) in roots.iter().enumerate() {
        let is_last = i == roots.len() - 1;
        flatten_recurse(doc, 0, is_last, &[], &children_map, &mut out);
    }

    (out.docs, out.depths, out.is_last_child, out.guide_rails)
}

/// Recursive helper for tree-flattening.
fn flatten_recurse(
    doc: &CachedDocument,
    depth: u16,
    is_last: bool,
    parent_rails: &[bool],
    children_map: &HashMap<Option<&str>, Vec<&CachedDocument>>,
    out: &mut TreeFlattenOut,
) {
    out.docs.push(doc.clone());
    out.depths.push(depth);
    out.is_last_child.push(is_last);
    out.guide_rails.push(parent_rails.to_vec());

    let children = children_map
        .get(&Some(doc.id.as_str()))
        .cloned()
        .unwrap_or_default();

    // Build guide rails for children: inherit parent's rails + whether
    // this node has more siblings after it (draw │ if not last).
    let mut child_rails: Vec<bool> = parent_rails.to_vec();
    child_rails.push(!is_last);

    for (i, child) in children.iter().enumerate() {
        let child_is_last = i == children.len() - 1;
        flatten_recurse(
            child,
            depth + 1,
            child_is_last,
            &child_rails,
            children_map,
            out,
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

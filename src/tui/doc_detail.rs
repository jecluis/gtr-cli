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

//! Document detail view showing full document information.
//!
//! Displays styled metadata fields, rendered markdown content,
//! forward references, and back-links in a scrollable view.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget, Wrap,
};

use super::nav::{NavLink, NavTarget};
use super::theme::{LABEL_PALETTE, Theme};
use crate::cache::{ReferenceRow, TaskCache};
use crate::config::Config;
use crate::display::{self, LabelColorIndex};
use crate::models::Document;
use crate::output::compute_min_prefix_len;

/// Padding width for field labels (e.g. "Namespace:  value").
const FIELD_LABEL_WIDTH: usize = 14;

/// Resolved child document info.
struct ChildInfo {
    id: String,
    title: String,
}

/// Resolved parent document info.
struct ParentDocInfo {
    id: String,
    title: String,
}

/// A reference row enriched with resolved entity title and slug.
struct ResolvedRef {
    /// The underlying reference row.
    row: ReferenceRow,
    /// Resolved title of the linked entity (if found).
    title: Option<String>,
    /// Resolved slug (documents only).
    slug: Option<String>,
}

/// State for the document detail view.
pub struct DocumentDetailState {
    /// Full document loaded from CRDT storage.
    doc: Document,
    /// Display name of the containing namespace.
    namespace_name: String,
    /// Breadcrumb trail for namespace ancestors.
    breadcrumb: String,
    /// Vertical scroll offset.
    scroll: u16,
    /// Total content height (updated on each render).
    content_height: u16,
    /// Minimum unique ID prefix length.
    prefix_len: usize,
    /// Label colour assignments.
    label_color_map: HashMap<String, LabelColorIndex>,
    /// Forward references with resolved titles.
    forward_refs: Vec<ResolvedRef>,
    /// Back-references with resolved titles.
    back_refs: Vec<ResolvedRef>,
    /// Resolved parent document info.
    parent_doc_info: Option<ParentDocInfo>,
    /// Resolved child documents.
    children: Vec<ChildInfo>,
    /// Navigable links in the detail content.
    nav_links: Vec<NavLink>,
    /// Currently selected link index (if any).
    selected_link: Option<usize>,
}

impl DocumentDetailState {
    /// Create a new detail view for a document.
    pub fn new(doc: Document, namespace_name: String, cache: &TaskCache, config: &Config) -> Self {
        let _ = config; // reserved for future use

        // Namespace breadcrumb.
        let ns_path = cache
            .get_namespace_path(&doc.namespace_id)
            .unwrap_or_default();
        let breadcrumb = if ns_path.len() > 1 {
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
            namespace_name.clone()
        };

        // Prefix length across all documents.
        let all_ids = cache.all_document_ids().unwrap_or_default();
        let prefix_len = compute_min_prefix_len(&all_ids);

        // Label colours.
        let label_color_map: HashMap<String, LabelColorIndex> =
            display::assign_label_colors(std::iter::once(doc.labels.as_slice()))
                .into_iter()
                .map(|(label, idx)| (label.to_string(), idx))
                .collect();

        // References — resolve titles for display.
        let forward_refs: Vec<ResolvedRef> = cache
            .get_forward_refs(&doc.id, "document")
            .unwrap_or_default()
            .into_iter()
            .map(|row| {
                let (title, slug) = resolve_entity_info(cache, &row.target_id, &row.target_type);
                ResolvedRef { row, title, slug }
            })
            .collect();
        let back_refs: Vec<ResolvedRef> = cache
            .get_back_refs(&doc.id, "document")
            .unwrap_or_default()
            .into_iter()
            .map(|row| {
                let (title, slug) = resolve_entity_info(cache, &row.source_id, &row.source_type);
                ResolvedRef { row, title, slug }
            })
            .collect();

        // Resolve parent document info.
        let parent_doc_info = doc.parent_id.as_ref().and_then(|pid| {
            cache
                .get_document(pid)
                .ok()
                .flatten()
                .map(|d| ParentDocInfo {
                    id: d.id,
                    title: d.title,
                })
        });

        // Resolve child documents.
        let children: Vec<ChildInfo> = cache
            .get_document_children(&doc.id)
            .unwrap_or_default()
            .into_iter()
            .map(|d| ChildInfo {
                id: d.id,
                title: d.title,
            })
            .collect();

        Self {
            doc,
            namespace_name,
            breadcrumb,
            scroll: 0,
            content_height: 0,
            prefix_len,
            label_color_map,
            forward_refs,
            back_refs,
            parent_doc_info,
            children,
            nav_links: Vec::new(),
            selected_link: None,
        }
    }

    /// Get the document ID.
    pub fn doc_id(&self) -> &str {
        &self.doc.id
    }

    /// Scroll up by one line.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down by one line.
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Scroll up by a page.
    pub fn scroll_page_up(&mut self, page_size: u16) {
        self.scroll = self.scroll.saturating_sub(page_size);
    }

    /// Scroll down by a page.
    pub fn scroll_page_down(&mut self, page_size: u16) {
        self.scroll = self.scroll.saturating_add(page_size);
    }

    /// Select the next navigable link (wraps around).
    pub fn select_next_link(&mut self) {
        if self.nav_links.is_empty() {
            return;
        }
        self.selected_link = Some(match self.selected_link {
            Some(i) => (i + 1) % self.nav_links.len(),
            None => 0,
        });
    }

    /// Select the previous navigable link (wraps around).
    pub fn select_prev_link(&mut self) {
        if self.nav_links.is_empty() {
            return;
        }
        self.selected_link = Some(match self.selected_link {
            Some(0) => self.nav_links.len() - 1,
            Some(i) => i - 1,
            None => self.nav_links.len() - 1,
        });
    }

    /// Deselect the current link. Returns true if a link was deselected.
    pub fn deselect_link(&mut self) -> bool {
        self.selected_link.take().is_some()
    }

    /// Get the navigation target of the currently selected link.
    pub fn selected_nav_target(&self) -> Option<&NavTarget> {
        self.selected_link
            .and_then(|i| self.nav_links.get(i))
            .map(|link| &link.target)
    }

    /// Whether this detail view has any navigable links.
    pub fn has_nav_links(&self) -> bool {
        !self.nav_links.is_empty()
    }

    /// Reload the document from storage, preserving scroll position.
    pub fn refresh(
        &mut self,
        storage: &crate::storage::TaskStorage,
        cache: &TaskCache,
        config: &Config,
    ) {
        let doc_id = self.doc.id.clone();
        if let Ok(doc) = storage.load_document(&doc_id) {
            let ns_name = self.namespace_name.clone();
            let scroll = self.scroll;
            *self = Self::new(doc, ns_name, cache, config);
            self.scroll = scroll;
        }
    }

    /// Render the detail view into the given area.
    pub fn render(&mut self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let (prefix, suffix) = display::split_id(&self.doc.id, self.prefix_len);
        let block = Block::bordered()
            .title(format!(" doc {prefix}\u{2502}{suffix} "))
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines = self.build_content(theme);
        self.content_height = lines.len() as u16;

        // Highlight the currently selected nav link line.
        if let Some(sel) = self.selected_link
            && let Some(link) = self.nav_links.get(sel)
        {
            let idx = link.line_index;
            if idx < lines.len() {
                let original = std::mem::take(&mut lines[idx]);
                let mut spans = vec![Span::styled("> ", theme.accent)];
                spans.extend(
                    original.spans.into_iter().map(|s| {
                        Span::styled(s.content.to_string(), theme.selected.patch(s.style))
                    }),
                );
                lines[idx] = Line::from(spans);
            }

            // Auto-scroll to keep selected link visible.
            let link_line = idx as u16;
            if link_line < self.scroll {
                self.scroll = link_line;
            } else if link_line >= self.scroll + inner.height {
                self.scroll = link_line.saturating_sub(inner.height - 1);
            }
        }

        let text = Text::from(lines);

        Paragraph::new(text)
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false })
            .render(inner, buf);

        // Vertical scrollbar when content overflows.
        if self.content_height > inner.height {
            let scrollbar_area = area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            });
            let mut scrollbar_state = ScrollbarState::new(self.content_height as usize)
                .position(self.scroll as usize)
                .viewport_content_length(inner.height as usize);
            StatefulWidget::render(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None),
                scrollbar_area,
                buf,
                &mut scrollbar_state,
            );
        }
    }

    /// Build the full content as styled lines, recording navigable links.
    fn build_content(&mut self, theme: &Theme) -> Vec<Line<'static>> {
        self.nav_links.clear();
        let mut lines: Vec<Line<'static>> = Vec::new();

        // ── Title Block ──
        self.build_title_block(theme, &mut lines);

        // ── Metadata Fields ──
        self.build_metadata_fields(theme, &mut lines);

        // ── Content (markdown) ──
        self.build_content_section(theme, &mut lines);

        // ── Children ──
        self.build_children_section(theme, &mut lines);

        // ── References ──
        self.build_references_section(theme, &mut lines);

        // ── Back-links ──
        self.build_backlinks_section(theme, &mut lines);

        lines.push(Line::default());
        lines
    }

    /// Render the title block with double-line separator.
    fn build_title_block(&self, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());

        // Word-wrap the title.
        let wrapped = display::wrap_text(&self.doc.title, 60);
        for chunk in &wrapped {
            lines.push(Line::from(Span::styled(
                format!("  {chunk}"),
                theme.emphasis.add_modifier(Modifier::BOLD),
            )));
        }

        // Double-line separator.
        let sep_len = wrapped[0].len().min(60) + 4;
        lines.push(Line::from(format!("  {}", "\u{2550}".repeat(sep_len))));
        lines.push(Line::default());
    }

    /// Render all metadata fields.
    fn build_metadata_fields(&mut self, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        let d = &self.doc;

        // ID: cyan prefix | gray suffix
        let (prefix, suffix) = display::split_id(&d.id, self.prefix_len);
        styled_field(
            "  ID",
            vec![
                Span::styled(format!("{prefix}\u{2502}"), theme.accent),
                Span::styled(suffix.to_string(), Style::default().fg(Color::Gray)),
            ],
            theme,
            lines,
        );

        // Slug
        if !d.slug.is_empty() {
            styled_field(
                "  Slug",
                vec![Span::styled(d.slug.clone(), theme.accent)],
                theme,
                lines,
            );
        }

        // Aliases
        if !d.slug_aliases.is_empty() {
            styled_field(
                "  Aliases",
                vec![Span::styled(d.slug_aliases.join(", "), theme.muted)],
                theme,
                lines,
            );
        }

        // Namespace: bold name + plain ID
        let ns_id_short = &d.namespace_id[..8.min(d.namespace_id.len())];
        styled_field(
            "  Namespace",
            vec![
                Span::styled(
                    self.breadcrumb.clone(),
                    theme.accent.add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {ns_id_short}")),
            ],
            theme,
            lines,
        );

        // Created
        if let Some(dt) = format_local_datetime(&d.created) {
            styled_field("  Created", vec![Span::raw(dt)], theme, lines);
        }

        // Modified
        if let Some(dt) = format_local_datetime(&d.modified) {
            styled_field("  Modified", vec![Span::raw(dt)], theme, lines);
        }

        // Deleted (only if set)
        if let Some(ref del_str) = d.deleted
            && let Some(dt) = format_local_datetime(del_str)
        {
            styled_field(
                "  Deleted",
                vec![Span::styled(dt, theme.danger)],
                theme,
                lines,
            );
        }

        // Parent (only if set)
        if let Some(ref parent) = self.parent_doc_info {
            let (pp, ps) = display::split_id(&parent.id, self.prefix_len);
            self.nav_links.push(NavLink {
                target: NavTarget::Document {
                    id: parent.id.clone(),
                },
                line_index: lines.len(),
            });
            styled_field(
                "  Parent",
                vec![
                    Span::styled(format!("{pp}\u{2502}"), theme.accent),
                    Span::styled(ps.to_string(), Style::default().fg(Color::Gray)),
                    Span::raw("  "),
                    Span::raw(parent.title.clone()),
                ],
                theme,
                lines,
            );
        }

        // Version
        styled_field(
            "  Version",
            vec![Span::raw(d.version.to_string())],
            theme,
            lines,
        );

        // Labels
        if !d.labels.is_empty() {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (i, label) in d.labels.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(", "));
                }
                let idx = self
                    .label_color_map
                    .get(label.as_str())
                    .copied()
                    .unwrap_or(0);
                let color = LABEL_PALETTE[idx];
                spans.push(Span::styled(label.clone(), Style::default().fg(color)));
            }
            styled_field("  Labels", spans, theme, lines);
        }
    }

    /// Render the content section using tui-markdown.
    fn build_content_section(&self, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());
        lines.push(section_header("Content", theme));

        if self.doc.content.is_empty() {
            lines.push(Line::from(Span::styled("  (No content)", theme.muted)));
        } else {
            let md_text = tui_markdown::from_str(&self.doc.content);
            for line in md_text.lines {
                let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
                spans.extend(
                    line.spans
                        .into_iter()
                        .map(|s| Span::styled(s.content.to_string(), s.style)),
                );
                lines.push(Line::from(spans));
            }
        }
    }

    /// Render child documents section.
    fn build_children_section(&mut self, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        if self.children.is_empty() {
            return;
        }

        lines.push(Line::default());
        lines.push(section_header(
            &format!("Children ({})", self.children.len()),
            theme,
        ));

        for child in &self.children {
            let (cp, cs) = display::split_id(&child.id, self.prefix_len);
            self.nav_links.push(NavLink {
                target: NavTarget::Document {
                    id: child.id.clone(),
                },
                line_index: lines.len(),
            });
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{cp}\u{2502}"), theme.accent),
                Span::styled(cs.to_string(), Style::default().fg(Color::Gray)),
                Span::raw("  "),
                Span::raw(child.title.clone()),
            ]));
        }
    }

    /// Render forward references section.
    fn build_references_section(&mut self, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        if self.forward_refs.is_empty() {
            return;
        }

        lines.push(Line::default());
        lines.push(section_header("References", theme));

        for rr in &self.forward_refs {
            let r = &rr.row;
            let (tp, ts) = display::split_id(&r.target_id, self.prefix_len);
            let target = match r.target_type.as_str() {
                "document" => NavTarget::Document {
                    id: r.target_id.clone(),
                },
                _ => NavTarget::Task {
                    id: r.target_id.clone(),
                },
            };
            self.nav_links.push(NavLink {
                target,
                line_index: lines.len(),
            });
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(r.ref_type.clone(), ref_type_style(&r.ref_type)),
                Span::raw(" "),
                Span::styled(format!("{tp}\u{2502}"), theme.accent),
                Span::styled(ts.to_string(), Style::default().fg(Color::Gray)),
                Span::styled(
                    format!(" ({})", r.target_type),
                    entity_type_style(&r.target_type),
                ),
            ];
            if let Some(ref slug) = rr.slug
                && !slug.is_empty()
            {
                spans.push(Span::styled(format!("  @{slug}"), theme.accent));
            }
            if let Some(ref title) = rr.title {
                spans.push(Span::raw(format!("  {title}")));
            }
            lines.push(Line::from(spans));
        }
    }

    /// Render back-links section.
    fn build_backlinks_section(&mut self, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        if self.back_refs.is_empty() {
            return;
        }

        lines.push(Line::default());
        lines.push(section_header("Back-links", theme));

        for rr in &self.back_refs {
            let r = &rr.row;
            let (sp, ss) = display::split_id(&r.source_id, self.prefix_len);
            let target = match r.source_type.as_str() {
                "document" => NavTarget::Document {
                    id: r.source_id.clone(),
                },
                _ => NavTarget::Task {
                    id: r.source_id.clone(),
                },
            };
            self.nav_links.push(NavLink {
                target,
                line_index: lines.len(),
            });
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(r.ref_type.clone(), ref_type_style(&r.ref_type)),
                Span::raw(" "),
                Span::styled(format!("{sp}\u{2502}"), theme.accent),
                Span::styled(ss.to_string(), Style::default().fg(Color::Gray)),
                Span::styled(
                    format!(" ({})", r.source_type),
                    entity_type_style(&r.source_type),
                ),
            ];
            if let Some(ref slug) = rr.slug
                && !slug.is_empty()
            {
                spans.push(Span::styled(format!("  @{slug}"), theme.accent));
            }
            if let Some(ref title) = rr.title {
                spans.push(Span::raw(format!("  {title}")));
            }
            lines.push(Line::from(spans));
        }
    }
}

/// Build a styled field line: "  Label:  value".
fn styled_field(
    label: &str,
    value: Vec<Span<'static>>,
    theme: &Theme,
    lines: &mut Vec<Line<'static>>,
) {
    let padded = format!(
        "{label}: {:>width$}",
        "",
        width = FIELD_LABEL_WIDTH.saturating_sub(label.len() + 2)
    );
    let mut spans = vec![Span::styled(padded, theme.emphasis)];
    spans.extend(value);
    lines.push(Line::from(spans));
}

/// Build a section header line.
fn section_header(title: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("  \u{2500}\u{2500}\u{2500} {title} \u{2500}\u{2500}\u{2500}"),
        theme.emphasis,
    ))
}

/// Format an RFC 3339 timestamp as a local datetime string.
fn format_local_datetime(rfc3339: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(rfc3339).ok().map(|d| {
        d.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    })
}

/// Resolve title and slug for a referenced entity.
fn resolve_entity_info(
    cache: &TaskCache,
    id: &str,
    entity_type: &str,
) -> (Option<String>, Option<String>) {
    match entity_type {
        "document" => {
            let doc = cache.get_document(id).ok().flatten();
            let title = doc.as_ref().map(|d| d.title.clone());
            let slug = doc
                .as_ref()
                .map(|d| d.slug.clone())
                .filter(|s| !s.is_empty());
            (title, slug)
        }
        "task" => {
            let title = cache.get_task_summary(id).ok().flatten().map(|s| s.title);
            (title, None)
        }
        _ => (None, None),
    }
}

/// Style for a reference type based on its kind.
fn ref_type_style(ref_type: &str) -> Style {
    let color = match ref_type {
        "inline" | "wiki-link" => Color::Magenta,
        "related" => Color::Yellow,
        "parent" | "child" | "parent-child" => Color::Green,
        _ => Color::Cyan,
    };
    Style::default().fg(color)
}

/// Style for an entity type indicator (e.g. "document", "task").
fn entity_type_style(entity_type: &str) -> Style {
    let color = match entity_type {
        "document" => Color::Blue,
        "task" => Color::Green,
        _ => return Style::default(),
    };
    Style::default().fg(color)
}

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

//! Task list view showing a project's tasks in a table.
//!
//! Displays tasks sorted with active (doing/stopped) tasks first, then
//! by priority, deadline urgency, and modification time. Mirrors the
//! CLI's list output using ratatui widgets.

use chrono::{DateTime, Utc};
use chrono_humanize::{Accuracy, HumanTime, Tense};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, Widget};

use super::theme::Theme;
use crate::cache::{TaskCache, TaskSummary};
use crate::icons::{Glyphs, IconTheme};
use crate::output::{compute_min_prefix_len, compute_subtask_counts};

/// State for the task list view.
pub struct TaskListState {
    /// Project ID whose tasks are being shown.
    pub project_id: String,
    /// Display name for the project (shown in title).
    pub project_name: String,
    /// Tasks in display order (sorted).
    tasks: Vec<TaskSummary>,
    /// Active work states keyed by task ID.
    work_states: std::collections::HashMap<String, String>,
    /// Subtask counts keyed by parent task ID.
    subtask_counts: std::collections::HashMap<String, usize>,
    /// Index of the selected task in the visible list.
    pub selected: usize,
    /// Minimum prefix length for unique ID display.
    prefix_len: usize,
    /// Active filter text (None = no filter active).
    filter: Option<String>,
    /// Indices into `tasks` that match the current filter.
    filtered_indices: Vec<usize>,
    /// Icon theme for style decisions (e.g. Nerd-specific coloring).
    icon_theme: IconTheme,
    /// Raw glyphs for rendering (no ANSI codes).
    glyphs: Glyphs,
}

impl TaskListState {
    /// Load tasks for a project from the cache.
    pub fn from_cache(
        cache: &TaskCache,
        project_id: &str,
        project_name: &str,
        icon_theme: IconTheme,
    ) -> crate::Result<Self> {
        let mut tasks: Vec<TaskSummary> = cache
            .list_tasks(project_id)?
            .into_iter()
            .filter(|t| t.done.is_none() && t.deleted.is_none())
            .collect();

        // Build work state lookup from active tasks.
        let active = cache.get_active_work_tasks().unwrap_or_default();
        let work_states: std::collections::HashMap<String, String> =
            active.into_iter().map(|a| (a.id, a.work_state)).collect();

        // Compute prefix length for unique IDs.
        let ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
        let prefix_len = compute_min_prefix_len(&ids);

        // Build subtask counts from parent_id references.
        let subtask_counts = compute_subtask_counts(tasks.iter().map(|t| t.parent_id.as_deref()));

        // Sort: doing first, stopped second, then by priority (now > later),
        // deadline (nearest first), modified (newest first).
        tasks.sort_by(|a, b| {
            let ws_a = work_states.get(&a.id).map(String::as_str);
            let ws_b = work_states.get(&b.id).map(String::as_str);

            let rank = |ws: Option<&str>| match ws {
                Some("doing") => 0,
                Some("stopped") => 1,
                _ => 2,
            };

            rank(ws_a)
                .cmp(&rank(ws_b))
                .then_with(|| priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))
                .then_with(|| cmp_deadline(a.deadline.as_deref(), b.deadline.as_deref()))
                .then_with(|| b.modified.cmp(&a.modified))
        });

        let filtered_indices: Vec<usize> = (0..tasks.len()).collect();
        let glyphs = Glyphs::new(icon_theme);
        Ok(Self {
            project_id: project_id.to_string(),
            project_name: project_name.to_string(),
            tasks,
            work_states,
            subtask_counts,
            selected: 0,
            prefix_len,
            filter: None,
            filtered_indices,
            icon_theme,
            glyphs,
        })
    }

    /// Whether the visible (filtered) task list is empty.
    pub fn is_empty(&self) -> bool {
        self.filtered_indices.is_empty()
    }

    /// Number of visible (filtered) tasks.
    pub fn len(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected + 1 < self.filtered_indices.len() {
            self.selected += 1;
        }
    }

    /// Get the ID of the currently selected task.
    pub fn selected_task_id(&self) -> Option<&str> {
        self.filtered_indices
            .get(self.selected)
            .and_then(|&idx| self.tasks.get(idx))
            .map(|t| t.id.as_str())
    }

    /// Whether the filter input is currently active.
    pub fn is_filtering(&self) -> bool {
        self.filter.is_some()
    }

    /// Activate the search filter.
    pub fn start_filter(&mut self) {
        self.filter = Some(String::new());
    }

    /// Cancel the search filter and show all tasks.
    pub fn cancel_filter(&mut self) {
        self.filter = None;
        self.filtered_indices = (0..self.tasks.len()).collect();
        self.selected = 0;
    }

    /// Get the current filter text.
    pub fn filter_text(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    /// Add a character to the filter and recompute matches.
    pub fn filter_push(&mut self, c: char) {
        if let Some(ref mut f) = self.filter {
            f.push(c);
        }
        self.recompute_filter();
    }

    /// Remove the last character from the filter.
    pub fn filter_pop(&mut self) {
        if let Some(ref mut f) = self.filter {
            f.pop();
        }
        self.recompute_filter();
    }

    /// Recompute filtered indices based on current filter text.
    fn recompute_filter(&mut self) {
        let query = self.filter.as_deref().unwrap_or("").to_lowercase();

        if query.is_empty() {
            self.filtered_indices = (0..self.tasks.len()).collect();
        } else {
            self.filtered_indices = self
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.title.to_lowercase().contains(&query))
                .map(|(i, _)| i)
                .collect();
        }

        // Clamp selection to the new visible range.
        if self.filtered_indices.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len() - 1;
        }
    }

    /// Render the task list into the given area.
    pub fn render(&self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let title = if let Some(ref q) = self.filter {
            format!(" {} \u{2502} /{q}\u{2588} ", self.project_name)
        } else {
            format!(" {} ", self.project_name)
        };

        let block = Block::bordered().title(title).border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        if self.filtered_indices.is_empty() {
            let msg = if self.filter.is_some() {
                "  No matching tasks"
            } else {
                "  No open tasks"
            };
            Paragraph::new(Line::from(Span::styled(msg, theme.muted))).render(inner, buf);
            return;
        }

        // Build header.
        let header = Row::new(vec!["  ID", "Title", "Pri", "Size", "Deadline", "Status"])
            .style(theme.emphasis)
            .bottom_margin(0);

        // Build rows from filtered indices.
        let rows: Vec<Row<'_>> = self
            .filtered_indices
            .iter()
            .enumerate()
            .map(|(vis_idx, &task_idx)| {
                let task = &self.tasks[task_idx];
                let is_selected = vis_idx == self.selected && focused;
                self.render_row(task, theme, is_selected)
            })
            .collect();

        // Column widths: ID(13) Title(fill) Pri(9) Size(6) Deadline(13) Status(8)
        let widths = [
            ratatui::layout::Constraint::Length(13),
            ratatui::layout::Constraint::Fill(1),
            ratatui::layout::Constraint::Length(9),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(13),
            ratatui::layout::Constraint::Length(8),
        ];

        let table = Table::new(rows, widths).header(header);
        Widget::render(table, inner, buf);

        // Footer: count of visible / total tasks.
        let count_text = if self.filter.is_some() {
            format!(
                " {}/{} tasks ",
                self.filtered_indices.len(),
                self.tasks.len()
            )
        } else {
            format!(" {} tasks ", self.tasks.len())
        };
        let footer_area = Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        );
        Line::from(Span::styled(count_text, theme.muted)).render(footer_area, buf);
    }

    /// Render a single task row (mirrors the CLI's `build_task_row` layout).
    fn render_row<'a>(&self, task: &TaskSummary, theme: &Theme, selected: bool) -> Row<'a> {
        let base = if selected {
            theme.selected
        } else {
            Style::default()
        };

        let row_style = match task.priority.as_str() {
            "now" => base.patch(Style::default().add_modifier(Modifier::BOLD)),
            _ => base,
        };

        // ── ID cell: cyan prefix │ dim suffix ──
        let id_short = &task.id[..8];
        let prefix = &id_short[..self.prefix_len];
        let suffix = &id_short[self.prefix_len..];
        let id_cell = Cell::from(Line::from(vec![
            Span::styled(format!("  {prefix}\u{2502}"), base.patch(theme.accent)),
            Span::styled(suffix.to_string(), base.patch(theme.muted)),
        ]));

        // ── Title cell: multi-line with subtitles ──
        let title_cell = self.build_title_cell(task, theme, base, row_style);

        // ── Priority cell: impact_glyph + priority_text ──
        let pri_cell = self.build_priority_cell(task, theme, base);

        // ── Size (centered) ──
        let size_cell =
            Cell::from(Line::styled(task.size.clone(), base).alignment(Alignment::Center));

        // ── Deadline ──
        let deadline = format_deadline_plain(task.deadline.as_deref());
        let deadline_style = if is_overdue(task.deadline.as_deref()) {
            base.patch(theme.danger)
        } else {
            base
        };
        let deadline_cell = Cell::from(Line::styled(deadline, deadline_style));

        // ── Status ──
        let ws = self
            .work_states
            .get(&task.id)
            .map(String::as_str)
            .unwrap_or("");
        let status_style = match ws {
            "doing" => base.patch(theme.success).add_modifier(Modifier::BOLD),
            "stopped" => base.patch(theme.warning),
            _ => base,
        };
        let status_cell =
            Cell::from(Line::styled(ws.to_string(), status_style).alignment(Alignment::Center));

        // Compute row height: 1 (title) + optional subtitle lines.
        let has_hierarchy =
            task.parent_id.is_some() || self.subtask_counts.get(&task.id).copied().unwrap_or(0) > 0;
        let has_labels = !task.labels.is_empty();
        let height = 1 + has_hierarchy as u16 + has_labels as u16;

        Row::new(vec![
            id_cell,
            title_cell,
            pri_cell,
            size_cell,
            deadline_cell,
            status_cell,
        ])
        .height(height)
        .style(base)
    }

    /// Build the multi-line title cell matching CLI layout:
    /// Line 1: [joy_icon] [bookmark] title
    /// Line 2: (optional) hierarchy subtitle
    /// Line 3: (optional) labels subtitle
    fn build_title_cell<'a>(
        &self,
        task: &TaskSummary,
        theme: &Theme,
        base: Style,
        row_style: Style,
    ) -> Cell<'a> {
        let mut lines: Vec<Line<'_>> = Vec::new();

        // Line 1: [overdue] [joy] [bookmark] title
        let mut title_spans: Vec<Span<'_>> = Vec::new();

        // Overdue/looming glyph
        if is_overdue(task.deadline.as_deref()) {
            title_spans.push(Span::styled(
                format!("{} ", self.glyphs.overdue),
                base.patch(theme.danger),
            ));
        }

        // Joy icon
        let joy_glyph = self.glyphs.joy_icon(task.joy);
        if !joy_glyph.is_empty() {
            let joy_style = match (task.joy, self.icon_theme) {
                (8..=10, IconTheme::Nerd) => base.fg(Color::Yellow),
                (0..=4, IconTheme::Nerd) => base.fg(Color::Blue),
                _ => base,
            };
            title_spans.push(Span::styled(format!("{joy_glyph} "), joy_style));
        }

        // Bookmark
        if task.is_bookmark {
            title_spans.push(Span::styled(
                format!("{} ", self.glyphs.bookmark),
                base.patch(theme.accent),
            ));
        }

        // Title text
        title_spans.push(Span::styled(task.title.clone(), row_style));
        lines.push(Line::from(title_spans));

        // Line 2: hierarchy subtitle (parent + subtask count)
        let has_parent = task.parent_id.is_some();
        let child_count = self.subtask_counts.get(&task.id).copied().unwrap_or(0);
        if has_parent || child_count > 0 {
            let subtitle = self.build_hierarchy_subtitle(task, base, theme);
            lines.push(subtitle);
        }

        // Line 3: labels
        if !task.labels.is_empty() {
            let label_line = self.build_label_subtitle(task, base, theme);
            lines.push(label_line);
        }

        Cell::from(Text::from(lines))
    }

    /// Build hierarchy subtitle: "  ↳ parent_id · ▶ N subtasks"
    fn build_hierarchy_subtitle<'a>(
        &self,
        task: &TaskSummary,
        base: Style,
        theme: &Theme,
    ) -> Line<'a> {
        let mut spans: Vec<Span<'_>> = vec![Span::styled("  ", base)];

        if let Some(ref parent_id) = task.parent_id {
            let pid_short = &parent_id[..8.min(parent_id.len())];
            let pid_prefix = &pid_short[..self.prefix_len.min(pid_short.len())];
            let pid_suffix = &pid_short[self.prefix_len.min(pid_short.len())..];
            spans.push(Span::styled(
                format!("{} ", self.glyphs.hierarchy_parent),
                base.fg(Color::Blue),
            ));
            spans.push(Span::styled(
                format!("{pid_prefix}\u{2502}"),
                base.patch(theme.accent),
            ));
            spans.push(Span::styled(
                pid_suffix.to_string(),
                base.patch(theme.muted),
            ));
        }

        let child_count = self.subtask_counts.get(&task.id).copied().unwrap_or(0);
        if child_count > 0 {
            if task.parent_id.is_some() {
                spans.push(Span::styled(
                    format!(" {} ", self.glyphs.hierarchy_separator),
                    base.patch(theme.muted),
                ));
            }
            let noun = if child_count == 1 {
                "subtask"
            } else {
                "subtasks"
            };
            spans.push(Span::styled(
                format!("{} ", self.glyphs.hierarchy_subtasks),
                base.patch(theme.success),
            ));
            spans.push(Span::styled(
                format!("{child_count} {noun}"),
                base.patch(theme.success),
            ));
        }

        Line::from(spans)
    }

    /// Build labels subtitle: "  🏷 label1, label2, label3"
    fn build_label_subtitle<'a>(&self, task: &TaskSummary, base: Style, theme: &Theme) -> Line<'a> {
        // Cycle through label colours matching the CLI palette.
        let label_colors = [
            Color::Cyan,
            Color::Yellow,
            Color::Green,
            Color::Magenta,
            Color::Blue,
            Color::Red,
        ];

        let mut spans: Vec<Span<'_>> = vec![
            Span::styled("  ", base),
            Span::styled(format!("{} ", self.glyphs.label), base.patch(theme.danger)),
        ];

        for (i, label) in task.labels.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(", ", base.patch(theme.muted)));
            }
            let color = label_colors[i % label_colors.len()];
            spans.push(Span::styled(label.clone(), base.fg(color)));
        }

        Line::from(spans)
    }

    /// Build the priority cell: impact_glyph + priority text.
    fn build_priority_cell<'a>(&self, task: &TaskSummary, theme: &Theme, base: Style) -> Cell<'a> {
        let (impact_glyph, impact_style) = match task.impact {
            1 => (self.glyphs.impact_critical, base.patch(theme.danger)),
            2 => (self.glyphs.impact_significant, base.fg(Color::Blue)),
            _ => ("", base),
        };

        let pri_style = match task.priority.as_str() {
            "now" => base.patch(theme.danger).add_modifier(Modifier::BOLD),
            _ => base,
        };

        let mut spans = Vec::new();
        if !impact_glyph.is_empty() {
            spans.push(Span::styled(impact_glyph.to_string(), impact_style));
        }
        let pad = if impact_glyph.is_empty() {
            self.glyphs.impact_pad
        } else {
            " "
        };
        spans.push(Span::raw(pad.to_string()));
        spans.push(Span::styled(task.priority.clone(), pri_style));

        Cell::from(Line::from(spans))
    }
}

/// Lower value = higher priority.
fn priority_rank(p: &str) -> u8 {
    match p {
        "now" => 0,
        "later" => 1,
        _ => 2,
    }
}

/// Compare deadlines: tasks with deadlines sort before those without.
/// Nearer deadlines sort first.
fn cmp_deadline(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Format a deadline for the TUI (plain text, no ANSI colors).
///
/// Uses chrono-humanize for relative formatting (same as CLI),
/// falls back to absolute date for deadlines >30 days out.
fn format_deadline_plain(deadline_str: Option<&str>) -> String {
    let Some(s) = deadline_str else {
        return String::new();
    };
    let Ok(deadline) = DateTime::parse_from_rfc3339(s) else {
        return String::new();
    };
    let now = Utc::now();
    let is_overdue = deadline < now;

    let duration = if is_overdue {
        now.signed_duration_since(deadline)
    } else {
        deadline.signed_duration_since(now)
    };

    if duration.num_days() > 30 {
        return deadline
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
    }

    let ht = HumanTime::from(deadline);
    let tense = if is_overdue {
        Tense::Past
    } else {
        Tense::Future
    };
    ht.to_text_en(Accuracy::Rough, tense)
}

/// Check if a deadline is past due.
fn is_overdue(deadline_str: Option<&str>) -> bool {
    let Some(s) = deadline_str else {
        return false;
    };
    let Ok(deadline) = DateTime::parse_from_rfc3339(s) else {
        return false;
    };
    deadline < Utc::now()
}

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

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, Widget};

use super::theme::Theme;
use crate::cache::{TaskCache, TaskSummary};
use crate::config::Config;
use crate::display::{self, DeadlineUrgency, LabelColorIndex};
use crate::icons::{Glyphs, IconTheme};
use crate::output::{compute_min_prefix_len, compute_subtask_counts};
use crate::threshold_cache::CachedThresholds;

/// Maximum title width before word-wrapping (matches CLI's 60-char wrap).
const TITLE_WRAP_WIDTH: usize = 60;

/// 8-colour palette for distinguishing projects (matches CLI order).
const PROJECT_PALETTE: [Color; 8] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::LightCyan,
    Color::LightGreen,
    Color::LightYellow,
];

/// State for the task list view.
pub struct TaskListState {
    /// Project ID whose tasks are being shown.
    pub project_id: String,
    /// Display name for the project (shown in title).
    pub project_name: String,
    /// Ancestor breadcrumb trail (e.g. "workspace > clyso > cbs").
    breadcrumb: String,
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
    /// Promotion thresholds for deadline urgency computation.
    thresholds: CachedThresholds,
    /// Stable label → colour index mapping (first-appearance order).
    label_color_map: std::collections::HashMap<String, LabelColorIndex>,
    /// Whether recursive listing is active (shows descendant projects).
    recursive: bool,
    /// Maps project_id → ancestor path names (populated in recursive mode).
    project_paths: std::collections::HashMap<String, Vec<String>>,
    /// Maps project_id → palette colour (populated in recursive mode).
    project_colors: std::collections::HashMap<String, Color>,
}

impl TaskListState {
    /// Load tasks for a project from the cache.
    pub fn from_cache(
        cache: &TaskCache,
        project_id: &str,
        project_name: &str,
        config: &Config,
    ) -> crate::Result<Self> {
        let icon_theme = config.effective_icon_theme();

        let mut tasks: Vec<TaskSummary> = cache
            .list_tasks(project_id)?
            .into_iter()
            .filter(|t| t.done.is_none() && t.deleted.is_none())
            .collect();

        // Build work state lookup from active tasks.
        let active = cache.get_active_work_tasks().unwrap_or_default();
        let work_states: std::collections::HashMap<String, String> =
            active.into_iter().map(|a| (a.id, a.work_state)).collect();

        // Compute prefix length for unique IDs across all projects.
        let all_ids = cache.all_task_ids().unwrap_or_default();
        let prefix_len = compute_min_prefix_len(&all_ids);

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
                .then_with(|| {
                    display::priority_rank(&a.priority).cmp(&display::priority_rank(&b.priority))
                })
                .then_with(|| display::cmp_deadline(a.deadline.as_deref(), b.deadline.as_deref()))
                .then_with(|| b.modified.cmp(&a.modified))
        });

        // Load promotion thresholds from local cache.
        let thresholds =
            crate::threshold_cache::read_cache(config).unwrap_or_else(|| CachedThresholds {
                deadline: crate::utils::default_thresholds(),
                impact_labels: crate::utils::default_impact_labels(),
                impact_multipliers: crate::utils::default_impact_multipliers(),
            });

        // Build stable label color map (first-appearance order across all tasks).
        let label_color_map: std::collections::HashMap<String, LabelColorIndex> =
            display::assign_label_colors(tasks.iter().map(|t| t.labels.as_slice()))
                .into_iter()
                .map(|(label, idx)| (label.to_string(), idx))
                .collect();

        // Build breadcrumb from project ancestor path.
        let path = cache.get_project_path(project_id).unwrap_or_default();
        let breadcrumb = if path.len() > 1 {
            // Join all ancestors including current project name.
            path.join(" > ")
        } else {
            project_name.to_string()
        };

        let filtered_indices: Vec<usize> = (0..tasks.len()).collect();
        let glyphs = Glyphs::new(icon_theme);
        Ok(Self {
            project_id: project_id.to_string(),
            project_name: project_name.to_string(),
            breadcrumb,
            tasks,
            work_states,
            subtask_counts,
            selected: 0,
            prefix_len,
            filter: None,
            filtered_indices,
            icon_theme,
            glyphs,
            thresholds,
            label_color_map,
            recursive: false,
            project_paths: std::collections::HashMap::new(),
            project_colors: std::collections::HashMap::new(),
        })
    }

    /// Whether recursive listing mode is active.
    pub fn is_recursive(&self) -> bool {
        self.recursive
    }

    /// Toggle recursive listing on/off.
    pub fn toggle_recursive(&mut self, cache: &TaskCache, config: &Config) {
        self.recursive = !self.recursive;

        if self.recursive {
            // Gather tasks from current project + all descendants.
            let descendants = cache
                .get_project_descendants(&self.project_id)
                .unwrap_or_default();

            let mut all_project_ids = vec![self.project_id.clone()];
            all_project_ids.extend(descendants);

            // Build project path and colour lookups.
            self.project_paths.clear();
            self.project_colors.clear();
            for (idx, pid) in all_project_ids.iter().enumerate() {
                if let Ok(path) = cache.get_project_path(pid) {
                    self.project_paths.insert(pid.clone(), path);
                }
                self.project_colors
                    .insert(pid.clone(), PROJECT_PALETTE[idx % PROJECT_PALETTE.len()]);
            }

            // Load tasks from all projects.
            let mut tasks: Vec<TaskSummary> = Vec::new();
            for pid in &all_project_ids {
                if let Ok(list) = cache.list_tasks(pid) {
                    tasks.extend(
                        list.into_iter()
                            .filter(|t| t.done.is_none() && t.deleted.is_none()),
                    );
                }
            }
            self.tasks = tasks;
        } else {
            // Reload just the current project's tasks.
            self.project_paths.clear();
            self.project_colors.clear();
            self.tasks = cache
                .list_tasks(&self.project_id)
                .unwrap_or_default()
                .into_iter()
                .filter(|t| t.done.is_none() && t.deleted.is_none())
                .collect();
        }

        // Re-sort tasks.
        let work_states = &self.work_states;
        self.tasks.sort_by(|a, b| {
            let ws_a = work_states.get(&a.id).map(String::as_str);
            let ws_b = work_states.get(&b.id).map(String::as_str);

            let rank = |ws: Option<&str>| match ws {
                Some("doing") => 0,
                Some("stopped") => 1,
                _ => 2,
            };

            rank(ws_a)
                .cmp(&rank(ws_b))
                .then_with(|| {
                    display::priority_rank(&a.priority).cmp(&display::priority_rank(&b.priority))
                })
                .then_with(|| display::cmp_deadline(a.deadline.as_deref(), b.deadline.as_deref()))
                .then_with(|| b.modified.cmp(&a.modified))
        });

        // Rebuild subtask counts.
        self.subtask_counts =
            compute_subtask_counts(self.tasks.iter().map(|t| t.parent_id.as_deref()));

        // Rebuild label color map.
        self.label_color_map =
            display::assign_label_colors(self.tasks.iter().map(|t| t.labels.as_slice()))
                .into_iter()
                .map(|(label, idx)| (label.to_string(), idx))
                .collect();

        // Update thresholds if needed.
        self.thresholds =
            crate::threshold_cache::read_cache(config).unwrap_or_else(|| CachedThresholds {
                deadline: crate::utils::default_thresholds(),
                impact_labels: crate::utils::default_impact_labels(),
                impact_multipliers: crate::utils::default_impact_multipliers(),
            });

        // Reset filter and selection.
        self.filter = None;
        self.filtered_indices = (0..self.tasks.len()).collect();
        self.selected = 0;
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
            format!(" {} \u{2502} /{q}\u{2588} ", self.breadcrumb)
        } else {
            format!(" {} ", self.breadcrumb)
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

        // Show progress column if any visible task has explicit progress or a work state.
        let show_progress = self.filtered_indices.iter().any(|&idx| {
            let t = &self.tasks[idx];
            t.progress.is_some() || self.work_states.contains_key(&t.id)
        });

        // Build header.
        let mut header_cells: Vec<Cell<'_>> = vec![Cell::from("  ID"), Cell::from("Title")];
        if self.recursive {
            header_cells.push(Cell::from("Project"));
        }
        header_cells.extend([
            Cell::from("Pri"),
            Cell::from("Size"),
            Cell::from("Deadline"),
        ]);
        if show_progress {
            header_cells.push(Cell::from(
                Line::from("Progress").alignment(Alignment::Center),
            ));
        }
        header_cells.push(Cell::from("Status"));
        let header = Row::new(header_cells)
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
                self.render_row(task, theme, is_selected, show_progress)
            })
            .collect();

        // Column widths.
        use ratatui::layout::Constraint;
        let mut widths: Vec<Constraint> = vec![Constraint::Length(13), Constraint::Fill(1)];
        if self.recursive {
            widths.push(Constraint::Length(12));
        }
        widths.extend([
            Constraint::Length(9),
            Constraint::Length(6),
            Constraint::Length(10),
        ]);
        if show_progress {
            widths.push(Constraint::Length(15));
        }
        widths.push(Constraint::Length(8));

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
    fn render_row<'a>(
        &self,
        task: &TaskSummary,
        theme: &Theme,
        selected: bool,
        show_progress: bool,
    ) -> Row<'a> {
        let base = if selected {
            theme.selected
        } else {
            Style::default()
        };

        let row_style = match task.priority.as_str() {
            "now" => base.patch(Style::default().add_modifier(Modifier::BOLD)),
            _ => base,
        };

        // Compute deadline urgency once for this row.
        let urgency = display::deadline_urgency(
            task.deadline.as_deref(),
            &task.size,
            task.impact,
            &self.thresholds,
        );

        // ── ID cell: cyan prefix │ dim suffix ──
        let (prefix, suffix) = display::split_id(&task.id, self.prefix_len);
        let id_cell = Cell::from(Line::from(vec![
            Span::styled(format!("  {prefix}\u{2502}"), base.patch(theme.accent)),
            Span::styled(suffix.to_string(), base.patch(theme.muted)),
        ]));

        // ── Title cell: multi-line with word-wrapped title + subtitles ──
        let (title_cell, title_lines) =
            self.build_title_cell(task, theme, base, row_style, urgency);

        // ── Priority cell: impact_glyph + priority_text ──
        let pri_cell = self.build_priority_cell(task, theme, base);

        // ── Size (centered) ──
        let size_cell =
            Cell::from(Line::styled(task.size.clone(), base).alignment(Alignment::Center));

        // ── Deadline (compact format) ──
        let (deadline_text, deadline_style) =
            match display::format_deadline_compact(task.deadline.as_deref()) {
                Some(d) => {
                    let style = match urgency {
                        DeadlineUrgency::Overdue => base.patch(theme.danger),
                        DeadlineUrgency::Warning => base.patch(theme.warning),
                        DeadlineUrgency::None => base,
                    };
                    (d.text, style)
                }
                None => (String::new(), base),
            };
        let deadline_cell = Cell::from(Line::styled(deadline_text, deadline_style));

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

        let mut cells = vec![id_cell, title_cell];
        let mut max_lines = title_lines;

        // Insert project column when in recursive mode (hierarchical tree).
        if self.recursive {
            let (proj_cell, proj_lines) = self.build_project_cell(&task.project_id, base, theme);
            cells.push(proj_cell);
            max_lines = max_lines.max(proj_lines);
        }

        cells.extend([pri_cell, size_cell, deadline_cell]);

        // Progress column: use explicit progress or infer 0% from work state.
        if show_progress {
            let effective_progress = task
                .progress
                .or_else(|| self.work_states.contains_key(&task.id).then_some(0));
            let progress_cell = match display::format_progress_bar(effective_progress, 8) {
                Some(pb) => {
                    let fill_color = match pb.percentage {
                        0..=49 => Color::Yellow,
                        50..=99 => Color::Cyan,
                        _ => Color::Green,
                    };
                    Cell::from(Line::from(vec![
                        Span::styled("\u{2588}".repeat(pb.filled), base.fg(fill_color)),
                        Span::styled("\u{00b7}".repeat(pb.empty), base.fg(Color::DarkGray)),
                        Span::styled(format!(" {:>3}%", pb.percentage), base),
                    ]))
                }
                None => Cell::from(
                    Line::styled("\u{2014}", base.fg(Color::DarkGray)).alignment(Alignment::Center),
                ),
            };
            cells.push(progress_cell);
        }

        cells.push(status_cell);

        Row::new(cells).height(max_lines as u16).style(base)
    }

    /// Build the multi-line title cell matching CLI layout.
    ///
    /// Returns `(Cell, line_count)` so the caller can set the correct
    /// row height. Title text is word-wrapped at [`TITLE_WRAP_WIDTH`].
    fn build_title_cell<'a>(
        &self,
        task: &TaskSummary,
        theme: &Theme,
        base: Style,
        row_style: Style,
        urgency: DeadlineUrgency,
    ) -> (Cell<'a>, usize) {
        let mut lines: Vec<Line<'_>> = Vec::new();

        // Prefix glyphs (urgency, joy, bookmark) — first wrapped line only.
        let mut prefix_spans: Vec<Span<'_>> = Vec::new();
        match urgency {
            DeadlineUrgency::Overdue => {
                prefix_spans.push(Span::styled(
                    format!("{} ", self.glyphs.overdue),
                    base.patch(theme.danger),
                ));
            }
            DeadlineUrgency::Warning => {
                prefix_spans.push(Span::styled(
                    format!("{} ", self.glyphs.deadline_warning),
                    base.patch(theme.warning),
                ));
            }
            DeadlineUrgency::None => {}
        }

        let joy_glyph = self.glyphs.joy_icon(task.joy);
        if !joy_glyph.is_empty() {
            let joy_style = match (task.joy, self.icon_theme) {
                (8..=10, IconTheme::Nerd) => base.fg(Color::Yellow),
                (0..=4, IconTheme::Nerd) => base.fg(Color::Blue),
                _ => base,
            };
            prefix_spans.push(Span::styled(format!("{joy_glyph} "), joy_style));
        }

        if task.is_bookmark {
            prefix_spans.push(Span::styled(
                format!("{} ", self.glyphs.bookmark),
                base.patch(theme.accent),
            ));
        }

        // Title text — apply warning colorization when looming
        let title_style = match urgency {
            DeadlineUrgency::Warning => base.patch(theme.warning),
            _ => row_style,
        };

        // Word-wrap the title text at TITLE_WRAP_WIDTH.
        let wrapped = display::wrap_text(&task.title, TITLE_WRAP_WIDTH);
        for (i, chunk) in wrapped.iter().enumerate() {
            if i == 0 {
                let mut first_line = prefix_spans.clone();
                first_line.push(Span::styled(chunk.clone(), title_style));
                lines.push(Line::from(first_line));
            } else {
                lines.push(Line::styled(chunk.clone(), title_style));
            }
        }

        // Hierarchy subtitle (parent + subtask count)
        let has_parent = task.parent_id.is_some();
        let child_count = self.subtask_counts.get(&task.id).copied().unwrap_or(0);
        if has_parent || child_count > 0 {
            lines.push(self.build_hierarchy_subtitle(task, base, theme));
        }

        // Labels subtitle
        if !task.labels.is_empty() {
            lines.push(self.build_label_subtitle(task, base, theme));
        }

        let count = lines.len();
        (Cell::from(Text::from(lines)), count)
    }

    /// Build the project cell for recursive mode using hierarchical tree
    /// format matching the CLI's `format_project_cell`.
    ///
    /// Returns `(Cell, line_count)`.
    fn build_project_cell<'a>(
        &self,
        project_id: &str,
        base: Style,
        _theme: &Theme,
    ) -> (Cell<'a>, usize) {
        let color = self
            .project_colors
            .get(project_id)
            .copied()
            .unwrap_or(Color::White);

        let path = self.project_paths.get(project_id);

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
                        let mut spans = vec![
                            Span::styled(indent, base),
                            Span::styled(connector.to_string(), base.fg(Color::DarkGray)),
                        ];
                        spans.push(Span::styled(segment.clone(), base.fg(color)));
                        lines.push(Line::from(spans));
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
                let short = &project_id[..8.min(project_id.len())];
                let line = Line::styled(short.to_string(), base.fg(color));
                (Cell::from(line), 1)
            }
        }
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
        // TUI label palette matching CLI's 12-colour palette order.
        const LABEL_PALETTE: [Color; display::LABEL_PALETTE_LEN] = [
            Color::Cyan,
            Color::Yellow,
            Color::Green,
            Color::Magenta,
            Color::Blue,
            Color::Red,
            Color::LightCyan,
            Color::LightYellow,
            Color::LightGreen,
            Color::LightMagenta,
            Color::LightBlue,
            Color::LightRed,
        ];

        let mut spans: Vec<Span<'_>> = vec![
            Span::styled("  ", base),
            Span::styled(format!("{} ", self.glyphs.label), base.patch(theme.danger)),
        ];

        for (i, label) in task.labels.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(", ", base.patch(theme.muted)));
            }
            let idx = self
                .label_color_map
                .get(label.as_str())
                .copied()
                .unwrap_or(0);
            let color = LABEL_PALETTE[idx];
            spans.push(Span::styled(label.clone(), base.fg(color)));
        }

        Line::from(spans)
    }

    /// Build the priority cell: impact_glyph + priority text.
    fn build_priority_cell<'a>(&self, task: &TaskSummary, theme: &Theme, base: Style) -> Cell<'a> {
        let (impact_glyph, impact_style) = match display::impact_level(task.impact) {
            display::ImpactLevel::Critical => {
                (self.glyphs.impact_critical, base.patch(theme.danger))
            }
            display::ImpactLevel::Significant => {
                (self.glyphs.impact_significant, base.fg(Color::Blue))
            }
            display::ImpactLevel::Normal => ("", base),
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

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

//! Task detail view showing full task information.
//!
//! Displays styled metadata fields, markdown body, subtask list with
//! titles and done markers, and a scrollable change log.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget, Wrap,
};

use super::theme::Theme;
use crate::cache::TaskCache;
use crate::config::Config;
use crate::display::{self, DeadlineUrgency, LABEL_PALETTE_LEN, LabelColorIndex};
use crate::icons::{Glyphs, IconTheme};
use crate::models::{LogEntryType, Task};
use crate::output::compute_min_prefix_len;
use crate::promotion;
use crate::threshold_cache::CachedThresholds;

/// TUI label palette matching CLI's 12-colour palette order.
const LABEL_PALETTE: [Color; LABEL_PALETTE_LEN] = [
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

/// Resolved subtask with its title and done status.
struct SubtaskInfo {
    id: String,
    title: String,
    is_done: bool,
}

/// State for the task detail view.
pub struct TaskDetailState {
    /// The full task being displayed.
    task: Task,
    /// Project name for display context.
    pub project_name: String,
    /// Scroll offset for the content area.
    pub scroll: u16,
    /// Total content height (updated on each render).
    content_height: u16,
    /// Ancestor breadcrumb trail (e.g. "workspace > clyso > cbs").
    breadcrumb: String,
    /// Resolved subtask info (title + done status).
    subtask_info: Vec<SubtaskInfo>,
    /// Minimum prefix length for unique ID display.
    prefix_len: usize,
    /// Raw glyphs for rendering (no ANSI codes).
    glyphs: Glyphs,
    /// Icon theme for style decisions (e.g. Nerd-specific coloring).
    icon_theme: IconTheme,
    /// Promotion thresholds for deadline urgency and priority promotion.
    thresholds: CachedThresholds,
    /// Human-readable impact label (e.g. "Critical", "Normal").
    impact_label: String,
    /// Stable label -> colour index mapping.
    label_color_map: HashMap<String, LabelColorIndex>,
}

impl TaskDetailState {
    /// Create a new detail view for a task, pre-computing display data.
    pub fn new(task: Task, project_name: String, cache: &TaskCache, config: &Config) -> Self {
        let icon_theme = config.effective_icon_theme();
        let glyphs = Glyphs::new(icon_theme);

        // Breadcrumb: ancestor path joined with " > ".
        let path = cache.get_project_path(&task.project_id).unwrap_or_default();
        let breadcrumb = if path.len() > 1 {
            path.join(" > ")
        } else {
            project_name.clone()
        };

        // Resolve subtask titles and done status.
        let subtask_info: Vec<SubtaskInfo> = task
            .subtasks
            .iter()
            .map(|sub_id| {
                let (title, is_done) = cache
                    .get_task_summary(sub_id)
                    .ok()
                    .flatten()
                    .map(|s| (s.title, s.done.is_some()))
                    .unwrap_or_else(|| (String::new(), false));
                SubtaskInfo {
                    id: sub_id.clone(),
                    title,
                    is_done,
                }
            })
            .collect();

        // Prefix length for unique ID rendering.
        let all_ids = cache.all_task_ids().unwrap_or_default();
        let prefix_len = compute_min_prefix_len(&all_ids);

        // Promotion thresholds from local cache.
        let thresholds =
            crate::threshold_cache::read_cache(config).unwrap_or_else(|| CachedThresholds {
                deadline: crate::utils::default_thresholds(),
                impact_labels: crate::utils::default_impact_labels(),
                impact_multipliers: crate::utils::default_impact_multipliers(),
            });

        // Impact label from thresholds.
        let impact_label = thresholds
            .impact_labels
            .get(&task.impact.to_string())
            .cloned()
            .or_else(|| {
                crate::utils::default_impact_labels()
                    .get(&task.impact.to_string())
                    .cloned()
            })
            .unwrap_or_else(|| "Unknown".to_string());

        // Label color map from this task's labels.
        let label_color_map: HashMap<String, LabelColorIndex> =
            display::assign_label_colors(std::iter::once(task.labels.as_slice()))
                .into_iter()
                .map(|(label, idx)| (label.to_string(), idx))
                .collect();

        Self {
            task,
            project_name,
            scroll: 0,
            content_height: 0,
            breadcrumb,
            subtask_info,
            prefix_len,
            glyphs,
            icon_theme,
            thresholds,
            impact_label,
            label_color_map,
        }
    }

    /// Get the task ID.
    pub fn task_id(&self) -> &str {
        &self.task.id
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

    /// Render the detail view into the given area.
    pub fn render(&mut self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let (prefix, suffix) = display::split_id(&self.task.id, self.prefix_len);
        let block = Block::bordered()
            .title(format!(" task {prefix}\u{2502}{suffix} "))
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        let lines = self.build_content(theme);
        self.content_height = lines.len() as u16;
        let text = Text::from(lines);

        Paragraph::new(text)
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false })
            .render(inner, buf);

        // Vertical scrollbar when content overflows viewport.
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

    /// Build the full content as styled lines.
    fn build_content(&self, theme: &Theme) -> Vec<Line<'static>> {
        let t = &self.task;
        let mut lines: Vec<Line<'static>> = Vec::new();

        // ── Title Block ──
        self.build_title_block(t, theme, &mut lines);

        // ── Metadata Fields ──
        self.build_metadata_fields(t, theme, &mut lines);

        // ── Body (markdown) ──
        self.build_body_section(t, theme, &mut lines);

        // ── Subtasks ──
        self.build_subtasks_section(theme, &mut lines);

        // ── Log ──
        self.build_log_section(t, theme, &mut lines);

        lines.push(Line::default());
        lines
    }

    /// Render the title block with urgency glyphs and separator.
    fn build_title_block(&self, t: &Task, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());

        let urgency =
            display::deadline_urgency(t.deadline.as_deref(), &t.size, t.impact, &self.thresholds);

        // Build prefix glyphs (urgency + bookmark).
        let mut prefix_spans: Vec<Span<'static>> = Vec::new();
        match urgency {
            DeadlineUrgency::Overdue => {
                prefix_spans.push(Span::styled(
                    format!("{} ", self.glyphs.overdue),
                    theme.danger,
                ));
            }
            DeadlineUrgency::Warning => {
                prefix_spans.push(Span::styled(
                    format!("{} ", self.glyphs.deadline_warning),
                    theme.warning,
                ));
            }
            DeadlineUrgency::None => {}
        }
        if t.is_bookmark() {
            prefix_spans.push(Span::styled(
                format!("{} ", self.glyphs.bookmark),
                theme.accent,
            ));
        }

        // Word-wrap the title.
        let wrapped = display::wrap_text(&t.title, 60);
        for (i, chunk) in wrapped.iter().enumerate() {
            if i == 0 {
                let mut first_line = vec![Span::raw("  ")];
                first_line.extend(prefix_spans.clone());
                first_line.push(Span::styled(
                    chunk.clone(),
                    theme.emphasis.add_modifier(Modifier::BOLD),
                ));
                lines.push(Line::from(first_line));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("  {chunk}"),
                    theme.emphasis.add_modifier(Modifier::BOLD),
                )));
            }
        }

        // Double-line separator.
        let sep_len = wrapped[0].len().min(60) + 4;
        lines.push(Line::from(format!("  {}", "\u{2550}".repeat(sep_len))));
        lines.push(Line::default());
    }

    /// Render all metadata fields with styled values.
    fn build_metadata_fields(&self, t: &Task, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        // ID: cyan prefix | dim suffix
        let (prefix, suffix) = display::split_id(&t.id, self.prefix_len);
        styled_field(
            "  ID",
            vec![
                Span::styled(format!("{prefix}\u{2502}"), theme.accent),
                Span::styled(suffix.to_string(), theme.muted),
            ],
            theme,
            lines,
        );

        // Project: breadcrumb with ancestors in accent, leaf bold+accent.
        let parts: Vec<&str> = self.breadcrumb.split(" > ").collect();
        if parts.len() > 1 {
            let ancestors = parts[..parts.len() - 1].join(" > ");
            let leaf = parts[parts.len() - 1];
            styled_field(
                "  Project",
                vec![
                    Span::styled(format!("{ancestors} > "), theme.accent),
                    Span::styled(leaf.to_string(), theme.accent.add_modifier(Modifier::BOLD)),
                ],
                theme,
                lines,
            );
        } else {
            styled_field(
                "  Project",
                vec![Span::styled(
                    self.breadcrumb.clone(),
                    theme.accent.add_modifier(Modifier::BOLD),
                )],
                theme,
                lines,
            );
        }

        // Priority: effective priority with promotion indicator.
        let effective = promotion::effective_priority(t, &self.thresholds);
        let is_promoted = effective == "now" && t.priority != "now";
        let pri_style = match effective {
            "now" => theme.danger.add_modifier(Modifier::BOLD),
            _ => Style::default(),
        };
        let mut pri_spans = vec![Span::styled(effective.to_string(), pri_style)];
        if is_promoted {
            pri_spans.push(Span::styled(" (promoted)", theme.muted));
        }
        styled_field("  Priority", pri_spans, theme, lines);

        // Size
        styled_field("  Size", vec![Span::raw(t.size.clone())], theme, lines);

        // Status
        if let Some(ref ws) = t.current_work_state {
            let ws_style = match ws.as_str() {
                "doing" => theme.success.add_modifier(Modifier::BOLD),
                "stopped" => theme.warning,
                _ => Style::default(),
            };
            styled_field(
                "  Status",
                vec![Span::styled(ws.clone(), ws_style)],
                theme,
                lines,
            );
        } else {
            styled_field(
                "  Status",
                vec![Span::styled("pending", theme.muted)],
                theme,
                lines,
            );
        }

        // Deadline: absolute + relative with urgency coloring.
        if let Some(ref deadline_str) = t.deadline {
            self.build_deadline_field(deadline_str, t, theme, lines);
        }

        // Progress: styled bar (20-wide) + percentage.
        if let Some(progress) = t.progress {
            self.build_progress_field(progress, theme, lines);
        }

        // Impact: icon + label + value.
        self.build_impact_field(t, theme, lines);

        // Joy: value + icon.
        self.build_joy_field(t, theme, lines);

        // Labels: each in stable palette colour.
        if !t.labels.is_empty() {
            self.build_labels_field(t, theme, lines);
        }

        // Created
        if let Some(dt) = format_local_datetime(&t.created) {
            styled_field("  Created", vec![Span::raw(dt)], theme, lines);
        }

        // Modified
        if let Some(dt) = format_local_datetime(&t.modified) {
            styled_field("  Modified", vec![Span::raw(dt)], theme, lines);
        }

        // Done (only if set)
        if let Some(ref done_str) = t.done
            && let Some(dt) = format_local_datetime(done_str)
        {
            styled_field(
                "  Done",
                vec![Span::styled(dt, Style::new().fg(Color::Blue))],
                theme,
                lines,
            );
        }

        // Deleted (only if set)
        if let Some(ref del_str) = t.deleted
            && let Some(dt) = format_local_datetime(del_str)
        {
            styled_field(
                "  Deleted",
                vec![Span::styled(dt, theme.danger)],
                theme,
                lines,
            );
        }

        // Version
        styled_field(
            "  Version",
            vec![Span::raw(t.version.to_string())],
            theme,
            lines,
        );

        // References
        if !t.references.is_empty() {
            let mut ref_spans: Vec<Span<'static>> = Vec::new();
            for (i, r) in t.references.iter().enumerate() {
                if i > 0 {
                    ref_spans.push(Span::styled(", ", theme.muted));
                }
                let (rp, rs) = display::split_id(&r.target_id, self.prefix_len);
                ref_spans.push(Span::styled(format!("{rp}\u{2502}"), theme.accent));
                ref_spans.push(Span::styled(rs.to_string(), theme.muted));
                ref_spans.push(Span::styled(format!(" ({})", r.ref_type), theme.muted));
            }
            styled_field("  Refs", ref_spans, theme, lines);
        }
    }

    /// Render the deadline field with absolute date, relative text, and urgency color.
    fn build_deadline_field(
        &self,
        deadline_str: &str,
        t: &Task,
        theme: &Theme,
        lines: &mut Vec<Line<'static>>,
    ) {
        let urgency =
            display::deadline_urgency(Some(deadline_str), &t.size, t.impact, &self.thresholds);
        let urgency_style = match urgency {
            DeadlineUrgency::Overdue => theme.danger,
            DeadlineUrgency::Warning => theme.warning,
            DeadlineUrgency::None => Style::default(),
        };

        let abs = DateTime::parse_from_rfc3339(deadline_str)
            .ok()
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| deadline_str.to_string());

        let mut spans = vec![Span::styled(abs, urgency_style)];

        if let Some(rel) = display::format_deadline_relative(Some(deadline_str)) {
            spans.push(Span::styled(format!(" ({})", rel.text), urgency_style));
        }

        styled_field("  Deadline", spans, theme, lines);
    }

    /// Render the progress bar field (20-wide bar + percentage).
    fn build_progress_field(&self, progress: u8, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        if let Some(pb) = display::format_progress_bar(Some(progress), 20) {
            let fill_color = match pb.percentage {
                0..=49 => Color::Yellow,
                50..=99 => Color::Cyan,
                _ => Color::Green,
            };
            styled_field(
                "  Progress",
                vec![
                    Span::styled(
                        "\u{2588}".repeat(pb.filled),
                        Style::default().fg(fill_color),
                    ),
                    Span::styled(
                        "\u{00b7}".repeat(pb.empty),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(format!(" {:>3}%", pb.percentage), Style::default()),
                ],
                theme,
                lines,
            );
        }
    }

    /// Render the impact field: icon + label + value.
    fn build_impact_field(&self, t: &Task, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        let (impact_glyph, impact_style) = match display::impact_level(t.impact) {
            display::ImpactLevel::Critical => (self.glyphs.impact_critical, theme.danger),
            display::ImpactLevel::Significant => {
                (self.glyphs.impact_significant, Style::new().fg(Color::Blue))
            }
            display::ImpactLevel::Normal => ("", Style::default()),
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        if !impact_glyph.is_empty() {
            spans.push(Span::styled(format!("{impact_glyph} "), impact_style));
        }
        spans.push(Span::raw(format!("{} ({})", self.impact_label, t.impact)));

        styled_field("  Impact", spans, theme, lines);
    }

    /// Render the joy field: value + icon.
    fn build_joy_field(&self, t: &Task, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        let joy_glyph = self.glyphs.joy_icon(t.joy);
        let mut spans = vec![Span::raw(format!("{}/10", t.joy))];
        if !joy_glyph.is_empty() {
            let joy_style = match (t.joy, self.icon_theme) {
                (8..=10, IconTheme::Nerd) => Style::default().fg(Color::Yellow),
                (0..=4, IconTheme::Nerd) => Style::default().fg(Color::Blue),
                _ => Style::default(),
            };
            spans.push(Span::styled(format!(" {joy_glyph}"), joy_style));
        }
        styled_field("  Joy", spans, theme, lines);
    }

    /// Render the labels field: each label in its palette colour.
    fn build_labels_field(&self, t: &Task, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(
            format!("{} ", self.glyphs.label),
            theme.danger,
        ));
        for (i, label) in t.labels.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(", ", theme.muted));
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

    /// Render the body section using tui-markdown.
    fn build_body_section(&self, t: &Task, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());
        lines.push(section_header("Description", theme));

        if t.body.is_empty() {
            lines.push(Line::from(Span::styled("  (No description)", theme.muted)));
        } else {
            let md_text = tui_markdown::from_str(&t.body);
            for line in md_text.lines {
                // Indent each line and convert to owned spans.
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

    /// Render the subtasks section with titles and done markers.
    fn build_subtasks_section(&self, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        if self.subtask_info.is_empty() {
            return;
        }

        lines.push(Line::default());
        lines.push(section_header(
            &format!("Subtasks ({})", self.subtask_info.len()),
            theme,
        ));

        for sub in &self.subtask_info {
            let done_marker = if sub.is_done {
                Span::styled(format!("  {} ", self.glyphs.success), theme.success)
            } else {
                Span::raw("    ")
            };

            let (sp, ss) = display::split_id(&sub.id, self.prefix_len);
            let title_style = if sub.is_done {
                theme.muted
            } else {
                Style::default()
            };

            lines.push(Line::from(vec![
                done_marker,
                Span::styled(format!("{sp}\u{2502}"), theme.accent),
                Span::styled(ss.to_string(), theme.muted),
                Span::raw("  "),
                Span::styled(sub.title.clone(), title_style),
            ]));
        }
    }

    /// Render the log section (last 20 entries with relative timestamps).
    fn build_log_section(&self, t: &Task, theme: &Theme, lines: &mut Vec<Line<'static>>) {
        if t.log.is_empty() {
            return;
        }

        lines.push(Line::default());
        lines.push(section_header("Log", theme));
        let time_style = Style::default().fg(Color::Yellow);
        for entry in t.log.iter().rev().take(20) {
            let time = format_relative_time(&entry.timestamp);
            let desc = format_log_entry(&entry.entry_type);
            let entry_style = log_entry_style(&entry.entry_type, theme);
            lines.push(Line::from(vec![
                Span::styled(format!("  {time:>10}  "), time_style),
                Span::styled(desc, entry_style),
            ]));
        }
    }
}

/// Fixed column width for field labels (longest: "  Priority: " = 12 chars).
const FIELD_LABEL_WIDTH: usize = 12;

/// Build a field line with a right-padded label and styled value spans.
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
    DateTime::parse_from_rfc3339(rfc3339).ok().map(|d| {
        d.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    })
}

/// Format a timestamp as relative time (e.g., "2h ago", "3d ago").
fn format_relative_time(timestamp: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp);

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
}

/// Choose a style for a log entry based on what changed.
fn log_entry_style(entry: &LogEntryType, theme: &Theme) -> Style {
    match entry {
        LogEntryType::PriorityChanged { to, .. } => {
            if to == "now" {
                theme.danger
            } else {
                theme.accent
            }
        }
        LogEntryType::DeadlineChanged { to: None, .. } => theme.warning,
        LogEntryType::DeadlineChanged { .. } => theme.accent,
        LogEntryType::StatusChanged { .. } => theme.success,
        LogEntryType::WorkStateChanged { .. } => theme.success,
        LogEntryType::SizeChanged { .. } => Style::default(),
        LogEntryType::TitleChanged { .. } => Style::default(),
        LogEntryType::BodyChanged => Style::default(),
        LogEntryType::ProgressChanged { .. } => Style::default().fg(Color::Cyan),
        LogEntryType::ImpactChanged { .. } => Style::default().fg(Color::Magenta),
        LogEntryType::JoyChanged { .. } => Style::default().fg(Color::Magenta),
    }
}

/// Format a log entry type as a human-readable description.
fn format_log_entry(entry: &LogEntryType) -> String {
    match entry {
        LogEntryType::PriorityChanged { from, to } => {
            format!("Priority: {from} \u{2192} {to}")
        }
        LogEntryType::DeadlineChanged { to, .. } => match to {
            Some(d) => format!("Deadline set to {}", d.format("%Y-%m-%d")),
            None => "Deadline removed".to_string(),
        },
        LogEntryType::StatusChanged { status } => {
            format!("Status: {status:?}")
        }
        LogEntryType::SizeChanged { from, to } => {
            format!("Size: {from} \u{2192} {to}")
        }
        LogEntryType::WorkStateChanged { state } => {
            format!("Work: {state:?}")
        }
        LogEntryType::TitleChanged { to, .. } => {
            format!("Title: {to}")
        }
        LogEntryType::BodyChanged => "Body updated".to_string(),
        LogEntryType::ProgressChanged { to, .. } => match to {
            Some(p) => format!("Progress: {p}%"),
            None => "Progress cleared".to_string(),
        },
        LogEntryType::ImpactChanged { to, .. } => format!("Impact: {to}"),
        LogEntryType::JoyChanged { to, .. } => format!("Joy: {to}"),
    }
}

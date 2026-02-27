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
//! Displays metadata fields, body content, subtask list, and change
//! log for a single task loaded from CRDT storage.

use chrono::{DateTime, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use super::theme::Theme;
use crate::models::{LogEntryType, Task};

/// State for the task detail view.
pub struct TaskDetailState {
    /// The full task being displayed.
    task: Task,
    /// Project name for display context.
    pub project_name: String,
    /// Scroll offset for the content area.
    pub scroll: u16,
}

impl TaskDetailState {
    /// Create a new detail view for a task.
    pub fn new(task: Task, project_name: String) -> Self {
        Self {
            task,
            project_name,
            scroll: 0,
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
    pub fn render(&self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let id_short = &self.task.id[..8.min(self.task.id.len())];
        let block = Block::bordered()
            .title(format!(" task {id_short} "))
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        let lines = self.build_content(theme);
        let text = Text::from(lines);

        Paragraph::new(text)
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }

    /// Build the full content as styled lines.
    fn build_content(&self, theme: &Theme) -> Vec<Line<'static>> {
        let t = &self.task;
        let mut lines: Vec<Line<'static>> = Vec::new();

        // Title
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("  {}", t.title),
            theme.emphasis.add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from("  \u{2500}".repeat(t.title.len().min(40) + 2)));
        lines.push(Line::default());

        // Metadata fields
        lines.push(field_line("  Project", &self.project_name, theme));
        lines.push(field_line("  Priority", &t.priority, theme));
        lines.push(field_line("  Size", &t.size, theme));

        if let Some(ref ws) = t.current_work_state {
            lines.push(field_line("  Status", ws, theme));
        }

        if let Some(ref deadline) = t.deadline {
            let formatted = format_deadline_detail(deadline);
            lines.push(field_line("  Deadline", &formatted, theme));
        }

        if let Some(progress) = t.progress {
            let bar = progress_bar(progress);
            lines.push(field_line(
                "  Progress",
                &format!("{bar} {progress}%"),
                theme,
            ));
        }

        lines.push(field_line("  Impact", &format!("{}/10", t.impact), theme));
        lines.push(field_line("  Joy", &format!("{}/10", t.joy), theme));

        if !t.labels.is_empty() {
            lines.push(field_line("  Labels", &t.labels.join(", "), theme));
        }

        if !t.references.is_empty() {
            let refs: Vec<String> = t
                .references
                .iter()
                .map(|r| {
                    format!(
                        "{} ({})",
                        &r.target_id[..8.min(r.target_id.len())],
                        r.ref_type
                    )
                })
                .collect();
            lines.push(field_line("  Refs", &refs.join(", "), theme));
        }

        // Body
        if !t.body.is_empty() {
            lines.push(Line::default());
            lines.push(section_header("Description", theme));
            for body_line in t.body.lines() {
                lines.push(Line::from(format!("  {body_line}")));
            }
        }

        // Subtasks
        if !t.subtasks.is_empty() {
            lines.push(Line::default());
            lines.push(section_header(
                &format!("Subtasks ({})", t.subtasks.len()),
                theme,
            ));
            for sub_id in &t.subtasks {
                let short = &sub_id[..8.min(sub_id.len())];
                lines.push(Line::from(vec![Span::styled(
                    format!("  {short}"),
                    theme.accent,
                )]));
            }
        }

        // Log
        if !t.log.is_empty() {
            lines.push(Line::default());
            lines.push(section_header("Log", theme));
            for entry in t.log.iter().rev().take(20) {
                let time = format_relative_time(&entry.timestamp);
                let desc = format_log_entry(&entry.entry_type);
                lines.push(Line::from(vec![
                    Span::styled(format!("  {time:>10}  "), theme.muted),
                    Span::from(desc),
                ]));
            }
        }

        lines.push(Line::default());
        lines
    }
}

/// Build a key-value field line.
fn field_line(label: &str, value: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), theme.muted),
        Span::from(value.to_string()),
    ])
}

/// Build a section header line.
fn section_header(title: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("  \u{2500}\u{2500}\u{2500} {title} \u{2500}\u{2500}\u{2500}"),
        theme.emphasis,
    ))
}

/// Simple 10-char progress bar.
fn progress_bar(pct: u8) -> String {
    let filled = (pct as usize * 10) / 100;
    let empty = 10 - filled;
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

/// Format a deadline for the detail view (relative + absolute).
fn format_deadline_detail(deadline_str: &str) -> String {
    let Ok(deadline) = DateTime::parse_from_rfc3339(deadline_str) else {
        return deadline_str.to_string();
    };
    let now = Utc::now();
    let days = deadline.signed_duration_since(now).num_days();
    let abs = deadline
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M");

    if days < -1 {
        format!("{abs} ({}d overdue)", -days)
    } else if days < 0 {
        format!("{abs} (overdue)")
    } else if days == 0 {
        format!("{abs} (today)")
    } else if days == 1 {
        format!("{abs} (tomorrow)")
    } else {
        format!("{abs} (in {days}d)")
    }
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

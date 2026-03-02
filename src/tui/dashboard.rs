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

//! Dashboard view — the TUI home screen.
//!
//! Shows feels bar, active tasks, counts, and a next-up list sorted by
//! urgency. Mirrors the CLI's `gtr status` + `gtr next` output.

use chrono::{Local, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::cache::{ActiveTask, FeelsRow, FeelsState, TaskCache, TaskSummary};
use crate::config::Config;
use crate::display::{self, ENERGY_LABELS, FOCUS_LABELS};
use crate::icons::Glyphs;
use crate::output::compute_min_prefix_len;
use crate::threshold_cache::{self, CachedThresholds};
use crate::urgency::calculate_urgency_score;

use super::theme::Theme;

/// Maximum number of tasks shown in the next-up section.
const NEXT_UP_LIMIT: usize = 5;

/// Secondary text style — gray foreground instead of DIM modifier,
/// which renders as invisible black on many dark terminal themes.
const SECONDARY: Style = Style::new().fg(Color::Gray);

/// Dashboard state loaded from the cache.
pub struct DashboardState {
    feels: Option<FeelsRow>,
    active_tasks: Vec<ActiveTask>,
    overdue: i64,
    due_today: i64,
    done_today: i64,
    pending_sync: i64,
    next_up: Vec<TaskSummary>,
    pub selected: usize,
    prefix_len: usize,
    glyphs: Glyphs,
    thresholds: CachedThresholds,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            feels: None,
            active_tasks: Vec::new(),
            overdue: 0,
            due_today: 0,
            done_today: 0,
            pending_sync: 0,
            next_up: Vec::new(),
            selected: 0,
            prefix_len: 2,
            glyphs: Glyphs::new(crate::icons::IconTheme::Unicode),
            thresholds: CachedThresholds {
                deadline: crate::utils::default_thresholds(),
                impact_labels: crate::utils::default_impact_labels(),
                impact_multipliers: crate::utils::default_impact_multipliers(),
            },
        }
    }
}

impl DashboardState {
    /// Create a new dashboard state from the cache.
    pub fn new(cache: &TaskCache, config: &Config) -> Self {
        let glyphs = Glyphs::new(config.effective_icon_theme());
        let thresholds = threshold_cache::read_cache(config).unwrap_or_else(|| CachedThresholds {
            deadline: crate::utils::default_thresholds(),
            impact_labels: crate::utils::default_impact_labels(),
            impact_multipliers: crate::utils::default_impact_multipliers(),
        });

        let mut state = DashboardState {
            feels: None,
            active_tasks: Vec::new(),
            overdue: 0,
            due_today: 0,
            done_today: 0,
            pending_sync: 0,
            next_up: Vec::new(),
            selected: 0,
            prefix_len: 2,
            glyphs,
            thresholds,
        };
        state.load(cache);
        state
    }

    /// Reload all dashboard data from the cache.
    pub fn refresh(&mut self, cache: &TaskCache) {
        self.load(cache);
    }

    fn load(&mut self, cache: &TaskCache) {
        let today = Local::now().date_naive();
        self.feels = cache.get_today_feels(&today).ok().flatten();
        self.active_tasks = cache.get_active_work_tasks().unwrap_or_default();
        self.overdue = cache.count_overdue().unwrap_or(0);
        self.due_today = cache.count_due_today().unwrap_or(0);
        self.done_today = cache.count_done_today().unwrap_or(0);
        self.pending_sync = cache.count_pending_sync().unwrap_or(0);

        // Build next-up list: workable tasks excluding "doing", sorted by urgency.
        let (energy, focus) = self.feels_values();
        let now = Utc::now();
        let mut workable = cache.list_workable_tasks().unwrap_or_default();
        workable.retain(|t| t.current_work_state.as_deref() != Some("doing"));

        workable.sort_by(|a, b| {
            let sa = calculate_urgency_score(a, &now, &self.thresholds, energy, focus);
            let sb = calculate_urgency_score(b, &now, &self.thresholds, energy, focus);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });

        workable.truncate(NEXT_UP_LIMIT);

        // Compute prefix length for all visible IDs (deduplicated so
        // stopped tasks appearing in both lists don't inflate prefix_len).
        let mut all_ids: Vec<String> = self
            .active_tasks
            .iter()
            .map(|t| t.id.clone())
            .chain(workable.iter().map(|t| t.id.clone()))
            .collect();
        all_ids.sort();
        all_ids.dedup();
        self.prefix_len = compute_min_prefix_len(&all_ids);
        self.next_up = workable;

        // Clamp selection.
        if self.next_up.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.next_up.len() {
            self.selected = self.next_up.len() - 1;
        }
    }

    /// Get the task ID of the currently selected next-up task.
    pub fn selected_task_id(&self) -> Option<&str> {
        self.next_up.get(self.selected).map(|t| t.id.as_str())
    }

    /// Get the title of the currently selected next-up task.
    pub fn selected_task_title(&self) -> Option<&str> {
        self.next_up.get(self.selected).map(|t| t.title.as_str())
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn select_next(&mut self) {
        if !self.next_up.is_empty() && self.selected < self.next_up.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn has_next_up(&self) -> bool {
        !self.next_up.is_empty()
    }

    /// Render the dashboard into the given area.
    pub fn render(&self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        let border = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let block = Block::bordered().title(" dashboard ").border_style(border);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let mut lines: Vec<Line<'_>> = Vec::new();

        // ─── Feels ───
        lines.push(Line::default());
        self.render_feels(theme, &mut lines);

        // ─── Working On ───
        lines.push(Line::default());
        lines.push(section_divider("Working On", inner.width, theme));
        self.render_active_tasks(theme, &mut lines);

        // ─── Counts ───
        lines.push(Line::default());
        lines.push(section_divider("Counts", inner.width, theme));
        self.render_counts(theme, &mut lines);

        // ─── Next Up ───
        lines.push(Line::default());
        lines.push(section_divider("Next Up", inner.width, theme));
        self.render_next_up(theme, &mut lines);

        Paragraph::new(lines).render(inner, buf);
    }

    fn feels_values(&self) -> (u8, u8) {
        match &self.feels {
            Some(row) if row.state == FeelsState::Set => (row.energy, row.focus),
            _ => (0, 0),
        }
    }

    fn render_feels<'a>(&'a self, theme: &'a Theme, lines: &mut Vec<Line<'a>>) {
        match &self.feels {
            None => {
                lines.push(Line::from(vec![
                    Span::styled("  not set", SECONDARY),
                    Span::styled(" \u{2014} ", SECONDARY),
                    Span::styled(":feels", theme.accent),
                    Span::styled(" to set", SECONDARY),
                ]));
            }
            Some(row) if row.state == FeelsState::Skipped => {
                lines.push(Line::from(Span::styled("  skipped today", SECONDARY)));
            }
            Some(row) if row.state == FeelsState::Deferred => {
                lines.push(Line::from(Span::styled("  deferred", SECONDARY)));
            }
            Some(row) => {
                let mut spans = Vec::new();
                spans.push(Span::raw("  Energy "));
                spans.extend(gauge_spans(row.energy, theme));
                let e_label = ENERGY_LABELS
                    .get((row.energy as usize).saturating_sub(1))
                    .unwrap_or(&"?");
                spans.push(Span::raw(format!(" {}/5 ({})", row.energy, e_label)));
                spans.push(Span::raw("   Focus "));
                spans.extend(gauge_spans(row.focus, theme));
                let f_label = FOCUS_LABELS
                    .get((row.focus as usize).saturating_sub(1))
                    .unwrap_or(&"?");
                spans.push(Span::raw(format!(" {}/5 ({})", row.focus, f_label)));
                lines.push(Line::from(spans));
            }
        }
    }

    fn render_active_tasks<'a>(&'a self, theme: &'a Theme, lines: &mut Vec<Line<'a>>) {
        if self.active_tasks.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No tasks in progress",
                SECONDARY,
            )));
            return;
        }

        for task in &self.active_tasks {
            let icon = if task.work_state == "doing" {
                self.glyphs.work_doing
            } else {
                self.glyphs.work_stopped
            };
            let state_style = if task.work_state == "doing" {
                theme.success
            } else {
                theme.warning
            };

            let (prefix, suffix) = display::split_id(&task.id, self.prefix_len);
            lines.push(Line::from(vec![
                Span::raw(format!("  {} ", icon)),
                Span::styled(prefix, theme.accent),
                Span::styled("\u{2502}", SECONDARY),
                Span::styled(suffix, SECONDARY),
                Span::styled(" \u{2502}", SECONDARY),
                Span::raw(format!(" {:<30} ", truncate_title(&task.title, 30))),
                Span::styled(&task.work_state, state_style),
            ]));
        }
    }

    fn render_counts<'a>(&'a self, theme: &'a Theme, lines: &mut Vec<Line<'a>>) {
        // Line 1: overdue + due today
        let overdue_style = if self.overdue > 0 {
            theme.danger
        } else {
            SECONDARY
        };
        let due_style = if self.due_today > 0 {
            theme.warning
        } else {
            SECONDARY
        };

        let left_col = 16; // fixed width for the left stat column
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!(
                    "{:<left_col$}",
                    format!("{} {} overdue", self.glyphs.overdue, self.overdue)
                ),
                overdue_style,
            ),
            Span::styled(
                format!(
                    "{} {} due today",
                    self.glyphs.deadline_warning, self.due_today
                ),
                due_style,
            ),
        ]));

        // Line 2: done today + pending sync
        let done_style = if self.done_today > 0 {
            theme.success
        } else {
            SECONDARY
        };
        let sync_style = if self.pending_sync > 0 {
            theme.warning
        } else {
            SECONDARY
        };

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!(
                    "{:<left_col$}",
                    format!("{} {} done", self.glyphs.success, self.done_today)
                ),
                done_style,
            ),
            Span::styled(
                format!("{} {} pending sync", self.glyphs.queued, self.pending_sync),
                sync_style,
            ),
        ]));
    }

    fn render_next_up<'a>(&'a self, theme: &'a Theme, lines: &mut Vec<Line<'a>>) {
        if self.next_up.is_empty() {
            lines.push(Line::from(Span::styled("  No tasks to suggest", SECONDARY)));
            return;
        }

        // Pad after marker: work_pad minus one char (the marker itself occupies
        // the first cell that the icon would in Working On).
        let marker_pad = &self.glyphs.work_pad[1..];
        for (i, task) in self.next_up.iter().enumerate() {
            let is_selected = i == self.selected;
            let marker = if is_selected {
                format!("  >{}", marker_pad)
            } else {
                format!("   {}", marker_pad)
            };

            let (prefix, suffix) = display::split_id(&task.id, self.prefix_len);
            let eff_pri = crate::promotion::effective_priority(task, &self.thresholds);
            let pri_style = if eff_pri == "now" {
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                SECONDARY
            };

            let mut spans = vec![
                Span::raw(marker),
                Span::styled(prefix, theme.accent),
                Span::styled("\u{2502}", SECONDARY),
                Span::styled(suffix, SECONDARY),
                Span::styled(" \u{2502}", SECONDARY),
                Span::raw(format!(" {:<28} ", truncate_title(&task.title, 28))),
                Span::styled(format!("{:<4}", eff_pri), pri_style),
                Span::raw(format!(" {:<3}", task.size)),
            ];

            // Compact deadline
            if let Some(cd) = display::format_deadline_compact(task.deadline.as_deref()) {
                let dl_style = if cd.is_overdue {
                    theme.danger
                } else {
                    theme.warning
                };
                spans.push(Span::raw("  "));
                spans.push(Span::styled(cd.text, dl_style));
            }

            let mut line = Line::from(spans);
            if is_selected {
                line = line.style(theme.selected);
            }
            lines.push(line);
        }
    }
}

/// Build a section divider line: `  ─── Title ───────────...`
fn section_divider<'a>(title: &'a str, width: u16, theme: &'a Theme) -> Line<'a> {
    let prefix = "\u{2500}\u{2500}\u{2500} ";
    let suffix_len = (width as usize)
        .saturating_sub(2 + prefix.len() + title.len() + 1)
        .min(60);
    let suffix: String = "\u{2500}".repeat(suffix_len);
    Line::from(vec![
        Span::styled(format!("  {}", prefix), SECONDARY),
        Span::styled(title, theme.emphasis),
        Span::styled(format!(" {}", suffix), SECONDARY),
    ])
}

/// Render a 5-character gauge bar: `████·` for value out of 5.
fn gauge_spans(value: u8, _theme: &Theme) -> Vec<Span<'static>> {
    let filled = value.min(5) as usize;
    let empty = 5 - filled;
    let color = match value {
        4..=5 => Color::Green,
        2..=3 => Color::Yellow,
        _ => Color::Red,
    };

    let mut spans = Vec::new();
    if filled > 0 {
        spans.push(Span::styled(
            "\u{2588}".repeat(filled),
            Style::new().fg(color),
        ));
    }
    if empty > 0 {
        spans.push(Span::styled("\u{00b7}".repeat(empty), SECONDARY));
    }
    spans
}

/// Truncate a title to fit in the given width, appending "..." if needed.
fn truncate_title(title: &str, max: usize) -> String {
    if title.len() <= max {
        title.to_string()
    } else {
        format!("{}...", &title[..max.saturating_sub(3)])
    }
}

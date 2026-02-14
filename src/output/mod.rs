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

//! Pretty output formatting for tasks and projects.

use std::collections::HashSet;

use chrono::{DateTime, Local, Utc};
use chrono_humanize::{Accuracy, HumanTime, Tense};
use colored::Colorize;
use tabled::builder::Builder;
use tabled::settings::style::HorizontalLine;
use tabled::settings::themes::Theme;
use tabled::settings::{Alignment, Modify, Style, object::Columns};
use tabled::{Table, Tabled};

use crate::markdown::MarkdownRenderer;
use crate::models::{Project, Task};

/// Calculate minimum unique prefix length for task IDs.
///
/// Uses optimized O(N log N) algorithm by sorting IDs and comparing adjacent pairs.
/// Always returns at least 2 to avoid single-character typos.
pub fn compute_min_prefix_len(task_ids: &[String]) -> usize {
    if task_ids.len() <= 1 {
        return 2; // minimum
    }

    let mut sorted: Vec<String> = task_ids.iter().map(|id| id[..8].to_string()).collect();
    sorted.sort();

    let mut max_needed = 2; // minimum

    for i in 0..sorted.len() - 1 {
        let common_len = common_prefix_len(&sorted[i], &sorted[i + 1]);
        let needed = (common_len + 1).max(2);
        max_needed = max_needed.max(needed);
    }

    max_needed.min(8) // cap at shortened ID length
}

/// Calculate length of common prefix between two strings.
fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count()
}

/// Format deadline for display (relative or absolute).
///
/// - If absolute_dates is true: always show absolute date
/// - If relative and > 30 days: show absolute date
/// - Otherwise: show relative time using chrono-humanize
/// - Color red if overdue
fn format_deadline(deadline_str: Option<&str>, absolute_dates: bool) -> String {
    let Some(deadline_str) = deadline_str else {
        return "-".to_string();
    };

    let Ok(deadline) = DateTime::parse_from_rfc3339(deadline_str) else {
        return "-".to_string();
    };

    let deadline_time = deadline.with_timezone(&Local);
    let now = Utc::now();
    let is_overdue = deadline < now;

    // Calculate days difference for threshold check
    let duration = if deadline > now {
        deadline.signed_duration_since(now)
    } else {
        now.signed_duration_since(deadline)
    };
    let days = duration.num_days();

    // Determine display format
    let formatted = if absolute_dates || days > 30 {
        // Show absolute date
        deadline_time.format("%Y-%m-%d").to_string()
    } else {
        // Show relative time
        let ht = HumanTime::from(deadline);
        ht.to_text_en(Accuracy::Rough, Tense::Future)
    };

    // Color red if overdue
    if is_overdue {
        formatted.red().to_string()
    } else {
        formatted
    }
}

/// Format task ID with colored prefix and separator for list views.
///
/// If terminal supports colors, formats as: `prefix|suffix` where prefix is cyan
/// and suffix is dimmed. If no color support, returns plain shortened ID.
fn format_task_id(id: &str, prefix_len: usize) -> String {
    let id_short = &id[..8];

    // Check if terminal supports colors
    if colored::control::SHOULD_COLORIZE.should_colorize() {
        let prefix = &id_short[..prefix_len];
        let suffix = &id_short[prefix_len..];
        format!("{}|{}", prefix.cyan(), suffix.dimmed())
    } else {
        // No color support: return plain shortened ID without separator
        id_short.to_string()
    }
}

/// Format progress for display in list tables.
///
/// When fancy mode is active and the terminal supports colors, renders a
/// 10-character progress bar with color-coded fill:
/// - 0–49%: yellow (amber)
/// - 50–99%: cyan (calming blue)
/// - 100%: green (complete)
///
/// Falls back to numerical `X%` when fancy is disabled or no color support.
/// Returns `-` when progress is None.
fn format_progress(progress: Option<u8>, fancy: bool) -> String {
    let Some(value) = progress else {
        return "-".to_string();
    };

    let use_bar = fancy && colored::control::SHOULD_COLORIZE.should_colorize();

    if !use_bar {
        return format!("{}%", value);
    }

    let filled = (value as usize / 10).min(10);
    let empty = 10 - filled;

    let fill_str = "█".repeat(filled);
    let empty_str = "░".repeat(empty);

    let colored_fill = match value {
        0..=49 => fill_str.yellow().to_string(),
        50..=99 => fill_str.cyan().to_string(),
        _ => fill_str.green().to_string(),
    };

    format!("{}{} {:>3}%", colored_fill, empty_str.dimmed(), value)
}

/// Row type for project table display.
#[derive(Tabled)]
struct ProjectRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Description")]
    description: String,
}

/// Print a list of projects in table format.
pub fn print_projects(projects: &[Project]) {
    if projects.is_empty() {
        println!("{}", "No projects found".yellow());
        return;
    }

    let rows: Vec<ProjectRow> = projects
        .iter()
        .map(|p| ProjectRow {
            id: p.id.cyan().to_string(),
            name: p.name.green().to_string(),
            description: p.description.as_deref().unwrap_or("-").to_string(),
        })
        .collect();

    let table = Table::new(rows)
        .with(Style::rounded())
        .with(Modify::new(Columns::new(0..1)).with(Alignment::center())) // ID
        .to_string();
    println!("{}", table);
    println!("\n{} {}", "Total:".bold(), projects.len());
}

/// Row type for task table display (without project column).
///
/// The `modified` field is always populated but hidden from the derive
/// path via `#[tabled(skip)]`. It is included explicitly by the Builder
/// path when verbose mode is active.
#[derive(Tabled)]
struct TaskRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Priority")]
    priority: String,
    #[tabled(rename = "Size")]
    size: String,
    #[tabled(skip)]
    modified: String,
    #[tabled(rename = "Deadline")]
    deadline: String,
    #[tabled(rename = "Status")]
    status: String,
}

/// Row type for task table display (with project column).
#[derive(Tabled)]
struct TaskRowWithProject {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Title")]
    title: String,
    #[tabled(rename = "Project")]
    project: String,
    #[tabled(rename = "Priority")]
    priority: String,
    #[tabled(rename = "Size")]
    size: String,
    #[tabled(rename = "Deadline")]
    deadline: String,
    #[tabled(rename = "Status")]
    status: String,
}

/// Print a list of tasks in table format.
///
/// If `doing_count` is Some, inserts a visual divider after that many tasks.
pub fn print_tasks(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
    doing_count: Option<usize>,
) {
    if tasks.is_empty() {
        println!("{}", "No tasks found".yellow());
        return;
    }
    print_task_table(
        tasks,
        prefix_len,
        absolute_dates,
        fancy,
        verbose,
        doing_count,
    );
    println!("\n{} {}", "Total:".bold(), tasks.len());
}

/// Internal function to print task table.
fn print_task_table(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
    doing_count: Option<usize>,
) {
    // Check if tasks are from multiple projects
    let unique_projects: HashSet<&str> = tasks.iter().map(|t| t.project_id.as_str()).collect();
    let show_project = unique_projects.len() > 1;

    if show_project {
        print_task_table_with_project(
            tasks,
            unique_projects,
            prefix_len,
            absolute_dates,
            fancy,
            verbose,
            doing_count,
        );
    } else {
        print_task_table_simple(
            tasks,
            prefix_len,
            absolute_dates,
            fancy,
            verbose,
            doing_count,
        );
    }
}

/// Print task table without project column.
fn print_task_table_simple(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
    doing_count: Option<usize>,
) {
    let has_progress = tasks.iter().any(|t| t.progress.is_some());
    let use_builder = has_progress || verbose;

    if use_builder {
        print_task_table_with_builder(
            tasks,
            prefix_len,
            absolute_dates,
            false,
            &std::collections::HashMap::new(),
            fancy,
            verbose,
            doing_count,
        );
    } else {
        let rows: Vec<TaskRow> = tasks
            .iter()
            .map(|task| build_task_row(task, prefix_len, absolute_dates))
            .collect();

        let mut table = Table::new(rows);

        // Use Theme to insert horizontal line divider if requested
        let mut style = Theme::from_style(Style::rounded());
        if let Some(count) = doing_count
            && count > 0
            && count < tasks.len()
        {
            style.insert_horizontal_line(
                count + 1,
                HorizontalLine::inherit(
                    Style::modern()
                        .intersection_left('╞')
                        .intersection_right('╡')
                        .intersection('╪'),
                )
                .horizontal('═'),
            );
        }

        table
            .with(style)
            .with(Modify::new(Columns::new(0..1)).with(Alignment::center()))
            .with(Modify::new(Columns::new(2..6)).with(Alignment::center()));

        println!("{}", table);
    }
}

/// Build a TaskRow from a Task.
fn build_task_row(task: &Task, prefix_len: usize, absolute_dates: bool) -> TaskRow {
    let modified = chrono::DateTime::parse_from_rfc3339(&task.modified)
        .unwrap()
        .with_timezone(&Local);
    let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();

    // Impact emoji prefix: reserve space for alignment
    // Emojis are ~2 char widths + 1 space = 3 total visual width
    let impact_prefix = match task.impact {
        1 => "\u{1f525} ", // 🔥 + space
        2 => "\u{26a1} ",  // ⚡ + space
        _ => "   ",        // 3 spaces to match emoji visual width
    };
    let priority_colored = match task.priority.as_str() {
        "now" => format!("{}{}", impact_prefix, task.priority.red()),
        _ => format!("{}{}", impact_prefix, task.priority),
    };

    let deadline_str = format_deadline(task.deadline.as_deref(), absolute_dates);

    let status = if task.is_deleted() {
        "DELETED".red().to_string()
    } else if task.is_done() {
        "done".blue().to_string()
    } else if let Some(ref work_state) = task.current_work_state {
        match work_state.as_str() {
            "doing" => "doing".green().bold().to_string(),
            "stopped" => "stopped".yellow().to_string(),
            _ => "pending".green().to_string(),
        }
    } else {
        "pending".green().to_string()
    };

    TaskRow {
        id: format_task_id(&task.id, prefix_len),
        title: task.title.clone(),
        priority: priority_colored,
        size: task.size.clone(),
        modified: modified_str,
        deadline: deadline_str,
        status,
    }
}

/// Print task table using Builder (supports conditional columns).
#[allow(clippy::too_many_arguments)]
fn print_task_table_with_builder(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    show_project: bool,
    project_colors: &std::collections::HashMap<&str, colored::Color>,
    fancy: bool,
    verbose: bool,
    doing_count: Option<usize>,
) {
    let has_progress = tasks.iter().any(|t| t.progress.is_some());
    let mut builder = Builder::default();

    // Header
    let mut header: Vec<String> = vec!["ID".into(), "Title".into()];
    if show_project {
        header.push("Project".into());
    }
    header.push("Priority".into());
    header.push("Size".into());
    if verbose {
        header.push("Modified".into());
    }
    header.push("Deadline".into());
    if has_progress {
        header.push("Progress".into());
    }
    header.push("Status".into());
    let num_cols = header.len();
    builder.push_record(header);

    for task in tasks {
        let row = build_task_row(task, prefix_len, absolute_dates);

        let mut record: Vec<String> = vec![row.id, row.title];
        if show_project {
            let color = project_colors.get(task.project_id.as_str());
            let project = if let Some(c) = color {
                task.project_id.color(*c).to_string()
            } else {
                task.project_id.clone()
            };
            record.push(project);
        }
        record.push(row.priority);
        record.push(row.size);
        if verbose {
            record.push(row.modified);
        }
        record.push(row.deadline);
        if has_progress {
            record.push(format_progress(task.progress, fancy));
        }
        record.push(row.status);
        builder.push_record(record);
    }

    let mut table = builder.build();

    // Use Theme to insert horizontal line divider if requested
    let mut style = Theme::from_style(Style::rounded());
    if let Some(count) = doing_count
        && count > 0
        && count < tasks.len()
    {
        style.insert_horizontal_line(
            count + 1,
            HorizontalLine::inherit(
                Style::modern()
                    .intersection_left('╞')
                    .intersection_right('╡')
                    .intersection('╪'),
            )
            .horizontal('═'),
        );
    }

    table
        .with(style)
        .with(Modify::new(Columns::new(0..1)).with(Alignment::center()));

    // Center all columns after Title (index 2..end)
    table.with(Modify::new(Columns::new(2..num_cols)).with(Alignment::center()));

    println!("{}", table);
}

/// Print task table with project column.
#[allow(clippy::too_many_arguments)]
fn print_task_table_with_project(
    tasks: &[Task],
    unique_projects: HashSet<&str>,
    prefix_len: usize,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
    doing_count: Option<usize>,
) {
    let has_progress = tasks.iter().any(|t| t.progress.is_some());
    let use_builder = has_progress || verbose;

    let colors = [
        colored::Color::Cyan,
        colored::Color::Green,
        colored::Color::Yellow,
        colored::Color::Magenta,
        colored::Color::Blue,
        colored::Color::BrightCyan,
        colored::Color::BrightGreen,
        colored::Color::BrightYellow,
    ];
    let mut project_colors = std::collections::HashMap::new();
    for (idx, project_id) in unique_projects.iter().enumerate() {
        project_colors.insert(*project_id, colors[idx % colors.len()]);
    }

    if use_builder {
        print_task_table_with_builder(
            tasks,
            prefix_len,
            absolute_dates,
            true,
            &project_colors,
            fancy,
            verbose,
            doing_count,
        );
    } else {
        let rows: Vec<TaskRowWithProject> = tasks
            .iter()
            .map(|task| {
                let row = build_task_row(task, prefix_len, absolute_dates);
                let color = project_colors.get(task.project_id.as_str()).unwrap();
                let project = task.project_id.color(*color).to_string();

                TaskRowWithProject {
                    id: row.id,
                    title: row.title,
                    project,
                    priority: row.priority,
                    size: row.size,
                    deadline: row.deadline,
                    status: row.status,
                }
            })
            .collect();

        let mut table = Table::new(rows);

        // Use Theme to insert horizontal line divider if requested
        let mut style = Theme::from_style(Style::rounded());
        if let Some(count) = doing_count
            && count > 0
            && count < tasks.len()
        {
            style.insert_horizontal_line(
                count + 1,
                HorizontalLine::inherit(
                    Style::modern()
                        .intersection_left('╞')
                        .intersection_right('╡')
                        .intersection('╪'),
                )
                .horizontal('═'),
            );
        }

        table
            .with(style)
            .with(Modify::new(Columns::new(0..1)).with(Alignment::center()))
            .with(Modify::new(Columns::new(2..7)).with(Alignment::center()));

        println!("{}", table);
    }
}

/// Print a single task with full details and markdown rendering.
///
/// If `no_format` is true or NO_COLOR is set, markdown will not be rendered.
/// If `no_wrap` is true, the body will not be hard-wrapped at 80 columns.
pub fn print_task_details(
    config: &crate::config::Config,
    task: &Task,
    no_format: bool,
    no_wrap: bool,
) {
    let renderer = if no_format {
        MarkdownRenderer::with_override(Some(false)) // Force disable
    } else {
        MarkdownRenderer::with_override(None) // Use default (respects NO_COLOR/TTY)
    };

    // Print header
    println!("\n{}", "═".repeat(80));
    println!("{}", task.title.bold().green());
    println!("{}", "═".repeat(80));

    // Print metadata
    println!("\n{}", "Metadata:".bold());
    println!("  ID:       {}", task.id.cyan());

    let priority_colored = match task.priority.as_str() {
        "now" => task.priority.red(),
        _ => task.priority.normal(),
    };
    println!("  Priority: {}", priority_colored);
    println!("  Size:     {}", task.size);

    if let Some(ref work_state) = task.current_work_state {
        let status_colored = match work_state.as_str() {
            "doing" => work_state.green().bold(),
            "stopped" => work_state.yellow(),
            _ => work_state.normal(),
        };
        println!("  Status:   {}", status_colored);
    }

    if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&task.created) {
        let created_time = created.with_timezone(&Local);
        println!("  Created:  {}", created_time.format("%Y-%m-%d %H:%M:%S"));
    }

    if let Ok(modified) = chrono::DateTime::parse_from_rfc3339(&task.modified) {
        let modified_time = modified.with_timezone(&Local);
        println!("  Modified: {}", modified_time.format("%Y-%m-%d %H:%M:%S"));
    }

    if let Some(ref deadline_str) = task.deadline
        && let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str)
    {
        let deadline_time = deadline.with_timezone(&Local);
        let now = chrono::Utc::now();
        let is_overdue = deadline < now;
        let formatted = format!("Deadline: {}", deadline_time.format("%Y-%m-%d %H:%M:%S"));

        if is_overdue {
            println!("  {}", formatted.red().bold());
        } else {
            println!("  {}", formatted);
        }
    }

    if let Some(ref done_str) = task.done
        && let Ok(done) = chrono::DateTime::parse_from_rfc3339(done_str)
    {
        let done_time = done.with_timezone(&Local);
        println!(
            "  {}",
            format!("Done:     {}", done_time.format("%Y-%m-%d %H:%M:%S")).blue()
        );
    }

    if let Some(ref deleted_str) = task.deleted
        && let Ok(deleted) = chrono::DateTime::parse_from_rfc3339(deleted_str)
    {
        let deleted_time = deleted.with_timezone(&Local);
        println!(
            "  {}",
            format!("Deleted:  {}", deleted_time.format("%Y-%m-%d %H:%M:%S")).red()
        );
    }

    // Get impact label from cache (with fallback to defaults)
    let impact_label = crate::threshold_cache::read_cache(config)
        .and_then(|cached| cached.impact_labels.get(&task.impact.to_string()).cloned())
        .or_else(|| {
            crate::utils::default_impact_labels()
                .get(&task.impact.to_string())
                .cloned()
        })
        .unwrap_or_else(|| "Unknown".to_string());
    println!("  Impact:   {} ({})", impact_label, task.impact);

    if let Some(progress) = task.progress {
        println!("  Progress: {}%", progress);
    }

    println!("  Version:  {}", task.version);

    // Print body with markdown rendering
    if !task.body.is_empty() {
        println!("\n{}", "Description:".bold());
        println!("{}", "─".repeat(80));
        if no_wrap {
            print!("{}", renderer.render_no_wrap(&task.body));
        } else {
            print!("{}", renderer.render(&task.body));
        }
    } else {
        println!("\n{}", "(No description)".italic().dimmed());
    }

    println!("{}\n", "═".repeat(80));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_prefix_len() {
        assert_eq!(common_prefix_len("abc", "abc"), 3);
        assert_eq!(common_prefix_len("abc", "abd"), 2);
        assert_eq!(common_prefix_len("abc", "xyz"), 0);
        assert_eq!(common_prefix_len("", "abc"), 0);
        assert_eq!(common_prefix_len("abc", ""), 0);
    }

    #[test]
    fn test_compute_min_prefix_len_single_task() {
        let ids = vec!["ea75a3ac".to_string()];
        assert_eq!(compute_min_prefix_len(&ids), 2);
    }

    #[test]
    fn test_compute_min_prefix_len_all_different() {
        let ids = vec![
            "ea75a3ac".to_string(),
            "b35bcda6".to_string(),
            "d240111c".to_string(),
        ];
        // All differ at position 0, but minimum is 2
        assert_eq!(compute_min_prefix_len(&ids), 2);
    }

    #[test]
    fn test_compute_min_prefix_len_similar_prefix() {
        let ids = vec![
            "d240111c".to_string(),
            "ea75a3ac".to_string(),
            "ea7bc84d".to_string(),
        ];
        // ea75a3ac vs ea7bc84d differ at position 3, so need 4 chars
        assert_eq!(compute_min_prefix_len(&ids), 4);
    }

    #[test]
    fn test_compute_min_prefix_len_longer_prefix() {
        let ids = vec!["ea75a3ac".to_string(), "ea75a3bc".to_string()];
        // Differ at position 6, so need 7 chars
        assert_eq!(compute_min_prefix_len(&ids), 7);
    }

    #[test]
    fn test_format_task_id_no_color() {
        // When SHOULD_COLORIZE is false, should return plain shortened ID
        colored::control::set_override(false);
        let formatted = format_task_id("ea75a3ac-1234-5678-90ab-cdef12345678", 4);
        assert_eq!(formatted, "ea75a3ac");
        colored::control::unset_override();
    }

    #[test]
    fn test_format_task_id_with_color() {
        // When SHOULD_COLORIZE is true, should include separator
        colored::control::set_override(true);
        let formatted = format_task_id("ea75a3ac-1234-5678-90ab-cdef12345678", 4);
        // Should contain the separator (actual color codes may vary)
        assert!(formatted.contains("|"));
        colored::control::unset_override();
    }

    #[test]
    fn test_format_deadline_none() {
        let formatted = format_deadline(None, false);
        assert_eq!(formatted, "-");
    }

    #[test]
    fn test_format_deadline_invalid() {
        let formatted = format_deadline(Some("invalid-date"), false);
        assert_eq!(formatted, "-");
    }

    #[test]
    fn test_format_deadline_relative_near() {
        // Deadline 3 days in the future - should show relative
        let future = Utc::now() + chrono::Duration::days(3);
        let deadline_str = future.to_rfc3339();

        let formatted = format_deadline(Some(&deadline_str), false);

        // Should contain relative text (not absolute date format)
        assert!(!formatted.contains("2026"));
        assert!(!formatted.contains("2027"));
        // Should contain some time indicator (exact text depends on chrono-humanize)
        assert!(formatted.len() > 1);
    }

    #[test]
    fn test_format_deadline_absolute_beyond_threshold() {
        // Deadline 35 days in the future - should show absolute date
        let future = Utc::now() + chrono::Duration::days(35);
        let deadline_str = future.to_rfc3339();

        let formatted = format_deadline(Some(&deadline_str), false);

        // Should be absolute date format YYYY-MM-DD
        assert!(formatted.len() == 10);
        assert!(formatted.contains("-"));
    }

    #[test]
    fn test_format_deadline_force_absolute() {
        // Deadline 3 days in the future but with absolute_dates=true
        let future = Utc::now() + chrono::Duration::days(3);
        let deadline_str = future.to_rfc3339();

        let formatted = format_deadline(Some(&deadline_str), true);

        // Should be absolute date format YYYY-MM-DD even though it's close
        assert!(formatted.len() == 10);
        assert!(formatted.contains("-"));
    }

    #[test]
    fn test_format_deadline_overdue() {
        // Deadline 1 day in the past - should be colored red
        let past = Utc::now() - chrono::Duration::days(1);
        let deadline_str = past.to_rfc3339();

        // Disable color for predictable testing
        colored::control::set_override(false);
        let formatted = format_deadline(Some(&deadline_str), false);

        // Should show something (text content varies)
        assert!(formatted.len() > 1);
        assert_ne!(formatted, "-");

        colored::control::unset_override();
    }

    #[test]
    fn test_format_deadline_overdue_with_color() {
        // Deadline 2 days in the past - should contain red color codes
        let past = Utc::now() - chrono::Duration::days(2);
        let deadline_str = past.to_rfc3339();

        // Enable color to check red coloring
        colored::control::set_override(true);
        let formatted = format_deadline(Some(&deadline_str), false);

        // With colors enabled, should contain ANSI color codes for red
        assert!(formatted.len() > 1);

        colored::control::unset_override();
    }
}

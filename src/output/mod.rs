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

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local, Utc};
use chrono_humanize::{Accuracy, HumanTime, Tense};
use colored::Colorize;
use tabled::builder::Builder;
use tabled::settings::style::HorizontalLine;
use tabled::settings::themes::Theme;
use tabled::settings::width::Width;
use tabled::settings::{Alignment, Modify, Style, object::Columns};
use tabled::{Table, Tabled};

use crate::icons::Icons;
use crate::markdown::MarkdownRenderer;
use crate::models::{Project, Task};
use crate::promotion;
use crate::threshold_cache::CachedThresholds;

/// Detect terminal width, with fallback to 80 if detection fails.
fn detect_terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(terminal_size::Width(w), _)| w as usize)
        .unwrap_or(80)
}

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
        let tense = if is_overdue {
            Tense::Past
        } else {
            Tense::Future
        };
        ht.to_text_en(Accuracy::Rough, tense)
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
/// When `colorize` is true, formats as: `prefix|suffix` where prefix is cyan
/// and suffix is dimmed. Otherwise returns plain shortened ID.
pub fn format_task_id(id: &str, prefix_len: usize, colorize: bool) -> String {
    let id_short = &id[..8];

    if colorize {
        let prefix = &id_short[..prefix_len];
        let suffix = &id_short[prefix_len..];
        format!("{}|{}", prefix.cyan(), suffix.dimmed())
    } else {
        id_short.to_string()
    }
}

/// Format a full UUID with an appended short ID slug.
///
/// Produces output like: `ea75a3ac-...-def012345678 (ea75|a3ac)`
/// where the short slug uses `format_task_id` for highlighting.
pub fn format_full_id(id: &str, prefix_len: usize) -> String {
    let colorize = colored::control::SHOULD_COLORIZE.should_colorize();
    let short = format_task_id(id, prefix_len, colorize);
    if colorize {
        format!("{} ({})", id.cyan(), short)
    } else {
        format!("{} ({})", id, short)
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
/// Falls back to numerical `X%` when fancy is disabled or colorize is false.
/// Returns `-` when progress is None.
fn format_progress(progress: Option<u8>, fancy: bool, colorize: bool) -> String {
    let Some(value) = progress else {
        return "-".to_string();
    };

    let use_bar = fancy && colorize;

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

/// Configuration for which columns to display in task tables.
#[derive(Debug, Clone)]
struct TableColumns {
    /// Whether to show the project column (for multi-project views)
    show_project: bool,
    /// Whether to show the modified timestamp column (verbose mode)
    show_modified: bool,
    /// Whether to show the progress column (when tasks have progress set)
    show_progress: bool,
}

/// Internal struct to hold formatted task row data.
struct TaskRowData {
    id: String,
    title: String,
    priority: String,
    size: String,
    modified: String,
    deadline: String,
    status: String,
}

/// Print a list of tasks in table format.
///
/// If `doing_count` is Some, inserts a visual divider after that many tasks.
#[allow(clippy::too_many_arguments)]
pub fn print_tasks(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
    doing_count: Option<usize>,
    thresholds: &CachedThresholds,
    icons: &Icons,
    compact: bool,
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
        thresholds,
        icons,
        compact,
    );
    println!("\n{} {}", "Total:".bold(), tasks.len());
}

/// Internal function to print task table.
#[allow(clippy::too_many_arguments)]
fn print_task_table(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
    doing_count: Option<usize>,
    thresholds: &CachedThresholds,
    icons: &Icons,
    compact: bool,
) {
    // Detect which columns to show
    let unique_projects: HashSet<&str> = tasks.iter().map(|t| t.project_id.as_str()).collect();
    let columns = TableColumns {
        show_project: unique_projects.len() > 1,
        show_modified: verbose,
        show_progress: tasks.iter().any(|t| t.progress.is_some()),
    };

    // Build project color mapping
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

    // Detect terminal width and route to appropriate renderer
    let terminal_width = detect_terminal_width();

    if terminal_width >= 150 {
        // Wide terminal: use default table
        render_task_table(
            tasks,
            prefix_len,
            absolute_dates,
            columns,
            &project_colors,
            fancy,
            doing_count,
            thresholds,
            icons,
            compact,
        );
    } else {
        // Narrow terminal: use simplified format
        render_simplified_table(
            tasks,
            prefix_len,
            absolute_dates,
            columns,
            &project_colors,
            fancy,
            doing_count,
            thresholds,
            icons,
            compact,
        );
    }
}

/// Compute subtask counts from a task list.
///
/// Returns a map from task ID to the number of direct children present
/// in the given slice.
fn compute_subtask_counts(tasks: &[Task]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for task in tasks {
        if let Some(ref parent_id) = task.parent_id {
            *counts.entry(parent_id.clone()).or_default() += 1;
        }
    }
    counts
}

/// Build formatted task row data from a Task.
fn build_task_row(
    task: &Task,
    prefix_len: usize,
    absolute_dates: bool,
    thresholds: &CachedThresholds,
    colorize: bool,
    icons: &Icons,
    subtask_counts: &HashMap<String, usize>,
) -> TaskRowData {
    let modified = chrono::DateTime::parse_from_rfc3339(&task.modified)
        .unwrap()
        .with_timezone(&Local);
    let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();

    // Joy icon prefix for task title
    let je = icons.joy_icon(task.joy);
    let joy_prefix = if je.is_empty() {
        String::new()
    } else {
        format!("{je} ")
    };

    // Impact icon prefix: spacing depends on theme
    let impact_prefix = match task.impact {
        1 => &icons.impact_critical,
        2 => &icons.impact_significant,
        _ => &icons.impact_none,
    };
    let eff_priority = promotion::effective_priority(task, thresholds);
    let priority_colored = match eff_priority {
        "now" => format!("{}{}", impact_prefix, eff_priority.red()),
        _ => format!("{}{}", impact_prefix, eff_priority),
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

    // Build hierarchy subtitle line (below the title)
    let subtitle = build_hierarchy_subtitle(task, prefix_len, colorize, icons, subtask_counts);
    let title = if subtitle.is_empty() {
        format!("{}{}", joy_prefix, task.title)
    } else {
        format!("{}{}\n{}", joy_prefix, task.title, subtitle)
    };

    TaskRowData {
        id: format_task_id(&task.id, prefix_len, colorize),
        title,
        priority: priority_colored,
        size: task.size.clone(),
        modified: modified_str,
        deadline: deadline_str,
        status,
    }
}

/// Build the hierarchy subtitle line for a task's title cell.
///
/// Shows parent ID and/or subtask count on a dimmed line below the title.
/// Returns empty string if the task has neither parent nor children.
fn build_hierarchy_subtitle(
    task: &Task,
    prefix_len: usize,
    colorize: bool,
    icons: &Icons,
    subtask_counts: &HashMap<String, usize>,
) -> String {
    let has_parent = task.parent_id.is_some();
    let child_count = subtask_counts.get(&task.id).copied().unwrap_or(0);

    if !has_parent && child_count == 0 {
        return String::new();
    }

    let mut parts: Vec<String> = Vec::new();

    // Parent reference
    if let Some(ref parent_id) = task.parent_id {
        let parent_id_formatted = format_task_id(parent_id, prefix_len, colorize);
        if colorize {
            parts.push(format!(
                "{} {}",
                icons.hierarchy_parent.blue(),
                parent_id_formatted
            ));
        } else {
            parts.push(format!(
                "{} {}",
                icons.hierarchy_parent, parent_id_formatted
            ));
        }
    }

    // Subtask count
    if child_count > 0 {
        let label = if child_count == 1 {
            "1 subtask".to_string()
        } else {
            format!("{child_count} subtasks")
        };
        if colorize {
            parts.push(format!(
                "{} {}",
                icons.hierarchy_subtasks.green(),
                label.green()
            ));
        } else {
            parts.push(format!("{} {label}", icons.hierarchy_subtasks));
        }
    }

    if colorize {
        let sep = format!(" {} ", icons.hierarchy_separator).bright_black();
        format!("  {}", parts.join(&sep.to_string()))
    } else {
        format!(
            "  {}",
            parts.join(&format!(" {} ", icons.hierarchy_separator))
        )
    }
}

/// Render a task table with configurable columns using the Builder pattern.
#[allow(clippy::too_many_arguments)]
/// Minimum number of rows before row separators are inserted.
const ROW_SEPARATOR_THRESHOLD: usize = 10;

#[allow(clippy::too_many_arguments)]
fn render_task_table(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    columns: TableColumns,
    project_colors: &std::collections::HashMap<&str, colored::Color>,
    fancy: bool,
    doing_count: Option<usize>,
    thresholds: &CachedThresholds,
    icons: &Icons,
    compact: bool,
) {
    let mut builder = Builder::default();

    // Build header based on column configuration
    let mut header: Vec<String> = vec!["ID".into(), "Title".into()];
    if columns.show_project {
        header.push("Project".into());
    }
    header.push("Priority".into());
    header.push("Size".into());
    if columns.show_modified {
        header.push("Modified".into());
    }
    header.push("Deadline".into());
    if columns.show_progress {
        header.push("Progress".into());
    }
    header.push("Status".into());
    let num_cols = header.len();
    builder.push_record(header);

    // Build rows based on column configuration
    let colorize = colored::control::SHOULD_COLORIZE.should_colorize();
    let subtask_counts = compute_subtask_counts(tasks);
    for task in tasks {
        let row = build_task_row(
            task,
            prefix_len,
            absolute_dates,
            thresholds,
            colorize,
            icons,
            &subtask_counts,
        );

        let mut record: Vec<String> = vec![row.id, row.title];
        if columns.show_project {
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
        if columns.show_modified {
            record.push(row.modified);
        }
        record.push(row.deadline);
        if columns.show_progress {
            record.push(format_progress(task.progress, fancy, colorize));
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

    // Insert thin row separators when the table is long or has multi-line
    // rows (hierarchy subtitles or wrapped titles), unless --compact.
    let has_multiline_rows = tasks
        .iter()
        .any(|t| t.parent_id.is_some() || subtask_counts.contains_key(&t.id) || t.title.len() > 60);
    if !compact && (tasks.len() > ROW_SEPARATOR_THRESHOLD || has_multiline_rows) {
        let doing_sep_pos = doing_count
            .filter(|&c| c > 0 && c < tasks.len())
            .map(|c| c + 1);

        for row in 2..=tasks.len() {
            // Skip the doing/other separator position (already has ═══)
            if Some(row) == doing_sep_pos {
                continue;
            }
            style.insert_horizontal_line(
                row,
                HorizontalLine::inherit(Style::modern())
                    .horizontal('┈')
                    .intersection('┊'),
            );
        }
    }

    table
        .with(style)
        .with(Modify::new(Columns::new(0..1)).with(Alignment::center()));

    // Center all columns after Title (index 2..end)
    table.with(Modify::new(Columns::new(2..num_cols)).with(Alignment::center()));

    // Wrap Title column (index 1) at 60 characters, preserving word boundaries
    table.with(Modify::new(Columns::new(1..2)).with(Width::wrap(60).keep_words(true)));

    println!("{}", table);
}

/// Render simplified table for narrow terminals (<150 cols).
///
/// Uses 3-line format per task, capped at 70 columns total.
#[allow(clippy::too_many_arguments)]
fn render_simplified_table(
    tasks: &[Task],
    prefix_len: usize,
    absolute_dates: bool,
    _columns: TableColumns,
    project_colors: &std::collections::HashMap<&str, colored::Color>,
    _fancy: bool,
    doing_count: Option<usize>,
    thresholds: &CachedThresholds,
    icons: &Icons,
    _compact: bool,
) {
    let colorize = colored::control::SHOULD_COLORIZE.should_colorize();
    let subtask_counts = compute_subtask_counts(tasks);
    for (idx, task) in tasks.iter().enumerate() {
        let row = build_task_row(
            task,
            prefix_len,
            absolute_dates,
            thresholds,
            colorize,
            icons,
            &subtask_counts,
        );

        // Insert separator between doing and other tasks
        if let Some(count) = doing_count
            && idx == count
        {
            println!("{}", "═".repeat(70).dimmed());
        }

        // Line 1: ID - PROJECT - STATUS
        let project_colored = if let Some(color) = project_colors.get(task.project_id.as_str()) {
            task.project_id.color(*color).to_string()
        } else {
            task.project_id.clone()
        };
        println!("{} - {} - {}", row.id, project_colored, row.status);

        // Line 2: TITLE (wrapped at 60 columns)
        let wrapped_title = wrap_text(&task.title, 60);
        for line in wrapped_title {
            println!("{}", line);
        }

        // Line 3: PRIORITY - DEADLINE - SIZE
        println!("{} - {} - {}", row.priority, row.deadline, row.size);

        // Task separator (except for last task)
        if idx < tasks.len() - 1 {
            println!("{}", "─".repeat(70).dimmed());
        }
    }
}

/// Wrap text at specified width, preserving word boundaries.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        // Check if adding this word would exceed width
        let test_line = if current_line.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current_line, word)
        };

        if test_line.len() <= width {
            current_line = test_line;
        } else {
            // Current line is full, start new line
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            current_line = word.to_string();
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // If no lines were created (empty text), return single empty line
    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Wrap text at `width` columns, indenting continuation lines by `indent` spaces.
///
/// The first line is assumed to already have `indent` characters of prefix
/// printed by the caller, so it gets `width - indent` characters of text.
/// Subsequent lines are prefixed with `indent` spaces.
/// Returns the wrapped text with a trailing newline.
pub fn wrap_with_indent(text: &str, width: usize, indent: usize) -> String {
    let content_width = width.saturating_sub(indent).max(20);
    let lines = wrap_text(text, content_width);
    let indent_str = " ".repeat(indent);
    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            result.push_str(line);
        } else {
            result.push_str(&indent_str);
            result.push_str(line);
        }
        result.push('\n');
    }
    result
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
    thresholds: &CachedThresholds,
    icons: &Icons,
    prefix_len: usize,
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
    println!("  ID:       {}", format_full_id(&task.id, prefix_len));

    let eff_priority = promotion::effective_priority(task, thresholds);
    let promoted = eff_priority != task.priority.as_str();
    let priority_colored = match eff_priority {
        "now" => eff_priority.red(),
        _ => eff_priority.normal(),
    };
    if promoted {
        println!("  Priority: {} {}", priority_colored, "(promoted)".dimmed());
    } else {
        println!("  Priority: {}", priority_colored);
    }
    println!("  Size:     {}", task.size);

    // Always show status (priority: done > deleted > work_state > pending)
    let status_colored = if task.done.is_some() {
        "done".blue()
    } else if task.deleted.is_some() {
        "deleted".red()
    } else if let Some(ref work_state) = task.current_work_state {
        match work_state.as_str() {
            "doing" => work_state.green().bold(),
            "stopped" => work_state.yellow(),
            _ => work_state.normal(),
        }
    } else {
        "pending".dimmed()
    };
    println!("  Status:   {}", status_colored);

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

    let je = icons.joy_icon(task.joy);
    let joy_suffix = if je.is_empty() { "" } else { " " };
    println!("  Joy:      {}{}{}", task.joy, joy_suffix, je);

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
        let formatted = format_task_id("ea75a3ac-1234-5678-90ab-cdef12345678", 4, false);
        assert_eq!(formatted, "ea75a3ac");
    }

    #[test]
    fn test_format_task_id_with_color() {
        let formatted = format_task_id("ea75a3ac-1234-5678-90ab-cdef12345678", 4, true);
        assert!(formatted.contains("|"));
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
        let past = Utc::now() - chrono::Duration::days(1);
        let deadline_str = past.to_rfc3339();

        let formatted = format_deadline(Some(&deadline_str), false);

        // Should show something (text content varies with color state)
        assert!(formatted.len() > 1);
        assert_ne!(formatted, "-");
    }

    #[test]
    fn test_format_deadline_overdue_with_color() {
        let past = Utc::now() - chrono::Duration::days(2);
        let deadline_str = past.to_rfc3339();

        let formatted = format_deadline(Some(&deadline_str), false);

        // Overdue deadlines get colored red — output is always non-trivial
        assert!(formatted.len() > 1);
    }
}

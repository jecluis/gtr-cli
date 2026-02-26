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

use unicode_width::UnicodeWidthStr;

use crate::icons::Icons;
use crate::markdown::MarkdownRenderer;
use crate::models::{Document, Namespace, Project, Task};
use crate::promotion;
use crate::threshold_cache::CachedThresholds;

/// A single key-value field in a detail view (e.g. "Priority: high").
pub struct DetailField {
    pub label: String,
    pub value: String,
}

/// A titled list section (e.g. "References:" followed by lines).
pub struct DetailSection {
    pub header: String,
    pub lines: Vec<String>,
}

/// The markdown body of a detail view.
pub struct DetailBody {
    pub header: String,
    pub text: String,
    pub no_wrap: bool,
}

/// Unified detail rendering for tasks and documents.
///
/// Collects title, metadata fields, optional sections, and body into a
/// single structure that renders with consistent formatting and indent
/// support for recursive/nested display.
pub struct DetailView {
    pub title: String,
    pub fields: Vec<DetailField>,
    pub sections: Vec<DetailSection>,
    pub body: Option<DetailBody>,
    pub empty_body_text: String,
    pub no_format: bool,
}

impl DetailView {
    /// Render the detail view to stdout with the given indent prefix.
    ///
    /// Each output line is prefixed with `indent`, and separators/body
    /// wrapping are adjusted to fit within 80 columns minus the indent
    /// width.
    pub fn render(&self, indent: &str) {
        let indent_width = UnicodeWidthStr::width(indent);
        let sep_width = (80usize.saturating_sub(indent_width)).max(40);

        // Blank separator lines carry the indent (for │ continuity)
        let blank = indent.trim_end();

        let sep = "═".repeat(sep_width);
        let thin_sep = "─".repeat(sep_width);

        // Header
        println!("{}", blank);
        println!("{}{}", indent, sep);
        println!("{}{}", indent, self.title.bold().green());
        println!("{}{}", indent, sep);

        // Metadata
        println!("{}", blank);
        println!("{}{}", indent, "Metadata:".bold());
        let max_label_len = self.fields.iter().map(|f| f.label.len()).max().unwrap_or(0);
        for field in &self.fields {
            println!(
                "{}  {:<width$}  {}",
                indent,
                field.label,
                field.value,
                width = max_label_len
            );
        }

        // Sections
        for section in &self.sections {
            println!("{}", blank);
            println!("{}{}", indent, section.header.bold());
            for line in &section.lines {
                println!("{}  {}", indent, line);
            }
        }

        // Body
        let renderer = if self.no_format {
            MarkdownRenderer::with_override(Some(false))
        } else {
            MarkdownRenderer::with_override(None)
        };

        match &self.body {
            Some(body) if !body.text.is_empty() => {
                println!("{}", blank);
                println!("{}{}", indent, body.header.bold());
                println!("{}{}", indent, thin_sep);
                let rendered = if body.no_wrap {
                    renderer.render_no_wrap(&body.text)
                } else {
                    renderer.render_at_width(&body.text, sep_width)
                };
                for line in rendered.lines() {
                    println!("{}{}", indent, line);
                }
            }
            _ => {
                println!("{}", blank);
                println!("{}{}", indent, self.empty_body_text.italic().dimmed());
            }
        }

        // Footer
        println!("{}{}", indent, sep);
        println!("{}", blank);
    }
}

/// Map from project_id to its ancestor chain (root-first).
///
/// Example: `"dev" -> ["home", "dev"]` means "dev" is a child of "home".
pub type ProjectPaths = HashMap<String, Vec<String>>;

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
    let Some(ds) = deadline_str else {
        return "-".to_string();
    };

    if absolute_dates {
        // Force absolute date regardless of distance
        let Ok(deadline) = DateTime::parse_from_rfc3339(ds) else {
            return "-".to_string();
        };
        let local = deadline.with_timezone(&Local);
        let formatted = local.format("%Y-%m-%d").to_string();
        if deadline < Utc::now() {
            return formatted.red().to_string();
        }
        return formatted;
    }

    match crate::display::format_deadline_relative(deadline_str) {
        Some(d) if d.is_overdue => d.text.red().to_string(),
        Some(d) => d.text,
        None => "-".to_string(),
    }
}

/// Format task ID with colored prefix and separator for list views.
///
/// When `colorize` is true, formats as: `prefix|suffix` where prefix is cyan
/// and suffix is dimmed. Otherwise returns plain shortened ID.
pub fn format_task_id(id: &str, prefix_len: usize, colorize: bool) -> String {
    let (prefix, suffix) = crate::display::split_id(id, prefix_len);

    if colorize {
        format!("{}|{}", prefix.cyan(), suffix.dimmed())
    } else {
        format!("{prefix}{suffix}")
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

/// Format a project path as a multi-line tree cell for task tables.
///
/// For a path `["home", "dev"]`, renders:
/// ```text
/// home
/// └ dev
/// ```
///
/// For a single-element path, returns the project name from the path.
fn format_project_cell(
    project_id: &str,
    project_paths: &ProjectPaths,
    color: Option<colored::Color>,
) -> String {
    let path = match project_paths.get(project_id) {
        Some(p) if p.len() > 1 => p,
        Some(p) if !p.is_empty() => {
            // Single-element path — use the name from path
            let name = &p[0];
            return if let Some(c) = color {
                name.color(c).to_string()
            } else {
                name.clone()
            };
        }
        _ => {
            // Not in map — fall back to raw project_id
            return if let Some(c) = color {
                project_id.color(c).to_string()
            } else {
                project_id.to_string()
            };
        }
    };

    let mut lines = Vec::with_capacity(path.len());
    for (i, segment) in path.iter().enumerate() {
        let is_last = i == path.len() - 1;
        let colored_seg = if let Some(c) = color {
            segment.color(c).to_string()
        } else {
            segment.clone()
        };

        if i == 0 {
            lines.push(colored_seg);
        } else {
            let connector = if is_last { "└ " } else { "├ " };
            let indent = "  ".repeat(i.saturating_sub(1));
            lines.push(format!("{}{}{}", indent, connector.dimmed(), colored_seg));
        }
    }

    lines.join("\n")
}

/// Print a list of projects as a tree showing parent-child relationships.
pub fn print_projects(projects: &[Project]) {
    print_projects_with_links(projects, None, None);
}

/// Print projects with optional namespace link info.
pub fn print_projects_with_links(
    projects: &[Project],
    links: Option<&HashMap<String, Vec<String>>>,
    icons: Option<&Icons>,
) {
    if projects.is_empty() {
        println!("{}", "No projects found".yellow());
        return;
    }

    // Build lookup: parent_id -> children, and track which IDs exist
    let mut children_map: HashMap<Option<&str>, Vec<&Project>> = HashMap::new();
    for p in projects {
        children_map
            .entry(p.parent_id.as_deref())
            .or_default()
            .push(p);
    }

    // Sort each group alphabetically by name
    for group in children_map.values_mut() {
        group.sort_by(|a, b| a.name.cmp(&b.name));
    }

    // Print tree starting from roots (parent_id = None)
    let roots = children_map.get(&None).cloned().unwrap_or_default();

    // Also collect orphans whose parent_id points to a project not in the list
    let known_ids: HashSet<&str> = projects.iter().map(|p| p.id.as_str()).collect();
    let mut orphans: Vec<&Project> = Vec::new();
    for p in projects {
        if let Some(ref pid) = p.parent_id
            && !known_ids.contains(pid.as_str())
        {
            orphans.push(p);
        }
    }

    for (i, root) in roots.iter().enumerate() {
        let is_last = i == roots.len() - 1 && orphans.is_empty();
        print_project_node(root, "", is_last, true, &children_map, links, icons);
    }

    // Print orphans (parent not in list) at root level
    for (i, orphan) in orphans.iter().enumerate() {
        let is_last = i == orphans.len() - 1;
        print_project_node(orphan, "", is_last, true, &children_map, links, icons);
    }

    println!("\n{} {}", "Total:".bold(), projects.len());
}

/// Recursively print a project node with tree connectors.
fn print_project_node(
    project: &Project,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    children_map: &HashMap<Option<&str>, Vec<&Project>>,
    links: Option<&HashMap<String, Vec<String>>>,
    icons: Option<&Icons>,
) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let desc = project
        .description
        .as_deref()
        .map(|d| format!(" - {}", d.dimmed()))
        .unwrap_or_default();

    let display_name = if project.name.is_empty() {
        &project.id
    } else {
        &project.name
    };

    let link_glyph = icons.map(|i| &i.link as &str).unwrap_or("\u{1f517}");
    let link_tag = links
        .and_then(|m| m.get(&project.id))
        .filter(|names| !names.is_empty())
        .map(|names| format!("  {} {}", link_glyph, names.join(", ").yellow()))
        .unwrap_or_default();

    println!(
        "{}{}{}{}{}",
        prefix,
        connector,
        display_name.cyan().bold(),
        desc,
        link_tag
    );

    // Recurse into children
    let children = children_map
        .get(&Some(project.id.as_str()))
        .cloned()
        .unwrap_or_default();

    let child_prefix = if is_root {
        "  ".to_string()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    for (i, child) in children.iter().enumerate() {
        let child_is_last = i == children.len() - 1;
        print_project_node(
            child,
            &child_prefix,
            child_is_last,
            false,
            children_map,
            links,
            icons,
        );
    }
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
/// `project_paths` provides ancestor chains for rendering hierarchical project
/// columns; pass an empty map to show flat project IDs.
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
    project_paths: &ProjectPaths,
    with_labels: bool,
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
        project_paths,
        with_labels,
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
    project_paths: &ProjectPaths,
    with_labels: bool,
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

    // Build label color mapping (order of first appearance across tasks)
    let label_palette = [
        colored::Color::Cyan,
        colored::Color::Yellow,
        colored::Color::Green,
        colored::Color::Magenta,
        colored::Color::Blue,
        colored::Color::Red,
        colored::Color::BrightCyan,
        colored::Color::BrightYellow,
        colored::Color::BrightGreen,
        colored::Color::BrightMagenta,
        colored::Color::BrightBlue,
        colored::Color::BrightRed,
    ];
    let label_colors: HashMap<&str, colored::Color> = if with_labels {
        crate::display::assign_label_colors(tasks.iter().map(|t| t.labels.as_slice()))
            .into_iter()
            .map(|(label, idx)| (label, label_palette[idx]))
            .collect()
    } else {
        HashMap::new()
    };

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
            project_paths,
            with_labels,
            &label_colors,
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
            project_paths,
            with_labels,
            &label_colors,
        );
    }
}

/// Compute subtask counts from an iterator of optional parent IDs.
///
/// Returns a map from parent task ID to the number of direct children.
pub fn compute_subtask_counts<'a>(
    parent_ids: impl Iterator<Item = Option<&'a str>>,
) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for parent_id in parent_ids.flatten() {
        *counts.entry(parent_id.to_string()).or_default() += 1;
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

    // Impact icon prefix: spacing depends on theme
    let impact_prefix = match crate::display::impact_level(task.impact) {
        crate::display::ImpactLevel::Critical => &icons.impact_critical,
        crate::display::ImpactLevel::Significant => &icons.impact_significant,
        crate::display::ImpactLevel::Normal => &icons.impact_none,
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

    // Build title with correct order: [urgency][joy][bookmark]title
    let urgency = if task.done.is_none() && task.deleted.is_none() {
        crate::display::deadline_urgency(
            task.deadline.as_deref(),
            &task.size,
            task.impact,
            thresholds,
        )
    } else {
        crate::display::DeadlineUrgency::None
    };

    let mut title_prefix = String::new();

    // Urgency glyph first
    match urgency {
        crate::display::DeadlineUrgency::Overdue => {
            title_prefix.push_str(&icons.overdue);
        }
        crate::display::DeadlineUrgency::Warning => {
            title_prefix.push_str(&icons.deadline_warning);
        }
        crate::display::DeadlineUrgency::None => {}
    }

    // Joy icon
    let je = icons.joy_icon(task.joy);
    if !je.is_empty() {
        title_prefix.push_str(&format!("{je} "));
    }

    // Bookmark + title (via display_title)
    let display = task.display_title(icons);

    // Build hierarchy subtitle line (below the title)
    let subtitle = build_hierarchy_subtitle(task, prefix_len, colorize, icons, subtask_counts);

    let title_line = format!("{title_prefix}{display}");
    // Apply warning colorization to the entire title line
    let title_line = match urgency {
        crate::display::DeadlineUrgency::Warning => title_line.yellow().to_string(),
        _ => title_line,
    };

    let title = if subtitle.is_empty() {
        title_line
    } else {
        format!("{title_line}\n{subtitle}")
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
    project_paths: &ProjectPaths,
    with_labels: bool,
    label_colors: &HashMap<&str, colored::Color>,
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
    let subtask_counts = compute_subtask_counts(tasks.iter().map(|t| t.parent_id.as_deref()));
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

        let title_with_labels = if with_labels && !task.labels.is_empty() {
            let label_line = format!(
                "  {} {}",
                icons.label,
                task.labels
                    .iter()
                    .map(|l| {
                        let c = label_colors
                            .get(l.as_str())
                            .copied()
                            .unwrap_or(colored::Color::White);
                        l.color(c).to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            format!("{}\n{}", row.title, label_line)
        } else {
            row.title
        };
        let mut record: Vec<String> = vec![row.id, title_with_labels];
        if columns.show_project {
            let color = project_colors.get(task.project_id.as_str()).copied();
            record.push(format_project_cell(&task.project_id, project_paths, color));
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
    project_paths: &ProjectPaths,
    with_labels: bool,
    label_colors: &HashMap<&str, colored::Color>,
) {
    let colorize = colored::control::SHOULD_COLORIZE.should_colorize();
    let subtask_counts = compute_subtask_counts(tasks.iter().map(|t| t.parent_id.as_deref()));
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

        // Line 1: ID - PROJECT (leaf name, colored) - STATUS
        let color = project_colors.get(task.project_id.as_str()).copied();
        let project_name = project_paths
            .get(&task.project_id)
            .and_then(|p| p.last())
            .map(|s| s.as_str())
            .unwrap_or(&task.project_id);
        let project_colored = if let Some(c) = color {
            project_name.color(c).to_string()
        } else {
            project_name.to_string()
        };
        println!("{} - {} - {}", row.id, project_colored, row.status);

        // Show project ancestry (if any) on a dimmed line
        if let Some(path) = project_paths.get(&task.project_id)
            && path.len() > 1
        {
            let ancestors: Vec<_> = path[..path.len() - 1].to_vec();
            println!("  {}", format!("in {}", ancestors.join(" › ")).dimmed());
        }

        // Line 2: TITLE (wrapped at 60 columns)
        let wrapped_title = wrap_text(&task.display_title(icons), 60);
        for line in wrapped_title {
            println!("{}", line);
        }

        // Labels line (if --with-labels and task has labels)
        if with_labels && !task.labels.is_empty() {
            println!(
                "  {} {}",
                icons.label,
                task.labels
                    .iter()
                    .map(|l| {
                        let c = label_colors
                            .get(l.as_str())
                            .copied()
                            .unwrap_or(colored::Color::White);
                        l.color(c).to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            );
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
/// `indent` is prepended to every output line (used for nested/recursive
/// display).
#[allow(clippy::too_many_arguments)]
pub fn print_task_details(
    config: &crate::config::Config,
    task: &Task,
    no_format: bool,
    no_wrap: bool,
    thresholds: &CachedThresholds,
    icons: &Icons,
    prefix_len: usize,
    project_paths: &ProjectPaths,
    indent: &str,
) {
    let mut fields = Vec::new();

    // ID
    fields.push(DetailField {
        label: "ID:".into(),
        value: format_full_id(&task.id, prefix_len),
    });

    // Project with ancestry breadcrumb
    let project_value = if let Some(path) = project_paths.get(&task.project_id) {
        if path.len() > 1 {
            let breadcrumb: Vec<_> = path
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i == path.len() - 1 {
                        s.cyan().bold().to_string()
                    } else {
                        s.dimmed().to_string()
                    }
                })
                .collect();
            breadcrumb.join(&" › ".dimmed().to_string())
        } else {
            path[0].cyan().to_string()
        }
    } else {
        task.project_id.cyan().to_string()
    };
    fields.push(DetailField {
        label: "Project:".into(),
        value: project_value,
    });

    // Priority
    let eff_priority = promotion::effective_priority(task, thresholds);
    let promoted = eff_priority != task.priority.as_str();
    let priority_colored = match eff_priority {
        "now" => eff_priority.red(),
        _ => eff_priority.normal(),
    };
    fields.push(DetailField {
        label: "Priority:".into(),
        value: if promoted {
            format!("{} {}", priority_colored, "(promoted)".dimmed())
        } else {
            priority_colored.to_string()
        },
    });

    // Size
    fields.push(DetailField {
        label: "Size:".into(),
        value: task.size.clone(),
    });

    // Status
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
    fields.push(DetailField {
        label: "Status:".into(),
        value: status_colored.to_string(),
    });

    // Created
    if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&task.created) {
        let created_time = created.with_timezone(&Local);
        fields.push(DetailField {
            label: "Created:".into(),
            value: created_time.format("%Y-%m-%d %H:%M:%S").to_string(),
        });
    }

    // Modified
    if let Ok(modified) = chrono::DateTime::parse_from_rfc3339(&task.modified) {
        let modified_time = modified.with_timezone(&Local);
        fields.push(DetailField {
            label: "Modified:".into(),
            value: modified_time.format("%Y-%m-%d %H:%M:%S").to_string(),
        });
    }

    // Deadline (conditional)
    if let Some(ref deadline_str) = task.deadline
        && let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str)
    {
        let deadline_time = deadline.with_timezone(&Local);
        let now = chrono::Utc::now();
        let is_overdue = deadline < now;
        let formatted = deadline_time.format("%Y-%m-%d %H:%M:%S").to_string();
        fields.push(DetailField {
            label: "Deadline:".into(),
            value: if is_overdue {
                formatted.red().bold().to_string()
            } else {
                formatted
            },
        });
    }

    // Done (conditional)
    if let Some(ref done_str) = task.done
        && let Ok(done) = chrono::DateTime::parse_from_rfc3339(done_str)
    {
        let done_time = done.with_timezone(&Local);
        fields.push(DetailField {
            label: "Done:".into(),
            value: done_time
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .blue()
                .to_string(),
        });
    }

    // Deleted (conditional)
    if let Some(ref deleted_str) = task.deleted
        && let Ok(deleted) = chrono::DateTime::parse_from_rfc3339(deleted_str)
    {
        let deleted_time = deleted.with_timezone(&Local);
        fields.push(DetailField {
            label: "Deleted:".into(),
            value: deleted_time
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .red()
                .to_string(),
        });
    }

    // Impact
    let impact_label = crate::threshold_cache::read_cache(config)
        .and_then(|cached| cached.impact_labels.get(&task.impact.to_string()).cloned())
        .or_else(|| {
            crate::utils::default_impact_labels()
                .get(&task.impact.to_string())
                .cloned()
        })
        .unwrap_or_else(|| "Unknown".to_string());
    fields.push(DetailField {
        label: "Impact:".into(),
        value: format!("{} ({})", impact_label, task.impact),
    });

    // Joy
    let je = icons.joy_icon(task.joy);
    let joy_suffix = if je.is_empty() { "" } else { " " };
    fields.push(DetailField {
        label: "Joy:".into(),
        value: format!("{}{}{}", task.joy, joy_suffix, je),
    });

    // Progress (conditional)
    if let Some(progress) = task.progress {
        fields.push(DetailField {
            label: "Progress:".into(),
            value: format!("{}%", progress),
        });
    }

    // Version
    fields.push(DetailField {
        label: "Version:".into(),
        value: task.version.to_string(),
    });

    // Labels (conditional)
    if !task.labels.is_empty() {
        let label_strs: Vec<String> = task.labels.iter().map(|l| l.cyan().to_string()).collect();
        fields.push(DetailField {
            label: "Labels:".into(),
            value: label_strs.join(", "),
        });
    }

    let view = DetailView {
        title: task.display_title(icons),
        fields,
        sections: Vec::new(),
        body: if task.body.is_empty() {
            None
        } else {
            Some(DetailBody {
                header: "Description:".into(),
                text: task.body.clone(),
                no_wrap,
            })
        },
        empty_body_text: "(No description)".into(),
        no_format,
    };

    view.render(indent);
}

/// Print a list of documents in a compact format.
///
/// Shows each document with prefix-highlighted short ID, title, modification
/// time, and optionally labels and namespace. If `show_namespace` is true,
/// the namespace path is shown for each document. `ns_names` maps namespace
/// UUIDs to display paths.
/// Print documents grouped under their namespace hierarchy as a tree.
///
/// Used when listing documents across all namespaces (`gtr doc list`
/// without `--namespace`). Namespaces form the top-level structure with
/// documents nested inside. Child documents branch off their parents.
pub fn print_document_tree(
    namespaces: &[Namespace],
    docs: &[Document],
    icons: &Icons,
    with_labels: bool,
    prefix_len: usize,
) {
    if namespaces.is_empty() && docs.is_empty() {
        println!("{}", format!("{} No documents found.", icons.info).dimmed());
        return;
    }

    let colorize = colored::control::SHOULD_COLORIZE.should_colorize();

    // Build namespace parent -> children map
    let mut ns_children_map: HashMap<Option<&str>, Vec<&Namespace>> = HashMap::new();
    for ns in namespaces {
        ns_children_map
            .entry(ns.parent_id.as_deref())
            .or_default()
            .push(ns);
    }
    for group in ns_children_map.values_mut() {
        group.sort_by(|a, b| a.name.cmp(&b.name));
    }

    // Build doc by namespace map
    let mut doc_by_ns: HashMap<&str, Vec<&Document>> = HashMap::new();
    for doc in docs {
        doc_by_ns
            .entry(doc.namespace_id.as_str())
            .or_default()
            .push(doc);
    }

    // Build doc parent -> children map
    let doc_ids: HashSet<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let mut doc_children_map: HashMap<Option<&str>, Vec<&Document>> = HashMap::new();
    for doc in docs {
        let key = match doc.parent_id.as_deref() {
            Some(pid) if doc_ids.contains(pid) => Some(pid),
            _ => None,
        };
        doc_children_map.entry(key).or_default().push(doc);
    }
    for group in doc_children_map.values_mut() {
        group.sort_by(|a, b| a.title.cmp(&b.title));
    }

    // Label color palette
    let label_colors = build_doc_label_colors(docs, with_labels);

    // Find roots and orphans
    let roots = ns_children_map.get(&None).cloned().unwrap_or_default();
    let known_ids: HashSet<&str> = namespaces.iter().map(|ns| ns.id.as_str()).collect();
    let mut orphans: Vec<&Namespace> = Vec::new();
    for ns in namespaces {
        if let Some(ref pid) = ns.parent_id
            && !known_ids.contains(pid.as_str())
            && !roots.iter().any(|r| r.id == ns.id)
        {
            orphans.push(ns);
        }
    }

    let total_top = roots.len() + orphans.len();
    for (i, root) in roots.iter().enumerate() {
        let is_last = i == roots.len() - 1 && orphans.is_empty();
        print_ns_doc_tree_node(
            root,
            "",
            is_last,
            total_top == 1,
            &ns_children_map,
            &doc_by_ns,
            &doc_children_map,
            icons,
            with_labels,
            prefix_len,
            colorize,
            &label_colors,
        );
    }
    for (i, orphan) in orphans.iter().enumerate() {
        let is_last = i == orphans.len() - 1;
        print_ns_doc_tree_node(
            orphan,
            "",
            is_last,
            total_top == 1,
            &ns_children_map,
            &doc_by_ns,
            &doc_children_map,
            icons,
            with_labels,
            prefix_len,
            colorize,
            &label_colors,
        );
    }

    let ns_count = namespaces.iter().filter(|ns| !ns.is_deleted()).count();
    println!(
        "\n{} {} across {} {}",
        "Total:".bold(),
        docs.len(),
        ns_count,
        if ns_count == 1 {
            "namespace"
        } else {
            "namespaces"
        }
    );
}

/// Recursively print a namespace node containing documents.
#[allow(clippy::too_many_arguments)]
fn print_ns_doc_tree_node(
    ns: &Namespace,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    ns_children_map: &HashMap<Option<&str>, Vec<&Namespace>>,
    doc_by_ns: &HashMap<&str, Vec<&Document>>,
    doc_children_map: &HashMap<Option<&str>, Vec<&Document>>,
    icons: &Icons,
    with_labels: bool,
    prefix_len: usize,
    colorize: bool,
    label_colors: &HashMap<&str, colored::Color>,
) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let deleted_tag = if ns.is_deleted() {
        " [deleted]".red().to_string()
    } else {
        String::new()
    };

    println!(
        "{}{}{}{}",
        prefix,
        connector,
        ns.name.cyan().bold(),
        deleted_tag
    );

    let child_prefix = if is_root {
        "  ".to_string()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    // Root docs for this namespace (parent_id absent or orphaned within
    // the filtered set)
    let root_docs: Vec<&&Document> = doc_by_ns
        .get(ns.id.as_str())
        .map(|ds| {
            ds.iter()
                .filter(|d| {
                    d.parent_id.is_none()
                        || !doc_by_ns
                            .get(ns.id.as_str())
                            .map(|all| {
                                all.iter()
                                    .any(|o| Some(o.id.as_str()) == d.parent_id.as_deref())
                            })
                            .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();

    let child_nses = ns_children_map
        .get(&Some(ns.id.as_str()))
        .cloned()
        .unwrap_or_default();

    for (i, doc) in root_docs.iter().enumerate() {
        let is_last_child = i == root_docs.len() - 1 && child_nses.is_empty();
        print_doc_tree_node(
            doc,
            &child_prefix,
            is_last_child,
            false,
            doc_children_map,
            icons,
            with_labels,
            prefix_len,
            colorize,
            label_colors,
        );
    }

    for (i, child_ns) in child_nses.iter().enumerate() {
        let is_last_child = i == child_nses.len() - 1;
        print_ns_doc_tree_node(
            child_ns,
            &child_prefix,
            is_last_child,
            false,
            ns_children_map,
            doc_by_ns,
            doc_children_map,
            icons,
            with_labels,
            prefix_len,
            colorize,
            label_colors,
        );
    }
}

/// Print documents as a flat-root tree (single namespace mode).
///
/// Used when `--namespace` is specified. Root documents are top-level
/// entries with child documents branching off their parents.
pub fn print_documents_as_tree(
    docs: &[Document],
    icons: &Icons,
    with_labels: bool,
    prefix_len: usize,
) {
    if docs.is_empty() {
        println!("{}", format!("{} No documents found.", icons.info).dimmed());
        return;
    }

    let colorize = colored::control::SHOULD_COLORIZE.should_colorize();

    let doc_ids: HashSet<&str> = docs.iter().map(|d| d.id.as_str()).collect();
    let mut doc_children_map: HashMap<Option<&str>, Vec<&Document>> = HashMap::new();
    for doc in docs {
        let key = match doc.parent_id.as_deref() {
            Some(pid) if doc_ids.contains(pid) => Some(pid),
            _ => None,
        };
        doc_children_map.entry(key).or_default().push(doc);
    }
    for group in doc_children_map.values_mut() {
        group.sort_by(|a, b| a.title.cmp(&b.title));
    }

    let label_colors = build_doc_label_colors(docs, with_labels);

    let roots = doc_children_map.get(&None).cloned().unwrap_or_default();
    for (i, doc) in roots.iter().enumerate() {
        let is_last = i == roots.len() - 1;
        print_doc_tree_node(
            doc,
            "",
            is_last,
            true,
            &doc_children_map,
            icons,
            with_labels,
            prefix_len,
            colorize,
            &label_colors,
        );
    }

    println!("\n{} {}", "Total:".bold(), docs.len());
}

/// Recursively print a document node with tree connectors.
#[allow(clippy::too_many_arguments)]
fn print_doc_tree_node(
    doc: &Document,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    doc_children_map: &HashMap<Option<&str>, Vec<&Document>>,
    icons: &Icons,
    with_labels: bool,
    prefix_len: usize,
    colorize: bool,
    label_colors: &HashMap<&str, colored::Color>,
) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let short_id = format_task_id(&doc.id, prefix_len, colorize);
    let modified_rel = chrono::DateTime::parse_from_rfc3339(&doc.modified)
        .map(|dt| {
            let ht = HumanTime::from(dt);
            ht.to_text_en(Accuracy::Rough, Tense::Past)
        })
        .unwrap_or_else(|_| "-".to_string());

    let deleted_tag = if doc.is_deleted() {
        " [deleted]".red().to_string()
    } else {
        String::new()
    };

    let label_tag = if with_labels && !doc.labels.is_empty() {
        let label_strs: Vec<String> = doc
            .labels
            .iter()
            .map(|l| {
                if let Some(&color) = label_colors.get(l.as_str()) {
                    l.color(color).to_string()
                } else {
                    l.cyan().to_string()
                }
            })
            .collect();
        format!("  {} {}", icons.label, label_strs.join(", "))
    } else {
        String::new()
    };

    let slug_tag = if !doc.slug.is_empty() {
        format!("  {}", doc.slug.dimmed())
    } else {
        String::new()
    };

    println!(
        "{}{}{} {}{}{}  ({}){}",
        prefix,
        connector,
        short_id,
        doc.title,
        slug_tag,
        deleted_tag,
        modified_rel.dimmed(),
        label_tag,
    );

    let child_prefix = if is_root {
        format!("{}  ", prefix)
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    let children = doc_children_map
        .get(&Some(doc.id.as_str()))
        .cloned()
        .unwrap_or_default();

    for (i, child) in children.iter().enumerate() {
        let child_is_last = i == children.len() - 1;
        print_doc_tree_node(
            child,
            &child_prefix,
            child_is_last,
            false,
            doc_children_map,
            icons,
            with_labels,
            prefix_len,
            colorize,
            label_colors,
        );
    }
}

/// Build label -> color mapping for documents.
fn build_doc_label_colors(docs: &[Document], with_labels: bool) -> HashMap<&str, colored::Color> {
    let label_palette = [
        colored::Color::Cyan,
        colored::Color::Yellow,
        colored::Color::Green,
        colored::Color::Magenta,
        colored::Color::Blue,
        colored::Color::Red,
        colored::Color::BrightCyan,
        colored::Color::BrightYellow,
        colored::Color::BrightGreen,
        colored::Color::BrightMagenta,
        colored::Color::BrightBlue,
        colored::Color::BrightRed,
    ];
    let mut map = HashMap::new();
    if with_labels {
        for doc in docs {
            for label in &doc.labels {
                if !map.contains_key(label.as_str()) {
                    let idx = map.len();
                    map.insert(label.as_str(), label_palette[idx % label_palette.len()]);
                }
            }
        }
    }
    map
}

/// Print full details for a single document.
///
/// Shows title as header, metadata block, references, and content with
/// optional markdown rendering. If `no_format` is true, content is shown
/// as plain text. `indent` is prepended to every output line.
pub fn print_document_detail(
    doc: &Document,
    icons: &Icons,
    no_format: bool,
    no_wrap: bool,
    prefix_len: usize,
    ns_display: &str,
    indent: &str,
) {
    let _ = icons; // reserved for future glyph use

    let mut fields = Vec::new();

    fields.push(DetailField {
        label: "ID:".into(),
        value: format_full_id(&doc.id, prefix_len),
    });
    if !doc.slug.is_empty() {
        fields.push(DetailField {
            label: "Slug:".into(),
            value: doc.slug.cyan().to_string(),
        });
    }
    if !doc.slug_aliases.is_empty() {
        fields.push(DetailField {
            label: "Aliases:".into(),
            value: doc.slug_aliases.join(", ").dimmed().to_string(),
        });
    }
    fields.push(DetailField {
        label: "Namespace:".into(),
        value: format!(
            "{} {}",
            ns_display.cyan().bold(),
            format_task_id(&doc.namespace_id, prefix_len, true).dimmed()
        ),
    });

    if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&doc.created) {
        let local = created.with_timezone(&Local);
        fields.push(DetailField {
            label: "Created:".into(),
            value: local.format("%Y-%m-%d %H:%M:%S").to_string(),
        });
    }
    if let Ok(modified) = chrono::DateTime::parse_from_rfc3339(&doc.modified) {
        let local = modified.with_timezone(&Local);
        fields.push(DetailField {
            label: "Modified:".into(),
            value: local.format("%Y-%m-%d %H:%M:%S").to_string(),
        });
    }
    if let Some(ref deleted) = doc.deleted
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(deleted)
    {
        let local = dt.with_timezone(&Local);
        fields.push(DetailField {
            label: "Deleted:".into(),
            value: local
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
                .red()
                .to_string(),
        });
    }
    if let Some(ref pid) = doc.parent_id {
        fields.push(DetailField {
            label: "Parent:".into(),
            value: format_full_id(pid, prefix_len),
        });
    }
    fields.push(DetailField {
        label: "Version:".into(),
        value: doc.version.to_string(),
    });
    if !doc.labels.is_empty() {
        let label_strs: Vec<String> = doc.labels.iter().map(|l| l.cyan().to_string()).collect();
        fields.push(DetailField {
            label: "Labels:".into(),
            value: label_strs.join(", "),
        });
    }

    let mut sections = Vec::new();
    if !doc.references.is_empty() {
        let lines = doc
            .references
            .iter()
            .map(|r| {
                format!(
                    "{} {} ({})",
                    r.ref_type.dimmed(),
                    r.target_id.cyan(),
                    r.target_type
                )
            })
            .collect();
        sections.push(DetailSection {
            header: "References:".into(),
            lines,
        });
    }

    let view = DetailView {
        title: doc.title.clone(),
        fields,
        sections,
        body: if doc.content.is_empty() {
            None
        } else {
            Some(DetailBody {
                header: "Content:".into(),
                text: doc.content.clone(),
                no_wrap,
            })
        },
        empty_body_text: "(No content)".into(),
        no_format,
    };

    view.render(indent);
}

/// Print namespaces as a tree showing parent-child relationships.
///
/// Uses the same tree-connector style as `print_projects()`.
pub fn print_namespaces(namespaces: &[Namespace]) {
    print_namespaces_with_links(namespaces, None, None);
}

/// Print namespaces with optional project link info.
pub fn print_namespaces_with_links(
    namespaces: &[Namespace],
    links: Option<&HashMap<String, Vec<String>>>,
    icons: Option<&Icons>,
) {
    if namespaces.is_empty() {
        println!("{}", "No namespaces found".yellow());
        return;
    }

    // Build lookup: parent_id -> children
    let mut children_map: HashMap<Option<&str>, Vec<&Namespace>> = HashMap::new();
    for ns in namespaces {
        children_map
            .entry(ns.parent_id.as_deref())
            .or_default()
            .push(ns);
    }

    // Sort each group alphabetically
    for group in children_map.values_mut() {
        group.sort_by(|a, b| a.name.cmp(&b.name));
    }

    // Roots (parent_id = None)
    let roots = children_map.get(&None).cloned().unwrap_or_default();

    // Orphans whose parent_id points to a namespace not in the list
    let known_ids: HashSet<&str> = namespaces.iter().map(|ns| ns.id.as_str()).collect();
    let mut orphans: Vec<&Namespace> = Vec::new();
    for ns in namespaces {
        if let Some(ref pid) = ns.parent_id
            && !known_ids.contains(pid.as_str())
            && !roots.iter().any(|r| r.id == ns.id)
        {
            orphans.push(ns);
        }
    }

    for (i, root) in roots.iter().enumerate() {
        let is_last = i == roots.len() - 1 && orphans.is_empty();
        print_namespace_tree_node(root, "", is_last, true, &children_map, links, icons);
    }

    for (i, orphan) in orphans.iter().enumerate() {
        let is_last = i == orphans.len() - 1;
        print_namespace_tree_node(orphan, "", is_last, true, &children_map, links, icons);
    }

    println!("\n{} {}", "Total:".bold(), namespaces.len());
}

/// Recursively print a namespace node with tree connectors.
fn print_namespace_tree_node(
    ns: &Namespace,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    children_map: &HashMap<Option<&str>, Vec<&Namespace>>,
    links: Option<&HashMap<String, Vec<String>>>,
    icons: Option<&Icons>,
) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let desc = ns
        .description
        .as_deref()
        .map(|d| format!(" - {}", d.dimmed()))
        .unwrap_or_default();

    let deleted_tag = if ns.is_deleted() {
        " [deleted]".red().to_string()
    } else {
        String::new()
    };

    let link_glyph = icons.map(|i| &i.link as &str).unwrap_or("\u{1f517}");
    let link_tag = links
        .and_then(|m| m.get(&ns.id))
        .filter(|names| !names.is_empty())
        .map(|names| format!("  {} {}", link_glyph, names.join(", ").yellow()))
        .unwrap_or_default();

    println!(
        "{}{}{}{}{}{}",
        prefix,
        connector,
        ns.name.cyan().bold(),
        desc,
        deleted_tag,
        link_tag
    );

    let children = children_map
        .get(&Some(ns.id.as_str()))
        .cloned()
        .unwrap_or_default();

    let child_prefix = if is_root {
        "  ".to_string()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    for (i, child) in children.iter().enumerate() {
        let child_is_last = i == children.len() - 1;
        print_namespace_tree_node(
            child,
            &child_prefix,
            child_is_last,
            false,
            children_map,
            links,
            icons,
        );
    }
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

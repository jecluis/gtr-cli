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

use chrono::Local;
use colored::Colorize;
use tabled::settings::{Alignment, Modify, Style, object::Columns};
use tabled::{Table, Tabled};

use crate::markdown::MarkdownRenderer;
use crate::models::{Project, Task};

/// Calculate minimum unique prefix length for task IDs.
///
/// Uses optimized O(N log N) algorithm by sorting IDs and comparing adjacent pairs.
/// Always returns at least 2 to avoid single-character typos.
fn find_min_unique_prefix_len(task_ids: &[String]) -> usize {
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
    #[tabled(rename = "Modified")]
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
    #[tabled(rename = "Modified")]
    modified: String,
    #[tabled(rename = "Deadline")]
    deadline: String,
    #[tabled(rename = "Status")]
    status: String,
}

/// Print tasks grouped by work state (doing vs others).
pub fn print_tasks_grouped(doing_tasks: &[Task], other_tasks: &[Task]) {
    if doing_tasks.is_empty() && other_tasks.is_empty() {
        println!("{}", "No tasks found".yellow());
        return;
    }

    if !doing_tasks.is_empty() {
        println!("\n{}", "═══ DOING ═══".bold().cyan());
        print_task_table(doing_tasks);
    }

    if !other_tasks.is_empty() {
        if !doing_tasks.is_empty() {
            println!("\n{}", "═══ TASKS ═══".bold());
        }
        print_task_table(other_tasks);
    }

    let total = doing_tasks.len() + other_tasks.len();
    println!("\n{} {}", "Total:".bold(), total);
}

/// Print a list of tasks in table format.
pub fn print_tasks(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("{}", "No tasks found".yellow());
        return;
    }
    print_task_table(tasks);
    println!("\n{} {}", "Total:".bold(), tasks.len());
}

/// Internal function to print task table.
fn print_task_table(tasks: &[Task]) {
    // Check if tasks are from multiple projects
    let unique_projects: HashSet<&str> = tasks.iter().map(|t| t.project_id.as_str()).collect();
    let show_project = unique_projects.len() > 1;

    if show_project {
        print_task_table_with_project(tasks, unique_projects);
    } else {
        print_task_table_simple(tasks);
    }
}

/// Print task table without project column.
fn print_task_table_simple(tasks: &[Task]) {
    // Calculate minimum unique prefix length for all task IDs
    let task_ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    let prefix_len = find_min_unique_prefix_len(&task_ids);

    let rows: Vec<TaskRow> = tasks
        .iter()
        .map(|task| {
            let modified = chrono::DateTime::parse_from_rfc3339(&task.modified)
                .unwrap()
                .with_timezone(&Local);
            let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();

            let priority_colored = match task.priority.as_str() {
                "now" => task.priority.red().to_string(),
                _ => task.priority.to_string(),
            };

            let deadline_str = if let Some(ref deadline_str) = task.deadline {
                if let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str) {
                    let deadline_time = deadline.with_timezone(&Local);
                    let now = chrono::Utc::now();
                    let formatted = deadline_time.format("%Y-%m-%d").to_string();

                    if deadline < now {
                        formatted.red().to_string()
                    } else {
                        formatted
                    }
                } else {
                    "-".to_string()
                }
            } else {
                "-".to_string()
            };

            let status = if task.is_deleted() {
                "DELETED".red().to_string()
            } else if task.is_done() {
                "done".blue().to_string()
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
        })
        .collect();

    let mut binding = Table::new(rows);
    let table = binding
        .with(Style::rounded())
        .with(Modify::new(Columns::new(0..1)).with(Alignment::center())) // ID
        .with(Modify::new(Columns::new(2..7)).with(Alignment::center())); // Priority, Size, Modified, Deadline, Status

    println!("{}", table);
}

/// Print task table with project column.
fn print_task_table_with_project(tasks: &[Task], unique_projects: HashSet<&str>) {
    // Calculate minimum unique prefix length for all task IDs
    let task_ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    let prefix_len = find_min_unique_prefix_len(&task_ids);

    // Generate colors for each project (cycle through a set of colors)
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

    let rows: Vec<TaskRowWithProject> = tasks
        .iter()
        .map(|task| {
            let modified = chrono::DateTime::parse_from_rfc3339(&task.modified)
                .unwrap()
                .with_timezone(&Local);
            let modified_str = modified.format("%Y-%m-%d %H:%M").to_string();

            let priority_colored = match task.priority.as_str() {
                "now" => task.priority.red().to_string(),
                _ => task.priority.to_string(),
            };

            let deadline_str = if let Some(ref deadline_str) = task.deadline {
                if let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str) {
                    let deadline_time = deadline.with_timezone(&Local);
                    let now = chrono::Utc::now();
                    let formatted = deadline_time.format("%Y-%m-%d").to_string();

                    if deadline < now {
                        formatted.red().to_string()
                    } else {
                        formatted
                    }
                } else {
                    "-".to_string()
                }
            } else {
                "-".to_string()
            };

            let status = if task.is_deleted() {
                "DELETED".red().to_string()
            } else if task.is_done() {
                "done".blue().to_string()
            } else {
                "pending".green().to_string()
            };

            let color = project_colors.get(task.project_id.as_str()).unwrap();
            let project = task.project_id.color(*color).to_string();

            TaskRowWithProject {
                id: format_task_id(&task.id, prefix_len),
                title: task.title.clone(),
                project,
                priority: priority_colored,
                size: task.size.clone(),
                modified: modified_str,
                deadline: deadline_str,
                status,
            }
        })
        .collect();

    let mut binding = Table::new(rows);
    let table = binding
        .with(Style::rounded())
        .with(Modify::new(Columns::new(0..1)).with(Alignment::center())) // ID
        .with(Modify::new(Columns::new(2..3)).with(Alignment::center())) // Project
        .with(Modify::new(Columns::new(3..8)).with(Alignment::center())); // Priority, Size, Modified, Deadline, Status

    println!("{}", table);
}

/// Print a single task with full details and markdown rendering.
///
/// If `no_format` is true or NO_COLOR is set, markdown will not be rendered.
pub fn print_task_details(task: &Task, no_format: bool) {
    let renderer = MarkdownRenderer::with_override(Some(no_format));

    // Print header
    println!("\n{}", "═".repeat(60));
    println!("{}", task.title.bold().green());
    println!("{}", "═".repeat(60));

    // Print metadata
    println!("\n{}", "Metadata:".bold());
    println!("  ID:       {}", task.id.cyan());

    let priority_colored = match task.priority.as_str() {
        "now" => task.priority.red(),
        _ => task.priority.normal(),
    };
    println!("  Priority: {}", priority_colored);
    println!("  Size:     {}", task.size);

    if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&task.created) {
        let created_time = created.with_timezone(&Local);
        println!("  Created:  {}", created_time.format("%Y-%m-%d %H:%M:%S"));
    }

    if let Ok(modified) = chrono::DateTime::parse_from_rfc3339(&task.modified) {
        let modified_time = modified.with_timezone(&Local);
        println!("  Modified: {}", modified_time.format("%Y-%m-%d %H:%M:%S"));
    }

    if let Some(ref deadline_str) = task.deadline {
        if let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str) {
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
    }

    if let Some(ref done_str) = task.done {
        if let Ok(done) = chrono::DateTime::parse_from_rfc3339(done_str) {
            let done_time = done.with_timezone(&Local);
            println!(
                "  {}",
                format!("Done:     {}", done_time.format("%Y-%m-%d %H:%M:%S")).blue()
            );
        }
    }

    if let Some(ref deleted_str) = task.deleted {
        if let Ok(deleted) = chrono::DateTime::parse_from_rfc3339(deleted_str) {
            let deleted_time = deleted.with_timezone(&Local);
            println!(
                "  {}",
                format!("Deleted:  {}", deleted_time.format("%Y-%m-%d %H:%M:%S")).red()
            );
        }
    }

    println!("  Version:  {}", task.version);

    // Print body with markdown rendering
    if !task.body.is_empty() {
        println!("\n{}", "Description:".bold());
        println!("{}", "─".repeat(60));
        print!("{}", renderer.render(&task.body));
    } else {
        println!("\n{}", "(No description)".italic().dimmed());
    }

    println!("{}\n", "═".repeat(60));
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
    fn test_find_min_unique_prefix_len_single_task() {
        let ids = vec!["ea75a3ac".to_string()];
        assert_eq!(find_min_unique_prefix_len(&ids), 2);
    }

    #[test]
    fn test_find_min_unique_prefix_len_all_different() {
        let ids = vec![
            "ea75a3ac".to_string(),
            "b35bcda6".to_string(),
            "d240111c".to_string(),
        ];
        // All differ at position 0, but minimum is 2
        assert_eq!(find_min_unique_prefix_len(&ids), 2);
    }

    #[test]
    fn test_find_min_unique_prefix_len_similar_prefix() {
        let ids = vec![
            "d240111c".to_string(),
            "ea75a3ac".to_string(),
            "ea7bc84d".to_string(),
        ];
        // ea75a3ac vs ea7bc84d differ at position 3, so need 4 chars
        assert_eq!(find_min_unique_prefix_len(&ids), 4);
    }

    #[test]
    fn test_find_min_unique_prefix_len_longer_prefix() {
        let ids = vec!["ea75a3ac".to_string(), "ea75a3bc".to_string()];
        // Differ at position 6, so need 7 chars
        assert_eq!(find_min_unique_prefix_len(&ids), 7);
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
}

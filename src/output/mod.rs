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

use chrono::Local;
use colored::Colorize;
use prettytable::{Table, format, row};
use termimad::MadSkin;

use crate::models::{Project, Task};

/// Print a list of projects in table format.
pub fn print_projects(projects: &[Project]) {
    if projects.is_empty() {
        println!("{}", "No projects found".yellow());
        return;
    }

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(row!["ID".bold(), "Name".bold(), "Description".bold()]);

    for project in projects {
        let desc = project.description.as_deref().unwrap_or("-");
        table.add_row(row![project.id.cyan(), project.name.green(), desc]);
    }

    table.printstd();
    println!("\n{} {}", "Total:".bold(), projects.len());
}

/// Print a list of tasks in table format.
pub fn print_tasks(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("{}", "No tasks found".yellow());
        return;
    }

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(row![
        "ID".bold(),
        "Title".bold(),
        "Priority".bold(),
        "Size".bold(),
        "Modified".bold(),
        "Status".bold()
    ]);

    for task in tasks {
        let id_short = &task.metadata.id.to_string()[..8];
        let modified = task.metadata.modified.with_timezone(&Local);
        let modified_str = modified.format("%Y-%m-%d %H:%M");

        let priority_colored = match task.metadata.priority.as_str() {
            "now" => task.metadata.priority.red().to_string(),
            _ => task.metadata.priority.normal().to_string(),
        };

        let status = if task.is_deleted() {
            "DELETED".red()
        } else {
            "active".green()
        };

        table.add_row(row![
            id_short.cyan(),
            task.title,
            priority_colored,
            task.metadata.size,
            modified_str,
            status
        ]);
    }

    table.printstd();
    println!("\n{} {}", "Total:".bold(), tasks.len());
}

/// Print a single task with full details and markdown rendering.
pub fn print_task_details(task: &Task) {
    let skin = MadSkin::default();

    // Print header
    println!("\n{}", "═".repeat(60));
    println!("{}", task.title.bold().green());
    println!("{}", "═".repeat(60));

    // Print metadata
    println!("\n{}", "Metadata:".bold());
    println!("  ID:       {}", task.metadata.id.to_string().cyan());

    let priority_colored = match task.metadata.priority.as_str() {
        "now" => task.metadata.priority.red(),
        _ => task.metadata.priority.normal(),
    };
    println!("  Priority: {}", priority_colored);
    println!("  Size:     {}", task.metadata.size);

    let created = task.metadata.created.with_timezone(&Local);
    let modified = task.metadata.modified.with_timezone(&Local);
    println!("  Created:  {}", created.format("%Y-%m-%d %H:%M:%S"));
    println!("  Modified: {}", modified.format("%Y-%m-%d %H:%M:%S"));

    if let Some(deleted) = task.metadata.deleted {
        let deleted_time = deleted.with_timezone(&Local);
        println!(
            "  {}",
            format!("Deleted:  {}", deleted_time.format("%Y-%m-%d %H:%M:%S")).red()
        );
    }

    println!("  Version:  {}", task.metadata.version);

    // Print body with markdown rendering
    if !task.body.is_empty() {
        println!("\n{}", "Description:".bold());
        println!("{}", "─".repeat(60));
        skin.print_text(&task.body);
    } else {
        println!("\n{}", "(No description)".italic().dimmed());
    }

    println!("{}\n", "═".repeat(60));
}

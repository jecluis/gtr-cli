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

use crate::models::{Project, Task};

/// Print a project in pretty format.
pub fn print_project(project: &Project) {
    println!("Project: {} ({})", project.name, project.id);
    if let Some(desc) = &project.description {
        println!("  {}", desc);
    }
}

/// Print a list of projects in table format.
pub fn print_projects(projects: &[Project]) {
    // TODO: Use prettytable-rs
    for project in projects {
        print_project(project);
    }
}

/// Print a task in pretty format with markdown rendering.
pub fn print_task(task: &Task) {
    println!("Task: {} ({})", task.title, task.metadata.id);
    println!(
        "  Priority: {}, Size: {}",
        task.metadata.priority, task.metadata.size
    );
    // TODO: Use termimad for markdown rendering
    println!("  Body: {}", task.body);
}

/// Print a list of tasks in table format.
pub fn print_tasks(tasks: &[Task]) {
    // TODO: Use prettytable-rs
    for task in tasks {
        print_task(task);
        println!();
    }
}

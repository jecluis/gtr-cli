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

//! List command implementation.

use crate::client::Client;
use crate::config::Config;
use crate::models::Task;
use crate::output;
use crate::{Error, Result};

/// List tasks.
#[allow(clippy::too_many_arguments)]
pub async fn tasks(
    config: &Config,
    project: Option<String>,
    priority: Option<String>,
    size: Option<String>,
    include_done: bool,
    include_deleted: bool,
    due_soon: bool,
    overdue: bool,
    limit: Option<u32>,
    reversed: bool,
) -> Result<()> {
    let client = Client::new(config)?;

    // Project ID is required for listing tasks
    let project_id = project.ok_or_else(|| {
        Error::InvalidInput(
            "project ID required. Use --project <id> or 'gtr project list' for all projects"
                .to_string(),
        )
    })?;

    let mut tasks = client
        .list_tasks(
            &project_id,
            priority.as_deref(),
            size.as_deref(),
            include_done,
            include_deleted,
            due_soon,
            overdue,
            limit,
        )
        .await?;

    // Sort and split tasks
    let (doing_tasks, other_tasks) = split_by_work_state(&mut tasks);

    // Sort both groups by priority then deadline
    let doing_tasks = sort_tasks(doing_tasks);
    let mut other_tasks = sort_tasks(other_tasks);

    // Reverse other tasks if flag is set
    if reversed {
        other_tasks.reverse();
    }

    output::print_tasks_grouped(&doing_tasks, &other_tasks);
    Ok(())
}

/// Split tasks into doing and other groups.
fn split_by_work_state(tasks: &mut [Task]) -> (Vec<Task>, Vec<Task>) {
    let mut doing = Vec::new();
    let mut other = Vec::new();

    for task in tasks {
        if task.current_work_state.as_deref() == Some("doing") {
            doing.push(task.clone());
        } else {
            other.push(task.clone());
        }
    }

    (doing, other)
}

/// Sort tasks by priority (now > later) then deadline (sooner first).
fn sort_tasks(mut tasks: Vec<Task>) -> Vec<Task> {
    tasks.sort_by(|a, b| {
        // First by priority (now < later for sorting, so now comes first)
        let priority_cmp = match (a.priority.as_str(), b.priority.as_str()) {
            ("now", "later") => std::cmp::Ordering::Less,
            ("later", "now") => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        };

        if priority_cmp != std::cmp::Ordering::Equal {
            return priority_cmp;
        }

        // Then by deadline (sooner first, None last)
        match (&a.deadline, &b.deadline) {
            (Some(a_deadline), Some(b_deadline)) => a_deadline.cmp(b_deadline),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    tasks
}

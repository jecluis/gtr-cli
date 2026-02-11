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

use chrono::{DateTime, Duration, Utc};

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::Task;
use crate::{Result, output, utils};

/// List tasks (local-first from cache).
#[allow(clippy::too_many_arguments)]
pub async fn tasks(
    config: &Config,
    project: Vec<String>,
    all_projects: bool,
    priority: Option<String>,
    size: Option<String>,
    include_done: bool,
    include_deleted: bool,
    due_soon: bool,
    overdue: bool,
    limit: Option<u32>,
    reversed: bool,
    no_sync: bool,
    absolute_dates: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    // Determine which projects to query
    let project_ids = if all_projects {
        // Get all projects from server (TODO: cache projects too)
        client
            .list_projects()
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
    } else if !project.is_empty() {
        // Use specified project IDs
        project
    } else {
        // Resolve single project interactively
        vec![utils::resolve_project(&client, None).await?]
    };

    // Load tasks from local cache and storage
    let mut all_tasks = Vec::new();

    for project_id in &project_ids {
        // Get task summaries from cache (includes project_id)
        let summaries = ctx.cache.list_tasks(project_id)?;

        for summary in summaries {
            // Load full task from storage
            if let Ok(task) = ctx.storage.load_task(&summary.project_id, &summary.id) {
                all_tasks.push(task);
            }
        }
    }

    // Apply filters
    all_tasks.retain(|task| {
        // Filter by done/deleted status
        let status_ok = match (task.done.is_some(), task.deleted.is_some()) {
            (true, _) => include_done,
            (_, true) => include_deleted,
            _ => true,
        };

        // Filter by priority
        let priority_ok = priority
            .as_ref()
            .map(|p| task.priority == *p)
            .unwrap_or(true);

        // Filter by size
        let size_ok = size.as_ref().map(|s| task.size == *s).unwrap_or(true);

        // Filter by deadline (due soon or overdue)
        let deadline_ok = if due_soon || overdue {
            if let Some(ref deadline_str) = task.deadline {
                if let Ok(deadline) = DateTime::parse_from_rfc3339(deadline_str) {
                    let now = Utc::now();
                    let deadline_utc = deadline.with_timezone(&Utc);
                    if overdue {
                        deadline_utc < now
                    } else if due_soon {
                        deadline_utc < now + Duration::hours(48)
                    } else {
                        true
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            true
        };

        status_ok && priority_ok && size_ok && deadline_ok
    });

    // Apply limit if specified
    if let Some(lim) = limit {
        all_tasks.truncate(lim as usize);
    }

    // Sort and split tasks
    let (doing_tasks, other_tasks) = split_by_work_state(&mut all_tasks);

    // Sort both groups by priority then deadline
    let doing_tasks = sort_tasks(doing_tasks);
    let mut other_tasks = sort_tasks(other_tasks);

    // Reverse other tasks if flag is set
    if reversed {
        other_tasks.reverse();
    }

    output::print_tasks_grouped(&doing_tasks, &other_tasks, absolute_dates);
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

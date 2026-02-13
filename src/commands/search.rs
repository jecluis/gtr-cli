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

//! Search command implementation.

use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::{Result, output};

/// Search tasks (local full-text search on cache).
pub async fn run(
    config: &Config,
    query: &str,
    project: Option<String>,
    limit: Option<u32>,
    no_sync: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    // Determine which projects to search
    let project_ids = if let Some(proj) = project {
        vec![proj]
    } else {
        // Search all projects (get from server for now)
        client
            .list_projects()
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect()
    };

    // Load all tasks and filter by query
    let task_ids = ctx.cache.list_task_ids(&project_ids)?;
    let query_lower = query.to_lowercase();
    let mut matching_tasks = Vec::new();

    for task_id in task_ids {
        let project_id = ctx
            .cache
            .get_task_summary(&task_id)
            .ok()
            .flatten()
            .map(|s| s.project_id)
            .unwrap_or_default();
        if let Ok(task) = ctx.storage.load_task(&project_id, &task_id) {
            // Search in title and body (case-insensitive)
            if task.title.to_lowercase().contains(&query_lower)
                || task.body.to_lowercase().contains(&query_lower)
            {
                matching_tasks.push(task);
                if let Some(lim) = limit
                    && matching_tasks.len() >= lim as usize
                {
                    break;
                }
            }
        }
    }

    if matching_tasks.is_empty() {
        println!(
            "{}",
            format!("No tasks found matching '{}'", query).yellow()
        );
        return Ok(());
    }

    println!("{}", format!("Search results for '{}':", query).bold());
    println!();
    // Search results default to relative dates for better UX
    output::print_tasks(&matching_tasks, false);

    Ok(())
}

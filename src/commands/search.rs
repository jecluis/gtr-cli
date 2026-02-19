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
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::{Result, output, threshold_cache};

/// Search tasks (local full-text search on cache).
pub async fn run(
    config: &Config,
    query: &str,
    project: Option<String>,
    limit: Option<u32>,
    all: bool,
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

    for task_id in &task_ids {
        if let Ok(task) = ctx.storage.load_task(task_id) {
            // Skip done and deleted tasks unless --all is specified
            if !all && (task.done.is_some() || task.deleted.is_some()) {
                continue;
            }

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

    // Calculate prefix length based on ALL tasks (not just search results)
    let prefix_len = crate::output::compute_min_prefix_len(&task_ids);

    // Search results default to relative dates for better UX
    let cached = threshold_cache::fetch_thresholds(config, &client, no_sync).await;
    let icons = Icons::new(config.effective_icon_theme());
    let project_paths = ctx.cache.build_project_paths(&matching_tasks);
    output::print_tasks(
        &matching_tasks,
        prefix_len,
        false,
        true,
        false,
        None,
        &cached,
        &icons,
        false,
        &project_paths,
    );

    Ok(())
}

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

//! Start command implementation.

use colored::Colorize;
use dialoguer::Select;

use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::Task;
use crate::{Result, mutations, output, utils};

/// Start working on a task (set work state to "doing").
///
/// When no task_id is provided, picks from pending non-doing tasks.
/// If the task has no progress set, auto-sets it to 0%.
pub async fn run(
    config: &Config,
    task_id: Option<String>,
    filter: Option<String>,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    let full_id = if let Some(ref id) = task_id {
        utils::resolve_task_id(&client, id).await?
    } else {
        resolve_startable_task(&client, &ctx, filter.as_deref(), &icons).await?
    };

    // Ensure task is available locally
    ctx.load_task(&client, &full_id).await?;

    let result = mutations::start_task(&ctx.storage, &ctx.cache, &full_id)?;

    if result.was_noop {
        let all_ids = ctx.cache.all_task_ids()?;
        let prefix_len = output::compute_min_prefix_len(&all_ids);
        println!(
            "{} {} is already in progress",
            icons.info.blue(),
            output::format_full_id(&result.task.id, prefix_len)
        );
        return Ok(());
    }

    println!(
        "{}",
        format!("{} Task started!", icons.success).green().bold()
    );
    let all_ids = ctx.cache.all_task_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);
    println!(
        "  ID:       {}",
        output::format_full_id(&result.task.id, prefix_len)
    );
    println!("  Title:    {}", result.task.display_title(&icons));
    println!("  Status:   {}", "doing".green());

    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
        }
    }

    Ok(())
}

/// Pick from pending tasks that are NOT currently "doing".
///
/// If filter is provided, matches tasks by title first, then by body.
async fn resolve_startable_task(
    client: &Client,
    ctx: &LocalContext,
    filter: Option<&str>,
    icons: &Icons,
) -> Result<String> {
    let projects = client.list_projects().await?;

    let mut candidates: Vec<Task> = Vec::new();
    for project in &projects {
        let summaries = ctx.cache.list_tasks(&project.id)?;
        for summary in summaries {
            if summary.done.is_none()
                && summary.deleted.is_none()
                && let Ok(task) = ctx.storage.load_task(&summary.id)
                && task.is_pending()
                && task.current_work_state.as_deref() != Some("doing")
            {
                // Apply filter if provided (case-insensitive match on title, then body)
                if let Some(filter_text) = filter {
                    let filter_lower = filter_text.to_lowercase();
                    let title_match = task.title.to_lowercase().contains(&filter_lower);
                    let body_match = task.body.to_lowercase().contains(&filter_lower);

                    if !title_match && !body_match {
                        continue;
                    }
                }

                candidates.push(task);
            }
        }
    }

    if candidates.is_empty() {
        let msg = if filter.is_some() {
            "No startable tasks matching filter".to_string()
        } else {
            "No startable tasks found".to_string()
        };
        return Err(crate::Error::UserFacing(msg));
    }

    // Only auto-select if no filter provided and exactly one candidate
    if candidates.len() == 1 && filter.is_none() {
        return Ok(candidates[0].id.clone());
    }

    pick_task(&candidates, icons)
}

/// Interactive task picker using dialoguer::Select.
fn pick_task(tasks: &[Task], icons: &Icons) -> Result<String> {
    let items: Vec<String> = tasks
        .iter()
        .map(|t| {
            let progress_str = t.progress.map(|p| format!(" ({}%)", p)).unwrap_or_default();
            format!(
                "{} {}{}",
                t.id[..8].cyan(),
                t.display_title(icons),
                progress_str.dimmed()
            )
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select task to start")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    let Some(idx) = selection else {
        return Err(crate::Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok(tasks[idx].id.clone())
}

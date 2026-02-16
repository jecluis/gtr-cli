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

//! Stop command implementation.

use chrono::Utc;
use colored::Colorize;
use dialoguer::Select;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType, Task, WorkState};
use crate::utils;

/// Stop working on a task (clear work state).
///
/// When no task_id is provided, picks from currently "doing" tasks.
pub async fn run(config: &Config, task_id: Option<String>, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    let full_id = if let Some(ref id) = task_id {
        utils::resolve_task_id(&client, id).await?
    } else {
        resolve_doing_task(&client, &ctx).await?
    };

    let mut task = ctx.load_task(&client, &full_id).await?;

    if task.current_work_state.as_deref() != Some("doing") {
        println!(
            "{} {} is not currently in progress",
            icons.info.blue(),
            task.id[..8].cyan()
        );
        return Ok(());
    }

    let now = Utc::now();
    task.current_work_state = Some("stopped".to_string());
    task.modified = now.to_rfc3339();
    task.version += 1;

    // Add log entry for work state change
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::WorkStateChanged {
            state: WorkState::Stopped,
        },
        source: crate::models::LogSource::User,
    });

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!(
        "{}",
        format!("{} Task stopped!", icons.success).green().bold()
    );
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);
    println!("  Status:   {}", "stopped".dimmed());

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

/// Pick from tasks currently in "doing" state.
async fn resolve_doing_task(client: &Client, ctx: &LocalContext) -> Result<String> {
    let projects = client.list_projects().await?;

    let mut doing_tasks: Vec<Task> = Vec::new();
    for project in &projects {
        let summaries = ctx.cache.list_tasks(&project.id)?;
        for summary in summaries {
            if summary.done.is_none()
                && summary.deleted.is_none()
                && let Ok(task) = ctx.storage.load_task(&summary.project_id, &summary.id)
                && task.current_work_state.as_deref() == Some("doing")
            {
                doing_tasks.push(task);
            }
        }
    }

    if doing_tasks.is_empty() {
        return Err(crate::Error::UserFacing(
            "No started tasks found".to_string(),
        ));
    }

    if doing_tasks.len() == 1 {
        return Ok(doing_tasks[0].id.clone());
    }

    pick_task(&doing_tasks)
}

/// Interactive task picker using dialoguer::Select.
fn pick_task(tasks: &[Task]) -> Result<String> {
    let items: Vec<String> = tasks
        .iter()
        .map(|t| {
            let progress_str = t.progress.map(|p| format!(" ({}%)", p)).unwrap_or_default();
            format!("{} {}{}", t.id[..8].cyan(), t.title, progress_str.dimmed())
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select task to stop")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    let Some(idx) = selection else {
        return Err(crate::Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok(tasks[idx].id.clone())
}

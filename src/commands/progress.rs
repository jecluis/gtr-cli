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

//! Progress command implementation.

use chrono::Utc;
use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType, LogSource, TaskStatus};
use crate::utils;

/// Set task progress (local-first with optional sync).
///
/// When no task_id is provided, smart resolution picks from "doing" tasks
/// or falls back to all pending tasks.
pub async fn run(config: &Config, value: u8, task_id: Option<String>, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    let full_id = if let Some(ref id) = task_id {
        utils::resolve_task_id(&client, id).await?
    } else {
        utils::pick_task(&client, &ctx, "Select task to update progress", true).await?
    };

    let mut task = ctx.load_task(&client, &full_id).await?;

    let old_progress = task.progress;
    let now = Utc::now();

    task.progress = Some(value);
    task.modified = now.to_rfc3339();
    task.version += 1;

    // Add log entry for progress change
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::ProgressChanged {
            from: old_progress,
            to: Some(value),
        },
        source: LogSource::User,
    });

    // Auto-mark as done when progress reaches 100%
    let auto_done = value == 100 && task.done.is_none();
    if auto_done {
        task.done = Some(now.to_rfc3339());
        task.current_work_state = None;

        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::StatusChanged {
                status: TaskStatus::Done,
            },
            source: LogSource::User,
        });
    }

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!(
        "{}",
        format!("{} Progress updated locally!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);

    let old_str = old_progress
        .map(|p| format!("{}%", p))
        .unwrap_or_else(|| "none".to_string());
    println!(
        "  Progress: {} → {}",
        old_str.dimmed().strikethrough(),
        format!("{}%", value).green()
    );

    if auto_done {
        println!(
            "  {}",
            format!("{} Task auto-marked as done (100% complete)", icons.success).green()
        );
    }

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

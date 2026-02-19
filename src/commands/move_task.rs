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

//! Move command implementation.

use chrono::Utc;
use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::utils;

/// Move a task to a different project (local-first with optional sync).
pub async fn run(
    config: &Config,
    task_id: &str,
    target_project: &str,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    // Load task
    let mut task = ctx.load_task(&client, &full_id).await?;

    let old_project = task.project_id.clone();

    if old_project == target_project {
        println!(
            "{}",
            format!(
                "{} Task is already in project '{}'",
                icons.success, target_project
            )
            .dimmed()
        );
        return Ok(());
    }

    // Update project_id (flat storage means no file move needed)
    task.project_id = target_project.to_string();
    task.modified = Utc::now().to_rfc3339();
    task.version += 1;

    // Save locally (CRDT now includes project_id change)
    ctx.storage.update_task(&task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!(
        "{}",
        format!("{} Task moved!", icons.success).green().bold()
    );
    println!("  ID:      {}", task.id.cyan());
    println!("  Title:   {}", task.display_title(&icons));
    println!(
        "  Project: {} → {}",
        old_project.dimmed().strikethrough(),
        target_project.cyan()
    );

    // Sync: call the server's move endpoint, then push CRDT
    if !no_sync {
        match client.move_task(&full_id, target_project).await {
            Ok(_) => {
                // Also push CRDT so server has the updated project_id in the document
                let _ = ctx.try_sync().await;
                println!(
                    "{}",
                    format!("  {} Synced with server", icons.success).green()
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "server move failed, queued for sync");
                println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
            }
        }
    }

    Ok(())
}

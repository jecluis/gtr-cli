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

//! Done command implementation.

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::{mutations, output, utils};

/// Mark one or more tasks as done (local-first with optional sync).
pub async fn run(config: &Config, mut task_ids: Vec<String>, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());

    // If no task IDs provided, show picker
    if task_ids.is_empty() {
        let client = Client::new(config)?;
        let ctx = LocalContext::new(config, !no_sync)?;
        let selected_id =
            utils::pick_task(&client, &ctx, "Select task to mark as done", true, &icons).await?;
        task_ids.push(selected_id);
    }

    let mut success_count = 0;
    let mut failures = Vec::new();

    for task_id in task_ids {
        match mark_task_done(config, &task_id, no_sync).await {
            Ok((full_id, title, prefix_len)) => {
                success_count += 1;
                println!(
                    "{}",
                    format!("{} Task marked as done locally!", icons.success)
                        .green()
                        .bold()
                );
                println!("  ID:    {}", output::format_full_id(&full_id, prefix_len));
                println!("  Title: {}", title);
            }
            Err(e) => {
                failures.push((task_id, e));
            }
        }
    }

    // Print summary
    if success_count > 0 {
        println!(
            "\n{}",
            format!("✓ Marked {} task(s) as done", success_count)
                .green()
                .bold()
        );
    }

    if !failures.is_empty() {
        eprintln!("\n{}", format!("{} Failures:", icons.failure).red().bold());
        for (id, err) in failures {
            eprintln!("  {} - {}", id.red(), err);
        }
        return Err(crate::Error::UserFacing(
            "Some tasks failed to be marked as done".to_string(),
        ));
    }

    Ok(())
}

/// Mark a single task as done. Returns (full_id, title, prefix_len).
async fn mark_task_done(
    config: &Config,
    task_id: &str,
    no_sync: bool,
) -> Result<(String, String, usize)> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    // Ensure task is available locally
    ctx.load_task(&client, &full_id).await?;

    let result = mutations::mark_done(&ctx.storage, &ctx.cache, &full_id)?;
    let title = result.task.display_title(&icons);

    if result.descendants_completed > 0 {
        println!(
            "  {}",
            format!(
                "+ {} subtask(s) also marked done",
                result.descendants_completed
            )
            .green()
            .bold()
        );
    }

    // Sync
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

    let all_ids = ctx.cache.all_task_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);
    Ok((full_id, title, prefix_len))
}

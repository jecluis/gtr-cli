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

//! Delete command implementation.

use chrono::Utc;
use colored::Colorize;
use tracing::warn;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::utils;

/// Delete a task (tombstone, local-first with optional sync).
///
/// When `recursive` is false (default), direct children are promoted
/// to the deleted task's parent (or become root tasks). When `recursive`
/// is true, all descendants are also marked as deleted.
pub async fn run(config: &Config, task_id: &str, recursive: bool, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());

    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    let mut task = ctx.load_task(&client, &full_id).await?;
    let now = Utc::now();

    // Capture parent before deleting (children will be promoted here)
    let deleted_parent_id = task.parent_id.clone();

    task.deleted = Some(now.to_rfc3339());
    task.modified = now.to_rfc3339();
    task.version += 1;

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!(
        "{}",
        format!("{} Task deleted locally!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:    {}", task.id.cyan());
    println!("  Title: {}", task.title);

    if recursive {
        // Cascade delete to all descendants
        let descendants = ctx.cache.get_all_descendants(&full_id)?;
        let count = descendants.len();
        for desc_id in descendants {
            match ctx.storage.load_task(&task.project_id, &desc_id) {
                Ok(mut desc_task) => {
                    if desc_task.deleted.is_some() {
                        continue;
                    }
                    desc_task.deleted = Some(now.to_rfc3339());
                    desc_task.modified = now.to_rfc3339();
                    desc_task.version += 1;
                    ctx.storage.update_task(&desc_task.project_id, &desc_task)?;
                    ctx.cache.upsert_task(&desc_task, true)?;
                }
                Err(e) => {
                    warn!(task_id = %desc_id, error = %e, "failed to cascade delete");
                }
            }
        }
        if count > 0 {
            println!(
                "  {}",
                format!("+ {} subtask(s) also deleted", count)
                    .green()
                    .bold()
            );
        }
    } else {
        // Promote direct children to the deleted task's parent
        let children = ctx.cache.get_children(&full_id)?;
        let count = children.len();
        for child_summary in &children {
            match ctx.storage.load_task(&task.project_id, &child_summary.id) {
                Ok(mut child_task) => {
                    child_task.parent_id = deleted_parent_id.clone();
                    child_task.modified = now.to_rfc3339();
                    child_task.version += 1;
                    ctx.storage
                        .update_task(&child_task.project_id, &child_task)?;
                    ctx.cache.upsert_task(&child_task, true)?;
                }
                Err(e) => {
                    warn!(task_id = %child_summary.id, error = %e, "failed to promote child");
                }
            }
        }
        if count > 0 {
            let target = deleted_parent_id
                .as_deref()
                .map(|id| &id[..8])
                .unwrap_or("root");
            println!(
                "  {}",
                format!("{} child(ren) promoted to {}", count, target).dimmed()
            );
        }
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

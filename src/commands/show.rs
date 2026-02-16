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

//! Show command implementation.

use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::{Result, output, threshold_cache, utils};

/// Show a specific task (local-first with optional refresh).
pub async fn run(
    config: &Config,
    task_id: &str,
    no_sync: bool,
    no_format: bool,
    no_wrap: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    // Load from local storage (or fetch from server if not cached)
    let task = ctx.load_task(&client, &full_id).await?;

    let cached = threshold_cache::fetch_thresholds(config, &client, no_sync).await;
    output::print_task_details(config, &task, no_format, no_wrap, &cached);

    // Show parent info
    if let Some(ref parent_id) = task.parent_id {
        let parent_title = ctx
            .cache
            .get_task_title(parent_id)?
            .unwrap_or_else(|| "?".to_string());
        println!(
            "{} {} {}",
            "Parent:".bold(),
            parent_id[..8].cyan(),
            parent_title.dimmed()
        );
    }

    // Show children
    let children = ctx.cache.get_children(&full_id)?;
    if !children.is_empty() {
        println!("\n{}", "Subtasks:".bold());
        for child in &children {
            let is_done = child.done.is_some();
            let status_colored = if is_done {
                "done".blue().to_string()
            } else {
                "pending".green().to_string()
            };
            println!(
                "  {} {} [{}]",
                child.id[..8].cyan(),
                child.title,
                status_colored
            );
        }
    }

    // Try to refresh from server in background if sync enabled
    if !no_sync {
        match tokio::time::timeout(std::time::Duration::from_secs(2), client.get_task(&full_id))
            .await
        {
            Ok(Ok(fresh)) if fresh.version > task.version => {
                // Update local with fresh version
                ctx.storage.update_task(&fresh.project_id, &fresh)?;
                ctx.cache.upsert_task(&fresh, false)?;
                eprintln!(
                    "\n(Refreshed from server - version {} → {})",
                    task.version, fresh.version
                );
            }
            _ => {}
        }
    }

    Ok(())
}

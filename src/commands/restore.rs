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

//! Restore command implementation.

use chrono::Utc;
use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::{output, utils};

/// Restore a deleted task (local-first with optional sync).
pub async fn run(config: &Config, task_id: &str, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    let mut task = ctx.load_task(&client, &full_id).await?;

    task.deleted = None;
    task.modified = Utc::now().to_rfc3339();
    task.version += 1;

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!(
        "{}",
        format!("{} Task restored locally!", icons.success)
            .green()
            .bold()
    );
    let all_ids = ctx.cache.all_task_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);
    println!("  ID:    {}", output::format_full_id(&task.id, prefix_len));
    println!("  Title: {}", task.title);

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

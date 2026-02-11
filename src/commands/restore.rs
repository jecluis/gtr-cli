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
use crate::local::LocalContext;
use crate::utils;

/// Restore a deleted task (local-first with optional sync).
pub async fn run(config: &Config, task_id: &str, no_sync: bool) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    let mut task = match ctx.storage.load_task("", &full_id) {
        Ok(t) => t,
        Err(_) => {
            let fetched = client.get_task(&full_id).await?;
            ctx.storage.create_task(&fetched.project_id, &fetched)?;
            ctx.cache.upsert_task(&fetched, false)?;
            fetched
        }
    };

    task.deleted = None;
    task.modified = Utc::now().to_rfc3339();
    task.version += 1;

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!("{}", "✓ Task restored locally!".green().bold());
    println!("  ID:    {}", task.id.cyan());
    println!("  Title: {}", task.title);

    if !no_sync {
        if ctx.try_sync().await {
            println!("{}", "  ✓ Synced with server".green());
        } else {
            println!("{}", "  ⊙ Queued for sync".yellow());
        }
    }

    Ok(())
}

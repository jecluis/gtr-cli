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

//! Now command implementation.

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::{mutations, utils};

/// Set task priority to "now" (local-first with optional sync).
pub async fn run(config: &Config, task_id: &str, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    // Ensure task is available locally
    ctx.load_task(&client, &full_id).await?;

    let result = mutations::set_priority(&ctx.storage, &ctx.cache, &full_id, "now")?;

    println!(
        "{}",
        format!("{} Task priority updated locally!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:       {}", result.task.id.cyan());
    println!("  Title:    {}", result.task.display_title(&icons));
    println!(
        "  Priority: {} → {}",
        result.old_priority.dimmed().strikethrough(),
        result.task.priority.red()
    );

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

    Ok(())
}

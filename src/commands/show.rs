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

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::{Result, output, utils};

/// Show a specific task (local-first with optional refresh).
pub async fn run(config: &Config, task_id: &str, no_sync: bool) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    // Try to load from local storage
    let task = match ctx.storage.load_task("", &full_id) {
        Ok(t) => t,
        Err(_) => {
            // Not cached, fetch from server
            let fetched = client.get_task(&full_id).await?;
            ctx.storage.create_task(&fetched.project_id, &fetched)?;
            ctx.cache.upsert_task(&fetched, false)?;
            fetched
        }
    };

    output::print_task_details(&task);

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

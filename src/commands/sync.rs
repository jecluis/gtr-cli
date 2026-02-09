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

//! Sync command implementations.

use colored::Colorize;

use crate::Result;
use crate::cache::TaskCache;
use crate::config::Config;
use crate::storage::{StorageConfig, TaskStorage};
use crate::sync::SyncManager;

/// Initialize sync manager from config.
fn init_sync(config: &Config) -> Result<SyncManager> {
    let storage_config = StorageConfig::new(config.cache_dir.clone(), "default".to_string());
    let storage = TaskStorage::new(storage_config);

    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    SyncManager::new(config, storage, cache)
}

/// gtr sync now - Manual bidirectional sync.
pub async fn now(config: &Config) -> Result<()> {
    let sync = init_sync(config)?;

    println!("{}", "Syncing with server...".dimmed());

    match sync.sync_all().await {
        Ok(()) => {
            println!("{}", "✓ Sync completed successfully".green());
            Ok(())
        }
        Err(e) => {
            println!("{} {}", "✗ Sync failed:".red(), e);
            Err(e)
        }
    }
}

/// gtr sync status - Show sync state.
pub async fn status(config: &Config) -> Result<()> {
    let sync = init_sync(config)?;
    let status = sync.sync_status()?;

    println!("{}", "Sync Status:".bold());

    // Server reachability
    let server_status = if status.server_reachable {
        "✓ reachable".green()
    } else {
        "✗ unreachable".red()
    };
    println!("  Server:     {}", server_status);

    // Pending changes
    if status.pending_push > 0 {
        println!("  {} {} tasks to push", "↑".yellow(), status.pending_push);
    } else {
        println!("  {} No pending changes", "✓".green());
    }

    Ok(())
}

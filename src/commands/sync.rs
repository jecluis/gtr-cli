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
use crate::icons::Icons;
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
    let icons = Icons::new(config.effective_icon_theme());
    let sync = init_sync(config)?;

    println!("{}", "Syncing with server...".dimmed());

    match sync.sync_full().await {
        Ok(report) => {
            let pushed = report.pushed_tasks + report.pushed_documents;
            let pulled = report.pulled_tasks + report.pulled_documents;

            println!(
                "{}",
                format!("{} Sync completed successfully", icons.success).green()
            );

            if pushed > 0 {
                let mut parts = Vec::new();
                if report.pushed_tasks > 0 {
                    parts.push(format!(
                        "{} task{}",
                        report.pushed_tasks,
                        if report.pushed_tasks == 1 { "" } else { "s" }
                    ));
                }
                if report.pushed_documents > 0 {
                    parts.push(format!(
                        "{} document{}",
                        report.pushed_documents,
                        if report.pushed_documents == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ));
                }
                println!("  {} pushed {}", "↑".green(), parts.join(", "));
            }
            if pulled > 0 {
                let mut parts = Vec::new();
                if report.pulled_tasks > 0 {
                    parts.push(format!(
                        "{} task{}",
                        report.pulled_tasks,
                        if report.pulled_tasks == 1 { "" } else { "s" }
                    ));
                }
                if report.pulled_documents > 0 {
                    parts.push(format!(
                        "{} document{}",
                        report.pulled_documents,
                        if report.pulled_documents == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ));
                }
                println!("  {} pulled {}", "↓".green(), parts.join(", "));
            }
            if pushed == 0 && pulled == 0 {
                println!("  {}", "Already up to date".dimmed());
            }

            Ok(())
        }
        Err(e) => {
            println!("{} {}", format!("{} Sync failed:", icons.failure).red(), e);
            Err(e)
        }
    }
}

/// gtr sync status - Show sync state.
pub async fn status(config: &Config) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let sync = init_sync(config)?;
    let status = sync.sync_status().await?;

    println!("{}", "Sync Status:".bold());

    // Server reachability
    let server_status = if status.server_reachable {
        format!("{} reachable", icons.success).green()
    } else {
        format!("{} unreachable", icons.failure).red()
    };
    println!("  Server:     {}", server_status);

    // Pending changes
    if status.pending_push > 0 {
        println!("  {} {} tasks to push", "↑".yellow(), status.pending_push);
    } else {
        println!("  {} No pending changes", icons.success.green());
    }

    Ok(())
}

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

//! Sync logic for bidirectional task synchronization.

use std::time::Duration;

use crate::cache::TaskCache;
use crate::client::Client;
use crate::config::Config;
use crate::storage::TaskStorage;
use crate::Result;

/// Sync coordinator handling local-remote synchronization.
pub struct SyncManager {
    client: Client,
    storage: TaskStorage,
    cache: TaskCache,
}

impl SyncManager {
    pub fn new(config: &Config, storage: TaskStorage, cache: TaskCache) -> Result<Self> {
        let client = Client::new(config)?;
        Ok(Self {
            client,
            storage,
            cache,
        })
    }

    /// Attempt sync with timeout. Returns true if successful.
    pub async fn try_sync(&self, timeout: Duration) -> bool {
        let sync_future = self.sync_all();
        match tokio::time::timeout(timeout, sync_future).await {
            Ok(Ok(())) => true,
            Ok(Err(_)) | Err(_) => false,
        }
    }

    /// Sync all pending changes bidirectionally.
    pub async fn sync_all(&self) -> Result<()> {
        // First push local changes
        self.push_pending().await?;

        // Then pull remote changes (not implemented yet - would need server API)
        // self.pull_updates().await?;

        Ok(())
    }

    /// Push all locally modified tasks to server.
    async fn push_pending(&self) -> Result<()> {
        let pending_ids = self.cache.get_pending_tasks()?;

        for task_id in pending_ids {
            if let Err(e) = self.push_task(&task_id).await {
                // Log error but continue with other tasks
                eprintln!("Failed to push task {}: {}", task_id, e);
            }
        }

        Ok(())
    }

    /// Push a single task to server.
    async fn push_task(&self, task_id: &str) -> Result<()> {
        // Get task from local storage
        let task = self.storage.load_task("", task_id)?; // TODO: get project_id from cache

        // Get CRDT bytes for server merge
        let bytes = self.storage.get_task_bytes(&task.project_id, task_id)?;

        // Post to server sync endpoint
        let merged_task = self
            .client
            .post_sync(&task.project_id, task_id, &bytes)
            .await?;

        // Update cache with merged result
        self.cache.upsert_task(&merged_task, false)?;
        self.cache.mark_synced(task_id)?;

        Ok(())
    }

    /// Get sync status summary.
    pub fn sync_status(&self) -> Result<SyncStatus> {
        let pending_count = self.cache.get_pending_tasks()?.len();

        // Try to reach server (quick check)
        let server_reachable = false; // TODO: implement health check

        Ok(SyncStatus {
            pending_push: pending_count,
            server_reachable,
        })
    }
}

/// Sync status information.
#[derive(Debug)]
pub struct SyncStatus {
    pub pending_push: usize,
    pub server_reachable: bool,
}

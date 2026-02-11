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

use crate::Result;
use crate::cache::TaskCache;
use crate::client::Client;
use crate::config::Config;
use crate::storage::TaskStorage;

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

    /// Sync all pending changes (push only).
    ///
    /// This is used for automatic sync after updates. It pushes local changes
    /// and fetches the merged state, but doesn't pull all tasks to avoid
    /// unnecessary network traffic.
    pub async fn sync_all(&self) -> Result<()> {
        // Push local changes (push_task now fetches merged CRDT back)
        // This ensures local storage has the authoritative merged state
        self.push_pending().await?;

        Ok(())
    }

    /// Full bidirectional sync (push and pull).
    ///
    /// This is used for manual sync commands. It pushes local changes and
    /// then pulls updates from the server for all tasks, ensuring we have
    /// the latest state from other devices.
    pub async fn sync_full(&self) -> Result<()> {
        // Push local changes first
        self.push_pending().await?;

        // Pull remote changes for all tasks
        self.pull_updates().await?;

        Ok(())
    }

    /// Pull updates from server for all projects.
    async fn pull_updates(&self) -> Result<()> {
        // Get all projects
        let projects = self.client.list_projects().await?;

        for project in projects {
            // Get all tasks for this project
            let tasks = self
                .client
                .list_tasks(&project.id, None, None, true, true, false, false, None)
                .await?;

            for task in tasks {
                // Always pull and merge - CRDT handles conflicts automatically
                if let Err(e) = self.pull_task(&project.id, &task.id).await {
                    eprintln!("Failed to pull task {}: {}", task.id, e);
                    // Continue with other tasks
                }
            }
        }

        Ok(())
    }

    /// Pull a single task from server and merge with local version.
    async fn pull_task(&self, project_id: &str, task_id: &str) -> Result<()> {
        // Fetch CRDT bytes from server
        let remote_bytes = self.client.fetch_sync(task_id).await?;

        // Check if we have a local version to merge with
        let merged_bytes = match self.storage.get_task_bytes(project_id, task_id) {
            Ok(local_bytes) => {
                // We have local version - merge with remote
                let mut local_doc = crate::crdt::TaskDocument::load(&local_bytes)?;
                let mut remote_doc = crate::crdt::TaskDocument::load(&remote_bytes)?;

                // CRDT merge handles conflicts automatically
                local_doc.merge(&mut remote_doc)?;

                local_doc.save()
            }
            Err(_) => {
                // No local version - use remote as-is
                remote_bytes
            }
        };

        // Save merged result to local storage
        self.storage
            .save_task_bytes(project_id, task_id, &merged_bytes)?;

        // Extract task data for cache
        let doc = crate::crdt::TaskDocument::load(&merged_bytes)?;
        let task = doc.to_task()?;

        // Update cache
        self.cache.upsert_task(&task, false)?;

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
    ///
    /// Uses change-based sync for existing tasks (more efficient, preserves causality).
    /// Falls back to full-document sync for new tasks or if change-based sync fails.
    async fn push_task(&self, task_id: &str) -> Result<()> {
        // Get project_id from cache
        let summary = self
            .cache
            .get_task_summary(task_id)?
            .ok_or_else(|| crate::Error::TaskNotFound(format!("task {task_id} not in cache")))?;

        // Load local CRDT document
        let bytes = self.storage.get_task_bytes(&summary.project_id, task_id)?;

        // Try change-based sync first (more efficient for updates)
        let result = {
            let local_doc = crate::crdt::TaskDocument::load(&bytes)?;

            // Get heads that represent last server state (empty for now - we'll track this later)
            // For initial implementation, we send all changes
            let last_server_heads: Vec<Vec<u8>> = vec![];

            // Get changes since last sync with server
            let changes = local_doc.get_changes_since_bytes(&last_server_heads);

            // Try change-based sync
            self.client
                .sync_changes(task_id, changes, last_server_heads)
                .await
        };

        // Handle result or fall back to full-document sync
        let (merged_task, merged_bytes) = match result {
            Ok(sync_response) => {
                // Change-based sync succeeded - apply server's changes
                let local_doc = crate::crdt::TaskDocument::load(&bytes)?;
                let mut local_doc = local_doc;
                local_doc.apply_changes(sync_response.changes)?;
                let updated_bytes = local_doc.save();
                (sync_response.task, updated_bytes)
            }
            Err(e) => {
                // Check if it's a 404 error (task doesn't exist)
                let is_not_found = matches!(e, crate::Error::TaskNotFound(_))
                    || matches!(&e, crate::Error::Server(msg) if msg.contains("404"));

                if is_not_found {
                    // Task doesn't exist on server - use full-document sync
                    let merged_task = self
                        .client
                        .post_sync(&summary.project_id, task_id, &bytes)
                        .await?;

                    // Fetch merged state from server
                    let merged_bytes = self.client.fetch_sync(task_id).await?;
                    (merged_task, merged_bytes)
                } else {
                    return Err(e);
                }
            }
        };

        // Save updated document
        self.storage
            .save_task_bytes(&summary.project_id, task_id, &merged_bytes)?;

        // Update cache with merged result
        self.cache.upsert_task(&merged_task, false)?;
        self.cache.mark_synced(task_id)?;

        Ok(())
    }

    /// Get sync status summary.
    pub async fn sync_status(&self) -> Result<SyncStatus> {
        let pending_count = self.cache.get_pending_tasks()?.len();

        // Try to reach server (quick check with timeout)
        let server_reachable = self.client.health_check().await;

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

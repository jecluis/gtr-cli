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

//! Local-first operations with optional sync.

use std::time::Duration;
use tracing::{debug, info, warn};

use crate::Result;
use crate::cache::TaskCache;
use crate::client::Client;
use crate::config::Config;
use crate::models::Task;
use crate::storage::{StorageConfig, TaskStorage};
use crate::sync::SyncManager;

/// Context for local-first operations.
pub struct LocalContext {
    pub storage: TaskStorage,
    pub cache: TaskCache,
    config: Config,
    enable_sync: bool,
}

impl LocalContext {
    /// Initialize local context from config.
    pub fn new(config: &Config, enable_sync: bool) -> Result<Self> {
        let storage_config = StorageConfig::new(config.cache_dir.clone(), "default".to_string());

        // Migrate from per-project dirs to flat layout (idempotent)
        crate::storage::migration::migrate_to_flat_layout(&storage_config)?;

        let storage = TaskStorage::new(storage_config);

        let cache_path = config.cache_dir.join("index.db");
        let cache = TaskCache::open(&cache_path)?;

        Ok(Self {
            storage,
            cache,
            config: config.clone(),
            enable_sync,
        })
    }

    /// Load a task from local storage, falling back to server CRDT fetch.
    ///
    /// Resolves the project_id from cache to find the correct storage path.
    /// If the task is not available locally, fetches the raw CRDT bytes from
    /// the server (preserving document history) rather than creating a new
    /// independent document.
    pub async fn load_task(&self, client: &Client, task_id: &str) -> Result<Task> {
        // Try loading from local storage using cache for project_id
        if let Some(summary) = self.cache.get_task_summary(task_id)? {
            match self.storage.load_task(task_id) {
                Ok(task) => {
                    debug!(
                        task_id,
                        project_id = %summary.project_id,
                        "loaded task from local storage"
                    );
                    return Ok(task);
                }
                Err(e) => {
                    warn!(
                        task_id,
                        project_id = %summary.project_id,
                        "task in cache but missing from storage, fetching from server: {e}"
                    );
                }
            }
        } else {
            debug!(task_id, "task not in local cache, fetching from server");
        }

        // Fetch CRDT bytes from server to preserve document history.
        // Using raw bytes instead of JSON + TaskDocument::new() avoids
        // creating an independent document whose metadata map object
        // would conflict with the server's during CRDT merge.
        let remote_bytes = client.fetch_sync(task_id).await?;
        let doc = crate::crdt::TaskDocument::load(&remote_bytes)?;
        let task = doc.to_task()?;

        self.storage.save_task_bytes(task_id, &remote_bytes)?;
        self.cache.upsert_task(&task, false)?;

        info!(
            task_id,
            project_id = %task.project_id,
            "fetched task CRDT from server"
        );
        Ok(task)
    }

    /// Attempt sync with timeout if enabled.
    /// Creates fresh SyncManager instance for the operation.
    pub async fn try_sync(&self) -> bool {
        if !self.enable_sync {
            return false;
        }

        // Create fresh instances for sync
        let storage_config =
            StorageConfig::new(self.config.cache_dir.clone(), "default".to_string());
        let storage = TaskStorage::new(storage_config);

        let cache_path = self.config.cache_dir.join("index.db");
        let cache = match TaskCache::open(&cache_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let sync = match SyncManager::new(&self.config, storage, cache) {
            Ok(s) => s,
            Err(_) => return false,
        };

        sync.try_sync(Duration::from_secs(3)).await
    }

    /// Check if sync is enabled.
    pub fn sync_enabled(&self) -> bool {
        self.enable_sync
    }
}

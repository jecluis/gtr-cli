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

use crate::Result;
use crate::cache::TaskCache;
use crate::config::Config;
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

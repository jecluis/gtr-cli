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

//! Local storage configuration.

use std::fs;
use std::path::PathBuf;

use crate::Result;

/// Configuration for local file storage.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Base cache directory for all local task files.
    pub cache_dir: PathBuf,

    /// User identifier.
    pub user_id: String,

    /// Partition threshold (0 = disabled, flat layout).
    pub partition_threshold: usize,
}

impl StorageConfig {
    pub fn new(cache_dir: PathBuf, user_id: String) -> Self {
        Self {
            cache_dir,
            user_id,
            partition_threshold: 0, // Flat layout for CLI
        }
    }

    /// Get the base path for a project's tasks.
    pub fn project_dir(&self, project_id: &str) -> PathBuf {
        self.cache_dir.join(&self.user_id).join(project_id)
    }

    /// Ensure project directory exists.
    pub fn ensure_project_dir(&self, project_id: &str) -> Result<PathBuf> {
        let dir = self.project_dir(project_id);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Get paths for a task's files (.automerge and .md).
    pub fn task_paths(&self, project_id: &str, task_id: &uuid::Uuid) -> TaskPaths {
        let project_dir = self.project_dir(project_id);
        let base = project_dir.join(task_id.to_string());

        TaskPaths {
            automerge: base.with_extension("automerge"),
            markdown: base.with_extension("md"),
        }
    }
}

/// Paths for a task's storage files.
#[derive(Debug, Clone)]
pub struct TaskPaths {
    pub automerge: PathBuf,
    pub markdown: PathBuf,
}

impl TaskPaths {
    pub fn exists(&self) -> bool {
        self.automerge.exists()
    }
}

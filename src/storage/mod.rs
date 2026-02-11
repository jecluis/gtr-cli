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

//! Local file storage for tasks.

pub mod config;

pub use config::{StorageConfig, TaskPaths};

use std::fs;

use crate::Result;
use crate::crdt::TaskDocument;
use crate::models::Task;

/// Local task storage handling .automerge files.
pub struct TaskStorage {
    config: StorageConfig,
}

impl TaskStorage {
    pub fn new(config: StorageConfig) -> Self {
        Self { config }
    }

    /// Create a new task locally.
    pub fn create_task(&self, project_id: &str, task: &Task) -> Result<()> {
        self.config.ensure_project_dir(project_id)?;

        let task_id = uuid::Uuid::parse_str(&task.id)
            .map_err(|e| crate::Error::InvalidInput(format!("invalid task ID: {e}")))?;

        let paths = self.config.task_paths(project_id, &task_id);
        let doc = TaskDocument::new(task)?;
        let bytes = doc.save();

        fs::write(&paths.automerge, bytes)?;

        Ok(())
    }

    /// Load a task from local storage.
    pub fn load_task(&self, project_id: &str, task_id: &str) -> Result<Task> {
        let uuid = uuid::Uuid::parse_str(task_id)
            .map_err(|e| crate::Error::InvalidInput(format!("invalid task ID: {e}")))?;

        let paths = self.config.task_paths(project_id, &uuid);

        if !paths.exists() {
            return Err(crate::Error::TaskNotFound(format!(
                "task {task_id} not found locally"
            )));
        }

        let bytes = fs::read(&paths.automerge)?;
        let doc = TaskDocument::load(&bytes)?;
        doc.to_task()
    }

    /// Update an existing task locally.
    pub fn update_task(&self, project_id: &str, task: &Task) -> Result<()> {
        let task_id = uuid::Uuid::parse_str(&task.id)
            .map_err(|e| crate::Error::InvalidInput(format!("invalid task ID: {e}")))?;

        let paths = self.config.task_paths(project_id, &task_id);

        // Load existing document to preserve CRDT history
        let bytes = fs::read(&paths.automerge)?;
        let mut doc = TaskDocument::load(&bytes)?;

        // Update document with new task data
        doc.update_task(task)?;

        // Save updated document
        let updated_bytes = doc.save();
        fs::write(&paths.automerge, updated_bytes)?;

        Ok(())
    }

    /// Check if a task exists locally.
    pub fn task_exists(&self, project_id: &str, task_id: &str) -> bool {
        if let Ok(uuid) = uuid::Uuid::parse_str(task_id) {
            let paths = self.config.task_paths(project_id, &uuid);
            paths.exists()
        } else {
            false
        }
    }

    /// Merge remote task bytes into local task.
    pub fn merge_task(&self, project_id: &str, task_id: &str, remote_bytes: &[u8]) -> Result<Task> {
        let uuid = uuid::Uuid::parse_str(task_id)
            .map_err(|e| crate::Error::InvalidInput(format!("invalid task ID: {e}")))?;

        let paths = self.config.task_paths(project_id, &uuid);

        // Load local document
        let local_bytes = fs::read(&paths.automerge)?;
        let mut local_doc = TaskDocument::load(&local_bytes)?;

        // Load remote document
        let mut remote_doc = TaskDocument::load(remote_bytes)?;

        // Merge
        local_doc.merge(&mut remote_doc)?;

        // Save merged result
        let merged_bytes = local_doc.save();
        fs::write(&paths.automerge, merged_bytes)?;

        // Return merged task
        local_doc.to_task()
    }

    /// Get task bytes for syncing to server.
    pub fn get_task_bytes(&self, project_id: &str, task_id: &str) -> Result<Vec<u8>> {
        let uuid = uuid::Uuid::parse_str(task_id)
            .map_err(|e| crate::Error::InvalidInput(format!("invalid task ID: {e}")))?;

        let paths = self.config.task_paths(project_id, &uuid);
        Ok(fs::read(&paths.automerge)?)
    }

    /// Save CRDT bytes directly (for pull sync).
    pub fn save_task_bytes(&self, project_id: &str, task_id: &str, bytes: &[u8]) -> Result<()> {
        self.config.ensure_project_dir(project_id)?;

        let uuid = uuid::Uuid::parse_str(task_id)
            .map_err(|e| crate::Error::InvalidInput(format!("invalid task ID: {e}")))?;

        let paths = self.config.task_paths(project_id, &uuid);
        fs::write(&paths.automerge, bytes)?;

        Ok(())
    }
}

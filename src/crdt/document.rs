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

//! Automerge CRDT document wrapper for tasks.

use automerge::{Automerge, ObjType, ROOT, ReadDoc, transaction::Transactable};

use crate::models::Task;
use crate::{Error, Result};

/// A task represented as an Automerge CRDT document.
pub struct TaskDocument {
    doc: Automerge,
}

impl TaskDocument {
    /// Create a new document from a task.
    pub fn new(task: &Task) -> Result<Self> {
        let mut doc = Automerge::new();

        // Serialize complex fields before transaction
        let custom_json = serde_json::to_string(&task.custom)
            .map_err(|e| Error::Storage(format!("custom serialization failed: {e}")))?;
        let log_json = serde_json::to_string(&task.log)
            .map_err(|e| Error::Storage(format!("log serialization failed: {e}")))?;

        doc.transact::<_, _, automerge::AutomergeError>(|tx| {
            // Create metadata map
            let meta = tx.put_object(ROOT, "metadata", ObjType::Map)?;

            tx.put(&meta, "id", task.id.as_str())?;
            tx.put(&meta, "project_id", task.project_id.as_str())?;
            tx.put(&meta, "priority", task.priority.as_str())?;
            tx.put(&meta, "size", task.size.as_str())?;
            tx.put(&meta, "created", task.created.as_str())?;
            tx.put(&meta, "modified", task.modified.as_str())?;
            tx.put(&meta, "version", task.version as i64)?;

            if let Some(ref done) = task.done {
                tx.put(&meta, "done", done.as_str())?;
            }

            if let Some(ref deleted) = task.deleted {
                tx.put(&meta, "deleted", deleted.as_str())?;
            }

            if let Some(ref deadline) = task.deadline {
                tx.put(&meta, "deadline", deadline.as_str())?;
            }

            if let Some(ref work_state) = task.current_work_state {
                tx.put(&meta, "current_work_state", work_state.as_str())?;
            }

            // Subtasks list
            let subtasks = tx.put_object(&meta, "subtasks", ObjType::List)?;
            for (i, subtask_id) in task.subtasks.iter().enumerate() {
                tx.insert(&subtasks, i, subtask_id.as_str())?;
            }

            // Custom fields (stored as JSON string)
            tx.put(&meta, "custom", custom_json.as_str())?;

            // Log (stored as JSON string)
            tx.put(&meta, "log", log_json.as_str())?;

            // Title and body
            tx.put(ROOT, "title", task.title.as_str())?;
            tx.put(ROOT, "body", task.body.as_str())?;

            Ok(())
        })
        .map_err(|e| Error::Storage(format!("failed to create document: {e:?}")))?;

        Ok(Self { doc })
    }

    /// Load a document from bytes.
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let doc = Automerge::load(bytes)
            .map_err(|e| Error::Storage(format!("failed to load document: {e:?}")))?;
        Ok(Self { doc })
    }

    /// Save document to bytes.
    pub fn save(&self) -> Vec<u8> {
        self.doc.save()
    }

    /// Convert document to Task.
    pub fn to_task(&self) -> Result<Task> {
        let meta_id = match self.doc.get(ROOT, "metadata") {
            Ok(Some((automerge::Value::Object(automerge::ObjType::Map), obj_id))) => obj_id,
            _ => return Err(Error::Storage("missing metadata map".to_string())),
        };

        let id = self.get_str(&meta_id, "id")?;
        let project_id = self.get_str(&meta_id, "project_id")?;
        let title = self.get_str(&ROOT, "title")?;
        let body = self.get_str(&ROOT, "body")?;
        let priority = self.get_str(&meta_id, "priority")?;
        let size = self.get_str(&meta_id, "size")?;
        let created = self.get_str(&meta_id, "created")?;
        let modified = self.get_str(&meta_id, "modified")?;
        let version = self.get_i64(&meta_id, "version")? as u64;

        let done = self.try_get_str(&meta_id, "done")?;
        let deleted = self.try_get_str(&meta_id, "deleted")?;
        let deadline = self.try_get_str(&meta_id, "deadline")?;
        let current_work_state = self.try_get_str(&meta_id, "current_work_state")?;

        // Parse subtasks
        let subtasks = self.read_subtasks(&meta_id)?;

        // Parse custom fields
        let custom_json = self.get_str(&meta_id, "custom")?;
        let custom: serde_json::Value = serde_json::from_str(&custom_json)
            .map_err(|e| Error::Storage(format!("invalid custom JSON: {e}")))?;

        // Parse log
        let log_json = self.get_str(&meta_id, "log")?;
        let log = serde_json::from_str(&log_json)
            .map_err(|e| Error::Storage(format!("invalid log JSON: {e}")))?;

        Ok(Task {
            id,
            project_id,
            title,
            body,
            priority,
            size,
            created,
            modified,
            done,
            deleted,
            deadline,
            version,
            subtasks,
            custom,
            log,
            current_work_state,
        })
    }

    /// Merge another document into this one.
    pub fn merge(&mut self, other: &mut TaskDocument) -> Result<()> {
        self.doc
            .merge(&mut other.doc)
            .map_err(|e| Error::Storage(format!("merge failed: {e:?}")))?;
        Ok(())
    }

    // Helper methods for reading fields

    fn get_str(&self, obj: &automerge::ObjId, key: &str) -> Result<String> {
        match self.doc.get(obj, key) {
            Ok(Some((automerge::Value::Scalar(s), _))) => s
                .to_str()
                .map(|s| s.to_string())
                .ok_or_else(|| Error::Storage(format!("field {key} is not a string"))),
            _ => Err(Error::Storage(format!("missing field: {key}"))),
        }
    }

    fn try_get_str(&self, obj: &automerge::ObjId, key: &str) -> Result<Option<String>> {
        match self.doc.get(obj, key) {
            Ok(Some((automerge::Value::Scalar(s), _))) => Ok(s.to_str().map(|s| s.to_string())),
            Ok(None) | Ok(Some(_)) => Ok(None),
            Err(e) => Err(Error::Storage(format!("error reading {key}: {e:?}"))),
        }
    }

    fn get_i64(&self, obj: &automerge::ObjId, key: &str) -> Result<i64> {
        match self.doc.get(obj, key) {
            Ok(Some((automerge::Value::Scalar(s), _))) => s
                .to_i64()
                .ok_or_else(|| Error::Storage(format!("invalid i64 field: {key}"))),
            _ => Err(Error::Storage(format!("missing field: {key}"))),
        }
    }

    fn read_subtasks(&self, meta_id: &automerge::ObjId) -> Result<Vec<String>> {
        let subtasks_id = match self.doc.get(meta_id, "subtasks") {
            Ok(Some((automerge::Value::Object(automerge::ObjType::List), obj_id))) => obj_id,
            _ => return Err(Error::Storage("missing subtasks list".to_string())),
        };

        let length = self.doc.length(&subtasks_id);
        let mut result = Vec::new();

        for i in 0..length {
            if let Ok(Some((automerge::Value::Scalar(s), _))) = self.doc.get(&subtasks_id, i) {
                result.push(s.to_string());
            }
        }

        Ok(result)
    }
}

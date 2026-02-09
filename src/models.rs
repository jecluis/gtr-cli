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

//! Domain models matching the server API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Project representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// Task representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub title: String,
    pub body: String,
    pub metadata: TaskMetadata,
}

/// Task metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    pub id: Uuid,
    pub priority: String,
    pub size: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub done: Option<DateTime<Utc>>,
    pub deleted: Option<DateTime<Utc>>,
    pub deadline: Option<DateTime<Utc>>,
    pub version: u64,
    #[serde(default)]
    pub log: Vec<LogEntry>,
    pub current_work_state: Option<WorkState>,
    pub subtasks: Vec<Uuid>,
    pub custom: serde_json::Value,
}

/// A single log entry recording a state change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub entry_type: LogEntryType,
    pub source: LogSource,
}

/// Source of a log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogSource {
    User,
    System { reason: String },
    Import,
}

/// Type of log entry describing what changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogEntryType {
    PriorityChanged {
        from: String,
        to: String,
    },
    DeadlineChanged {
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    },
    StatusChanged {
        status: TaskStatus,
    },
    SizeChanged {
        from: String,
        to: String,
    },
    WorkStateChanged {
        state: WorkState,
    },
    TitleChanged {
        from: String,
        to: String,
    },
    BodyChanged,
}

/// Task status for logging.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Done,
    Deleted,
    Restored,
}

/// Work state for time tracking.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Doing,
    Stopped,
}

/// Request to create a project.
#[derive(Debug, Serialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
}

/// Request to create a task.
#[derive(Debug, Serialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub body: String,
    pub priority: String,
    pub size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
}

/// Request to update a task.
#[derive(Debug, Serialize)]
pub struct UpdateTaskRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
}

impl Task {
    /// Check if task is pending (not done and not deleted).
    pub fn is_pending(&self) -> bool {
        self.metadata.done.is_none() && self.metadata.deleted.is_none()
    }

    /// Check if task is done (completed successfully).
    pub fn is_done(&self) -> bool {
        self.metadata.done.is_some()
    }

    /// Check if task is deleted (tombstone).
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted.is_some()
    }
}

// -- Config models --

/// Configuration response from server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub deadline_thresholds: std::collections::HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<ConfigOverrides>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<std::collections::HashMap<String, String>>,
}

/// Configuration overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOverrides {
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub deadline_thresholds: std::collections::HashMap<String, String>,
}

/// Configuration update request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_thresholds: Option<std::collections::HashMap<String, Option<String>>>,
}

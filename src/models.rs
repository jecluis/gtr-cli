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

use crate::icons::Icons;

/// Project representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub deleted: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Task representation (matches server's TaskResponse).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub body: String,
    pub priority: String,
    pub size: String,
    pub created: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    pub version: u64,
    #[serde(default)]
    pub subtasks: Vec<String>,
    #[serde(default)]
    pub custom: serde_json::Value,
    #[serde(default)]
    pub log: Vec<LogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_work_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub progress: Option<u8>,
    #[serde(default = "default_impact")]
    pub impact: u8,
    #[serde(default = "default_joy")]
    pub joy: u8,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

fn default_impact() -> u8 {
    3
}

fn default_joy() -> u8 {
    5
}

/// A single log entry recording a state change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub entry_type: LogEntryType,
    pub source: LogSource,
}

/// Source of a log entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogSource {
    User,
    System { reason: String },
    Import,
}

/// Type of log entry describing what changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    ProgressChanged {
        from: Option<u8>,
        to: Option<u8>,
    },
    ImpactChanged {
        from: u8,
        to: u8,
    },
    JoyChanged {
        from: u8,
        to: u8,
    },
}

/// Task status for logging.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Done,
    Deleted,
    Restored,
}

/// Work state for time tracking.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Doing,
    Stopped,
}

/// Request to create a project.
#[derive(Debug, Serialize)]
pub struct CreateProjectRequest {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Request to update a project.
#[derive(Debug, Serialize)]
pub struct UpdateProjectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joy: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Request to move a task to another project.
#[derive(Debug, Serialize)]
pub struct MoveTaskRequest {
    pub target_project_id: String,
}

/// Request to update a task.
#[derive(Debug, Default, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joy: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

impl Task {
    /// Check if task is pending (not done and not deleted).
    pub fn is_pending(&self) -> bool {
        self.done.is_none() && self.deleted.is_none()
    }

    /// Check if task is done (completed successfully).
    pub fn is_done(&self) -> bool {
        self.done.is_some()
    }

    /// Check if task is deleted (tombstone).
    pub fn is_deleted(&self) -> bool {
        self.deleted.is_some()
    }

    /// Check if this task is a bookmark (via `custom.is_bookmark`).
    pub fn is_bookmark(&self) -> bool {
        self.custom
            .get("is_bookmark")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Return the display title, prepending the bookmark glyph when appropriate.
    pub fn display_title(&self, icons: &Icons) -> String {
        if self.is_bookmark() {
            format!("{}{}", icons.bookmark, self.title)
        } else {
            self.title.clone()
        }
    }
}

// -- Version models --

/// Version information from server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub name: String,
    pub version: String,
    pub git_sha: String,
}

// -- Config models --

/// Resolved impact configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactConfigResolved {
    pub labels: std::collections::HashMap<String, String>,
    pub multipliers: std::collections::HashMap<String, f64>,
}

/// Resolved promotion thresholds (all categories merged with defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionThresholdsResolved {
    pub deadline: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub impact: Option<ImpactConfigResolved>,
}

/// Configuration response from server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub promotion_thresholds: PromotionThresholdsResolved,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<ConfigOverrides>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults: Option<PromotionThresholdsResolved>,
}

/// Impact configuration in overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImpactConfig {
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub labels: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub multipliers: std::collections::HashMap<String, f64>,
}

/// Promotion thresholds in overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromotionThresholds {
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub deadline: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub impact: ImpactConfig,
}

/// Configuration overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOverrides {
    #[serde(default)]
    pub promotion_thresholds: PromotionThresholds,
}

/// Update for impact config in API requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactConfigUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, Option<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multipliers: Option<std::collections::HashMap<String, Option<f64>>>,
}

/// Update for promotion thresholds in API requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionThresholdsUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<std::collections::HashMap<String, Option<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<ImpactConfigUpdate>,
}

/// Configuration update request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_thresholds: Option<PromotionThresholdsUpdate>,
}

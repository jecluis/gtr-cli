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

//! Shared task mutation functions used by both CLI commands and TUI.
//!
//! Each function follows the pattern: load → mutate → persist to
//! storage + cache. Callers handle sync, output, and UI concerns.

use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

use crate::Result;
use crate::cache::TaskCache;
use crate::hierarchy;
use crate::models::{LogEntry, LogEntryType, LogSource, Task, TaskStatus, WorkState};
use crate::storage::TaskStorage;

/// Result of a start/stop/toggle work state mutation.
pub struct WorkStateResult {
    pub task: Task,
    pub was_noop: bool,
}

/// Result of marking a task as done.
pub struct DoneResult {
    pub task: Task,
    pub descendants_completed: usize,
}

/// Result of deleting a task.
pub struct DeleteResult {
    pub task: Task,
    pub children_promoted: usize,
}

/// Result of changing task priority.
pub struct PriorityResult {
    pub task: Task,
    pub old_priority: String,
}

/// Start working on a task (set work state to "doing").
///
/// Auto-sets progress to 0% if not already set.
pub fn start_task(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
) -> Result<WorkStateResult> {
    let mut task = storage.load_task(task_id)?;

    if task.current_work_state.as_deref() == Some("doing") {
        return Ok(WorkStateResult {
            task,
            was_noop: true,
        });
    }

    let now = Utc::now();
    task.current_work_state = Some("doing".to_string());
    task.modified = now.to_rfc3339();
    task.version += 1;

    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::WorkStateChanged {
            state: WorkState::Doing,
        },
        source: LogSource::User,
    });

    if task.progress.is_none() {
        let old_progress = task.progress;
        task.progress = Some(0);
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::ProgressChanged {
                from: old_progress,
                to: Some(0),
            },
            source: LogSource::User,
        });
    }

    storage.update_task(&task)?;
    cache.upsert_task(&task, true)?;

    Ok(WorkStateResult {
        task,
        was_noop: false,
    })
}

/// Stop working on a task (set work state to "stopped").
pub fn stop_task(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
) -> Result<WorkStateResult> {
    let mut task = storage.load_task(task_id)?;

    if task.current_work_state.as_deref() != Some("doing") {
        return Ok(WorkStateResult {
            task,
            was_noop: true,
        });
    }

    let now = Utc::now();
    task.current_work_state = Some("stopped".to_string());
    task.modified = now.to_rfc3339();
    task.version += 1;

    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::WorkStateChanged {
            state: WorkState::Stopped,
        },
        source: LogSource::User,
    });

    storage.update_task(&task)?;
    cache.upsert_task(&task, true)?;

    Ok(WorkStateResult {
        task,
        was_noop: false,
    })
}

/// Toggle between doing and stopped/pending.
///
/// If currently "doing", stops the task. Otherwise, starts it.
pub fn toggle_work_state(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
) -> Result<WorkStateResult> {
    let task = storage.load_task(task_id)?;
    if task.current_work_state.as_deref() == Some("doing") {
        stop_task(storage, cache, task_id)
    } else {
        start_task(storage, cache, task_id)
    }
}

/// Mark a task as done.
///
/// Sets done timestamp, clears work state, sets progress to 100%,
/// cascades completion to all descendants, and updates ancestor
/// progress.
pub fn mark_done(storage: &TaskStorage, cache: &TaskCache, task_id: &str) -> Result<DoneResult> {
    let mut task = storage.load_task(task_id)?;
    let now = Utc::now();

    task.done = Some(now.to_rfc3339());
    task.modified = now.to_rfc3339();
    task.version += 1;
    task.current_work_state = None;

    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::StatusChanged {
            status: TaskStatus::Done,
        },
        source: LogSource::User,
    });

    let old_progress = task.progress;
    task.progress = Some(100);
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::ProgressChanged {
            from: old_progress,
            to: Some(100),
        },
        source: LogSource::User,
    });

    storage.update_task(&task)?;
    cache.upsert_task(&task, true)?;

    // Cascade to descendants
    let descendants = cache.get_all_descendants(task_id)?;
    let mut descendants_completed = 0;
    for desc_id in &descendants {
        match storage.load_task(desc_id) {
            Ok(mut desc_task) => {
                if desc_task.done.is_some() {
                    continue;
                }
                desc_task.done = Some(now.to_rfc3339());
                desc_task.modified = now.to_rfc3339();
                desc_task.version += 1;
                desc_task.current_work_state = None;
                let old_prog = desc_task.progress;
                desc_task.progress = Some(100);
                desc_task.log.push(LogEntry {
                    timestamp: now,
                    entry_type: LogEntryType::StatusChanged {
                        status: TaskStatus::Done,
                    },
                    source: LogSource::User,
                });
                desc_task.log.push(LogEntry {
                    timestamp: now,
                    entry_type: LogEntryType::ProgressChanged {
                        from: old_prog,
                        to: Some(100),
                    },
                    source: LogSource::User,
                });
                storage.update_task(&desc_task)?;
                cache.upsert_task(&desc_task, true)?;
                descendants_completed += 1;
            }
            Err(e) => {
                warn!(task_id = %desc_id, error = %e, "failed to cascade done to descendant");
            }
        }
    }

    hierarchy::update_ancestor_progress(cache, storage, task_id)?;

    Ok(DoneResult {
        task,
        descendants_completed,
    })
}

/// Delete a task (tombstone).
///
/// Direct children are promoted to the deleted task's parent.
pub fn delete_task(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
) -> Result<DeleteResult> {
    let mut task = storage.load_task(task_id)?;
    let now = Utc::now();

    let deleted_parent_id = task.parent_id.clone();

    task.deleted = Some(now.to_rfc3339());
    task.modified = now.to_rfc3339();
    task.version += 1;

    storage.update_task(&task)?;
    cache.upsert_task(&task, true)?;

    // Promote direct children to the deleted task's parent
    let children = cache.get_children(task_id)?;
    let mut children_promoted = 0;
    for child_summary in &children {
        match storage.load_task(&child_summary.id) {
            Ok(mut child_task) => {
                child_task.parent_id = deleted_parent_id.clone();
                child_task.modified = now.to_rfc3339();
                child_task.version += 1;
                storage.update_task(&child_task)?;
                cache.upsert_task(&child_task, true)?;
                children_promoted += 1;
            }
            Err(e) => {
                warn!(task_id = %child_summary.id, error = %e, "failed to promote child");
            }
        }
    }

    Ok(DeleteResult {
        task,
        children_promoted,
    })
}

/// Set task priority to a specific value.
pub fn set_priority(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
    priority: &str,
) -> Result<PriorityResult> {
    let mut task = storage.load_task(task_id)?;
    let old_priority = task.priority.clone();

    task.priority = priority.to_string();
    task.modified = Utc::now().to_rfc3339();
    task.version += 1;

    storage.update_task(&task)?;
    cache.upsert_task(&task, true)?;

    Ok(PriorityResult { task, old_priority })
}

/// Toggle priority between "now" and "later".
pub fn toggle_priority(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
) -> Result<PriorityResult> {
    let task = storage.load_task(task_id)?;
    let new_priority = if task.priority == "now" {
        "later"
    } else {
        "now"
    };
    set_priority(storage, cache, task_id, new_priority)
}

/// Update a task's title and/or body.
pub fn update_body(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
    title: Option<String>,
    body: String,
) -> Result<Task> {
    let mut task = storage.load_task(task_id)?;

    if let Some(new_title) = title {
        task.title = new_title;
    }
    task.body = body;
    task.modified = Utc::now().to_rfc3339();
    task.version += 1;

    storage.update_task(&task)?;
    cache.upsert_task(&task, true)?;

    Ok(task)
}

/// Update specific fields of an existing task.
///
/// Only fields with `Some` values are modified. Returns the updated
/// task. For `parent_id`, `Some(None)` clears the parent while
/// `Some(Some(id))` sets a new parent.
#[allow(clippy::too_many_arguments)]
pub fn update_task(
    storage: &TaskStorage,
    cache: &TaskCache,
    task_id: &str,
    title: Option<String>,
    priority: Option<String>,
    size: Option<String>,
    impact: Option<u8>,
    joy: Option<u8>,
    labels: Option<Vec<String>>,
    parent_id: Option<Option<String>>,
) -> Result<Task> {
    let mut task = storage.load_task(task_id)?;
    let now = Utc::now();

    if let Some(new_title) = title {
        let old = task.title.clone();
        task.title = new_title;
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::TitleChanged {
                from: old,
                to: task.title.clone(),
            },
            source: LogSource::User,
        });
    }

    if let Some(new_priority) = priority {
        let old = task.priority.clone();
        task.priority = new_priority;
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::PriorityChanged {
                from: old,
                to: task.priority.clone(),
            },
            source: LogSource::User,
        });
    }

    if let Some(new_size) = size {
        let old = task.size.clone();
        task.size = new_size;
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::SizeChanged {
                from: old,
                to: task.size.clone(),
            },
            source: LogSource::User,
        });
    }

    if let Some(new_impact) = impact {
        let old = task.impact;
        task.impact = new_impact;
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::ImpactChanged {
                from: old,
                to: new_impact,
            },
            source: LogSource::User,
        });
    }

    if let Some(new_joy) = joy {
        let old = task.joy;
        task.joy = new_joy;
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::JoyChanged {
                from: old,
                to: new_joy,
            },
            source: LogSource::User,
        });
    }

    if let Some(new_labels) = labels {
        task.labels = new_labels;
    }

    if let Some(new_parent) = parent_id {
        let old_parent = task.parent_id.clone();
        task.parent_id = new_parent;

        // Update ancestor progress for both old and new parents
        task.modified = now.to_rfc3339();
        task.version += 1;
        storage.update_task(&task)?;
        cache.upsert_task(&task, true)?;

        if let Some(ref old_pid) = old_parent {
            hierarchy::update_ancestor_progress(cache, storage, old_pid)?;
        }
        if let Some(ref new_pid) = task.parent_id {
            hierarchy::update_ancestor_progress(cache, storage, new_pid)?;
        }

        return Ok(task);
    }

    task.modified = now.to_rfc3339();
    task.version += 1;
    storage.update_task(&task)?;
    cache.upsert_task(&task, true)?;

    Ok(task)
}

/// Create a new task.
///
/// Basic fields (title, priority, size) are required. Extended fields
/// (impact, joy, labels, parent_id) are optional and fall back to
/// defaults when `None`.
#[allow(clippy::too_many_arguments)]
pub fn create_task(
    storage: &TaskStorage,
    cache: &TaskCache,
    project_id: &str,
    title: &str,
    priority: &str,
    size: &str,
    impact: Option<u8>,
    joy: Option<u8>,
    labels: Vec<String>,
    parent_id: Option<String>,
) -> Result<Task> {
    let task_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let task = Task {
        id: task_id,
        project_id: project_id.to_string(),
        title: title.to_string(),
        body: String::new(),
        priority: priority.to_string(),
        size: size.to_string(),
        created: now.clone(),
        modified: now,
        done: None,
        deleted: None,
        deadline: None,
        version: 1,
        subtasks: vec![],
        custom: serde_json::Value::Object(serde_json::Map::new()),
        log: vec![],
        current_work_state: None,
        progress: None,
        impact: impact.unwrap_or(3),
        joy: joy.unwrap_or(5),
        parent_id,
        labels,
        references: vec![],
    };

    storage.create_task(&task)?;
    cache.upsert_task(&task, true)?;

    Ok(task)
}

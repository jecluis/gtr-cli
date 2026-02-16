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

//! Hierarchy helpers for parent/child task relationships.

use chrono::Utc;
use tracing::warn;

use crate::Result;
use crate::cache::TaskCache;
use crate::models::{LogEntry, LogEntryType, LogSource};
use crate::storage::TaskStorage;

/// Recalculate auto-progress for all ancestors of `task_id`.
///
/// Starting from the task's parent, computes progress as
/// `(done_children / total_children) * 100` and walks up the
/// ancestor chain until reaching a root task.
pub fn update_ancestor_progress(
    cache: &TaskCache,
    storage: &TaskStorage,
    project_id: &str,
    task_id: &str,
) -> Result<()> {
    let mut current_id = match cache.get_parent_id(task_id)? {
        Some(pid) => pid,
        None => return Ok(()), // no parent, nothing to update
    };

    let now = Utc::now();

    loop {
        let (total, done) = cache.count_children(&current_id)?;
        if total == 0 {
            break;
        }

        let new_progress = ((done as f64 / total as f64) * 100.0).round() as u8;

        match storage.load_task(project_id, &current_id) {
            Ok(mut parent_task) => {
                let old_progress = parent_task.progress;
                if old_progress != Some(new_progress) {
                    parent_task.progress = Some(new_progress);
                    parent_task.modified = now.to_rfc3339();
                    parent_task.version += 1;
                    parent_task.log.push(LogEntry {
                        timestamp: now,
                        entry_type: LogEntryType::ProgressChanged {
                            from: old_progress,
                            to: Some(new_progress),
                        },
                        source: LogSource::System {
                            reason: "auto-progress from children".to_string(),
                        },
                    });
                    storage.update_task(&parent_task.project_id, &parent_task)?;
                    cache.upsert_task(&parent_task, true)?;
                }
            }
            Err(e) => {
                warn!(
                    task_id = %current_id,
                    error = %e,
                    "failed to update ancestor progress"
                );
                break;
            }
        }

        // Walk up to next ancestor
        match cache.get_parent_id(&current_id)? {
            Some(pid) => current_id = pid,
            None => break,
        }
    }

    Ok(())
}

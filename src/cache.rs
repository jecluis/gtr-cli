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

//! Local SQLite cache for task metadata and sync state.

use std::path::Path;

use chrono::{Local, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};

use crate::models::Task;
use crate::{Error, Result};

/// State of today's feels entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeelsState {
    /// Values have been set.
    Set,
    /// User chose "skip for today".
    Skipped,
    /// User deferred; will re-prompt after the defer_until time.
    Deferred,
}

/// What the feels prompt flow should do.
#[derive(Debug, Clone)]
pub enum FeelsPrompt {
    /// No feels set today — show the initial 3-option picker.
    Initial,
    /// Previously set, 4h+ ago — offer to keep/update/skip.
    Reprompt { energy: u8, focus: u8 },
    /// Nothing to do (already set recently, or skipped/deferred).
    No,
}

/// Cached feels row for today.
#[derive(Debug, Clone)]
pub struct FeelsRow {
    pub energy: u8,
    pub focus: u8,
    pub state: FeelsState,
    pub set_at: Option<String>,
    pub defer_until: Option<String>,
}

/// Local cache for task metadata and sync tracking.
pub struct TaskCache {
    conn: Connection,
}

impl TaskCache {
    /// Open or create the cache database.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| Error::Database(format!("failed to open cache: {e}")))?;

        let cache = Self { conn };
        cache.init_schema()?;
        Ok(cache)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<()> {
        self.conn
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                title TEXT NOT NULL,
                priority TEXT NOT NULL,
                size TEXT NOT NULL,
                created TEXT NOT NULL,
                modified TEXT NOT NULL,
                done TEXT,
                deleted TEXT,
                deadline TEXT,
                version INTEGER NOT NULL,
                needs_push INTEGER NOT NULL DEFAULT 0,
                last_synced TEXT,
                sync_state BLOB,
                impact INTEGER NOT NULL DEFAULT 3,
                joy INTEGER NOT NULL DEFAULT 5,
                current_work_state TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_project ON tasks(project_id);
            CREATE INDEX IF NOT EXISTS idx_priority ON tasks(priority);
            CREATE INDEX IF NOT EXISTS idx_size ON tasks(size);
            CREATE INDEX IF NOT EXISTS idx_needs_push ON tasks(needs_push);
            "#,
            )
            .map_err(|e| Error::Database(format!("schema init failed: {e}")))?;

        // Migrate existing caches: add impact column if missing
        let _ = self.conn.execute(
            "ALTER TABLE tasks ADD COLUMN impact INTEGER NOT NULL DEFAULT 3",
            [],
        );

        // Migrate existing caches: add joy column if missing
        let _ = self.conn.execute(
            "ALTER TABLE tasks ADD COLUMN joy INTEGER NOT NULL DEFAULT 5",
            [],
        );

        // Migrate existing caches: add current_work_state column if missing
        let _ = self
            .conn
            .execute("ALTER TABLE tasks ADD COLUMN current_work_state TEXT", []);

        // Feels table: one row per calendar day tracking energy/focus state
        self.conn
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS feels (
                date TEXT PRIMARY KEY,
                energy INTEGER NOT NULL DEFAULT 0,
                focus INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL DEFAULT 'initial',
                set_at TEXT,
                defer_until TEXT
            );
            "#,
            )
            .map_err(|e| Error::Database(format!("feels schema init failed: {e}")))?;

        Ok(())
    }

    /// Insert or update a task in the cache.
    pub fn upsert_task(&self, task: &Task, needs_push: bool) -> Result<()> {
        self.conn
            .execute(
                r#"
            INSERT INTO tasks (
                id, project_id, title, priority, size, created, modified,
                done, deleted, deadline, version, needs_push, impact, joy,
                current_work_state
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                title = excluded.title,
                priority = excluded.priority,
                size = excluded.size,
                modified = excluded.modified,
                done = excluded.done,
                deleted = excluded.deleted,
                deadline = excluded.deadline,
                version = excluded.version,
                needs_push = excluded.needs_push OR needs_push,
                impact = excluded.impact,
                joy = excluded.joy,
                current_work_state = excluded.current_work_state
            "#,
                params![
                    task.id,
                    task.project_id,
                    task.title,
                    task.priority,
                    task.size,
                    task.created,
                    task.modified,
                    task.done,
                    task.deleted,
                    task.deadline,
                    task.version as i64,
                    needs_push as i64,
                    task.impact as i64,
                    task.joy as i64,
                    task.current_work_state,
                ],
            )
            .map_err(|e| Error::Database(format!("upsert failed: {e}")))?;

        Ok(())
    }

    /// Mark a task as synced.
    pub fn mark_synced(&self, task_id: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks SET needs_push = 0, last_synced = datetime('now') WHERE id = ?1",
                params![task_id],
            )
            .map_err(|e| Error::Database(format!("mark synced failed: {e}")))?;

        Ok(())
    }

    /// Get tasks that need to be pushed to server.
    pub fn get_pending_tasks(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM tasks WHERE needs_push = 1")
            .map_err(|e| Error::Database(format!("prepare failed: {e}")))?;

        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| Error::Database(format!("query failed: {e}")))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(format!("collect failed: {e}")))?;

        Ok(ids)
    }

    /// Check if a task exists in cache.
    pub fn task_exists(&self, task_id: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE id = ?1",
                params![task_id],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("query failed: {e}")))?;

        Ok(count > 0)
    }

    /// Get task metadata from cache (for quick listing).
    pub fn get_task_summary(&self, task_id: &str) -> Result<Option<TaskSummary>> {
        self.conn
            .query_row(
                r#"
            SELECT id, project_id, title, priority, size, created, modified,
                   done, deleted, deadline, needs_push
            FROM tasks WHERE id = ?1
            "#,
                params![task_id],
                |row| {
                    Ok(TaskSummary {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        title: row.get(2)?,
                        priority: row.get(3)?,
                        size: row.get(4)?,
                        created: row.get(5)?,
                        modified: row.get(6)?,
                        done: row.get(7)?,
                        deleted: row.get(8)?,
                        deadline: row.get(9)?,
                        needs_push: row.get::<_, i64>(10)? != 0,
                    })
                },
            )
            .optional()
            .map_err(|e| Error::Database(format!("query failed: {e}")))
    }

    /// List tasks for a project.
    pub fn list_tasks(&self, project_id: &str) -> Result<Vec<TaskSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
            SELECT id, project_id, title, priority, size, created, modified,
                   done, deleted, deadline, needs_push
            FROM tasks
            WHERE project_id = ?1
            ORDER BY modified DESC
            "#,
            )
            .map_err(|e| Error::Database(format!("prepare failed: {e}")))?;

        let tasks = stmt
            .query_map(params![project_id], |row| {
                Ok(TaskSummary {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    priority: row.get(3)?,
                    size: row.get(4)?,
                    created: row.get(5)?,
                    modified: row.get(6)?,
                    done: row.get(7)?,
                    deleted: row.get(8)?,
                    deadline: row.get(9)?,
                    needs_push: row.get::<_, i64>(10)? != 0,
                })
            })
            .map_err(|e| Error::Database(format!("query failed: {e}")))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(format!("collect failed: {e}")))?;

        Ok(tasks)
    }

    /// List all task IDs for multiple projects.
    pub fn list_task_ids(&self, project_ids: &[String]) -> Result<Vec<String>> {
        if project_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = project_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "SELECT id FROM tasks WHERE project_id IN ({}) ORDER BY modified DESC",
            placeholders
        );

        let mut stmt = self
            .conn
            .prepare(&query)
            .map_err(|e| Error::Database(format!("prepare failed: {e}")))?;

        let params: Vec<&dyn rusqlite::ToSql> = project_ids
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();

        let ids = stmt
            .query_map(&params[..], |row| row.get::<_, String>(0))
            .map_err(|e| Error::Database(format!("query failed: {e}")))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(format!("collect failed: {e}")))?;

        Ok(ids)
    }

    /// Save sync state for a task.
    pub fn save_sync_state(&self, task_id: &str, state_bytes: &[u8]) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks SET sync_state = ?1 WHERE id = ?2",
                params![state_bytes, task_id],
            )
            .map_err(|e| Error::Database(format!("save sync state failed: {e}")))?;

        Ok(())
    }

    /// Load sync state for a task.
    pub fn load_sync_state(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        match self.conn.query_row(
            "SELECT sync_state FROM tasks WHERE id = ?1",
            params![task_id],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        ) {
            Ok(state) => Ok(state),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Database(format!("load sync state failed: {e}"))),
        }
    }
    // -- Status queries --

    /// Count tasks completed today (local date).
    pub fn count_done_today(&self) -> Result<i64> {
        let today = chrono::Local::now().date_naive().to_string();
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE done IS NOT NULL AND done LIKE ?1",
                params![format!("{today}%")],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("count done today failed: {e}")))
    }

    /// Get tasks with an active work state (doing or stopped), excluding done/deleted.
    pub fn get_active_work_tasks(&self) -> Result<Vec<ActiveTask>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
            SELECT id, project_id, title, priority, size, current_work_state, modified
            FROM tasks
            WHERE current_work_state IS NOT NULL
              AND done IS NULL AND deleted IS NULL
            ORDER BY
                CASE current_work_state WHEN 'doing' THEN 0 ELSE 1 END,
                modified DESC
            "#,
            )
            .map_err(|e| Error::Database(format!("prepare failed: {e}")))?;

        let tasks = stmt
            .query_map([], |row| {
                Ok(ActiveTask {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    priority: row.get(3)?,
                    size: row.get(4)?,
                    work_state: row.get(5)?,
                    modified: row.get(6)?,
                })
            })
            .map_err(|e| Error::Database(format!("query failed: {e}")))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Database(format!("collect failed: {e}")))?;

        Ok(tasks)
    }

    /// Count overdue tasks (deadline before now, not done/deleted).
    pub fn count_overdue(&self) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .query_row(
                r#"
            SELECT COUNT(*) FROM tasks
            WHERE deadline IS NOT NULL AND deadline < ?1
              AND done IS NULL AND deleted IS NULL
            "#,
                params![now],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("count overdue failed: {e}")))
    }

    /// Count tasks due today (deadline is today, not yet overdue, not done/deleted).
    pub fn count_due_today(&self) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        let end_of_today = {
            let today = chrono::Local::now().date_naive();
            let tomorrow = today + chrono::Duration::days(1);
            format!("{tomorrow}T00:00:00+00:00")
        };
        self.conn
            .query_row(
                r#"
            SELECT COUNT(*) FROM tasks
            WHERE deadline IS NOT NULL
              AND deadline >= ?1 AND deadline < ?2
              AND done IS NULL AND deleted IS NULL
            "#,
                params![now, end_of_today],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("count due today failed: {e}")))
    }

    /// Count tasks pending sync.
    pub fn count_pending_sync(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE needs_push = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| Error::Database(format!("count pending sync failed: {e}")))
    }

    // -- Feels operations --

    /// Set today's energy and focus values.
    pub fn upsert_feels(&self, date: &NaiveDate, energy: u8, focus: u8) -> Result<()> {
        let now = Local::now().to_rfc3339();
        self.conn
            .execute(
                r#"
            INSERT INTO feels (date, energy, focus, state, set_at)
            VALUES (?1, ?2, ?3, 'set', ?4)
            ON CONFLICT(date) DO UPDATE SET
                energy = excluded.energy,
                focus = excluded.focus,
                state = 'set',
                set_at = excluded.set_at,
                defer_until = NULL
            "#,
                params![date.to_string(), energy as i64, focus as i64, now],
            )
            .map_err(|e| Error::Database(format!("upsert feels failed: {e}")))?;

        Ok(())
    }

    /// Get today's feels state.
    pub fn get_today_feels(&self, today: &NaiveDate) -> Result<Option<FeelsRow>> {
        self.conn
            .query_row(
                "SELECT energy, focus, state, set_at, defer_until FROM feels WHERE date = ?1",
                params![today.to_string()],
                |row| {
                    let state_str: String = row.get(2)?;
                    let state = match state_str.as_str() {
                        "set" => FeelsState::Set,
                        "skipped" => FeelsState::Skipped,
                        "deferred" => FeelsState::Deferred,
                        _ => FeelsState::Set,
                    };
                    Ok(FeelsRow {
                        energy: row.get::<_, i64>(0)? as u8,
                        focus: row.get::<_, i64>(1)? as u8,
                        state,
                        set_at: row.get(3)?,
                        defer_until: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| Error::Database(format!("get feels failed: {e}")))
    }

    /// Mark today as "skipped" (no re-prompt for the rest of the day).
    pub fn mark_feels_skipped(&self, today: &NaiveDate) -> Result<()> {
        self.conn
            .execute(
                r#"
            INSERT INTO feels (date, energy, focus, state)
            VALUES (?1, 0, 0, 'skipped')
            ON CONFLICT(date) DO UPDATE SET
                state = 'skipped',
                defer_until = NULL
            "#,
                params![today.to_string()],
            )
            .map_err(|e| Error::Database(format!("mark skipped failed: {e}")))?;

        Ok(())
    }

    /// Defer the feels prompt for 1 hour from now.
    pub fn mark_feels_deferred(&self, today: &NaiveDate) -> Result<()> {
        let defer_until = (Local::now() + chrono::Duration::hours(1)).to_rfc3339();
        self.conn
            .execute(
                r#"
            INSERT INTO feels (date, energy, focus, state, defer_until)
            VALUES (?1, 0, 0, 'deferred', ?2)
            ON CONFLICT(date) DO UPDATE SET
                state = 'deferred',
                defer_until = excluded.defer_until
            "#,
                params![today.to_string(), defer_until],
            )
            .map_err(|e| Error::Database(format!("mark deferred failed: {e}")))?;

        Ok(())
    }

    /// Determine whether to prompt for feels and what kind of prompt.
    pub fn should_prompt_feels(&self, today: &NaiveDate) -> Result<FeelsPrompt> {
        let row = self.get_today_feels(today)?;

        let Some(row) = row else {
            return Ok(FeelsPrompt::Initial);
        };

        match row.state {
            FeelsState::Skipped => Ok(FeelsPrompt::No),
            FeelsState::Deferred => {
                // Check if defer period has elapsed
                if let Some(ref until) = row.defer_until
                    && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(until)
                    && Local::now() < dt
                {
                    return Ok(FeelsPrompt::No);
                }
                // Defer expired → show initial prompt
                Ok(FeelsPrompt::Initial)
            }
            FeelsState::Set => {
                // Check if 4h have passed since set_at
                if let Some(ref set_at) = row.set_at
                    && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(set_at)
                    && Local::now().signed_duration_since(dt) >= chrono::Duration::hours(4)
                {
                    return Ok(FeelsPrompt::Reprompt {
                        energy: row.energy,
                        focus: row.focus,
                    });
                }
                Ok(FeelsPrompt::No)
            }
        }
    }
}

/// A task with an active work state (doing/stopped).
#[derive(Debug, Clone)]
pub struct ActiveTask {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub priority: String,
    pub size: String,
    pub work_state: String,
    pub modified: String,
}

/// Summary of a task from the cache (for listing).
#[derive(Debug, Clone)]
pub struct TaskSummary {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub priority: String,
    pub size: String,
    pub created: String,
    pub modified: String,
    pub done: Option<String>,
    pub deleted: Option<String>,
    pub deadline: Option<String>,
    pub needs_push: bool,
}

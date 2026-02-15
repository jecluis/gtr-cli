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

use rusqlite::{Connection, OptionalExtension, params};

use crate::models::Task;
use crate::{Error, Result};

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
                joy INTEGER NOT NULL DEFAULT 5
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

        Ok(())
    }

    /// Insert or update a task in the cache.
    pub fn upsert_task(&self, task: &Task, needs_push: bool) -> Result<()> {
        self.conn
            .execute(
                r#"
            INSERT INTO tasks (
                id, project_id, title, priority, size, created, modified,
                done, deleted, deadline, version, needs_push
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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
                needs_push = excluded.needs_push OR needs_push
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

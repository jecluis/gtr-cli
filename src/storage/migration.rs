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

//! Migration from per-project storage layout to flat layout.
//!
//! Old: `<cache_dir>/<user_id>/<project_id>/<uuid>.automerge`
//! New: `<cache_dir>/tasks/<uuid>.automerge`

use std::fs;
use std::path::Path;

use tracing::{info, warn};

use super::StorageConfig;

/// Migrate from per-project directories to flat `tasks/` layout.
///
/// Scans `<cache_dir>/<user_id>/` for project directories, moves all
/// `.automerge` files to `<cache_dir>/tasks/`, then removes empty
/// project directories. Idempotent — safe to run multiple times.
pub fn migrate_to_flat_layout(config: &StorageConfig) -> crate::Result<()> {
    let user_dir = config.user_dir();
    if !user_dir.exists() {
        return Ok(());
    }

    let tasks_dir = config.tasks_dir();
    fs::create_dir_all(&tasks_dir)?;

    let entries = match fs::read_dir(&user_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    let mut moved = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Skip the "tasks" directory itself (already flat)
        if path.file_name().and_then(|n| n.to_str()) == Some("tasks") {
            continue;
        }

        // Move all .automerge files from this project dir
        moved += move_files_from_dir(&path, &tasks_dir)?;

        // Remove empty directory
        remove_if_empty(&path);
    }

    // Remove user_dir if it's now empty
    remove_if_empty(&user_dir);

    if moved > 0 {
        info!(
            moved,
            "migrated .automerge files from per-project dirs to flat layout"
        );
    }

    Ok(())
}

/// Move all .automerge files from `src_dir` to `dest_dir`.
fn move_files_from_dir(src_dir: &Path, dest_dir: &Path) -> crate::Result<u32> {
    let mut count = 0;
    let entries = match fs::read_dir(src_dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension()
            && ext == "automerge"
        {
            let file_name = path.file_name().unwrap();
            let dest = dest_dir.join(file_name);
            if !dest.exists() {
                fs::rename(&path, &dest)?;
                count += 1;
            } else {
                warn!(
                    src = %path.display(),
                    dest = %dest.display(),
                    "skipping: destination already exists"
                );
            }
        }
    }

    Ok(count)
}

/// Remove directory if it's empty.
fn remove_if_empty(dir: &Path) {
    if let Ok(mut entries) = fs::read_dir(dir)
        && entries.next().is_none()
    {
        let _ = fs::remove_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn test_config(dir: &Path) -> StorageConfig {
        StorageConfig::new(dir.to_path_buf(), "default".to_string())
    }

    #[test]
    fn migrate_moves_automerge_files() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());

        // Create old-layout files
        let old_project_dir = temp.path().join("default").join("my-project");
        fs::create_dir_all(&old_project_dir).unwrap();

        let task_id = uuid::Uuid::new_v4();
        fs::write(
            old_project_dir.join(format!("{task_id}.automerge")),
            b"crdt-data",
        )
        .unwrap();

        migrate_to_flat_layout(&config).unwrap();

        // File should be in tasks/
        let new_path = temp
            .path()
            .join("tasks")
            .join(format!("{task_id}.automerge"));
        assert!(new_path.exists());

        // Old project dir should be removed
        assert!(!old_project_dir.exists());
    }

    #[test]
    fn migrate_is_idempotent() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());

        // Create a file already in the flat layout
        let tasks_dir = temp.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let task_id = uuid::Uuid::new_v4();
        fs::write(tasks_dir.join(format!("{task_id}.automerge")), b"existing").unwrap();

        // Run migration (no old dirs exist)
        migrate_to_flat_layout(&config).unwrap();

        // File should still be there
        let path = tasks_dir.join(format!("{task_id}.automerge"));
        assert!(path.exists());
        assert_eq!(fs::read(&path).unwrap(), b"existing");
    }

    #[test]
    fn migrate_skips_duplicate_files() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());

        let task_id = uuid::Uuid::new_v4();

        // Create file in old layout
        let old_dir = temp.path().join("default").join("proj");
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join(format!("{task_id}.automerge")), b"old-data").unwrap();

        // Create file already in flat layout
        let tasks_dir = temp.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();
        fs::write(tasks_dir.join(format!("{task_id}.automerge")), b"new-data").unwrap();

        migrate_to_flat_layout(&config).unwrap();

        // The flat layout file should be untouched
        let path = tasks_dir.join(format!("{task_id}.automerge"));
        assert_eq!(fs::read(&path).unwrap(), b"new-data");
    }

    #[test]
    fn migrate_no_old_dirs_is_noop() {
        let temp = tempdir().unwrap();
        let config = test_config(temp.path());

        // No old directories exist
        migrate_to_flat_layout(&config).unwrap();

        // Should succeed without error
    }
}

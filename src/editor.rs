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

//! External editor integration for task body editing.

use std::fs;
use std::io::{self, Write};
use std::process::Command;

use colored::Colorize;
use tempfile::NamedTempFile;

use crate::config::Config;
use crate::{Error, Result};

/// Get the editor command from config or environment.
///
/// Priority: config.editor > EDITOR env var > error
pub fn get_editor(config: &Config) -> Result<String> {
    if let Some(ref editor) = config.editor {
        return Ok(editor.clone());
    }

    if let Ok(editor) = std::env::var("EDITOR") {
        if !editor.is_empty() {
            return Ok(editor);
        }
    }

    Err(Error::InvalidInput(
        "No editor configured. Set 'editor' in config or EDITOR environment variable".to_string(),
    ))
}

/// Validate that the editor command exists and is executable.
fn validate_editor(editor_cmd: &str) -> Result<()> {
    // Parse the command (first word before any spaces/args)
    let command = editor_cmd
        .split_whitespace()
        .next()
        .ok_or_else(|| Error::InvalidInput("Empty editor command".to_string()))?;

    // Check if command exists in PATH
    match which::which(command) {
        Ok(path) => {
            // Verify it's executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(&path)?;
                let permissions = metadata.permissions();
                if permissions.mode() & 0o111 == 0 {
                    return Err(Error::InvalidInput(format!(
                        "Editor '{}' is not executable",
                        command
                    )));
                }
            }
            Ok(())
        }
        Err(_) => Err(Error::InvalidInput(format!(
            "Editor command '{}' not found in PATH",
            command
        ))),
    }
}

/// Check if content is effectively empty (only whitespace).
fn is_empty_content(content: &str) -> bool {
    content.trim().is_empty()
}

/// Prompt user for confirmation.
fn confirm(prompt: &str) -> Result<bool> {
    print!("{} ", prompt.yellow());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_lowercase() == "y")
}

/// Open an external editor to edit text.
///
/// - Creates a temporary .md file
/// - Pre-populates with initial_content
/// - Spawns the editor process
/// - Handles empty result with confirmation
/// - Returns edited content or error
pub fn edit_text(config: &Config, initial_content: &str) -> Result<String> {
    let editor_cmd = get_editor(config)?;
    validate_editor(&editor_cmd)?;

    // Create temp file with .md extension
    let temp_file = NamedTempFile::new_in(std::env::temp_dir())?;
    let temp_path = temp_file.path();
    let md_path = temp_path.with_extension("md");

    // Write initial content
    fs::write(&md_path, initial_content)?;

    // Parse editor command (handle args like "code --wait")
    let parts: Vec<&str> = editor_cmd.split_whitespace().collect();
    let (cmd, args) = parts
        .split_first()
        .ok_or_else(|| Error::InvalidInput("Empty editor command".to_string()))?;

    // Spawn editor and wait
    let status = Command::new(cmd)
        .args(args)
        .arg(&md_path)
        .status()
        .map_err(|e| Error::InvalidInput(format!("Failed to spawn editor: {}", e)))?;

    // Check if editor exited successfully
    if !status.success() {
        // Clean up temp file
        let _ = fs::remove_file(&md_path);
        return Err(Error::InvalidInput(
            "Editor exited without saving (cancelled)".to_string(),
        ));
    }

    // Read result
    let content = fs::read_to_string(&md_path)
        .map_err(|e| Error::InvalidInput(format!("Failed to read edited content: {}", e)))?;

    // Clean up temp file
    let _ = fs::remove_file(&md_path);

    // Check if content is empty and confirm
    if is_empty_content(&content) && !confirm("Body is empty. Save empty body? (y/N):")? {
        return Err(Error::InvalidInput("Operation cancelled".to_string()));
    }

    Ok(content)
}

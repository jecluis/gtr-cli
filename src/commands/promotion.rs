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

//! Promotion threshold configuration via editor-based editing.

use std::collections::HashMap;
use std::io::{self, Write};

use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::models::{ConfigResponse, ConfigUpdateRequest, PromotionThresholdsUpdate};
use crate::threshold_cache::{self, CachedThresholds};
use crate::{Error, Result, utils};

/// Show current promotion threshold configuration.
pub async fn show(config: &Config, project: Option<String>) -> Result<()> {
    let client = Client::new(config)?;
    let cfg = fetch_config(&client, project.as_deref()).await?;

    update_cache_from_response(config, &cfg);

    println!("{}", "Promotion Thresholds".bold().green());
    println!("{}", "─".repeat(50));

    println!("\n  {}", "Deadline".bold());

    let sizes = ["XS", "S", "M", "L", "XL"];
    for size in sizes {
        let threshold = cfg
            .promotion_thresholds
            .deadline
            .get(size)
            .map(String::as_str)
            .unwrap_or("-");

        let is_override = cfg
            .overrides
            .as_ref()
            .and_then(|o| o.promotion_thresholds.deadline.get(size))
            .is_some();

        if is_override {
            println!(
                "    {:<4} {:<10} {}",
                size.cyan(),
                threshold.yellow().bold(),
                "(override)".dimmed()
            );
        } else {
            println!(
                "    {:<4} {:<10} {}",
                size.cyan(),
                threshold,
                "(default)".dimmed()
            );
        }
    }

    if let Some(overrides) = &cfg.overrides
        && !overrides.promotion_thresholds.deadline.is_empty()
    {
        println!("\n  {}", "Active Overrides:".bold());
        for (size, duration) in &overrides.promotion_thresholds.deadline {
            println!("    {} = {}", size.cyan(), duration.yellow());
        }
    }

    println!();
    Ok(())
}

/// Edit promotion thresholds via editor or file.
pub async fn set(config: &Config, project: Option<String>, file: Option<String>) -> Result<()> {
    let client = Client::new(config)?;
    let cfg = fetch_config(&client, project.as_deref()).await?;

    // Build the current merged thresholds as editor content
    let original = build_editor_json(&cfg);

    // Get edited content
    let edited_json = if let Some(ref path) = file {
        std::fs::read_to_string(path)
            .map_err(|e| Error::InvalidInput(format!("Failed to read file '{}': {}", path, e)))?
    } else {
        edit_thresholds_json(config, &original)?
    };

    // Parse the edited JSON
    let edited: serde_json::Value = serde_json::from_str(&edited_json)
        .map_err(|e| Error::InvalidInput(format!("Invalid JSON: {}", e)))?;

    let edited_map = edited
        .as_object()
        .and_then(|o| o.get("deadline"))
        .and_then(|d| d.as_object())
        .ok_or_else(|| {
            Error::InvalidInput(
                "JSON must have shape: { \"deadline\": { \"XS\": \"12h\", ... } }".to_string(),
            )
        })?;

    // Parse edited values into a HashMap
    let mut edited_deadline = HashMap::new();
    for (key, val) in edited_map {
        let duration = val
            .as_str()
            .ok_or_else(|| Error::InvalidInput(format!("Value for '{}' must be a string", key)))?
            .to_string();
        edited_deadline.insert(key.clone(), duration);
    }

    // Compute diff against the original merged config
    let diff = compute_diff(&cfg.promotion_thresholds.deadline, &edited_deadline)?;

    if diff.is_empty() {
        println!("{}", "No changes detected.".dimmed());
        return Ok(());
    }

    // Show diff and confirm
    show_diff_summary(&diff, &cfg.promotion_thresholds.deadline);

    if !confirm_push()? {
        println!("{}", "Cancelled.".dimmed());
        return Ok(());
    }

    // Build update request and send
    let req = ConfigUpdateRequest {
        promotion_thresholds: Some(PromotionThresholdsUpdate {
            deadline: Some(diff),
        }),
    };

    let updated = if let Some(ref project_id) = project {
        client.update_project_config(project_id, &req).await?
    } else {
        client.update_user_config(&req).await?
    };

    update_cache_from_response(config, &updated);

    println!("{}", "✓ Configuration updated!".green().bold());
    Ok(())
}

/// Reset all promotion threshold overrides to defaults.
pub async fn reset(config: &Config, project: Option<String>) -> Result<()> {
    let client = Client::new(config)?;

    if let Some(project_id) = project {
        client.reset_project_config(&project_id).await?;
        println!(
            "{}",
            "✓ Project promotion config reset to defaults!"
                .green()
                .bold()
        );
    } else {
        client.reset_user_config().await?;
        println!(
            "{}",
            "✓ User promotion config reset to defaults!".green().bold()
        );
    }

    // Update cache with defaults
    let _ = threshold_cache::write_cache(
        config,
        &CachedThresholds {
            deadline: utils::default_thresholds(),
        },
    );

    Ok(())
}

/// Fetch config from server (user or project).
async fn fetch_config(client: &Client, project: Option<&str>) -> Result<ConfigResponse> {
    if let Some(project_id) = project {
        client.get_project_config(project_id).await
    } else {
        client.get_user_config().await
    }
}

/// Build the JSON document for the editor.
fn build_editor_json(cfg: &ConfigResponse) -> String {
    // Build ordered map for consistent display
    let sizes = ["XS", "S", "M", "L", "XL"];
    let mut deadline = serde_json::Map::new();
    for size in sizes {
        if let Some(val) = cfg.promotion_thresholds.deadline.get(size) {
            deadline.insert(size.to_string(), serde_json::Value::String(val.clone()));
        }
    }

    let mut root = serde_json::Map::new();
    root.insert("deadline".to_string(), serde_json::Value::Object(deadline));

    serde_json::to_string_pretty(&root).unwrap()
}

/// Open the editor with threshold JSON in a .json temp file.
fn edit_thresholds_json(config: &Config, initial_content: &str) -> Result<String> {
    let editor_cmd = crate::editor::get_editor(config)?;

    // Create temp file with .json extension
    let temp_file = tempfile::Builder::new()
        .prefix("gtr-thresholds-")
        .suffix(".json")
        .tempfile_in(std::env::temp_dir())
        .map_err(|e| Error::InvalidInput(format!("Failed to create temp file: {}", e)))?;

    let temp_path = temp_file.path().to_path_buf();

    // Write initial content
    std::fs::write(&temp_path, initial_content)?;

    // Parse editor command
    let parts: Vec<&str> = editor_cmd.split_whitespace().collect();
    let (cmd, args) = parts
        .split_first()
        .ok_or_else(|| Error::InvalidInput("Empty editor command".to_string()))?;

    let status = std::process::Command::new(cmd)
        .args(args)
        .arg(&temp_path)
        .status()
        .map_err(|e| Error::InvalidInput(format!("Failed to spawn editor: {}", e)))?;

    if !status.success() {
        return Err(Error::InvalidInput(
            "Editor exited without saving (cancelled)".to_string(),
        ));
    }

    let content = std::fs::read_to_string(&temp_path)
        .map_err(|e| Error::InvalidInput(format!("Failed to read edited content: {}", e)))?;

    Ok(content)
}

/// Compute the diff between original merged thresholds and edited values.
///
/// Returns a map where:
/// - `Some(value)` = changed or new value
/// - `None` = removed (reset to default)
fn compute_diff(
    original: &HashMap<String, String>,
    edited: &HashMap<String, String>,
) -> Result<HashMap<String, Option<String>>> {
    let valid_sizes = ["XS", "S", "M", "L", "XL"];
    let mut diff = HashMap::new();

    // Check for changed/new values
    for (size, new_value) in edited {
        if !valid_sizes.contains(&size.as_str()) {
            return Err(Error::InvalidInput(format!(
                "Invalid size: '{}'. Valid sizes: XS, S, M, L, XL",
                size
            )));
        }

        if utils::parse_threshold_secs(new_value).is_none() {
            return Err(Error::InvalidInput(format!(
                "Invalid duration for {}: '{}'. Use format: 12h, 2d, 1w",
                size, new_value
            )));
        }

        match original.get(size) {
            Some(old_value) if old_value == new_value => {} // unchanged
            _ => {
                diff.insert(size.clone(), Some(new_value.clone()));
            }
        }
    }

    // Check for removed keys (reset to default)
    for size in original.keys() {
        if !edited.contains_key(size) {
            diff.insert(size.clone(), None);
        }
    }

    Ok(diff)
}

/// Display a summary of changes to the user.
fn show_diff_summary(diff: &HashMap<String, Option<String>>, original: &HashMap<String, String>) {
    println!("\n{}", "Changes:".bold());

    let mut keys: Vec<&String> = diff.keys().collect();
    keys.sort();

    for key in keys {
        let old = original.get(key).map(String::as_str).unwrap_or("-");
        match diff.get(key).unwrap() {
            Some(new) => {
                println!(
                    "  {} {} -> {}",
                    key.cyan(),
                    old.dimmed(),
                    new.yellow().bold()
                );
            }
            None => {
                println!(
                    "  {} {} -> {}",
                    key.cyan(),
                    old.dimmed(),
                    "(reset to default)".yellow()
                );
            }
        }
    }
    println!();
}

/// Ask for confirmation before pushing changes.
fn confirm_push() -> Result<bool> {
    print!("{} ", "Push these changes? (y/N):".yellow());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_lowercase() == "y")
}

/// Update the local threshold cache from a config response.
fn update_cache_from_response(config: &Config, cfg: &ConfigResponse) {
    let _ = threshold_cache::write_cache(
        config,
        &CachedThresholds {
            deadline: cfg.promotion_thresholds.deadline.clone(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_diff_no_changes() {
        let original: HashMap<String, String> =
            [("M".to_string(), "24h".to_string())].into_iter().collect();
        let edited = original.clone();

        let diff = compute_diff(&original, &edited).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn compute_diff_changed_value() {
        let original: HashMap<String, String> =
            [("M".to_string(), "24h".to_string())].into_iter().collect();
        let edited: HashMap<String, String> =
            [("M".to_string(), "48h".to_string())].into_iter().collect();

        let diff = compute_diff(&original, &edited).unwrap();
        assert_eq!(diff.get("M"), Some(&Some("48h".to_string())));
    }

    #[test]
    fn compute_diff_removed_key() {
        let original: HashMap<String, String> = [
            ("M".to_string(), "24h".to_string()),
            ("L".to_string(), "48h".to_string()),
        ]
        .into_iter()
        .collect();
        let edited: HashMap<String, String> =
            [("M".to_string(), "24h".to_string())].into_iter().collect();

        let diff = compute_diff(&original, &edited).unwrap();
        assert_eq!(diff.len(), 1);
        assert_eq!(diff.get("L"), Some(&None));
    }

    #[test]
    fn compute_diff_invalid_size() {
        let original = HashMap::new();
        let edited: HashMap<String, String> = [("XXXL".to_string(), "24h".to_string())]
            .into_iter()
            .collect();

        assert!(compute_diff(&original, &edited).is_err());
    }

    #[test]
    fn compute_diff_invalid_duration() {
        let original = HashMap::new();
        let edited: HashMap<String, String> = [("M".to_string(), "invalid".to_string())]
            .into_iter()
            .collect();

        assert!(compute_diff(&original, &edited).is_err());
    }
}

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
use crate::models::{
    ConfigResponse, ConfigUpdateRequest, ImpactConfigUpdate, PromotionThresholdsUpdate,
};
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

    // Impact labels
    println!("\n  {}", "Impact Labels".bold());
    let default_labels = utils::default_impact_labels();
    let impact = cfg.promotion_thresholds.impact.as_ref();
    for key in ["1", "2", "3", "4", "5"] {
        let label = impact
            .and_then(|i| i.labels.get(key))
            .or_else(|| default_labels.get(key))
            .map(String::as_str)
            .unwrap_or("-");

        let is_override = cfg
            .overrides
            .as_ref()
            .map(|o| o.promotion_thresholds.impact.labels.contains_key(key))
            .unwrap_or(false);

        if is_override {
            println!(
                "    {:<4} {:<18} {}",
                key.cyan(),
                label.yellow().bold(),
                "(override)".dimmed()
            );
        } else {
            println!(
                "    {:<4} {:<18} {}",
                key.cyan(),
                label,
                "(default)".dimmed()
            );
        }
    }

    // Impact multipliers
    println!("\n  {}", "Impact Multipliers".bold());
    let default_mults = utils::default_impact_multipliers();
    for key in ["1", "2", "3", "4", "5"] {
        let mult = impact
            .and_then(|i| i.multipliers.get(key))
            .or_else(|| default_mults.get(key))
            .copied()
            .unwrap_or(1.0);

        let is_override = cfg
            .overrides
            .as_ref()
            .map(|o| o.promotion_thresholds.impact.multipliers.contains_key(key))
            .unwrap_or(false);

        if is_override {
            println!(
                "    {:<4} {:<18} {}",
                key.cyan(),
                format!("{:.2}x", mult).yellow().bold(),
                "(override)".dimmed()
            );
        } else {
            println!(
                "    {:<4} {:<18} {}",
                key.cyan(),
                format!("{:.2}x", mult),
                "(default)".dimmed()
            );
        }
    }

    if let Some(overrides) = &cfg.overrides {
        let has_deadline_overrides = !overrides.promotion_thresholds.deadline.is_empty();
        let has_impact_overrides = !overrides.promotion_thresholds.impact.labels.is_empty()
            || !overrides.promotion_thresholds.impact.multipliers.is_empty();

        if has_deadline_overrides || has_impact_overrides {
            println!("\n  {}", "Active Overrides:".bold());
            for (size, duration) in &overrides.promotion_thresholds.deadline {
                println!("    deadline {} = {}", size.cyan(), duration.yellow());
            }
            for (key, label) in &overrides.promotion_thresholds.impact.labels {
                println!("    impact label {} = {}", key.cyan(), label.yellow());
            }
            for (key, mult) in &overrides.promotion_thresholds.impact.multipliers {
                println!(
                    "    impact multiplier {} = {}",
                    key.cyan(),
                    format!("{:.2}x", mult).yellow()
                );
            }
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

    let edited_obj = edited.as_object().ok_or_else(|| {
        Error::InvalidInput(
            "JSON must be an object with \"deadline\" and/or \"impact\"".to_string(),
        )
    })?;

    let edited_deadline_map = edited_obj
        .get("deadline")
        .and_then(|d| d.as_object())
        .ok_or_else(|| {
            Error::InvalidInput("JSON must have \"deadline\": { \"XS\": \"12h\", ... }".to_string())
        })?;

    // Parse edited deadline values
    let mut edited_deadline = HashMap::new();
    for (key, val) in edited_deadline_map {
        let duration = val
            .as_str()
            .ok_or_else(|| Error::InvalidInput(format!("Value for '{}' must be a string", key)))?
            .to_string();
        edited_deadline.insert(key.clone(), duration);
    }

    // Compute deadline diff
    let deadline_diff = compute_diff(&cfg.promotion_thresholds.deadline, &edited_deadline)?;

    // Parse and compute impact diff
    let impact_update = if let Some(impact_val) = edited_obj.get("impact") {
        parse_impact_diff(impact_val, &cfg)?
    } else {
        None
    };

    let has_deadline_changes = !deadline_diff.is_empty();
    let has_impact_changes = impact_update.is_some();

    if !has_deadline_changes && !has_impact_changes {
        println!("{}", "No changes detected.".dimmed());
        return Ok(());
    }

    // Show diff and confirm
    if has_deadline_changes {
        show_diff_summary(&deadline_diff, &cfg.promotion_thresholds.deadline);
    }
    if let Some(ref impact) = impact_update {
        show_impact_diff_summary(impact);
    }

    if !confirm_push()? {
        println!("{}", "Cancelled.".dimmed());
        return Ok(());
    }

    // Build update request and send
    let req = ConfigUpdateRequest {
        promotion_thresholds: Some(PromotionThresholdsUpdate {
            deadline: if has_deadline_changes {
                Some(deadline_diff)
            } else {
                None
            },
            impact: impact_update,
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
            impact_labels: utils::default_impact_labels(),
            impact_multipliers: utils::default_impact_multipliers(),
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

    // Build impact section
    let default_labels = utils::default_impact_labels();
    let default_mults = utils::default_impact_multipliers();
    let impact_cfg = cfg.promotion_thresholds.impact.as_ref();

    let mut labels = serde_json::Map::new();
    let mut multipliers = serde_json::Map::new();
    for key in ["1", "2", "3", "4", "5"] {
        let label = impact_cfg
            .and_then(|i| i.labels.get(key))
            .or_else(|| default_labels.get(key))
            .cloned()
            .unwrap_or_default();
        labels.insert(key.to_string(), serde_json::Value::String(label));

        let mult = impact_cfg
            .and_then(|i| i.multipliers.get(key))
            .or_else(|| default_mults.get(key))
            .copied()
            .unwrap_or(1.0);
        multipliers.insert(
            key.to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(mult).unwrap()),
        );
    }

    let mut impact = serde_json::Map::new();
    impact.insert("labels".to_string(), serde_json::Value::Object(labels));
    impact.insert(
        "multipliers".to_string(),
        serde_json::Value::Object(multipliers),
    );

    let mut root = serde_json::Map::new();
    root.insert("deadline".to_string(), serde_json::Value::Object(deadline));
    root.insert("impact".to_string(), serde_json::Value::Object(impact));

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
    let impact = cfg.promotion_thresholds.impact.as_ref();
    let _ = threshold_cache::write_cache(
        config,
        &CachedThresholds {
            deadline: cfg.promotion_thresholds.deadline.clone(),
            impact_labels: impact
                .map(|i| i.labels.clone())
                .unwrap_or_else(utils::default_impact_labels),
            impact_multipliers: impact
                .map(|i| i.multipliers.clone())
                .unwrap_or_else(utils::default_impact_multipliers),
        },
    );
}

/// Parse impact changes from edited JSON and compute diff against current config.
fn parse_impact_diff(
    impact_val: &serde_json::Value,
    cfg: &ConfigResponse,
) -> Result<Option<ImpactConfigUpdate>> {
    let impact_obj = impact_val
        .as_object()
        .ok_or_else(|| Error::InvalidInput("\"impact\" must be an object".to_string()))?;

    let default_labels = utils::default_impact_labels();
    let default_mults = utils::default_impact_multipliers();
    let current_impact = cfg.promotion_thresholds.impact.as_ref();

    let mut label_diff: HashMap<String, Option<String>> = HashMap::new();
    let mut mult_diff: HashMap<String, Option<f64>> = HashMap::new();

    // Parse labels
    if let Some(labels_val) = impact_obj.get("labels") {
        let labels_obj = labels_val
            .as_object()
            .ok_or_else(|| Error::InvalidInput("\"labels\" must be an object".to_string()))?;

        let valid_keys = ["1", "2", "3", "4", "5"];
        for (key, val) in labels_obj {
            if !valid_keys.contains(&key.as_str()) {
                return Err(Error::InvalidInput(format!(
                    "Invalid impact key: '{}'. Must be 1-5",
                    key
                )));
            }
            let new_label = val
                .as_str()
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Impact label for '{}' must be a string", key))
                })?
                .to_string();

            let current_label = current_impact
                .and_then(|i| i.labels.get(key))
                .or_else(|| default_labels.get(key))
                .cloned()
                .unwrap_or_default();

            if new_label != current_label {
                label_diff.insert(key.clone(), Some(new_label));
            }
        }
    }

    // Parse multipliers
    if let Some(mults_val) = impact_obj.get("multipliers") {
        let mults_obj = mults_val
            .as_object()
            .ok_or_else(|| Error::InvalidInput("\"multipliers\" must be an object".to_string()))?;

        let valid_keys = ["1", "2", "3", "4", "5"];
        for (key, val) in mults_obj {
            if !valid_keys.contains(&key.as_str()) {
                return Err(Error::InvalidInput(format!(
                    "Invalid impact key: '{}'. Must be 1-5",
                    key
                )));
            }
            let new_mult = val.as_f64().ok_or_else(|| {
                Error::InvalidInput(format!("Impact multiplier for '{}' must be a number", key))
            })?;

            if new_mult <= 0.0 {
                return Err(Error::InvalidInput(format!(
                    "Impact multiplier for {} must be positive, got {}",
                    key, new_mult
                )));
            }

            let current_mult = current_impact
                .and_then(|i| i.multipliers.get(key))
                .or_else(|| default_mults.get(key))
                .copied()
                .unwrap_or(1.0);

            if (new_mult - current_mult).abs() > f64::EPSILON {
                mult_diff.insert(key.clone(), Some(new_mult));
            }
        }
    }

    if label_diff.is_empty() && mult_diff.is_empty() {
        return Ok(None);
    }

    Ok(Some(ImpactConfigUpdate {
        labels: if label_diff.is_empty() {
            None
        } else {
            Some(label_diff)
        },
        multipliers: if mult_diff.is_empty() {
            None
        } else {
            Some(mult_diff)
        },
    }))
}

/// Display a summary of impact configuration changes.
fn show_impact_diff_summary(update: &ImpactConfigUpdate) {
    println!("\n{}", "Impact Changes:".bold());

    if let Some(ref labels) = update.labels {
        let mut keys: Vec<&String> = labels.keys().collect();
        keys.sort();
        for key in keys {
            if let Some(Some(label)) = labels.get(key) {
                println!("  label {} -> {}", key.cyan(), label.yellow().bold());
            }
        }
    }

    if let Some(ref multipliers) = update.multipliers {
        let mut keys: Vec<&String> = multipliers.keys().collect();
        keys.sort();
        for key in keys {
            if let Some(Some(mult)) = multipliers.get(key) {
                println!(
                    "  multiplier {} -> {}",
                    key.cyan(),
                    format!("{:.2}x", mult).yellow().bold()
                );
            }
        }
    }

    println!();
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

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

//! Deadline threshold configuration command implementation.

use std::collections::HashMap;

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::models::{ConfigUpdateRequest, PromotionThresholdsUpdate};

/// Show current configuration.
pub async fn show(config: &Config, project: Option<String>) -> Result<()> {
    let client = Client::new(config)?;

    let cfg = if let Some(project_id) = project {
        client.get_project_config(&project_id).await?
    } else {
        client.get_user_config().await?
    };

    println!("{}", "Deadline Promotion Thresholds".bold().green());
    println!("{}", "─".repeat(50));

    let sizes = ["XS", "S", "M", "L", "XL"];
    for size in sizes {
        let threshold = cfg
            .promotion_thresholds
            .deadline
            .get(size)
            .map(String::as_str)
            .unwrap_or("-");

        // Check if this is an override
        let is_override = cfg
            .overrides
            .as_ref()
            .and_then(|o| o.promotion_thresholds.deadline.get(size))
            .is_some();

        if is_override {
            println!(
                "  {:<4} {:<10} {}",
                size.cyan(),
                threshold.yellow().bold(),
                "(override)".dimmed()
            );
        } else {
            println!(
                "  {:<4} {:<10} {}",
                size.cyan(),
                threshold,
                "(default)".dimmed()
            );
        }
    }

    if let Some(overrides) = &cfg.overrides
        && !overrides.promotion_thresholds.deadline.is_empty()
    {
        println!("\n{}", "Active Overrides:".bold());
        for (size, duration) in &overrides.promotion_thresholds.deadline {
            println!("  {} = {}", size.cyan(), duration.yellow());
        }
    }

    println!();
    Ok(())
}

/// Set deadline threshold for a specific size.
pub async fn set(
    config: &Config,
    size: String,
    duration: String,
    project: Option<String>,
) -> Result<()> {
    let client = Client::new(config)?;

    let mut thresholds = HashMap::new();
    thresholds.insert(size.clone(), Some(duration.clone()));

    let req = ConfigUpdateRequest {
        promotion_thresholds: Some(PromotionThresholdsUpdate {
            deadline: Some(thresholds),
        }),
    };

    let cfg = if let Some(project_id) = project {
        client.update_project_config(&project_id, &req).await?
    } else {
        client.update_user_config(&req).await?
    };

    println!("{}", "✓ Configuration updated!".green().bold());
    println!("  {} threshold set to {}", size.cyan(), duration.yellow());

    if let Some(merged) = cfg.promotion_thresholds.deadline.get(&size)
        && merged != &duration
    {
        println!(
            "  {} Effective value: {} (overridden by project config)",
            "ℹ".blue(),
            merged.yellow()
        );
    }

    Ok(())
}

/// Unset (remove) deadline threshold override for a specific size.
pub async fn unset(config: &Config, size: String, project: Option<String>) -> Result<()> {
    let client = Client::new(config)?;

    let mut thresholds = HashMap::new();
    thresholds.insert(size.clone(), None);

    let req = ConfigUpdateRequest {
        promotion_thresholds: Some(PromotionThresholdsUpdate {
            deadline: Some(thresholds),
        }),
    };

    let cfg = if let Some(project_id) = project {
        client.update_project_config(&project_id, &req).await?
    } else {
        client.update_user_config(&req).await?
    };

    println!("{}", "✓ Override removed!".green().bold());
    println!("  {} threshold reset to default", size.cyan());

    if let Some(default) = cfg.promotion_thresholds.deadline.get(&size) {
        println!("  Current value: {}", default.dimmed());
    }

    Ok(())
}

/// Reset all overrides to defaults.
pub async fn reset(config: &Config, project: Option<String>) -> Result<()> {
    let client = Client::new(config)?;

    if let Some(project_id) = project {
        client.reset_project_config(&project_id).await?;
        println!(
            "{}",
            "✓ Project configuration reset to defaults!".green().bold()
        );
    } else {
        client.reset_user_config().await?;
        println!(
            "{}",
            "✓ User configuration reset to defaults!".green().bold()
        );
    }

    Ok(())
}

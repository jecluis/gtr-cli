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

//! Feels command — set daily energy and focus levels.

use chrono::Local;
use colored::Colorize;
use dialoguer::Select;

use crate::Result;
use crate::cache::TaskCache;
use crate::client::Client;
use crate::config::Config;

/// Entry point: interactive picker when no args, direct set otherwise.
pub async fn run(
    config: &Config,
    energy: Option<u8>,
    focus: Option<u8>,
    no_sync: bool,
) -> Result<()> {
    let (energy, focus) = match (energy, focus) {
        (Some(e), Some(f)) => (e, f),
        _ => prompt_energy_focus()?,
    };

    set(config, energy, focus, no_sync).await
}

/// Set today's energy and focus, with best-effort server push.
pub async fn set(config: &Config, energy: u8, focus: u8, no_sync: bool) -> Result<()> {
    let today = Local::now().date_naive();
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    cache.upsert_feels(&today, energy, focus)?;

    let energy_label = energy_description(energy);
    let focus_label = focus_description(focus);

    println!("{}", "✓ Feels updated".green().bold());
    println!("  Energy:  {} ({})", energy, energy_label);
    println!("  Focus:   {} ({})", focus, focus_label);

    if !no_sync {
        let utc_offset = Local::now().offset().to_string();
        let client = Client::new(config)?;
        match client.post_feels(energy, focus, &utc_offset).await {
            Ok(()) => println!("{}", "  ✓ Synced with server".green()),
            Err(_) => println!("{}", "  ⊙ Queued for sync".yellow()),
        }
    }

    Ok(())
}

/// Interactive picker for energy and focus levels.
pub(crate) fn prompt_energy_focus() -> Result<(u8, u8)> {
    let energy_levels = [
        "1 — very low",
        "2 — low",
        "3 — moderate",
        "4 — good",
        "5 — high",
    ];
    let energy_idx = Select::new()
        .with_prompt("Energy (emotional availability)")
        .items(energy_levels)
        .default(2)
        .interact_opt()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {e}")))?;

    let Some(energy_idx) = energy_idx else {
        return Err(crate::Error::UserFacing("Selection cancelled".to_string()));
    };

    let focus_levels = [
        "1 — scattered",
        "2 — limited",
        "3 — moderate",
        "4 — good",
        "5 — deep",
    ];
    let focus_idx = Select::new()
        .with_prompt("Focus (capacity for complex work)")
        .items(focus_levels)
        .default(2)
        .interact_opt()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {e}")))?;

    let Some(focus_idx) = focus_idx else {
        return Err(crate::Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok((energy_idx as u8 + 1, focus_idx as u8 + 1))
}

pub(crate) fn energy_description(level: u8) -> &'static str {
    match level {
        1 => "very low — need easy wins",
        2 => "low — prefer enjoyable tasks",
        3 => "moderate",
        4 => "good — can handle some tedium",
        5 => "high — bring on anything",
        _ => "unknown",
    }
}

pub(crate) fn focus_description(level: u8) -> &'static str {
    match level {
        1 => "scattered — small tasks only",
        2 => "limited — prefer small/medium",
        3 => "moderate",
        4 => "good — can tackle large tasks",
        5 => "deep — ready for anything",
        _ => "unknown",
    }
}

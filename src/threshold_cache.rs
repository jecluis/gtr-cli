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

//! Local cache for promotion threshold configuration.
//!
//! Stores the most recently fetched thresholds as JSON so that
//! `gtr list` can read them without hitting the server every time.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::config::Config;
use crate::utils;

/// Cached promotion thresholds (matches the resolved shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedThresholds {
    pub deadline: HashMap<String, String>,
    #[serde(default)]
    pub impact_labels: HashMap<String, String>,
    #[serde(default)]
    pub impact_multipliers: HashMap<String, f64>,
}

/// Get the path to the threshold cache file.
pub fn cache_path(config: &Config) -> PathBuf {
    config.cache_dir.join("promotion-thresholds.json")
}

/// Read cached thresholds from disk.
///
/// Returns `None` if the file doesn't exist or can't be parsed.
pub fn read_cache(config: &Config) -> Option<CachedThresholds> {
    let path = cache_path(config);
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Write thresholds to the local cache.
pub fn write_cache(config: &Config, thresholds: &CachedThresholds) -> crate::Result<()> {
    let path = cache_path(config);
    let json = serde_json::to_string_pretty(thresholds)
        .map_err(|e| crate::Error::Config(format!("failed to serialize cache: {}", e)))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Fetch promotion thresholds (deadline + impact), checking cache first.
///
/// Resolution order:
/// 1. Local cache (always checked first)
/// 2. Server fetch with 2-second timeout (unless `no_sync`)
/// 3. Defaults
pub async fn fetch_thresholds(config: &Config, client: &Client, no_sync: bool) -> CachedThresholds {
    // Try local cache first
    if let Some(cached) = read_cache(config) {
        if no_sync {
            return cached;
        }
    } else if no_sync {
        return CachedThresholds {
            deadline: utils::default_thresholds(),
            impact_labels: utils::default_impact_labels(),
            impact_multipliers: utils::default_impact_multipliers(),
        };
    }

    // Fetch from server and update cache
    match tokio::time::timeout(std::time::Duration::from_secs(2), client.get_user_config()).await {
        Ok(Ok(cfg)) => {
            let impact = cfg.promotion_thresholds.impact.as_ref();
            let cached = CachedThresholds {
                deadline: cfg.promotion_thresholds.deadline,
                impact_labels: impact
                    .map(|i| i.labels.clone())
                    .unwrap_or_else(utils::default_impact_labels),
                impact_multipliers: impact
                    .map(|i| i.multipliers.clone())
                    .unwrap_or_else(utils::default_impact_multipliers),
            };
            let _ = write_cache(config, &cached);
            cached
        }
        _ => {
            // Fall back to cache, then defaults
            read_cache(config).unwrap_or_else(|| CachedThresholds {
                deadline: utils::default_thresholds(),
                impact_labels: utils::default_impact_labels(),
                impact_multipliers: utils::default_impact_multipliers(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(dir: &std::path::Path) -> Config {
        Config {
            server_url: "http://localhost:3000".to_string(),
            auth_token: "test".to_string(),
            client_id: "test-client".to_string(),
            editor: None,
            log_level: "info".to_string(),
            cache_dir: dir.to_path_buf(),
            config_path: dir.join("config.toml"),
            icon_theme: crate::icons::IconTheme::default(),
        }
    }

    #[test]
    fn round_trip_write_read() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let thresholds = CachedThresholds {
            deadline: [("M".to_string(), "48h".to_string())].into_iter().collect(),
            impact_labels: HashMap::new(),
            impact_multipliers: HashMap::new(),
        };

        write_cache(&config, &thresholds).unwrap();
        let loaded = read_cache(&config).unwrap();

        assert_eq!(loaded.deadline.get("M"), Some(&"48h".to_string()));
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        assert!(read_cache(&config).is_none());
    }
}

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

//! Configuration handling for the CLI.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::icons::IconTheme;
use crate::{Error, Result};

fn default_log_level() -> String {
    "info".to_string()
}

fn default_icon_theme() -> IconTheme {
    IconTheme::default()
}

/// CLI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server URL
    pub server_url: String,

    /// Authentication token
    pub auth_token: String,

    /// Client UUID for sync protocol
    pub client_id: String,

    /// Editor command (with optional args) for editing task bodies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,

    /// Log level (default: "info"). Overridden by GTR_LOG env var.
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Icon theme: "unicode" (default) or "nerd" (requires Nerd Font).
    #[serde(default = "default_icon_theme")]
    pub icon_theme: IconTheme,

    /// Cache directory (for Phase 1.5)
    #[serde(skip)]
    pub cache_dir: PathBuf,

    /// Config file path
    #[serde(skip)]
    pub config_path: PathBuf,
}

impl Config {
    /// Load configuration from file or create default.
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            Self::default_config_path()?
        };

        if !config_path.exists() {
            return Err(Error::Config(format!(
                "config file not found: {}. Run 'gtr init' to create it.",
                config_path.display()
            )));
        }

        let content = fs::read_to_string(&config_path)?;
        let mut config: Config = toml::from_str(&content)?;

        config.config_path = config_path;
        config.cache_dir = Self::default_cache_dir()?;

        // Ensure cache directory exists
        fs::create_dir_all(&config.cache_dir)?;

        Ok(config)
    }

    /// Save configuration to file.
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("failed to serialize config: {e}")))?;

        fs::write(&self.config_path, content)?;

        Ok(())
    }

    /// Create a new config with given server and token.
    pub fn new(server_url: String, auth_token: String) -> Result<Self> {
        let cache_dir = Self::default_cache_dir()?;
        // Ensure cache directory exists
        fs::create_dir_all(&cache_dir)?;

        // Generate unique client ID
        let client_id = uuid::Uuid::new_v4().to_string();

        Ok(Config {
            server_url,
            auth_token,
            client_id,
            editor: None,
            log_level: default_log_level(),
            icon_theme: default_icon_theme(),
            cache_dir,
            config_path: Self::default_config_path()?,
        })
    }

    /// Override server URL.
    pub fn with_server(mut self, server: Option<String>) -> Self {
        if let Some(s) = server {
            self.server_url = s;
        }
        self
    }

    /// Override auth token.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        if let Some(t) = token {
            self.auth_token = t;
        }
        self
    }

    /// Resolve effective icon theme (env var > config file).
    pub fn effective_icon_theme(&self) -> IconTheme {
        if let Ok(val) = std::env::var("GTR_ICONS")
            && let Ok(theme) = val.parse()
        {
            return theme;
        }
        self.icon_theme
    }

    /// Get default config file path.
    fn default_config_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("tech", "abysmo", "gtr")
            .ok_or_else(|| Error::Config("could not determine config directory".to_string()))?;

        Ok(dirs.config_dir().join("config.toml"))
    }

    /// Get default cache directory.
    fn default_cache_dir() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("tech", "abysmo", "gtr")
            .ok_or_else(|| Error::Config("could not determine cache directory".to_string()))?;

        Ok(dirs.cache_dir().to_path_buf())
    }
}

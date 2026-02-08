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

//! Initialize configuration command.

use colored::Colorize;

use crate::Result;
use crate::config::Config;

/// Initialize configuration file.
pub fn run(server: &str, token: &str) -> Result<()> {
    let config = Config::new(server.to_string(), token.to_string())?;
    config.save()?;

    println!(
        "{}",
        "Configuration initialized successfully!".green().bold()
    );
    println!("Config file: {}", config.config_path.display());
    println!("Server URL: {}", config.server_url);
    println!("Cache dir: {}", config.cache_dir.display());

    Ok(())
}

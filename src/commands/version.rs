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

//! Version command implementation.

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;

/// Show version information for CLI and server.
pub async fn run(config: Option<&Config>) -> Result<()> {
    // CLI version (always available)
    println!("{}", "CLI Version:".bold());
    println!("  Name:    {}", env!("CARGO_PKG_NAME"));
    println!("  Version: {}", env!("CARGO_PKG_VERSION"));
    println!("  Git SHA: {}", env!("GIT_SHA"));

    // Server version (requires config and connectivity)
    if let Some(cfg) = config {
        println!();
        match Client::new(cfg) {
            Ok(client) => match client.get_version().await {
                Ok(server_info) => {
                    println!("{}", "Server Version:".bold());
                    println!("  Name:    {}", server_info.name);
                    println!("  Version: {}", server_info.version);
                    println!("  Git SHA: {}", server_info.git_sha);
                }
                Err(e) => {
                    println!(
                        "{} {}",
                        "Server:".bold(),
                        format!("(unavailable: {})", e).dimmed()
                    );
                }
            },
            Err(e) => {
                println!(
                    "{} {}",
                    "Server:".bold(),
                    format!("(unavailable: {})", e).dimmed()
                );
            }
        }
    } else {
        println!();
        println!(
            "{} {}",
            "Server:".bold(),
            "(no config available - run 'gtr init')".dimmed()
        );
    }

    Ok(())
}

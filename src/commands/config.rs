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

//! General configuration command implementation.

use colored::Colorize;

use crate::Result;
use crate::config::Config;
use crate::icons::Icons;

/// Show current editor configuration.
pub fn show_editor(config: &Config) -> Result<()> {
    let editor = resolve_editor(config);
    let source = get_editor_source(config);

    println!("{}", "Editor Configuration".bold().green());
    println!("{}", "─".repeat(50));
    println!("  Current: {}", editor.cyan());
    println!("  Source:  {}", source.dimmed());
    println!();

    Ok(())
}

/// Set editor in configuration file.
pub fn set_editor(config: &mut Config, editor: String) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    config.editor = Some(editor.clone());
    config.save()?;

    println!(
        "{}",
        format!("{} Editor configuration updated!", icons.success)
            .green()
            .bold()
    );
    println!("  Editor set to: {}", editor.cyan());
    println!();

    Ok(())
}

/// Unset (remove) editor from configuration file.
pub fn unset_editor(config: &mut Config) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    config.editor = None;
    config.save()?;

    let fallback = resolve_editor(config);
    let source = get_editor_source(config);

    println!(
        "{}",
        format!("{} Editor configuration removed!", icons.success)
            .green()
            .bold()
    );
    println!("  Now using: {} ({})", fallback.cyan(), source.dimmed());
    println!();

    Ok(())
}

/// Resolve which editor to use, with fallback chain.
fn resolve_editor(config: &Config) -> String {
    config
        .editor
        .clone()
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .unwrap_or_else(|| "vi".to_string())
}

/// Get the source of the current editor setting.
fn get_editor_source(config: &Config) -> String {
    if config.editor.is_some() {
        "config file".to_string()
    } else if std::env::var("EDITOR").is_ok() {
        "$EDITOR".to_string()
    } else if std::env::var("VISUAL").is_ok() {
        "$VISUAL".to_string()
    } else {
        "default".to_string()
    }
}

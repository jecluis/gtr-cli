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

//! Icon theme support for terminal output.
//!
//! Provides two themes:
//! - **Unicode**: uses standard Unicode emoji (works everywhere)
//! - **Nerd**: uses Nerd Font glyphs (requires a patched font)
//!
//! Nerd Font glyphs live in the Private Use Area and render as exactly
//! 1 cell, avoiding the emoji-width alignment problems that plague the
//! Unicode theme.  Nerd theme icons include ANSI color codes so the
//! monochrome glyphs are visually distinguishable at a glance.

use std::fmt;

use colored::Colorize;
use serde::{Deserialize, Serialize};

/// Icon theme selector.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconTheme {
    #[default]
    Unicode,
    Nerd,
}

impl fmt::Display for IconTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unicode => write!(f, "unicode"),
            Self::Nerd => write!(f, "nerd"),
        }
    }
}

impl std::str::FromStr for IconTheme {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "unicode" => Ok(Self::Unicode),
            "nerd" => Ok(Self::Nerd),
            _ => Err(format!(
                "unknown icon theme: {s} (expected 'unicode' or 'nerd')"
            )),
        }
    }
}

/// Complete set of icons used throughout the CLI.
///
/// Each field is a `String` containing the glyph(s) ready for direct
/// interpolation into formatted output.  Nerd theme fields may include
/// ANSI color escape codes.
pub struct Icons {
    // -- Impact indicators (used as prefix before priority in tables) --
    /// Catastrophic impact (impact 1)
    pub impact_critical: String,
    /// Significant impact (impact 2)
    pub impact_significant: String,
    /// Padding when no impact icon is shown (impact 3+)
    pub impact_none: String,

    // -- Deadline urgency (prepended to task title in list) --
    /// Task is overdue
    pub overdue: String,
    /// Deadline is approaching (within warning threshold)
    pub deadline_warning: String,

    // -- Joy indicators (prepended to task title in list) --
    /// High joy (8-10)
    pub joy_high: String,
    /// Low joy (0-4)
    pub joy_low: String,

    // -- Next command: picker context --
    /// Priority "now" indicator in next picker
    pub priority_now: String,

    // -- Sync status --
    /// Operation succeeded / synced with server
    pub success: String,
    /// Operation failed / sync failed
    pub failure: String,
    /// Queued for later sync
    pub queued: String,

    // -- Hierarchy (subtitle line in list tables) --
    /// Parent indicator ("belongs to")
    pub hierarchy_parent: String,
    /// Subtask count indicator ("has children")
    pub hierarchy_subtasks: String,
    /// Separator between parent ID and subtask count
    pub hierarchy_separator: String,

    // -- Bookmark --
    /// Bookmark glyph (prepended to title for --bookmark tasks)
    pub bookmark: String,

    // -- Labels --
    /// Label glyph (shown when --with-labels is active)
    pub label: String,

    // -- Links --
    /// Link glyph (namespace-project association)
    pub link: String,

    // -- Informational --
    /// Non-blocking informational message
    pub info: String,
    /// Operation cancelled by user
    pub cancelled: String,
}

impl Icons {
    /// Return the joy icon for a given joy value, or empty string for neutral.
    pub fn joy_icon(&self, joy: u8) -> &str {
        match joy {
            8..=10 => &self.joy_high,
            0..=4 => &self.joy_low,
            _ => "",
        }
    }

    /// Build an icon set for the given theme.
    pub fn new(theme: IconTheme) -> Self {
        match theme {
            IconTheme::Unicode => Self::unicode(),
            IconTheme::Nerd => Self::nerd(),
        }
    }

    fn unicode() -> Self {
        Self {
            // Impact: emoji are 2 cells each, + 1 space = 3-cell prefix
            impact_critical: "\u{1f525} ".into(),   // 🔥 + space
            impact_significant: "\u{26a1} ".into(), // ⚡ + space
            impact_none: "   ".into(),              // 3 spaces

            // Deadline urgency
            overdue: "\u{1f4a5} ".into(),         // 💥 + space
            deadline_warning: "\u{26a0} ".into(), // ⚠ + space

            // Joy
            joy_high: "\u{1f31f}".into(), // 🌟
            joy_low: "\u{1f4a4}".into(),  // 💤

            // Next picker
            priority_now: "\u{1f534}".into(), // 🔴

            // Sync status
            success: "\u{2713}".into(), // ✓
            failure: "\u{2717}".into(), // ✗
            queued: "\u{2299}".into(),  // ⊙

            // Hierarchy
            hierarchy_parent: "\u{21b3}".into(),    // ↳
            hierarchy_subtasks: "\u{25b6}".into(),  // ▶
            hierarchy_separator: "\u{00b7}".into(), // ·

            // Bookmark
            bookmark: "\u{1f516} ".into(), // 🔖 + space

            // Labels
            label: "\u{1f3f7}".into(), // 🏷

            // Links
            link: "\u{1f517}".into(), // 🔗

            // Informational
            info: "\u{2139}".into(),      // ℹ
            cancelled: "\u{2717}".into(), // ✗
        }
    }

    fn nerd() -> Self {
        Self {
            // Impact: NF glyphs are 1 cell each, + 1 space = 2-cell prefix
            // Colors make monochrome glyphs visually distinguishable
            impact_critical: format!("{} ", "\u{f0238}".red()), // 󰈸 nf-md-fire (red)
            impact_significant: format!("{} ", "\u{f0e7}".blue()), // nf-fa-bolt (blue)
            impact_none: "  ".into(),                           // 2 spaces

            // Deadline urgency
            overdue: format!("{} ", "\u{f1e2}".red()), // nf-fa-bomb (red)
            deadline_warning: format!("{} ", "\u{f253}".yellow()), // nf-fa-hourglass_half (yellow)

            // Joy
            joy_high: format!("{}", "\u{f005}".yellow()), // nf-fa-star (yellow)
            joy_low: format!("{}", "\u{f0904}".blue()),   // 󰤄 nf-md-sleep (blue)

            // Next picker
            priority_now: format!("{}", "\u{f0238}".red()), // 󰈸 nf-md-fire (red)

            // Sync status
            success: "\u{f00c}".into(), // nf-fa-check
            failure: "\u{f00d}".into(), // nf-fa-close
            queued: "\u{f110}".into(),  // nf-fa-spinner

            // Hierarchy
            hierarchy_parent: "\u{f0da3}".into(), // 󰶣 nf-md (up-arrow)
            hierarchy_subtasks: "\u{ef81}".into(), // nf (folder tree)
            hierarchy_separator: "\u{f444}".into(), // nf-oct-dot_fill

            // Bookmark
            bookmark: format!("{} ", "\u{f00c0}".cyan()), // 󰃀 nf-md-bookmark (cyan)

            // Labels
            label: format!("{}", "\u{f03b}".red()), // nf-fa-tags (red)

            // Links
            link: format!("{}", "\u{f0c1}".cyan()), // nf-fa-link (cyan)

            // Informational
            info: "\u{f05a}".into(),      // nf-fa-info_circle
            cancelled: "\u{f00d}".into(), // nf-fa-close
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_default_is_unicode() {
        assert_eq!(IconTheme::default(), IconTheme::Unicode);
    }

    #[test]
    fn theme_round_trip_display_parse() {
        for theme in [IconTheme::Unicode, IconTheme::Nerd] {
            let s = theme.to_string();
            let parsed: IconTheme = s.parse().unwrap();
            assert_eq!(parsed, theme);
        }
    }

    #[test]
    fn theme_parse_case_insensitive() {
        assert_eq!("NERD".parse::<IconTheme>().unwrap(), IconTheme::Nerd);
        assert_eq!("Unicode".parse::<IconTheme>().unwrap(), IconTheme::Unicode);
    }

    #[test]
    fn theme_parse_invalid() {
        assert!("fancy".parse::<IconTheme>().is_err());
    }

    #[test]
    fn unicode_icons_are_non_empty() {
        let icons = Icons::new(IconTheme::Unicode);
        assert!(!icons.success.is_empty());
        assert!(!icons.failure.is_empty());
        assert!(!icons.queued.is_empty());
        assert!(!icons.info.is_empty());
        assert!(!icons.impact_critical.is_empty());
        assert!(!icons.impact_significant.is_empty());
        assert!(!icons.joy_high.is_empty());
        assert!(!icons.joy_low.is_empty());
    }

    #[test]
    fn nerd_icons_are_non_empty() {
        let icons = Icons::new(IconTheme::Nerd);
        assert!(!icons.success.is_empty());
        assert!(!icons.failure.is_empty());
        assert!(!icons.queued.is_empty());
        assert!(!icons.info.is_empty());
        assert!(!icons.impact_critical.is_empty());
        assert!(!icons.impact_significant.is_empty());
        assert!(!icons.joy_high.is_empty());
        assert!(!icons.joy_low.is_empty());
    }

    #[test]
    fn serde_round_trip() {
        let json = serde_json::to_string(&IconTheme::Nerd).unwrap();
        assert_eq!(json, "\"nerd\"");
        let parsed: IconTheme = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, IconTheme::Nerd);
    }
}

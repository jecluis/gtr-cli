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

/// Raw glyph codepoints without ANSI color codes or trailing spaces.
///
/// Unlike [`Icons`], which contains pre-formatted strings with ANSI color
/// escapes for CLI output, `Glyphs` holds bare codepoints suitable for
/// contexts that handle styling separately (e.g. ratatui TUI).
pub struct Glyphs {
    pub impact_critical: &'static str,
    pub impact_significant: &'static str,
    /// Theme-dependent padding width: 3 spaces (unicode) or 2 (nerd).
    pub impact_pad: &'static str,
    pub overdue: &'static str,
    pub deadline_warning: &'static str,
    pub joy_high: &'static str,
    pub joy_low: &'static str,
    pub priority_now: &'static str,
    pub success: &'static str,
    pub failure: &'static str,
    pub queued: &'static str,
    pub hierarchy_parent: &'static str,
    pub hierarchy_subtasks: &'static str,
    pub hierarchy_separator: &'static str,
    pub bookmark: &'static str,
    pub label: &'static str,
    pub link: &'static str,
    pub info: &'static str,
    pub cancelled: &'static str,
    pub work_doing: &'static str,
    pub work_stopped: &'static str,
    /// Padding equal to work-state icon + trailing space (3 for unicode, 2 for nerd).
    pub work_pad: &'static str,
}

impl Glyphs {
    /// Build a glyph set for the given theme.
    pub fn new(theme: IconTheme) -> Self {
        match theme {
            IconTheme::Unicode => Self::unicode(),
            IconTheme::Nerd => Self::nerd(),
        }
    }

    /// Return the joy glyph for a given joy value, or empty string for neutral.
    pub fn joy_icon(&self, joy: u8) -> &str {
        match joy {
            8..=10 => self.joy_high,
            0..=4 => self.joy_low,
            _ => "",
        }
    }

    const fn unicode() -> Self {
        Self {
            impact_critical: "\u{1f525}",     // 🔥
            impact_significant: "\u{26a1}",   // ⚡
            impact_pad: "   ",                // 3 spaces (emoji 2-cell + space)
            overdue: "\u{1f4a5}",             // 💥
            deadline_warning: "\u{26a0}",     // ⚠
            joy_high: "\u{1f31f}",            // 🌟
            joy_low: "\u{1f4a4}",             // 💤
            priority_now: "\u{1f534}",        // 🔴
            success: "\u{2713}",              // ✓
            failure: "\u{2717}",              // ✗
            queued: "\u{2299}",               // ⊙
            hierarchy_parent: "\u{21b3}",     // ↳
            hierarchy_subtasks: "\u{25b6}",   // ▶
            hierarchy_separator: "\u{00b7}",  // ·
            bookmark: "\u{1f516}",            // 🔖
            label: "\u{1f3f7}",               // 🏷
            link: "\u{1f517}",                // 🔗
            info: "\u{2139}",                 // ℹ
            cancelled: "\u{2717}",            // ✗
            work_doing: "\u{1f528}",          // 🔨
            work_stopped: "\u{23f8}\u{fe0e}", // ⏸︎
            work_pad: "   ",                  // 3 spaces (emoji 2-cell + space)
        }
    }

    const fn nerd() -> Self {
        Self {
            impact_critical: "\u{f0238}",    // 󰈸 nf-md-fire
            impact_significant: "\u{f0e7}",  // nf-fa-bolt
            impact_pad: "  ",                // 2 spaces (NF 1-cell + space)
            overdue: "\u{f1e2}",             // nf-fa-bomb
            deadline_warning: "\u{f253}",    // nf-fa-hourglass_half
            joy_high: "\u{f005}",            // nf-fa-star
            joy_low: "\u{f0904}",            // 󰤄 nf-md-sleep
            priority_now: "\u{f0238}",       // 󰈸 nf-md-fire
            success: "\u{f00c}",             // nf-fa-check
            failure: "\u{f00d}",             // nf-fa-close
            queued: "\u{f110}",              // nf-fa-spinner
            hierarchy_parent: "\u{f0da3}",   // 󰶣
            hierarchy_subtasks: "\u{ef81}",  //
            hierarchy_separator: "\u{f444}", //
            bookmark: "\u{f00c0}",           // 󰃀 nf-md-bookmark
            label: "\u{f03b}",               // nf-fa-tags
            link: "\u{f0c1}",                // nf-fa-link
            info: "\u{f05a}",                // nf-fa-info_circle
            cancelled: "\u{f00d}",           // nf-fa-close
            work_doing: "\u{f0ad}",          // nf-fa-wrench
            work_stopped: "\u{f04c}",        // nf-fa-pause
            work_pad: "  ",                  // 2 spaces (NF 1-cell + space)
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
        let g = Glyphs::new(theme);
        match theme {
            IconTheme::Unicode => Self {
                impact_critical: format!("{} ", g.impact_critical),
                impact_significant: format!("{} ", g.impact_significant),
                impact_none: g.impact_pad.into(),
                overdue: format!("{} ", g.overdue),
                deadline_warning: format!("{} ", g.deadline_warning),
                joy_high: g.joy_high.into(),
                joy_low: g.joy_low.into(),
                priority_now: g.priority_now.into(),
                success: g.success.into(),
                failure: g.failure.into(),
                queued: g.queued.into(),
                hierarchy_parent: g.hierarchy_parent.into(),
                hierarchy_subtasks: g.hierarchy_subtasks.into(),
                hierarchy_separator: g.hierarchy_separator.into(),
                bookmark: format!("{} ", g.bookmark),
                label: g.label.into(),
                link: g.link.into(),
                info: g.info.into(),
                cancelled: g.cancelled.into(),
            },
            IconTheme::Nerd => Self {
                impact_critical: format!("{} ", g.impact_critical.red()),
                impact_significant: format!("{} ", g.impact_significant.blue()),
                impact_none: g.impact_pad.into(),
                overdue: format!("{} ", g.overdue.red()),
                deadline_warning: format!("{} ", g.deadline_warning.yellow()),
                joy_high: format!("{}", g.joy_high.yellow()),
                joy_low: format!("{}", g.joy_low.blue()),
                priority_now: format!("{}", g.priority_now.red()),
                success: g.success.into(),
                failure: g.failure.into(),
                queued: g.queued.into(),
                hierarchy_parent: g.hierarchy_parent.into(),
                hierarchy_subtasks: g.hierarchy_subtasks.into(),
                hierarchy_separator: g.hierarchy_separator.into(),
                bookmark: format!("{} ", g.bookmark.cyan()),
                label: format!("{}", g.label.red()),
                link: format!("{}", g.link.cyan()),
                info: g.info.into(),
                cancelled: g.cancelled.into(),
            },
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

    #[test]
    fn unicode_glyphs_are_non_empty() {
        let g = Glyphs::new(IconTheme::Unicode);
        assert!(!g.impact_critical.is_empty());
        assert!(!g.impact_significant.is_empty());
        assert!(!g.overdue.is_empty());
        assert!(!g.joy_high.is_empty());
        assert!(!g.joy_low.is_empty());
        assert!(!g.bookmark.is_empty());
        assert!(!g.label.is_empty());
        assert!(!g.hierarchy_parent.is_empty());
    }

    #[test]
    fn nerd_glyphs_are_non_empty() {
        let g = Glyphs::new(IconTheme::Nerd);
        assert!(!g.impact_critical.is_empty());
        assert!(!g.impact_significant.is_empty());
        assert!(!g.overdue.is_empty());
        assert!(!g.joy_high.is_empty());
        assert!(!g.joy_low.is_empty());
        assert!(!g.bookmark.is_empty());
        assert!(!g.label.is_empty());
        assert!(!g.hierarchy_parent.is_empty());
    }

    #[test]
    fn glyphs_joy_icon_ranges() {
        let g = Glyphs::new(IconTheme::Unicode);
        assert_eq!(g.joy_icon(10), g.joy_high);
        assert_eq!(g.joy_icon(8), g.joy_high);
        assert_eq!(g.joy_icon(4), g.joy_low);
        assert_eq!(g.joy_icon(0), g.joy_low);
        assert_eq!(g.joy_icon(5), "");
        assert_eq!(g.joy_icon(7), "");
    }
}

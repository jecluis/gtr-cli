// SPDX-License-Identifier: AGPL-3.0-or-later
// gtr - ADHD-friendly task tracker with offline-first CRDT sync
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

//! Markdown rendering for task bodies and descriptions.

use termimad::{MadSkin, crossterm::style::Color};

/// Markdown renderer with GTR-themed styling.
pub struct MarkdownRenderer {
    skin: MadSkin,
    enabled: bool,
}

impl MarkdownRenderer {
    /// Create a new markdown renderer.
    ///
    /// Respects the NO_COLOR environment variable - if set, markdown
    /// rendering is disabled and plain text is returned.
    pub fn new() -> Self {
        Self::with_override(None)
    }

    /// Create a new markdown renderer with an explicit enabled/disabled override.
    ///
    /// - `None`: Use default logic (check NO_COLOR and TTY)
    /// - `Some(true)`: Force enable markdown rendering
    /// - `Some(false)`: Force disable markdown rendering
    pub fn with_override(override_enabled: Option<bool>) -> Self {
        let enabled = match override_enabled {
            Some(true) => true,
            Some(false) => false,
            None => should_render_markdown(),
        };

        let mut skin = MadSkin::default();

        if enabled {
            // Headers - yellow for visibility
            skin.headers[0].set_fg(Color::Yellow);
            skin.headers[1].set_fg(Color::Yellow);

            // Inline code - dark background
            skin.inline_code.set_bg(Color::Rgb {
                r: 40,
                g: 40,
                b: 40,
            });
            skin.inline_code.set_fg(Color::Rgb {
                r: 200,
                g: 200,
                b: 200,
            });

            // Bold - bright white
            skin.bold.set_fg(Color::White);

            // Italic - cyan
            skin.italic.set_fg(Color::Cyan);

            // Code blocks - same as inline code
            skin.code_block.set_bg(Color::Rgb {
                r: 40,
                g: 40,
                b: 40,
            });

            // Lists - green bullet points
            skin.bullet.set_fg(Color::Green);
        }

        Self { skin, enabled }
    }

    /// Render markdown to formatted terminal output.
    ///
    /// If markdown rendering is disabled (NO_COLOR set or --no-format),
    /// returns the input text unchanged.
    ///
    /// Output is hard-wrapped at 80 columns for consistent display across
    /// terminal widths.
    pub fn render(&self, markdown: &str) -> String {
        if !self.enabled || markdown.is_empty() {
            return markdown.to_string();
        }

        // Render with fixed 80-column width for consistent wrapping
        self.skin.text(markdown, Some(80)).to_string()
    }

    /// Check if markdown rendering is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine if markdown should be rendered.
///
/// Markdown rendering is disabled if:
/// - NO_COLOR environment variable is set
/// - Not running in a TTY (piped output)
fn should_render_markdown() -> bool {
    // Check NO_COLOR environment variable
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    // Check if stdout is a TTY
    #[cfg(unix)]
    {
        use std::io::IsTerminal;
        std::io::stdout().is_terminal()
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, assume TTY for now
        // TODO: Add Windows TTY detection if needed
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_markdown_returns_empty() {
        let renderer = MarkdownRenderer::new();
        assert_eq!(renderer.render(""), "");
    }

    #[test]
    fn plain_text_unchanged_when_disabled() {
        // Force disable markdown rendering
        let renderer = MarkdownRenderer::with_override(Some(false));
        assert_eq!(renderer.render("plain text"), "plain text");
    }

    #[test]
    fn renders_basic_markdown() {
        // Force enable markdown rendering for testing (stdout is not a TTY in tests)
        let renderer = MarkdownRenderer::with_override(Some(true));
        let input = "**bold** and *italic*";
        let output = renderer.render(input);

        // Output should be different from input (has formatting codes)
        // We can't easily test exact output due to ANSI codes,
        // but we can verify it's not the same
        assert_ne!(output, input);
    }
}

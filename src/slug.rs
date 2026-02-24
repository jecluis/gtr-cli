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

//! Slug utilities for CLI document resolution.

/// Extract the last 8-character hex suffix after the final `-`.
///
/// Returns `None` if the slug has no `-` or if the suffix is not exactly
/// 8 hex characters.
pub fn extract_hex_suffix(slug: &str) -> Option<&str> {
    if !slug.contains('-') {
        return None;
    }
    let suffix = slug.rsplit('-').next()?;
    if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(suffix)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_valid_hex_suffix() {
        assert_eq!(extract_hex_suffix("my-document-a1b2c3d4"), Some("a1b2c3d4"));
    }

    #[test]
    fn extracts_uppercase_hex() {
        assert_eq!(extract_hex_suffix("some-title-AABBCCDD"), Some("AABBCCDD"));
    }

    #[test]
    fn returns_none_for_no_dash() {
        assert_eq!(extract_hex_suffix("a1b2c3d4"), None);
    }

    #[test]
    fn returns_none_for_wrong_length() {
        assert_eq!(extract_hex_suffix("doc-abc"), None);
        assert_eq!(extract_hex_suffix("doc-a1b2c3d4e5"), None);
    }

    #[test]
    fn returns_none_for_non_hex() {
        assert_eq!(extract_hex_suffix("doc-a1b2g3d4"), None);
    }

    #[test]
    fn handles_multiple_dashes() {
        assert_eq!(
            extract_hex_suffix("my-long-title-here-deadbeef"),
            Some("deadbeef")
        );
    }
}

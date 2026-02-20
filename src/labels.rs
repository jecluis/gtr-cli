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

//! Label validation for per-project task labels.

use crate::{Error, Result};

/// Validate a label string.
///
/// Labels must be non-empty, start with `[a-z0-9]`, and contain only
/// lowercase letters, digits, and the characters `-:./\'`.
pub fn validate_label(label: &str) -> Result<()> {
    if label.is_empty() {
        return Err(Error::UserFacing("label cannot be empty".to_string()));
    }

    let first = label.chars().next().unwrap();
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(Error::UserFacing(format!(
            "label '{}' must start with a lowercase letter or digit",
            label
        )));
    }

    for ch in label.chars() {
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && !"-:./\\'".contains(ch) {
            return Err(Error::UserFacing(format!(
                "label '{}' contains invalid character '{}'; \
                 allowed: a-z, 0-9, - : . / '",
                label, ch
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_labels() {
        assert!(validate_label("bug").is_ok());
        assert!(validate_label("feature").is_ok());
        assert!(validate_label("p1").is_ok());
        assert!(validate_label("scope:frontend").is_ok());
        assert!(validate_label("area/cli").is_ok());
        assert!(validate_label("won't-fix").is_ok());
        assert!(validate_label("v1.0").is_ok());
    }

    #[test]
    fn empty_label() {
        assert!(validate_label("").is_err());
    }

    #[test]
    fn uppercase_rejected() {
        assert!(validate_label("Bug").is_err());
        assert!(validate_label("FEATURE").is_err());
    }

    #[test]
    fn starts_with_special_rejected() {
        assert!(validate_label("-bug").is_err());
        assert!(validate_label(":scope").is_err());
        assert!(validate_label("/path").is_err());
    }

    #[test]
    fn invalid_chars_rejected() {
        assert!(validate_label("bug fix").is_err());
        assert!(validate_label("bug@home").is_err());
        assert!(validate_label("bug#1").is_err());
    }
}

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

//! Label validation and migration helpers for per-project task labels.

use crate::cache::TaskCache;
use crate::{Error, Result};

/// Normalize a label: trim, lowercase, then validate.
///
/// Returns the cleaned label on success.
pub fn normalize_label(input: &str) -> Result<String> {
    let label = input.trim().to_ascii_lowercase();
    validate_label(&label)?;
    Ok(label)
}

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
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && !"-:./\\'&".contains(ch) {
            return Err(Error::UserFacing(format!(
                "label '{}' contains invalid character '{}'; \
                 allowed: a-z, 0-9, - : . / ' &",
                label, ch
            )));
        }
    }

    Ok(())
}

/// Find task labels that don't exist in a target project's effective
/// label registry (own + inherited). Returns the list of missing
/// labels. Callers decide whether to create them in the target or
/// remove them from the task.
pub fn find_missing_labels(
    task_labels: &[String],
    target_project_id: &str,
    cache: &TaskCache,
) -> Result<Vec<String>> {
    if task_labels.is_empty() {
        return Ok(Vec::new());
    }
    let target_labels = cache.get_effective_labels(target_project_id)?;
    Ok(task_labels
        .iter()
        .filter(|l| !target_labels.contains(l))
        .cloned()
        .collect())
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

    #[test]
    fn normalize_lowercases_and_trims() {
        assert_eq!(normalize_label("Bug").unwrap(), "bug");
        assert_eq!(normalize_label("FEATURE").unwrap(), "feature");
        assert_eq!(
            normalize_label("  scope:Frontend  ").unwrap(),
            "scope:frontend"
        );
        assert_eq!(normalize_label("Area/CLI").unwrap(), "area/cli");
    }

    #[test]
    fn normalize_rejects_invalid() {
        assert!(normalize_label("").is_err());
        assert!(normalize_label("-bug").is_err());
        assert!(normalize_label("bug fix").is_err());
    }
}

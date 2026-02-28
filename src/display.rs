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

//! Shared display logic for CLI and TUI task rendering.
//!
//! Pure-data types and functions with zero dependencies on `colored` or
//! `ratatui`. Both consumers call the same functions and map results to
//! their own styled output.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use chrono_humanize::{Accuracy, HumanTime, Tense};

use crate::threshold_cache::CachedThresholds;
use crate::utils;

/// Whether a task's deadline needs visual urgency treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlineUrgency {
    /// Deadline has passed.
    Overdue,
    /// Within 25% of the effective promotion threshold.
    Warning,
    /// No urgency (no deadline, far away, or done/deleted).
    None,
}

/// Impact severity derived from the numeric impact field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactLevel {
    /// Impact 1 — catastrophic.
    Critical,
    /// Impact 2 — significant.
    Significant,
    /// Impact 3+ — normal or lower.
    Normal,
}

/// Plain-text deadline rendering result.
pub struct DeadlineDisplay {
    /// Human-readable text (e.g. "in 3 days", "2 hours ago", "2026-03-15").
    pub text: String,
    /// Whether the deadline is in the past.
    pub is_overdue: bool,
}

/// Index into the 12-colour label palette.
pub type LabelColorIndex = usize;

/// Number of colours in the label palette.
pub const LABEL_PALETTE_LEN: usize = 12;

/// Compute deadline urgency for a task.
///
/// - **Overdue** if the deadline is in the past.
/// - **Warning** if the remaining time is ≤ 25 % of the effective
///   promotion threshold (base threshold for task size × impact
///   multiplier).
/// - **None** otherwise (no deadline, unparseable, or plenty of time).
pub fn deadline_urgency(
    deadline_str: Option<&str>,
    size: &str,
    impact: u8,
    thresholds: &CachedThresholds,
) -> DeadlineUrgency {
    let Some(ds) = deadline_str else {
        return DeadlineUrgency::None;
    };
    let Ok(deadline) = DateTime::parse_from_rfc3339(ds) else {
        return DeadlineUrgency::None;
    };

    let now = Utc::now();
    let deadline_utc = deadline.with_timezone(&Utc);

    if deadline_utc < now {
        return DeadlineUrgency::Overdue;
    }

    let base_secs = thresholds
        .deadline
        .get(size)
        .and_then(|s| utils::parse_threshold_secs(s))
        .unwrap_or(86400);

    let multiplier = thresholds
        .impact_multipliers
        .get(&impact.to_string())
        .copied()
        .unwrap_or(1.0);
    let effective_secs = (base_secs as f64 * multiplier) as i64;

    let warning_secs = effective_secs / 4;
    let remaining = (deadline_utc - now).num_seconds();

    if remaining <= warning_secs {
        DeadlineUrgency::Warning
    } else {
        DeadlineUrgency::None
    }
}

/// Format a deadline as plain text with an overdue flag.
///
/// Returns `None` when the input is `None` or unparseable.
/// Uses chrono-humanize for relative formatting; falls back to absolute
/// date (`YYYY-MM-DD`) for deadlines more than 30 days away.
pub fn format_deadline_relative(deadline_str: Option<&str>) -> Option<DeadlineDisplay> {
    let s = deadline_str?;
    let deadline = DateTime::parse_from_rfc3339(s).ok()?;
    let now = Utc::now();
    let is_overdue = deadline < now;

    let duration = if is_overdue {
        now.signed_duration_since(deadline)
    } else {
        deadline.signed_duration_since(now)
    };

    let text = if duration.num_days() > 30 {
        deadline
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string()
    } else {
        let ht = HumanTime::from(deadline);
        let tense = if is_overdue {
            Tense::Past
        } else {
            Tense::Future
        };
        ht.to_text_en(Accuracy::Rough, tense)
    };

    Some(DeadlineDisplay { text, is_overdue })
}

/// Split a task ID into `(prefix, suffix)` for styled rendering.
///
/// Takes the first 8 characters then splits at `prefix_len`.
pub fn split_id(id: &str, prefix_len: usize) -> (&str, &str) {
    let short = &id[..8];
    (&short[..prefix_len], &short[prefix_len..])
}

/// Map a priority string to a numeric rank (lower = higher priority).
///
/// `"now"` → 0, `"later"` → 1, anything else → 2.
pub fn priority_rank(priority: &str) -> u8 {
    match priority {
        "now" => 0,
        "later" => 1,
        _ => 2,
    }
}

/// Classify a numeric impact value into a severity level.
pub fn impact_level(impact: u8) -> ImpactLevel {
    match impact {
        1 => ImpactLevel::Critical,
        2 => ImpactLevel::Significant,
        _ => ImpactLevel::Normal,
    }
}

/// Compare two optional deadline strings for sorting.
///
/// Tasks with deadlines sort before those without; nearer deadlines
/// sort first.
pub fn cmp_deadline(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Assign a stable colour index to each label based on first appearance.
///
/// Iterates over tasks in display order and assigns the next palette
/// slot to each label the first time it is seen. Both CLI and TUI map
/// the returned index to their own colour type via a 12-element palette
/// array.
pub fn assign_label_colors<'a>(
    tasks_labels: impl Iterator<Item = &'a [String]>,
) -> HashMap<&'a str, LabelColorIndex> {
    let mut map = HashMap::new();
    for labels in tasks_labels {
        for label in labels {
            if !map.contains_key(label.as_str()) {
                let idx = map.len();
                map.insert(label.as_str(), idx % LABEL_PALETTE_LEN);
            }
        }
    }
    map
}

/// A compact, fixed-width deadline rendering for table columns.
pub struct CompactDeadline {
    /// Short text like "3d ago", "in 2w", "now" (max ~9 chars).
    pub text: String,
    /// Whether the deadline is in the past.
    pub is_overdue: bool,
}

/// Format a deadline as a compact string suitable for narrow table columns.
///
/// Returns `None` when the input is `None` or unparseable.
/// Max width: ~9 characters. No chrono-humanize dependency.
pub fn format_deadline_compact(deadline_str: Option<&str>) -> Option<CompactDeadline> {
    let s = deadline_str?;
    let deadline = DateTime::parse_from_rfc3339(s).ok()?;
    let now = Utc::now();
    let is_overdue = deadline < now;

    let secs = if is_overdue {
        now.signed_duration_since(deadline).num_seconds()
    } else {
        deadline.signed_duration_since(now).num_seconds()
    };

    let unit = if secs <= 10 {
        "now".to_string()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else if secs < 604800 {
        format!("{}d", secs / 86400)
    } else if secs < 2592000 {
        format!("{}w", secs / 604800)
    } else if secs < 31536000 {
        format!("{}mo", secs / 2592000)
    } else {
        format!("{}y", secs / 31536000)
    };

    let text = if unit == "now" {
        unit
    } else if is_overdue {
        format!("{unit} ago")
    } else {
        format!("in {unit}")
    };

    Some(CompactDeadline { text, is_overdue })
}

/// Pre-computed progress bar dimensions for a given bar width.
pub struct ProgressBar {
    /// Number of filled blocks.
    pub filled: usize,
    /// Number of empty blocks.
    pub empty: usize,
    /// The raw percentage value (0-100).
    pub percentage: u8,
}

/// Compute progress bar dimensions from an optional progress value.
///
/// Returns `None` when progress is `None`. Both CLI and TUI map
/// the result to their own styled output (colored/ratatui).
pub fn format_progress_bar(progress: Option<u8>, bar_width: usize) -> Option<ProgressBar> {
    let pct = progress?;
    let filled = (pct as usize * bar_width) / 100;
    let empty = bar_width - filled;
    Some(ProgressBar {
        filled,
        empty,
        percentage: pct,
    })
}

/// Wrap text at a maximum display-width, preserving word boundaries.
///
/// Uses `textwrap` with unicode-width support so multi-cell characters
/// (emoji, CJK) are measured correctly. Words longer than `width` are
/// kept intact on their own line (no mid-word breaking).
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let options = textwrap::Options::new(width)
        .word_separator(textwrap::WordSeparator::AsciiSpace)
        .break_words(false);
    textwrap::wrap(text, options)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Duration;

    fn test_thresholds() -> CachedThresholds {
        CachedThresholds {
            deadline: utils::default_thresholds(),
            impact_labels: utils::default_impact_labels(),
            impact_multipliers: utils::default_impact_multipliers(),
        }
    }

    // ── deadline_urgency ──

    #[test]
    fn deadline_urgency_overdue() {
        let past = (Utc::now() - Duration::hours(1)).to_rfc3339();
        assert_eq!(
            deadline_urgency(Some(&past), "M", 3, &test_thresholds()),
            DeadlineUrgency::Overdue,
        );
    }

    #[test]
    fn deadline_urgency_warning() {
        // M size, 24h base, impact 3 (1.0x) → warning at ≤ 6h
        let soon = (Utc::now() + Duration::hours(3)).to_rfc3339();
        assert_eq!(
            deadline_urgency(Some(&soon), "M", 3, &test_thresholds()),
            DeadlineUrgency::Warning,
        );
    }

    #[test]
    fn deadline_urgency_none_far_away() {
        let far = (Utc::now() + Duration::days(30)).to_rfc3339();
        assert_eq!(
            deadline_urgency(Some(&far), "M", 3, &test_thresholds()),
            DeadlineUrgency::None,
        );
    }

    #[test]
    fn deadline_urgency_no_deadline() {
        assert_eq!(
            deadline_urgency(None, "M", 3, &test_thresholds()),
            DeadlineUrgency::None,
        );
    }

    #[test]
    fn deadline_urgency_bad_parse() {
        assert_eq!(
            deadline_urgency(Some("not-a-date"), "M", 3, &test_thresholds()),
            DeadlineUrgency::None,
        );
    }

    // ── format_deadline_relative ──

    #[test]
    fn format_deadline_past() {
        let past = (Utc::now() - Duration::hours(2)).to_rfc3339();
        let d = format_deadline_relative(Some(&past)).unwrap();
        assert!(d.is_overdue);
        assert!(!d.text.is_empty());
    }

    #[test]
    fn format_deadline_future() {
        let future = (Utc::now() + Duration::hours(5)).to_rfc3339();
        let d = format_deadline_relative(Some(&future)).unwrap();
        assert!(!d.is_overdue);
        assert!(!d.text.is_empty());
    }

    #[test]
    fn format_deadline_far_future() {
        let far = (Utc::now() + Duration::days(60)).to_rfc3339();
        let d = format_deadline_relative(Some(&far)).unwrap();
        assert!(!d.is_overdue);
        // Should be absolute date format
        assert!(d.text.contains('-'));
    }

    #[test]
    fn format_deadline_none() {
        assert!(format_deadline_relative(None).is_none());
    }

    // ── split_id ──

    #[test]
    fn split_id_normal() {
        let (prefix, suffix) = split_id("abcdef01-2345-6789", 3);
        assert_eq!(prefix, "abc");
        assert_eq!(suffix, "def01");
    }

    #[test]
    fn split_id_short_prefix() {
        let (prefix, suffix) = split_id("abcdef01-2345-6789", 1);
        assert_eq!(prefix, "a");
        assert_eq!(suffix, "bcdef01");
    }

    // ── priority_rank ──

    #[test]
    fn priority_rank_all_cases() {
        assert_eq!(priority_rank("now"), 0);
        assert_eq!(priority_rank("later"), 1);
        assert_eq!(priority_rank("someday"), 2);
    }

    // ── impact_level ──

    #[test]
    fn impact_level_critical() {
        assert_eq!(impact_level(1), ImpactLevel::Critical);
    }

    #[test]
    fn impact_level_significant() {
        assert_eq!(impact_level(2), ImpactLevel::Significant);
    }

    #[test]
    fn impact_level_normal() {
        assert_eq!(impact_level(3), ImpactLevel::Normal);
        assert_eq!(impact_level(4), ImpactLevel::Normal);
        assert_eq!(impact_level(5), ImpactLevel::Normal);
    }

    // ── cmp_deadline ──

    #[test]
    fn cmp_deadline_both_present() {
        use std::cmp::Ordering;
        let a = "2026-01-01T00:00:00Z";
        let b = "2026-06-01T00:00:00Z";
        assert_eq!(cmp_deadline(Some(a), Some(b)), Ordering::Less);
        assert_eq!(cmp_deadline(Some(b), Some(a)), Ordering::Greater);
        assert_eq!(cmp_deadline(Some(a), Some(a)), Ordering::Equal);
    }

    #[test]
    fn cmp_deadline_one_absent() {
        use std::cmp::Ordering;
        assert_eq!(
            cmp_deadline(Some("2026-01-01T00:00:00Z"), None),
            Ordering::Less
        );
        assert_eq!(
            cmp_deadline(None, Some("2026-01-01T00:00:00Z")),
            Ordering::Greater
        );
    }

    #[test]
    fn cmp_deadline_neither() {
        assert_eq!(cmp_deadline(None, None), std::cmp::Ordering::Equal);
    }

    // ── assign_label_colors ──

    #[test]
    fn assign_label_colors_ordering() {
        let t1 = vec!["bug".to_string(), "ux".to_string()];
        let t2 = vec!["ux".to_string(), "perf".to_string()];
        let tasks: Vec<&[String]> = vec![&t1, &t2];
        let map = assign_label_colors(tasks.into_iter());

        assert_eq!(map["bug"], 0);
        assert_eq!(map["ux"], 1);
        assert_eq!(map["perf"], 2);
    }

    #[test]
    fn assign_label_colors_cycling() {
        // With more than LABEL_PALETTE_LEN labels, indices wrap around.
        let labels: Vec<String> = (0..15).map(|i| format!("label-{i}")).collect();
        let tasks: Vec<&[String]> = vec![&labels];
        let map = assign_label_colors(tasks.into_iter());

        assert_eq!(map["label-0"], 0);
        assert_eq!(map["label-12"], 0); // wraps
        assert_eq!(map["label-14"], 2);
    }

    #[test]
    fn assign_label_colors_empty() {
        let empty: Vec<String> = vec![];
        let tasks: Vec<&[String]> = vec![&empty];
        let map = assign_label_colors(tasks.into_iter());
        assert!(map.is_empty());
    }

    // ── format_deadline_compact ──

    #[test]
    fn compact_deadline_overdue() {
        let past = (Utc::now() - Duration::hours(3)).to_rfc3339();
        let d = format_deadline_compact(Some(&past)).unwrap();
        assert!(d.is_overdue);
        assert_eq!(d.text, "3h ago");
    }

    #[test]
    fn compact_deadline_future() {
        // Use a large enough offset so integer division doesn't round down.
        let future = (Utc::now() + Duration::days(5) + Duration::hours(1)).to_rfc3339();
        let d = format_deadline_compact(Some(&future)).unwrap();
        assert!(!d.is_overdue);
        assert_eq!(d.text, "in 5d");
    }

    #[test]
    fn compact_deadline_now() {
        let just_now = (Utc::now() + Duration::seconds(3)).to_rfc3339();
        let d = format_deadline_compact(Some(&just_now)).unwrap();
        assert_eq!(d.text, "now");
    }

    #[test]
    fn compact_deadline_seconds() {
        let soon = (Utc::now() + Duration::seconds(45)).to_rfc3339();
        let d = format_deadline_compact(Some(&soon)).unwrap();
        assert!(d.text.starts_with("in "));
        assert!(d.text.ends_with('s'));
    }

    #[test]
    fn compact_deadline_minutes() {
        let soon = (Utc::now() - Duration::minutes(15)).to_rfc3339();
        let d = format_deadline_compact(Some(&soon)).unwrap();
        assert_eq!(d.text, "15m ago");
    }

    #[test]
    fn compact_deadline_weeks() {
        let future = (Utc::now() + Duration::weeks(2) + Duration::hours(1)).to_rfc3339();
        let d = format_deadline_compact(Some(&future)).unwrap();
        assert_eq!(d.text, "in 2w");
    }

    #[test]
    fn compact_deadline_months() {
        let future = (Utc::now() + Duration::days(90)).to_rfc3339();
        let d = format_deadline_compact(Some(&future)).unwrap();
        assert!(d.text.starts_with("in "));
        assert!(d.text.ends_with("mo"));
    }

    #[test]
    fn compact_deadline_years() {
        let future = (Utc::now() + Duration::days(400)).to_rfc3339();
        let d = format_deadline_compact(Some(&future)).unwrap();
        assert_eq!(d.text, "in 1y");
    }

    #[test]
    fn compact_deadline_none() {
        assert!(format_deadline_compact(None).is_none());
    }

    // ── format_progress_bar ──

    #[test]
    fn progress_bar_zero() {
        let pb = format_progress_bar(Some(0), 10).unwrap();
        assert_eq!(pb.filled, 0);
        assert_eq!(pb.empty, 10);
        assert_eq!(pb.percentage, 0);
    }

    #[test]
    fn progress_bar_fifty() {
        let pb = format_progress_bar(Some(50), 10).unwrap();
        assert_eq!(pb.filled, 5);
        assert_eq!(pb.empty, 5);
        assert_eq!(pb.percentage, 50);
    }

    #[test]
    fn progress_bar_hundred() {
        let pb = format_progress_bar(Some(100), 8).unwrap();
        assert_eq!(pb.filled, 8);
        assert_eq!(pb.empty, 0);
        assert_eq!(pb.percentage, 100);
    }

    #[test]
    fn progress_bar_none() {
        assert!(format_progress_bar(None, 10).is_none());
    }

    #[test]
    fn progress_bar_rounding() {
        // 33% of 8 = 2.64 → truncates to 2
        let pb = format_progress_bar(Some(33), 8).unwrap();
        assert_eq!(pb.filled, 2);
        assert_eq!(pb.empty, 6);
    }

    // ── wrap_text ──

    #[test]
    fn wrap_text_short() {
        let lines = wrap_text("hello world", 60);
        assert_eq!(lines, vec!["hello world"]);
    }

    #[test]
    fn wrap_text_exact_boundary() {
        let lines = wrap_text("aaaa bbbb", 9);
        assert_eq!(lines, vec!["aaaa bbbb"]);
    }

    #[test]
    fn wrap_text_wraps() {
        let lines = wrap_text("aaaa bbbb cccc", 9);
        assert_eq!(lines, vec!["aaaa bbbb", "cccc"]);
    }

    #[test]
    fn wrap_text_long_word() {
        let lines = wrap_text("superlongword short", 10);
        assert_eq!(lines, vec!["superlongword", "short"]);
    }

    #[test]
    fn wrap_text_empty() {
        let lines = wrap_text("", 60);
        assert_eq!(lines, vec![""]);
    }
}

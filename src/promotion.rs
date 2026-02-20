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

//! Client-side deadline-driven priority promotion.
//!
//! [`effective_priority`] computes what priority a task _should display_
//! based on deadline proximity, size-based thresholds, and impact scaling.
//! The stored `task.priority` field is never mutated — this is a pure
//! display-time computation with no persistence side-effects.

use chrono::{DateTime, Utc};

use crate::models::Task;
use crate::threshold_cache::CachedThresholds;
use crate::utils;

/// Return the effective priority for display: `"now"` if the task should
/// be promoted due to an approaching deadline, otherwise the stored priority.
///
/// Promotion conditions (mirrors server `should_promote` logic):
/// 1. Stored priority must be `"later"` (already-now tasks stay now)
/// 2. Task must have a parseable deadline
/// 3. Deadline is overdue, OR time remaining <= effective threshold
///    (base threshold for task size * impact multiplier)
pub fn effective_priority<'a>(task: &'a Task, thresholds: &CachedThresholds) -> &'a str {
    if task.priority == "now" {
        return &task.priority;
    }

    let Some(ref deadline_str) = task.deadline else {
        return &task.priority;
    };

    let Ok(deadline) = DateTime::parse_from_rfc3339(deadline_str) else {
        return &task.priority;
    };

    let now = Utc::now();
    let deadline_utc = deadline.with_timezone(&Utc);

    // Overdue -> always promote
    if deadline_utc <= now {
        return "now";
    }

    // Compute effective threshold
    let base_secs = thresholds
        .deadline
        .get(&task.size)
        .and_then(|s| utils::parse_threshold_secs(s))
        .unwrap_or(86400); // fallback: 24h

    let multiplier = thresholds
        .impact_multipliers
        .get(&task.impact.to_string())
        .copied()
        .unwrap_or(1.0);

    let effective_secs = (base_secs as f64 * multiplier) as i64;
    let remaining = (deadline_utc - now).num_seconds();

    if remaining <= effective_secs {
        "now"
    } else {
        &task.priority
    }
}

/// Check if a task's deadline is overdue.
pub fn is_overdue(task: &Task) -> bool {
    task.deadline
        .as_ref()
        .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
        .map(|d| d.with_timezone(&Utc) < Utc::now())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(priority: &str, size: &str, deadline: Option<String>, impact: u8) -> Task {
        Task {
            id: "test-id-00000000-0000-0000-0000-000000000000".to_string(),
            project_id: "test-project".to_string(),
            title: "Test task".to_string(),
            body: String::new(),
            priority: priority.to_string(),
            size: size.to_string(),
            created: Utc::now().to_rfc3339(),
            modified: Utc::now().to_rfc3339(),
            done: None,
            deleted: None,
            deadline,
            version: 1,
            subtasks: Vec::new(),
            custom: serde_json::Value::Null,
            log: Vec::new(),
            current_work_state: None,
            progress: None,
            impact,
            joy: 5,
            parent_id: None,
            labels: Vec::new(),
        }
    }

    fn test_thresholds() -> CachedThresholds {
        CachedThresholds {
            deadline: utils::default_thresholds(),
            impact_labels: utils::default_impact_labels(),
            impact_multipliers: utils::default_impact_multipliers(),
        }
    }

    #[test]
    fn already_now_stays_now() {
        let task = make_task("now", "M", None, 3);
        assert_eq!(effective_priority(&task, &test_thresholds()), "now");
    }

    #[test]
    fn no_deadline_keeps_stored() {
        let task = make_task("later", "M", None, 3);
        assert_eq!(effective_priority(&task, &test_thresholds()), "later");
    }

    #[test]
    fn overdue_promotes_to_now() {
        let past = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let task = make_task("later", "M", Some(past), 3);
        assert_eq!(effective_priority(&task, &test_thresholds()), "now");
    }

    #[test]
    fn within_threshold_promotes() {
        // M size with 24h base threshold, impact 3 (1.0x multiplier)
        // Deadline 12h away -> within 24h threshold -> promoted
        let deadline = (Utc::now() + chrono::Duration::hours(12)).to_rfc3339();
        let task = make_task("later", "M", Some(deadline), 3);
        assert_eq!(effective_priority(&task, &test_thresholds()), "now");
    }

    #[test]
    fn outside_threshold_not_promoted() {
        // M size with 24h base threshold, impact 3 (1.0x)
        // Deadline 7 days away -> outside threshold -> not promoted
        let deadline = (Utc::now() + chrono::Duration::days(7)).to_rfc3339();
        let task = make_task("later", "M", Some(deadline), 3);
        assert_eq!(effective_priority(&task, &test_thresholds()), "later");
    }

    #[test]
    fn high_impact_scales_threshold_up() {
        // M size with 24h base, impact 1 (2.0x) -> effective 48h
        // Deadline 36h away -> within 48h -> promoted
        let deadline = (Utc::now() + chrono::Duration::hours(36)).to_rfc3339();
        let task = make_task("later", "M", Some(deadline), 1);
        assert_eq!(effective_priority(&task, &test_thresholds()), "now");
    }

    #[test]
    fn low_impact_scales_threshold_down() {
        // M size with 24h base, impact 5 (0.25x) -> effective 6h
        // Deadline 12h away -> outside 6h -> not promoted
        let deadline = (Utc::now() + chrono::Duration::hours(12)).to_rfc3339();
        let task = make_task("later", "M", Some(deadline), 5);
        assert_eq!(effective_priority(&task, &test_thresholds()), "later");
    }

    #[test]
    fn is_overdue_with_past_deadline() {
        let past = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let task = make_task("later", "M", Some(past), 3);
        assert!(is_overdue(&task));
    }

    #[test]
    fn is_overdue_with_future_deadline() {
        let future = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        let task = make_task("later", "M", Some(future), 3);
        assert!(!is_overdue(&task));
    }

    #[test]
    fn is_overdue_without_deadline() {
        let task = make_task("later", "M", None, 3);
        assert!(!is_overdue(&task));
    }
}

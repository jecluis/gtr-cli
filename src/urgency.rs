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

//! Urgency scoring for task prioritization.
//!
//! Provides a shared [`TaskFields`] trait and [`calculate_urgency_score`]
//! function used by both the CLI `next` command and the TUI dashboard.

use chrono::{DateTime, Utc};

use crate::promotion;
use crate::threshold_cache::CachedThresholds;

/// Common field accessors shared by [`Task`](crate::models::Task) and
/// [`TaskSummary`](crate::cache::TaskSummary).
pub trait TaskFields {
    fn priority(&self) -> &str;
    fn size(&self) -> &str;
    fn deadline(&self) -> Option<&str>;
    fn impact(&self) -> u8;
    fn joy(&self) -> u8;
    fn current_work_state(&self) -> Option<&str>;
}

impl TaskFields for crate::models::Task {
    fn priority(&self) -> &str {
        &self.priority
    }
    fn size(&self) -> &str {
        &self.size
    }
    fn deadline(&self) -> Option<&str> {
        self.deadline.as_deref()
    }
    fn impact(&self) -> u8 {
        self.impact
    }
    fn joy(&self) -> u8 {
        self.joy
    }
    fn current_work_state(&self) -> Option<&str> {
        self.current_work_state.as_deref()
    }
}

impl TaskFields for crate::cache::TaskSummary {
    fn priority(&self) -> &str {
        &self.priority
    }
    fn size(&self) -> &str {
        &self.size
    }
    fn deadline(&self) -> Option<&str> {
        self.deadline.as_deref()
    }
    fn impact(&self) -> u8 {
        self.impact
    }
    fn joy(&self) -> u8 {
        self.joy
    }
    fn current_work_state(&self) -> Option<&str> {
        self.current_work_state.as_deref()
    }
}

/// Calculate a composite urgency score for sorting.
///
/// Returns a single `f64` where lower = more urgent (sorts first).
///
/// Priority (now vs later) is the only hard tier boundary. Within the
/// same priority, all factors are blended into one number:
///
/// 1. **Overdue decay** — logarithmic diminishing returns prevent stale
///    overdue tasks from permanently dominating the list.
/// 2. **Impact scaling** — high-impact tasks perceive deadlines as closer;
///    low-impact tasks perceive them as further away.
/// 3. **Joy x impact bonus** — enjoyable high-impact tasks get a real
///    boost (hours, not seconds). Joy alone does little.
/// 4. **Size bonus** — small tasks get a nudge for ADHD quick-win
///    momentum.
/// 5. **Work state** — stopped tasks (already have context) get a nudge.
/// 6. **Feels modulation** — energy amplifies/attenuates joy effect,
///    focus amplifies/attenuates size effect. Both neutralized for
///    overdue tasks.
pub fn calculate_urgency_score(
    task: &impl TaskFields,
    now: &DateTime<Utc>,
    thresholds: &CachedThresholds,
    energy: u8,
    focus: u8,
) -> f64 {
    const HOUR: f64 = 3600.0;

    // Priority: hard tier boundary (now=0, later=1e12)
    let priority_tier = match promotion::effective_priority(task, thresholds) {
        "now" => 0.0,
        _ => 1e12,
    };

    // --- Time component ---
    // Approaching: raw seconds remaining, scaled by impact.
    // Overdue: logarithmic decay scaled by impact, so stale low-impact
    // overdue tasks don't dominate.
    let (time_component, is_overdue) = if let Some(deadline_str) = task.deadline() {
        if let Ok(deadline) = DateTime::parse_from_rfc3339(deadline_str) {
            let remaining = (deadline.with_timezone(&Utc) - *now).num_seconds() as f64;

            let impact_approach_scale = match task.impact() {
                1 => 0.5,  // catastrophic: 24h feels like 12h
                2 => 0.75, // significant
                3 => 1.0,  // neutral
                4 => 1.5,  // minor: 24h feels like 36h
                _ => 2.0,  // negligible: 24h feels like 48h
            };

            if remaining >= 0.0 {
                (remaining * impact_approach_scale, false)
            } else {
                // Overdue: log decay.  ln(1 + hours_overdue) * scale
                // First hour overdue matters a lot; 48h overdue is not
                // much more urgent than 24h.
                let overdue_hours = remaining.abs() / HOUR;
                let decayed = (1.0 + overdue_hours).ln() * HOUR;
                let impact_overdue_scale = match task.impact() {
                    1 => 2.0, // catastrophic overdue = very urgent
                    2 => 1.5,
                    3 => 1.0,
                    4 => 0.5,  // minor overdue = less urgent
                    _ => 0.25, // negligible overdue = barely matters
                };
                (-decayed * impact_overdue_scale, true)
            }
        } else {
            // Unparseable deadline — treat as no deadline
            (HOUR * 24.0 * 365.0, false)
        }
    } else {
        // No deadline — very far future, but not infinite so other
        // factors (impact, joy, size) can still differentiate.
        (HOUR * 24.0 * 365.0, false)
    };

    // --- Overdue attenuation ---
    // Joy and size are activation-energy factors for choosing what to
    // start next.  For overdue tasks, impact should dominate — "do I
    // feel like it?" and "is it small?" matter far less when you've
    // already missed a deadline.  Both are reduced to 10%.
    let motivation_attenuation = if is_overdue { 0.1 } else { 1.0 };

    // --- Feels factors ---
    // Energy modulates joy: low energy → joyful tasks bubble up.
    // Focus modulates size: low focus → small tasks bubble up.
    // 0 = not set → factor 1.0 (no change).
    // Overdue → factor 1.0 (urgency dominates feels).
    let energy_factor = if is_overdue || energy == 0 {
        1.0
    } else {
        (6.0 - energy as f64) / 3.0
    };
    let focus_factor = if is_overdue || focus == 0 {
        1.0
    } else {
        (6.0 - focus as f64) / 3.0
    };

    // --- Joy × impact bonus (in hours) ---
    // Joy alone gives a small nudge; combined with high impact the
    // bonus is substantial.  Range: up to ±30h for extreme values.
    let impact_weight = (6.0 - task.impact() as f64) / 5.0; // 1.0..0.2
    let joy_bonus = (task.joy() as f64 - 5.0)
        * 6.0
        * HOUR
        * impact_weight
        * motivation_attenuation
        * energy_factor;

    // --- Size bonus (ADHD quick-win momentum) ---
    let size_bonus = match task.size() {
        "XS" => -4.0 * HOUR,
        "S" => -2.0 * HOUR,
        "M" => 0.0,
        "L" => 2.0 * HOUR,
        "XL" => 4.0 * HOUR,
        _ => 0.0,
    } * motivation_attenuation
        * focus_factor;

    // --- Work state bonus ---
    // Stopped tasks already have context loaded; lower barrier.
    let work_state_bonus = match task.current_work_state() {
        Some("stopped") => -HOUR,
        _ => 0.0,
    };

    priority_tier + time_component - joy_bonus + size_bonus + work_state_bonus
}

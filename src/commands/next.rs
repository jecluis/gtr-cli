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

//! Next command implementation - suggests tasks to work on based on urgency.

use chrono::Utc;
use colored::Colorize;
use dialoguer::Select;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType, Task, WorkState};
use crate::promotion;
use crate::threshold_cache::{self, CachedThresholds};

/// Suggest next tasks to work on, ordered by urgency.
///
/// Filters out doing/done/deleted tasks, then sorts by urgency heuristic.
/// Always shows picker if tasks available, never auto-selects.
pub async fn run(config: &Config, project: Option<String>, no_sync: bool) -> Result<()> {
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    // Determine which projects to query
    let project_ids = if let Some(proj) = project {
        vec![proj]
    } else {
        client
            .list_projects()
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect()
    };

    // Collect all workable tasks
    let mut tasks = Vec::new();
    for project_id in &project_ids {
        let summaries = ctx.cache.list_tasks(project_id)?;
        for summary in summaries {
            // Filter: exclude done/deleted
            if summary.done.is_some() || summary.deleted.is_some() {
                continue;
            }

            // Load task and check if it's currently being worked on
            if let Ok(task) = ctx.storage.load_task(&summary.project_id, &summary.id) {
                // Exclude tasks in "doing" state
                if task.current_work_state.as_deref() == Some("doing") {
                    continue;
                }
                tasks.push(task);
            }
        }
    }

    if tasks.is_empty() {
        return Err(crate::Error::UserFacing(
            "No tasks available to work on".to_string(),
        ));
    }

    // Fetch thresholds for effective priority computation
    let cached = threshold_cache::fetch_thresholds(config, &client, no_sync).await;

    // Sort by urgency (highest to lowest)
    tasks.sort_by(|a, b| {
        let now = Utc::now();
        let score_a = calculate_urgency_score(a, &now, &cached);
        let score_b = calculate_urgency_score(b, &now, &cached);
        score_a
            .partial_cmp(&score_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Show picker (always, even for 1 task)
    let selected_id = pick_next_task(&tasks, &cached)?;

    // Load the selected task and transition to "doing"
    let mut task = ctx.load_task(&client, &selected_id).await?;

    if task.current_work_state.as_deref() == Some("doing") {
        println!(
            "{} {} is already in progress",
            "ℹ".blue(),
            task.id[..8].cyan()
        );
        return Ok(());
    }

    let now = Utc::now();
    task.current_work_state = Some("doing".to_string());
    task.modified = now.to_rfc3339();
    task.version += 1;

    // Add log entry for work state change
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::WorkStateChanged {
            state: WorkState::Doing,
        },
        source: crate::models::LogSource::User,
    });

    // Auto-set progress to 0% if not set
    if task.progress.is_none() {
        let old_progress = task.progress;
        task.progress = Some(0);
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::ProgressChanged {
                from: old_progress,
                to: Some(0),
            },
            source: crate::models::LogSource::User,
        });
    }

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!("{}", "✓ Task started!".green().bold());
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);
    println!("  Status:   {}", "doing".green());

    if !no_sync {
        if ctx.try_sync().await {
            println!("{}", "  ✓ Synced with server".green());
        } else {
            println!("{}", "  ⊙ Queued for sync".yellow());
        }
    }

    Ok(())
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
/// 3. **Joy × impact bonus** — enjoyable high-impact tasks get a real
///    boost (hours, not seconds). Joy alone does little.
/// 4. **Size bonus** — small tasks get a nudge for ADHD quick-win
///    momentum.
/// 5. **Work state** — stopped tasks (already have context) get a nudge.
fn calculate_urgency_score(
    task: &Task,
    now: &chrono::DateTime<chrono::Utc>,
    thresholds: &CachedThresholds,
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
    let (time_component, is_overdue) = if let Some(ref deadline_str) = task.deadline {
        if let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str) {
            let remaining = (deadline.with_timezone(&chrono::Utc) - *now).num_seconds() as f64;

            let impact_approach_scale = match task.impact {
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
                let impact_overdue_scale = match task.impact {
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

    // --- Joy × impact bonus (in hours) ---
    // Joy alone gives a small nudge; combined with high impact the
    // bonus is substantial.  Range: up to ±30h for extreme values.
    let impact_weight = (6.0 - task.impact as f64) / 5.0; // 1.0..0.2
    let joy_bonus = (task.joy as f64 - 5.0) * 6.0 * HOUR * impact_weight * motivation_attenuation;

    // --- Size bonus (ADHD quick-win momentum) ---
    let size_bonus = match task.size.as_str() {
        "XS" => -4.0 * HOUR,
        "S" => -2.0 * HOUR,
        "M" => 0.0,
        "L" => 2.0 * HOUR,
        "XL" => 4.0 * HOUR,
        _ => 0.0,
    } * motivation_attenuation;

    // --- Work state bonus ---
    // Stopped tasks already have context loaded; lower barrier.
    let work_state_bonus = match task.current_work_state.as_deref() {
        Some("stopped") => -HOUR,
        _ => 0.0,
    };

    priority_tier + time_component - joy_bonus + size_bonus + work_state_bonus
}

/// Interactive task picker showing urgency context.
fn pick_next_task(tasks: &[Task], thresholds: &CachedThresholds) -> Result<String> {
    let items: Vec<String> = tasks
        .iter()
        .map(|t| {
            // Build urgency context (only add items with actual content)
            let mut context_parts: Vec<String> = Vec::new();

            // Priority indicator (uses effective priority)
            if promotion::effective_priority(t, thresholds) == "now" {
                context_parts.push("🔴".to_string());
            }

            // Impact emoji
            match t.impact {
                1 => context_parts.push("🔥".to_string()),
                2 => context_parts.push("⚡".to_string()),
                _ => {}
            }

            // Joy emoji
            let je = t.joy_emoji();
            if !je.is_empty() {
                context_parts.push(je.to_string());
            }

            // Deadline indicator
            if let Some(ref deadline_str) = t.deadline
                && let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str)
            {
                let now = chrono::Utc::now();
                let deadline_utc = deadline.with_timezone(&chrono::Utc);

                if deadline_utc < now {
                    context_parts.push("⚠️  OVERDUE".red().to_string());
                } else {
                    let duration = deadline_utc - now;
                    let hours = duration.num_hours();
                    if hours < 48 {
                        context_parts.push(format!("⚠️  {}h", hours).yellow().to_string());
                    }
                }
            }

            // Work state indicator
            if t.current_work_state.as_deref() == Some("stopped") {
                context_parts.push("⏸️".to_string());
            }

            // Format with context if available
            if context_parts.is_empty() {
                format!("{} {}", t.id[..8].cyan(), t.title)
            } else {
                let context = context_parts.join(" ");
                format!("{} {} {}", t.id[..8].cyan(), t.title, context)
            }
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select next task to work on")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    let Some(idx) = selection else {
        return Err(crate::Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok(tasks[idx].id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils;

    /// Build a task with the given parameters; everything else defaults.
    fn task(priority: &str, size: &str, deadline: Option<String>, impact: u8, joy: u8) -> Task {
        Task {
            id: uuid::Uuid::new_v4().to_string(),
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
            joy,
        }
    }

    fn task_with_work_state(
        priority: &str,
        size: &str,
        deadline: Option<String>,
        impact: u8,
        joy: u8,
        work_state: &str,
    ) -> Task {
        let mut t = task(priority, size, deadline, impact, joy);
        t.current_work_state = Some(work_state.to_string());
        t
    }

    fn thresholds() -> CachedThresholds {
        CachedThresholds {
            deadline: utils::default_thresholds(),
            impact_labels: utils::default_impact_labels(),
            impact_multipliers: utils::default_impact_multipliers(),
        }
    }

    fn deadline_in(hours: i64) -> Option<String> {
        Some((Utc::now() + chrono::Duration::hours(hours)).to_rfc3339())
    }

    fn deadline_ago(hours: i64) -> Option<String> {
        Some((Utc::now() - chrono::Duration::hours(hours)).to_rfc3339())
    }

    fn score(t: &Task) -> f64 {
        calculate_urgency_score(t, &Utc::now(), &thresholds())
    }

    // -----------------------------------------------------------------
    // Priority is a hard tier boundary
    // -----------------------------------------------------------------

    #[test]
    fn now_always_beats_later() {
        // The "later" task has no deadline so it won't get auto-promoted
        // to "now" by the threshold system, ensuring the tier boundary
        // is what we're testing.
        let now_task = task("now", "XL", None, 5, 0);
        let later_task = task("later", "XS", None, 1, 10);
        assert!(score(&now_task) < score(&later_task));
    }

    // -----------------------------------------------------------------
    // The user's real scenario: joyful catastrophic beats stale overdue
    // -----------------------------------------------------------------

    #[test]
    fn joyful_catastrophic_approaching_beats_stale_neutral_overdue() {
        let stale_overdue = task("now", "S", deadline_ago(48), 3, 5);
        let joyful_cata = task("now", "S", deadline_in(24), 1, 8);
        assert!(
            score(&joyful_cata) < score(&stale_overdue),
            "joyful catastrophic approaching should beat stale neutral overdue"
        );
    }

    #[test]
    fn joyful_catastrophic_approaching_beats_significant_15h() {
        let sig_large = task("now", "L", deadline_in(15), 2, 5);
        let joyful_cata = task("now", "S", deadline_in(24), 1, 8);
        assert!(
            score(&joyful_cata) < score(&sig_large),
            "joyful catastrophic S approaching should beat significant L 15h"
        );
    }

    // -----------------------------------------------------------------
    // Overdue logarithmic decay
    // -----------------------------------------------------------------

    #[test]
    fn overdue_48h_barely_more_urgent_than_overdue_24h() {
        let overdue_24 = task("now", "M", deadline_ago(24), 3, 5);
        let overdue_48 = task("now", "M", deadline_ago(48), 3, 5);

        let diff = score(&overdue_24) - score(&overdue_48);
        // With log decay, the gap between 24h and 48h overdue should be
        // much smaller than a linear 24h (86400s) would give.
        assert!(
            diff < 3600.0,
            "48h overdue should barely beat 24h overdue (diff={diff:.0}s)"
        );
        // But 48h should still be slightly more urgent
        assert!(score(&overdue_48) < score(&overdue_24));
    }

    // -----------------------------------------------------------------
    // Impact scaling
    // -----------------------------------------------------------------

    #[test]
    fn catastrophic_approaching_beats_neutral_approaching_same_deadline() {
        let neutral = task("now", "M", deadline_in(24), 3, 5);
        let catastrophic = task("now", "M", deadline_in(24), 1, 5);
        assert!(
            score(&catastrophic) < score(&neutral),
            "catastrophic should be more urgent than neutral at same deadline"
        );
    }

    #[test]
    fn catastrophic_overdue_beats_neutral_overdue() {
        let neutral_od = task("now", "M", deadline_ago(12), 3, 5);
        let cata_od = task("now", "M", deadline_ago(12), 1, 5);
        assert!(
            score(&cata_od) < score(&neutral_od),
            "catastrophic overdue should be more urgent than neutral overdue"
        );
    }

    #[test]
    fn boring_catastrophic_overdue_beats_neutral_overdue() {
        // A boring (joy=2) but catastrophic overdue task must still
        // rank above neutral overdue tasks.  The joy penalty is
        // attenuated for overdue tasks so impact dominates.
        let neutral_od = task("now", "M", deadline_ago(24), 3, 5);
        let boring_cata_od = task("now", "M", deadline_ago(24), 1, 2);
        assert!(
            score(&boring_cata_od) < score(&neutral_od),
            "boring catastrophic overdue should still beat neutral overdue \
             (score: {:.0} vs {:.0})",
            score(&boring_cata_od),
            score(&neutral_od),
        );
    }

    #[test]
    fn boring_catastrophic_m_overdue_beats_neutral_s_overdue() {
        // Real-world scenario: a boring catastrophic M-size task must still
        // rank above a neutral S-size task when both are similarly overdue.
        // Without overdue attenuation on size, the S-size bonus (-2h) would
        // combined with joy penalty to overcome the impact scaling advantage.
        let neutral_s_od = task("now", "S", deadline_ago(39), 3, 5);
        let boring_cata_m_od = task("now", "M", deadline_ago(39), 1, 2);
        assert!(
            score(&boring_cata_m_od) < score(&neutral_s_od),
            "boring catastrophic M overdue should beat neutral S overdue \
             (score: {:.0} vs {:.0})",
            score(&boring_cata_m_od),
            score(&neutral_s_od),
        );
    }

    #[test]
    fn boring_catastrophic_overdue_beats_neutral_overdue_same_staleness() {
        // Same overdue duration but different impact + joy.  The
        // catastrophic task's 2.0x overdue scale should dominate over
        // the attenuated joy penalty.
        let neutral_od = task("now", "M", deadline_ago(48), 3, 5);
        let boring_cata_od = task("now", "M", deadline_ago(48), 1, 2);
        assert!(
            score(&boring_cata_od) < score(&neutral_od),
            "boring catastrophic overdue should beat neutral overdue at same staleness \
             (score: {:.0} vs {:.0})",
            score(&boring_cata_od),
            score(&neutral_od),
        );
    }

    #[test]
    fn negligible_overdue_less_urgent_than_neutral_overdue() {
        let neutral_od = task("now", "M", deadline_ago(24), 3, 5);
        let negligible_od = task("now", "M", deadline_ago(24), 5, 5);
        assert!(
            score(&negligible_od) > score(&neutral_od),
            "negligible overdue should be less urgent than neutral overdue"
        );
    }

    // -----------------------------------------------------------------
    // Joy × impact interaction
    // -----------------------------------------------------------------

    #[test]
    fn joy_with_high_impact_gives_large_bonus() {
        let neutral_joy = task("now", "M", deadline_in(24), 1, 5);
        let high_joy = task("now", "M", deadline_in(24), 1, 9);

        let diff = score(&neutral_joy) - score(&high_joy);
        // Joy 9, impact 1: bonus = (9-5)*6h*1.0 = 24h = 86400s
        assert!(
            diff > 80000.0,
            "joy=9 with catastrophic impact should give ~24h bonus (diff={diff:.0}s)"
        );
    }

    #[test]
    fn joy_with_low_impact_gives_tiny_bonus() {
        let neutral_joy = task("now", "M", deadline_in(24), 5, 5);
        let high_joy = task("now", "M", deadline_in(24), 5, 9);

        let diff = score(&neutral_joy) - score(&high_joy);
        // Joy 9, impact 5: bonus = (9-5)*6h*0.2 = 4.8h = 17280s
        assert!(
            diff < 20000.0,
            "joy=9 with negligible impact should give small bonus (diff={diff:.0}s)"
        );
    }

    #[test]
    fn low_joy_high_impact_penalizes() {
        let neutral_joy = task("now", "M", deadline_in(24), 1, 5);
        let low_joy = task("now", "M", deadline_in(24), 1, 1);
        assert!(
            score(&low_joy) > score(&neutral_joy),
            "low joy with high impact should rank lower (less motivating)"
        );
    }

    // -----------------------------------------------------------------
    // Size bonus (ADHD quick wins)
    // -----------------------------------------------------------------

    #[test]
    fn small_beats_large_all_else_equal() {
        let small = task("now", "S", deadline_in(24), 3, 5);
        let large = task("now", "L", deadline_in(24), 3, 5);
        assert!(
            score(&small) < score(&large),
            "small task should be more urgent than large (quick-win nudge)"
        );
    }

    #[test]
    fn xs_beats_xl_significantly() {
        let xs = task("now", "XS", deadline_in(24), 3, 5);
        let xl = task("now", "XL", deadline_in(24), 3, 5);

        let diff = score(&xl) - score(&xs);
        // XS bonus -4h, XL penalty +4h = 8h difference
        assert!(
            diff > 25000.0,
            "XS vs XL should differ by ~8h (diff={diff:.0}s)"
        );
    }

    // -----------------------------------------------------------------
    // Work state bonus
    // -----------------------------------------------------------------

    #[test]
    fn stopped_beats_pending_all_else_equal() {
        let pending = task("now", "M", deadline_in(24), 3, 5);
        let stopped = task_with_work_state("now", "M", deadline_in(24), 3, 5, "stopped");
        assert!(
            score(&stopped) < score(&pending),
            "stopped task should be more urgent (lower context switch cost)"
        );
    }

    // -----------------------------------------------------------------
    // No deadline — factors still differentiate
    // -----------------------------------------------------------------

    #[test]
    fn no_deadline_high_impact_joyful_beats_no_deadline_neutral() {
        let neutral = task("now", "M", None, 3, 5);
        let impactful_joyful = task("now", "S", None, 1, 8);
        assert!(
            score(&impactful_joyful) < score(&neutral),
            "even without deadlines, impact+joy+size should differentiate"
        );
    }

    // -----------------------------------------------------------------
    // Combined: the full scenario from the user
    // -----------------------------------------------------------------

    #[test]
    fn full_scenario_joyful_catastrophic_ranks_first() {
        let boring_a = task("now", "S", deadline_ago(48), 3, 5);
        let boring_b = task("now", "M", deadline_ago(48), 3, 5);
        let sig_large = task("now", "L", deadline_in(15), 2, 5);
        let sig_medium = task("now", "M", deadline_in(15), 2, 5);
        let fun_fix = task("now", "S", deadline_in(24), 1, 8);

        let scores = vec![
            ("boring_a(S)", score(&boring_a)),
            ("boring_b(M)", score(&boring_b)),
            ("sig_large", score(&sig_large)),
            ("sig_medium", score(&sig_medium)),
            ("fun_fix", score(&fun_fix)),
        ];

        // fun_fix should rank first (lowest score)
        assert!(
            score(&fun_fix) < score(&boring_a),
            "fun_fix should beat boring overdue A: {scores:?}"
        );
        assert!(
            score(&fun_fix) < score(&boring_b),
            "fun_fix should beat boring overdue B: {scores:?}"
        );
        assert!(
            score(&fun_fix) < score(&sig_large),
            "fun_fix should beat significant large: {scores:?}"
        );
        assert!(
            score(&fun_fix) < score(&sig_medium),
            "fun_fix should beat significant medium: {scores:?}"
        );
    }
}

//! Session builder (SPEC §8.1) and shelf introduction (§9.3). Automatic; no setup screen.

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::domain::{Attempt, Card, CardSched, CardState, ReviewKind, StepStatus};
use crate::session;
use crate::settings::{Introduction, KEY_INTRODUCTION};

/// Reviews averaged for a card's expected minutes (D-17, R-08).
pub const EXPECTED_WINDOW: usize = 5;
pub const DEFAULT_MINUTES_PER_STEP: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Review,
    New,
    Drill,
    /// Owner ruling 2026-11-12: practice any cards without touching due dates.
    Cram,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanItem {
    pub kind: PlanKind,
    pub card_id: String,
    /// Set for drills: the single step to practise.
    pub step_id: Option<String>,
    pub title: String,
    pub step_count: u32,
    pub est_minutes: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContinueItem {
    pub attempt_id: String,
    pub card_id: String,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub steps_done: u32,
    pub steps_total: u32,
    /// Older than seven days: offer Resume / Restart / Discard (§7.7).
    pub stale: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanCounts {
    pub quick: u32,
    pub projects: u32,
    pub new: u32,
    pub drills: u32,
    pub minutes: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionPlan {
    pub continue_items: Vec<ContinueItem>,
    pub items: Vec<PlanItem>,
    pub counts: PlanCounts,
    /// Due cards left out by the soft budget ("N more due — keep going?").
    pub more_due: u32,
}

/// Activates up to `new_per_day` shelf cards for `today` (FIFO, user-created first).
/// Returns how many were activated this call.
pub fn introduce(db: &Db, today: NaiveDate, now: DateTime<Utc>, device_id: &str) -> Result<u32> {
    let settings = db.settings()?;
    let mut intro = db
        .setting_get::<Introduction>(KEY_INTRODUCTION)?
        .filter(|i| i.day == today)
        .unwrap_or(Introduction {
            day: today,
            count: 0,
        });
    let room = settings.new_per_day.saturating_sub(intro.count) as usize;
    if room == 0 {
        return Ok(0);
    }
    let mut activated = 0;
    for id in db.shelf_fifo()?.into_iter().take(room) {
        db.set_card_state(&id, CardState::Active, now, device_id)?;
        activated += 1;
    }
    intro.count += activated;
    db.setting_set(KEY_INTRODUCTION, &intro)?;
    Ok(activated)
}

/// Mean of the last `EXPECTED_WINDOW` reviews' total `elapsed_ms`, else the author's
/// estimate, else per-step type defaults.
pub fn expected_minutes(db: &Db, card: &Card) -> Result<f64> {
    let mut reviews: Vec<_> = db
        .reviews_for_card(&card.id)?
        .into_iter()
        .filter(|r| r.kind == ReviewKind::Review)
        .collect();
    if !reviews.is_empty() {
        reviews.sort_by_key(|r| std::cmp::Reverse(r.ts_utc));
        let recent = &reviews[..reviews.len().min(EXPECTED_WINDOW)];
        let total_ms: i64 = recent
            .iter()
            .map(|r| r.steps.iter().map(|s| s.elapsed_ms).sum::<i64>())
            .sum();
        let mean = total_ms as f64 / recent.len() as f64 / 60_000.0;
        if mean > 0.0 {
            return Ok(mean);
        }
    }
    if let Some(m) = card.est_minutes {
        return Ok(f64::from(m));
    }
    Ok(card.steps.iter().map(|_| DEFAULT_MINUTES_PER_STEP).sum())
}

struct Candidate {
    card: Card,
    overdue: i64,
    est: f64,
}

/// Builds today's plan. Call `introduce` first so new cards are visible.
pub fn build(
    db: &Db,
    today: NaiveDate,
    now: DateTime<Utc>,
    include_all: bool,
) -> Result<SessionPlan> {
    let settings = db.settings()?;
    let in_progress = db.attempts_in_progress()?;
    let busy: HashSet<String> = in_progress.iter().map(|a| a.card_id.clone()).collect();

    let mut continue_items = Vec::with_capacity(in_progress.len());
    for a in &in_progress {
        if let Some(card) = db.get_card(&a.card_id)? {
            continue_items.push(continue_item(a, &card, now));
        }
    }

    let mut singles = Vec::new();
    let mut multis = Vec::new();
    let mut fresh = Vec::new();
    for (card, sched) in db.planning_cards()? {
        if busy.contains(&card.id) {
            continue;
        }
        let est = expected_minutes(db, &card)?;
        match sched {
            None => fresh.push(Candidate {
                card,
                overdue: 0,
                est,
            }),
            Some(CardSched { due_day, .. }) if due_day <= today => {
                let c = Candidate {
                    overdue: (today - due_day).num_days(),
                    card,
                    est,
                };
                if c.card.steps.len() == 1 {
                    singles.push(c);
                } else {
                    multis.push(c);
                }
            }
            Some(_) => {}
        }
    }
    singles.sort_by_key(|c| std::cmp::Reverse(c.overdue));
    multis.sort_by_key(|c| std::cmp::Reverse(c.overdue));
    let singles = interleave_categories(singles);

    let mut budget = f64::from(settings.daily_minutes);
    let mut items = Vec::new();
    let mut counts = PlanCounts::default();
    let mut more_due = 0u32;
    let mut queued: HashSet<String> = HashSet::new();

    for c in &singles {
        if include_all || budget > 0.0 {
            budget -= c.est;
            counts.quick += 1;
            queued.insert(c.card.id.clone());
            items.push(item(PlanKind::Review, &c.card, c.est));
        } else {
            more_due += 1;
        }
    }
    for (i, c) in multis.iter().enumerate() {
        // Always include at least one project when any is due.
        if include_all || budget > 0.0 || i == 0 {
            budget -= c.est;
            counts.projects += 1;
            queued.insert(c.card.id.clone());
            items.push(item(PlanKind::Review, &c.card, c.est));
        } else {
            more_due += 1;
        }
    }
    for c in &fresh {
        if include_all || budget > 0.0 {
            budget -= c.est;
            counts.new += 1;
            queued.insert(c.card.id.clone());
            items.push(item(PlanKind::New, &c.card, c.est));
        }
    }
    for (card_id, step_id) in db.flagged_steps()? {
        if queued.contains(&card_id) || busy.contains(&card_id) {
            continue;
        }
        if !(include_all || budget > 0.0) {
            break;
        }
        let Some(card) = db.get_card(&card_id)? else {
            continue;
        };
        if !card.steps.iter().any(|s| s.id == step_id) {
            continue;
        }
        let est = DEFAULT_MINUTES_PER_STEP;
        budget -= est;
        counts.drills += 1;
        items.push(PlanItem {
            kind: PlanKind::Drill,
            card_id,
            step_id: Some(step_id),
            title: card.title.clone(),
            step_count: 1,
            est_minutes: est,
        });
    }
    counts.minutes = items.iter().map(|i| i.est_minutes).sum::<f64>().round() as u32;
    Ok(SessionPlan {
        continue_items,
        items,
        counts,
        more_due,
    })
}

/// A cram session over the given cards, in that order, due or not (deleted ones skipped).
pub fn cram(db: &Db, card_ids: &[String]) -> Result<SessionPlan> {
    let mut plan = SessionPlan::default();
    let mut minutes = 0.0;
    for id in card_ids {
        let Some(card) = db.get_card(id)?.filter(|c| c.deleted_at.is_none()) else {
            continue;
        };
        let est = expected_minutes(db, &card)?;
        minutes += est;
        plan.items.push(item(PlanKind::Cram, &card, est));
    }
    plan.counts.minutes = minutes.round() as u32;
    Ok(plan)
}

fn item(kind: PlanKind, card: &Card, est: f64) -> PlanItem {
    PlanItem {
        kind,
        card_id: card.id.clone(),
        step_id: None,
        title: card.title.clone(),
        step_count: card.steps.len() as u32,
        est_minutes: est,
    }
}

fn continue_item(a: &Attempt, card: &Card, now: DateTime<Utc>) -> ContinueItem {
    ContinueItem {
        attempt_id: a.id.clone(),
        card_id: a.card_id.clone(),
        title: card.title.clone(),
        started_at: a.started_at,
        steps_done: a
            .steps
            .iter()
            .filter(|s| s.status == StepStatus::Graded)
            .count() as u32,
        steps_total: a.steps.len() as u32,
        stale: session::is_stale(a, now),
    }
}

/// Greedy: keep the overdue order but avoid two adjacent cards of the same category when
/// another candidate exists.
fn interleave_categories(mut sorted: Vec<Candidate>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::with_capacity(sorted.len());
    while !sorted.is_empty() {
        let last = out.last().map(|c| c.card.category_id.clone());
        let idx = sorted
            .iter()
            .position(|c| Some(&c.card.category_id) != last.as_ref())
            .unwrap_or(0);
        out.push(sorted.remove(idx));
    }
    out
}

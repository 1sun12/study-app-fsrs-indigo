//! Study orchestration (SPEC §8.2, §7.7): every transition is persisted, so killing the
//! app loses nothing. Pure rules live in `session.rs`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::domain::{
    Attempt, AttemptStatus, Card, CardState, Grade, Review, ReviewKind, ReviewStep, SchedMeta,
    StepStats, StepStatus,
};
use crate::error::invalid;
use crate::sched::fsrs;
use crate::{scoring, session};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AttemptView {
    pub attempt: Attempt,
    pub card: Card,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StepScore {
    pub step_id: String,
    pub score: f64,
}

/// What the SUMMARY screen shows after the last step (§8.2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Summary {
    pub grade: Grade,
    pub score: f64,
    pub step_scores: Vec<StepScore>,
    pub interval_days: u32,
    pub due_day: NaiveDate,
    pub flagged_step_ids: Vec<String>,
    pub reps: u32,
    /// Cram run: the schedule above is unchanged.
    pub cram: bool,
}

fn load(db: &Db, attempt_id: &str) -> Result<(Attempt, Card)> {
    let attempt = db
        .get_attempt(attempt_id)?
        .ok_or_else(|| invalid(format!("no attempt {attempt_id}")))?;
    let card = db
        .get_card(&attempt.card_id)?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| invalid("the card was deleted"))?;
    Ok((attempt, card))
}

fn view(attempt: Attempt, card: Card) -> AttemptView {
    AttemptView { attempt, card }
}

/// Starts a new attempt, or resumes the in-progress one for this card. A shelved card is
/// activated on the spot ("Study now", §9.3).
pub fn start(db: &Db, card_id: &str, now: DateTime<Utc>, device_id: &str) -> Result<AttemptView> {
    let mut card = db
        .get_card(card_id)?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| invalid(format!("no card {card_id}")))?;
    if let Some(existing) = db.attempt_in_progress_for_card(card_id, false)? {
        return Ok(view(existing, card));
    }
    if card.state == CardState::Shelved {
        db.set_card_state(card_id, CardState::Active, now, device_id)?;
        card.state = CardState::Active;
    }
    let attempt = session::start(&card, now);
    db.save_attempt(&attempt)?;
    Ok(view(attempt, card))
}

/// Starts (or resumes) a cram run: any live card, due or not; a shelved card stays shelved.
pub fn start_cram(db: &Db, card_id: &str, now: DateTime<Utc>) -> Result<AttemptView> {
    let card = db
        .get_card(card_id)?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| invalid(format!("no card {card_id}")))?;
    if let Some(existing) = db.attempt_in_progress_for_card(card_id, true)? {
        return Ok(view(existing, card));
    }
    let mut attempt = session::start(&card, now);
    attempt.cram = true;
    db.save_attempt(&attempt)?;
    Ok(view(attempt, card))
}

pub fn get(db: &Db, attempt_id: &str) -> Result<AttemptView> {
    let (a, c) = load(db, attempt_id)?;
    Ok(view(a, c))
}

pub fn step_show(
    db: &Db,
    attempt_id: &str,
    step_id: &str,
    now: DateTime<Utc>,
) -> Result<AttemptView> {
    let (mut a, c) = load(db, attempt_id)?;
    session::show(&mut a, step_id, now)?;
    db.save_attempt(&a)?;
    Ok(view(a, c))
}

pub fn step_reveal(
    db: &Db,
    attempt_id: &str,
    step_id: &str,
    typed_answer: Option<String>,
    now: DateTime<Utc>,
) -> Result<AttemptView> {
    let (mut a, c) = load(db, attempt_id)?;
    session::reveal(&mut a, step_id, now, typed_answer)?;
    db.save_attempt(&a)?;
    Ok(view(a, c))
}

/// The revealed step is done; checklist ticks are kept for Stats only.
pub fn step_complete(
    db: &Db,
    attempt_id: &str,
    step_id: &str,
    checked: Vec<String>,
) -> Result<AttemptView> {
    let (mut a, c) = load(db, attempt_id)?;
    session::complete_step(&mut a, &c, step_id, checked)?;
    db.save_attempt(&a)?;
    Ok(view(a, c))
}

/// `draft` is the typed answer on screen, kept on the unrevealed step so Resume shows it again.
pub fn pause(
    db: &Db,
    attempt_id: &str,
    draft: Option<String>,
    now: DateTime<Utc>,
) -> Result<AttemptView> {
    let (mut a, c) = load(db, attempt_id)?;
    session::pause(&mut a, now);
    if let (Some(draft), Some(step)) = (
        draft,
        a.steps.iter_mut().find(|s| s.status == StepStatus::Pending),
    ) {
        step.typed_answer = Some(draft);
    }
    db.save_attempt(&a)?;
    Ok(view(a, c))
}

/// Discard writes no review (§7.7).
pub fn discard(db: &Db, attempt_id: &str, now: DateTime<Utc>) -> Result<()> {
    let (mut a, _) = load(db, attempt_id)?;
    if a.status == AttemptStatus::InProgress {
        a.status = AttemptStatus::Discarded;
        a.completed_at = Some(now);
        db.save_attempt(&a)?;
    }
    Ok(())
}

/// Discard and begin again from the first step.
pub fn restart(
    db: &Db,
    attempt_id: &str,
    now: DateTime<Utc>,
    device_id: &str,
) -> Result<AttemptView> {
    let (a, _) = load(db, attempt_id)?;
    discard(db, attempt_id, now)?;
    start(db, &a.card_id, now, device_id)
}

/// Last step graded → append the review, replay the card, report (§7.6, §8.2 SUMMARY).
pub fn finish(
    db: &Db,
    attempt_id: &str,
    grade: Grade,
    now: DateTime<Utc>,
    device_id: &str,
    tz_offset_min: i32,
) -> Result<Summary> {
    let (mut a, c) = load(db, attempt_id)?;
    let cfg = db.sched_config()?;
    let mut review = session::finish(&mut a, &c, grade, now, device_id, tz_offset_min, &cfg)?;
    if a.cram {
        review.kind = ReviewKind::Cram;
    }
    db.insert_review(&review)?;
    db.save_attempt(&a)?;
    let step_scores = review
        .steps
        .iter()
        .map(|s| StepScore {
            step_id: s.step_id.clone(),
            score: s.score,
        })
        .collect();
    if a.cram {
        // Nothing to replay: report the schedule as it stands.
        let sched = db.card_sched(&c.id)?;
        return Ok(Summary {
            grade: review.grade,
            score: review.score,
            step_scores,
            interval_days: sched.as_ref().map_or(0, |s| s.interval_days),
            due_day: sched.as_ref().map_or(now.date_naive(), |s| s.due_day),
            flagged_step_ids: db
                .step_stats(&c.id)?
                .into_iter()
                .filter(|s| s.flagged)
                .map(|s| s.step_id)
                .collect(),
            reps: sched.as_ref().map_or(0, |s| s.reps),
            cram: true,
        });
    }
    let state = db.replay_card(&c.id, &cfg)?;
    let sched = state
        .sched
        .ok_or_else(|| invalid("replay produced no schedule"))?;
    Ok(Summary {
        grade: review.grade,
        score: review.score,
        step_scores,
        interval_days: sched.interval_days,
        due_day: sched.due_day,
        flagged_step_ids: state
            .steps
            .iter()
            .filter(|s| s.flagged)
            .map(|s| s.step_id.clone())
            .collect(),
        reps: sched.reps,
        cram: false,
    })
}

/// Days until the card returns for each grade (Forgot, Hard, Good, Easy), computed exactly as
/// `finish` will: the cached schedule plus one `next_sched` with this moment and grade.
pub fn grade_preview(
    db: &Db,
    card_id: &str,
    now: DateTime<Utc>,
    tz_offset_min: i32,
) -> Result<[u32; 4]> {
    let cfg = db.sched_config()?;
    let prev = db.card_sched(card_id)?;
    let mut days = [0; 4];
    for (d, g) in days.iter_mut().zip(Grade::ALL) {
        *d = crate::sched::next_sched(prev.as_ref(), card_id, g, now, tz_offset_min, &cfg)?
            .sched
            .interval_days;
    }
    Ok(days)
}

/// Step drill (§7.5): a practice review that updates step stats and never the schedule.
/// Self-graded with the four buttons.
#[allow(clippy::too_many_arguments)]
pub fn record_drill(
    db: &Db,
    card_id: &str,
    step_id: &str,
    grade: Grade,
    elapsed_ms: i64,
    now: DateTime<Utc>,
    device_id: &str,
    tz_offset_min: i32,
) -> Result<StepStats> {
    let cfg = db.sched_config()?;
    let card = db
        .get_card(card_id)?
        .ok_or_else(|| invalid(format!("no card {card_id}")))?;
    if !card.steps.iter().any(|s| s.id == step_id) {
        return Err(invalid(format!("card has no step {step_id}")));
    }
    let score = scoring::grade4_score(grade, &cfg);
    db.insert_review(&Review {
        id: uuid::Uuid::now_v7().to_string(),
        card_id: card_id.to_string(),
        ts_utc: now,
        tz_offset_min,
        device_id: device_id.to_string(),
        kind: ReviewKind::Practice,
        grade,
        score,
        steps: vec![ReviewStep {
            step_id: step_id.to_string(),
            score,
            elapsed_ms,
        }],
        sched: SchedMeta {
            algo: fsrs::algo_name(),
            lib_version: fsrs::LIB_VERSION.to_string(),
            config_hash: cfg.config_hash(),
        },
    })?;
    let state = db.replay_card(card_id, &cfg)?;
    state
        .steps
        .into_iter()
        .find(|s| s.step_id == step_id)
        .ok_or_else(|| invalid("step stats missing after replay"))
}

//! Attempt lifecycle (SPEC §7.7, §8.2, D-16, D-17) as a pure state machine with injected
//! timestamps. Persistence is the caller's job.

use chrono::{DateTime, TimeDelta, Utc};

use crate::domain::{
    Attempt, AttemptStatus, AttemptStep, Card, Grade, Review, ReviewKind, ReviewStep, SchedMeta,
    StepStatus,
};
use crate::error::invalid;
use crate::sched::{SchedConfig, fsrs};
use crate::{Result, scoring};

/// An attempt older than this is offered Resume / Restart / Discard on open.
pub const STALE_AFTER: TimeDelta = TimeDelta::days(7);

pub fn start(card: &Card, now: DateTime<Utc>) -> Attempt {
    Attempt {
        id: uuid::Uuid::now_v7().to_string(),
        card_id: card.id.clone(),
        status: AttemptStatus::InProgress,
        started_at: now,
        completed_at: None,
        cram: false,
        steps: card
            .steps
            .iter()
            .map(|s| AttemptStep {
                step_id: s.id.clone(),
                status: StepStatus::Pending,
                shown_at: None,
                elapsed_ms: 0,
                typed_answer: None,
                checked: Vec::new(),
                grade4: None,
                score: None,
            })
            .collect(),
    }
}

pub fn is_stale(attempt: &Attempt, now: DateTime<Utc>) -> bool {
    now - attempt.started_at > STALE_AFTER
}

/// The step is on screen: start (or resume) the clock. Idempotent while shown.
pub fn show(attempt: &mut Attempt, step_id: &str, now: DateTime<Utc>) -> Result<()> {
    let step = step_mut(attempt, step_id)?;
    if step.status == StepStatus::Pending && step.shown_at.is_none() {
        step.shown_at = Some(now);
    }
    Ok(())
}

/// Reveal commits the step (D-16): the answer freezes and the on-screen time is banked.
pub fn reveal(
    attempt: &mut Attempt,
    step_id: &str,
    now: DateTime<Utc>,
    typed_answer: Option<String>,
) -> Result<()> {
    let step = step_mut(attempt, step_id)?;
    if step.status != StepStatus::Pending {
        return Err(invalid(format!("step {step_id} already revealed")));
    }
    bank(step, now);
    step.typed_answer = typed_answer;
    step.status = StepStatus::Revealed;
    Ok(())
}

/// Leaving the study screen: bank the clock on whatever is shown.
pub fn pause(attempt: &mut Attempt, now: DateTime<Utc>) {
    for step in &mut attempt.steps {
        bank(step, now);
    }
}

/// After reveal: the step is done. Ticked checklist items are kept as data for Stats; they never
/// decide the grade (owner ruling 2026-11-14: the only grade is the one the user gives the card).
pub fn complete_step(
    attempt: &mut Attempt,
    card: &Card,
    step_id: &str,
    checked: Vec<String>,
) -> Result<()> {
    let criteria = &card_step(card, step_id)?.criteria;
    let score = (!criteria.is_empty()).then(|| scoring::checklist_score(criteria, &checked));
    let step = revealed_step_mut(attempt, step_id)?;
    step.checked = checked;
    step.score = score;
    step.status = StepStatus::Graded;
    Ok(())
}

/// Only after every step is done (§7.7), with the grade the user chose for the card: that grade
/// alone schedules it. Step scores (checklist share, else the grade's score) feed Stats only.
/// Produces the review to append; the caller persists it and replays the card.
pub fn finish(
    attempt: &mut Attempt,
    card: &Card,
    grade: Grade,
    now: DateTime<Utc>,
    device_id: &str,
    tz_offset_min: i32,
    cfg: &SchedConfig,
) -> Result<Review> {
    if attempt.status != AttemptStatus::InProgress {
        return Err(invalid("attempt is not in progress"));
    }
    let score = scoring::grade4_score(grade, cfg);
    let mut steps = Vec::with_capacity(attempt.steps.len());
    for s in &attempt.steps {
        if s.status != StepStatus::Graded {
            return Err(invalid(format!("step {} is not done", s.step_id)));
        }
        card_step(card, &s.step_id)?;
        steps.push(ReviewStep {
            step_id: s.step_id.clone(),
            score: s.score.unwrap_or(score),
            elapsed_ms: s.elapsed_ms,
        });
    }
    attempt.status = AttemptStatus::Completed;
    attempt.completed_at = Some(now);
    Ok(Review {
        id: uuid::Uuid::now_v7().to_string(),
        card_id: card.id.clone(),
        ts_utc: now,
        tz_offset_min,
        device_id: device_id.to_string(),
        kind: ReviewKind::Review,
        grade,
        score,
        steps,
        sched: SchedMeta {
            algo: fsrs::algo_name(),
            lib_version: fsrs::LIB_VERSION.to_string(),
            config_hash: cfg.config_hash(),
        },
    })
}

fn bank(step: &mut AttemptStep, now: DateTime<Utc>) {
    if let Some(shown) = step.shown_at.take() {
        step.elapsed_ms += (now - shown).num_milliseconds().max(0);
    }
}

fn step_mut<'a>(attempt: &'a mut Attempt, step_id: &str) -> Result<&'a mut AttemptStep> {
    attempt
        .steps
        .iter_mut()
        .find(|s| s.step_id == step_id)
        .ok_or_else(|| invalid(format!("no attempt step {step_id}")))
}

fn revealed_step_mut<'a>(attempt: &'a mut Attempt, step_id: &str) -> Result<&'a mut AttemptStep> {
    let step = step_mut(attempt, step_id)?;
    if step.status != StepStatus::Revealed {
        return Err(invalid(format!(
            "step {step_id} must be revealed before it is done"
        )));
    }
    Ok(step)
}

fn card_step<'a>(card: &'a Card, step_id: &str) -> Result<&'a crate::domain::Step> {
    card.steps
        .iter()
        .find(|s| s.id == step_id)
        .ok_or_else(|| invalid(format!("card has no step {step_id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CardState, Criterion, Difficulty, Response, Step};

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn card() -> Card {
        let step = |id: &str, criteria: Vec<Criterion>| Step {
            id: id.into(),
            prompt_md: String::new(),
            response: Response::Recall,
            solution_md: String::new(),
            solution_text: None,
            criteria,
            points: 1.0,
            media: vec![],
        };
        let crit = |id: &str| Criterion {
            id: id.into(),
            text: id.into(),
            weight: 1.0,
        };
        Card {
            id: "c".into(),
            title: "t".into(),
            category_id: "cat".into(),
            difficulty: Difficulty::Beginner,
            tags: vec![],
            est_minutes: None,
            steps: vec![step("s1", vec![crit("k1"), crit("k2")]), step("s2", vec![])],
            source: None,
            pack_id: None,
            pack_card_id: None,
            state: CardState::Active,
            created_at: utc("2026-01-01T00:00:00Z"),
            modified_at: utc("2026-01-01T00:00:00Z"),
            modified_by: "dev".into(),
            deleted_at: None,
        }
    }

    #[test]
    fn full_lifecycle_measures_time_and_takes_the_users_grade() {
        let card = card();
        let cfg = SchedConfig::default();
        let mut a = start(&card, utc("2026-01-01T10:00:00Z"));
        show(&mut a, "s1", utc("2026-01-01T10:00:00Z")).unwrap();
        pause(&mut a, utc("2026-01-01T10:01:00Z")); // 60 s banked, clock stopped
        show(&mut a, "s1", utc("2026-01-01T12:00:00Z")).unwrap(); // resume two hours later
        reveal(&mut a, "s1", utc("2026-01-01T12:00:30Z"), None).unwrap(); // +30 s
        assert_eq!(a.steps[0].elapsed_ms, 90_000);
        complete_step(&mut a, &card, "s1", vec!["k1".into(), "k2".into()]).unwrap();
        assert!(
            finish(
                &mut a,
                &card,
                Grade::Good,
                utc("2026-01-01T12:05:00Z"),
                "dev",
                0,
                &cfg
            )
            .is_err(),
            "s2 not done"
        );

        show(&mut a, "s2", utc("2026-01-01T12:01:00Z")).unwrap();
        assert!(
            complete_step(&mut a, &card, "s2", vec![]).is_err(),
            "not revealed yet"
        );
        reveal(&mut a, "s2", utc("2026-01-01T12:02:00Z"), None).unwrap();
        assert!(reveal(&mut a, "s2", utc("2026-01-01T12:02:01Z"), None).is_err());
        complete_step(&mut a, &card, "s2", vec![]).unwrap();

        // Every checklist item ticked, yet the user says Hard: Hard it is.
        let review = finish(
            &mut a,
            &card,
            Grade::Hard,
            utc("2026-01-01T12:05:00Z"),
            "dev",
            -360,
            &cfg,
        )
        .unwrap();
        assert_eq!(a.status, AttemptStatus::Completed);
        assert_eq!(review.grade, Grade::Hard);
        assert_eq!(review.score, scoring::grade4_score(Grade::Hard, &cfg));
        assert_eq!(review.steps[0].score, 1.0, "checklist share, for Stats");
        assert_eq!(
            review.steps[1].score, review.score,
            "no checklist: the card grade"
        );
        assert_eq!(review.steps[0].elapsed_ms, 90_000);
        assert_eq!(review.steps[1].elapsed_ms, 60_000);
        assert_eq!(review.tz_offset_min, -360);
        assert_eq!(review.sched.lib_version, "7.0.0");
        assert!(
            finish(
                &mut a,
                &card,
                Grade::Good,
                utc("2026-01-01T12:06:00Z"),
                "dev",
                0,
                &cfg
            )
            .is_err()
        );
    }

    #[test]
    fn stale_after_seven_days() {
        let a = start(&card(), utc("2026-01-01T10:00:00Z"));
        assert!(!is_stale(&a, utc("2026-01-08T09:00:00Z")));
        assert!(is_stale(&a, utc("2026-01-08T11:00:00Z")));
    }
}

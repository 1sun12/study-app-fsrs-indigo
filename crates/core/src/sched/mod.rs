//! Scheduling wrapper (SPEC §7): FSRS call, interval guards, local-day arithmetic.

pub mod config;
pub mod day;
pub mod fsrs;
pub mod guards;

use chrono::{DateTime, Days, Utc};

use crate::Result;
use crate::domain::{CardSched, Grade};
pub use config::SchedConfig;

pub struct Outcome {
    pub sched: CardSched,
    /// Library interval in fractional days before rounding, clamping and the streak cap.
    pub raw_interval: f32,
}

/// Schedules one `kind = review` event on top of the previous cache state (`None` for a new
/// card). Guards touch only the due date; the stored memory state is the library's.
pub fn next_sched(
    prev: Option<&CardSched>,
    card_id: &str,
    grade: Grade,
    ts_utc: DateTime<Utc>,
    tz_offset_min: i32,
    cfg: &SchedConfig,
) -> Result<Outcome> {
    let today = day::local_day(ts_utc, tz_offset_min, cfg.rollover_hour);
    let (prev_memory, elapsed) = match prev {
        None => (None, 0),
        Some(p) => (
            Some(fsrs::memory_of(p)),
            day::elapsed_days(day::review_day(p), today),
        ),
    };
    let item = fsrs::next_memory(prev_memory, grade, elapsed, cfg.desired_retention)?;

    let success_streak = match grade {
        Grade::Again => 0,
        _ => prev.map_or(0, |p| p.success_streak) + 1,
    };
    let interval_days = guards::guarded_interval(item.interval, success_streak, cfg);

    let sched = CardSched {
        card_id: card_id.to_string(),
        stability: item.memory.stability,
        difficulty: item.memory.difficulty,
        stability_fast: item.memory.stability_fast,
        last_review_ts: ts_utc,
        due_day: today + Days::new(u64::from(interval_days)),
        interval_days,
        reps: prev.map_or(0, |p| p.reps) + 1,
        lapses: prev.map_or(0, |p| p.lapses) + u32::from(grade == Grade::Again),
        success_streak,
    };
    Ok(Outcome {
        sched,
        raw_interval: item.interval,
    })
}

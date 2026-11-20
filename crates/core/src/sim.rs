//! Ladder simulator shared by the `sim` binary and the §7.8 test suite, so the two cannot
//! drift. Every row goes through `replay::apply`, the production path.

use chrono::{Days, NaiveDate};

use crate::domain::{Grade, Review, ReviewKind, ReviewStep, SchedMeta};
use crate::error::invalid;
use crate::replay::{self, ReplayState};
use crate::sched::{SchedConfig, fsrs};
use crate::{Result, scoring};

#[derive(Clone, Debug)]
pub struct LadderOpts {
    pub grades: Vec<Grade>,
    pub cfg: SchedConfig,
    pub start: NaiveDate,
    /// Multiplier on the scheduled gap: 1.0 = on the due day, 2.0 = twice as late, 0.5 = early.
    pub overdue: f64,
    /// Days between the first and second review, replacing the scheduled gap (0 = scheduled).
    pub idle_days: u32,
}

impl LadderOpts {
    pub fn new(grades: Vec<Grade>) -> Self {
        LadderOpts {
            grades,
            cfg: SchedConfig::default(),
            start: NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date"),
            overdue: 1.0,
            idle_days: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LadderRow {
    pub step: usize,
    pub grade: Grade,
    pub elapsed: u32,
    pub stability: f32,
    pub difficulty: f32,
    pub raw_interval: f32,
    pub interval: u32,
    pub due: NaiveDate,
}

/// `a,h,g,e` with optional separators and whitespace.
pub fn parse_grades(s: &str) -> Result<Vec<Grade>> {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .map(|c| {
            Grade::from_letter(c).ok_or_else(|| invalid(format!("unknown grade letter {c:?}")))
        })
        .collect()
}

pub fn run_ladder(opts: &LadderOpts) -> Result<Vec<LadderRow>> {
    let step_ids = vec!["s1".to_string()];
    let mut state = ReplayState::new(&step_ids);
    let mut rows = Vec::with_capacity(opts.grades.len());
    let mut day = opts.start;
    for (i, &grade) in opts.grades.iter().enumerate() {
        if let Some(prev) = &state.sched {
            let gap = if i == 1 && opts.idle_days > 0 {
                opts.idle_days
            } else {
                ((f64::from(prev.interval_days) * opts.overdue).round() as u32).max(1)
            };
            day = crate::sched::day::review_day(prev) + Days::new(u64::from(gap));
        }
        let ts = day.and_hms_opt(12, 0, 0).expect("noon").and_utc();
        let elapsed = state.sched.as_ref().map_or(0, |p| {
            crate::sched::day::elapsed_days(crate::sched::day::review_day(p), day)
        });
        let score = scoring::grade4_score(grade, &opts.cfg);
        let review = Review {
            id: format!("r{i:04}"),
            card_id: "sim".into(),
            ts_utc: ts,
            tz_offset_min: 0,
            device_id: "sim".into(),
            kind: ReviewKind::Review,
            grade,
            score,
            steps: vec![ReviewStep {
                step_id: "s1".into(),
                score,
                elapsed_ms: 0,
            }],
            sched: SchedMeta {
                algo: fsrs::algo_name(),
                lib_version: fsrs::LIB_VERSION.into(),
                config_hash: opts.cfg.config_hash(),
            },
        };
        let raw = replay::apply(&mut state, &review, &opts.cfg)?.expect("review kind schedules");
        let sched = state.sched.as_ref().expect("scheduled");
        rows.push(LadderRow {
            step: i + 1,
            grade,
            elapsed,
            stability: sched.stability,
            difficulty: sched.difficulty,
            raw_interval: raw,
            interval: sched.interval_days,
            due: sched.due_day,
        });
    }
    Ok(rows)
}

/// Intervals (days) after `PREVIEW_REVIEWS` good reviews at this retention: the Settings preview.
pub const PREVIEW_REVIEWS: usize = 6;
pub fn retention_preview(cfg: &SchedConfig, retention: f32) -> Result<Vec<u32>> {
    let mut opts = LadderOpts::new(vec![Grade::Good; PREVIEW_REVIEWS]);
    opts.cfg = SchedConfig {
        desired_retention: retention,
        ..cfg.clone()
    };
    opts.cfg.validate()?;
    Ok(run_ladder(&opts)?.iter().map(|r| r.interval).collect())
}

pub fn format_table(rows: &[LadderRow]) -> String {
    let mut out = String::from(
        "step | grade | elapsed | stability | difficulty | raw_interval | interval | due\n",
    );
    for r in rows {
        out.push_str(&format!(
            "{:>4} | {:<5} | {:>7} | {:>9.2} | {:>10.2} | {:>12.2} | {:>8} | {}\n",
            r.step,
            r.grade.as_str(),
            r.elapsed,
            r.stability,
            r.difficulty,
            r.raw_interval,
            r.interval,
            r.due
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_retention_spaces_reviews_further() {
        let cfg = SchedConfig::default();
        let low = retention_preview(&cfg, 0.70).unwrap();
        let high = retention_preview(&cfg, 0.95).unwrap();
        assert_eq!(low.len(), PREVIEW_REVIEWS);
        assert!(low.last() >= high.last());
        assert!(retention_preview(&cfg, 0.30).is_err());
    }

    /// Owner check (2026-11-14): at 99% FSRS itself asks for near-daily reviews at first. Our
    /// only changes to its raw interval are rounding and the one-day minimum (the streak caps
    /// sit above these values), memory keeps growing, and the gaps do widen.
    #[test]
    fn ninety_nine_percent_is_fsrs_unaltered() {
        let mut opts = LadderOpts::new(vec![Grade::Good; 15]);
        opts.cfg.desired_retention = 0.99;
        let rows = run_ladder(&opts).unwrap();
        for w in rows.windows(2) {
            assert!(w[1].stability > w[0].stability, "memory grows: {w:?}");
            assert!(w[1].raw_interval > w[0].raw_interval);
        }
        for r in &rows {
            assert_eq!(r.interval, (r.raw_interval.round() as u32).max(1), "{r:?}");
        }
        assert_eq!(rows[0].interval, 1);
        assert!(rows.last().unwrap().interval >= 10, "the daily phase ends");
    }
}

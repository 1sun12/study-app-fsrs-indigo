//! §7.8 numeric bounds. These are the owner's initial targets; if the pinned library's
//! defaults ever violate one, the failing ladder is printed and the owner decides.

use chrono::NaiveDate;
use indigo_core::domain::Grade;
use indigo_core::sched::{SchedConfig, next_sched};
use indigo_core::sim::{LadderOpts, LadderRow, format_table, parse_grades, run_ladder};

fn ladder(grades: &str) -> Vec<LadderRow> {
    run_ladder(&LadderOpts::new(parse_grades(grades).unwrap())).unwrap()
}

fn assert_in_bounds(rows: &[LadderRow], cfg: &SchedConfig) {
    for r in rows {
        assert!(
            (cfg.min_interval_days..=cfg.max_interval_days).contains(&r.interval),
            "interval out of bounds at step {}:\n{}",
            r.step,
            format_table(rows)
        );
        assert!(
            r.raw_interval.is_finite(),
            "non-finite raw interval\n{}",
            format_table(rows)
        );
    }
}

#[test]
fn every_ladder_stays_within_min_max() {
    let cfg = SchedConfig::default();
    for g in [
        "gggggggggggg",
        "eeeeeeeeeeee",
        "hhhhhhhhhhhh",
        "aaaaaaaaaaaa",
        "gagagagagaga",
        "gggaggg",
        "aaaaeeee",
    ] {
        assert_in_bounds(&ladder(g), &cfg);
    }
    let mut late = LadderOpts::new(parse_grades("gggggggg").unwrap());
    late.overdue = 5.0;
    assert_in_bounds(&run_ladder(&late).unwrap(), &cfg);
}

#[test]
fn three_good_lands_between_4_and_60_days() {
    let rows = ladder("ggg");
    let third = rows[2].interval;
    assert!(
        (4..=60).contains(&third),
        "3× good → {third} d\n{}",
        format_table(&rows)
    );
}

#[test]
fn three_easy_is_capped_at_60_days() {
    let rows = ladder("eee");
    assert!(rows[2].interval <= 60, "{}", format_table(&rows));
}

#[test]
fn twelve_good_never_exceeds_max() {
    let rows = ladder("gggggggggggg");
    assert!(
        rows.iter().all(|r| r.interval <= 365),
        "{}",
        format_table(&rows)
    );
    assert_eq!(
        rows.last().unwrap().interval,
        365,
        "{}",
        format_table(&rows)
    );
}

#[test]
fn a_lapse_never_lengthens_the_interval() {
    let rows = ladder("gggaggg");
    assert!(
        rows[3].interval <= rows[2].interval,
        "{}",
        format_table(&rows)
    );
    assert_eq!(
        rows[3].interval,
        1,
        "again is due tomorrow\n{}",
        format_table(&rows)
    );
    let rows = ladder("eeeea");
    assert!(
        rows[4].interval <= rows[3].interval,
        "{}",
        format_table(&rows)
    );
}

/// From every state reached on the all-good ladder, the four grades order their intervals.
#[test]
fn grade_ordering_is_monotone() {
    let cfg = SchedConfig::default();
    let noon = |d: NaiveDate| d.and_hms_opt(12, 0, 0).unwrap().and_utc();
    let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let mut prev = None;
    for step in 0..6 {
        let day = prev
            .as_ref()
            .map_or(start, |p: &indigo_core::domain::CardSched| p.due_day);
        let intervals: Vec<u32> = Grade::ALL
            .iter()
            .map(|&g| {
                next_sched(prev.as_ref(), "m", g, noon(day), 0, &cfg)
                    .unwrap()
                    .sched
                    .interval_days
            })
            .collect();
        assert!(
            intervals.windows(2).all(|w| w[0] <= w[1]),
            "step {step}: again/hard/good/easy = {intervals:?}"
        );
        prev = Some(
            next_sched(prev.as_ref(), "m", Grade::Good, noon(day), 0, &cfg)
                .unwrap()
                .sched,
        );
    }
}

/// The wrapper must not distort the library's interval solve: at the raw interval FSRS
/// returns, its own forgetting curve gives exactly the desired retention, for every grade
/// and retention, along a real history (new card, lapse, recovery, overdue).
#[test]
fn raw_interval_hits_desired_retention() {
    use indigo_core::sched::fsrs::{memory_of, retrievability_at};
    let mut checked = 0;
    for retention in [0.70, 0.90, 0.95, 0.99] {
        let cfg = SchedConfig {
            desired_retention: retention,
            ..SchedConfig::default()
        };
        let noon = |d: NaiveDate| d.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let mut day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let mut prev = None;
        for (step, carry) in [
            Grade::Good,
            Grade::Good,
            Grade::Again,
            Grade::Good,
            Grade::Easy,
        ]
        .into_iter()
        .enumerate()
        {
            for g in Grade::ALL {
                let out = next_sched(prev.as_ref(), "m", g, noon(day), 0, &cfg).unwrap();
                if out.raw_interval < 1.0 {
                    continue; // sub-day: the 1-day floor applies, nothing to compare
                }
                let r = retrievability_at(memory_of(&out.sched), out.raw_interval);
                assert!(
                    (r - retention).abs() < 1e-3,
                    "retention {retention}, step {step}, {g:?}: R({}) = {r}",
                    out.raw_interval
                );
                checked += 1;
            }
            let out = next_sched(prev.as_ref(), "m", carry, noon(day), 0, &cfg).unwrap();
            // Review twice as late as due, to cover elapsed > interval.
            day = day + chrono::Days::new(u64::from(out.sched.interval_days) * 2);
            prev = Some(out.sched);
        }
    }
    assert!(checked >= 30, "only {checked} intervals compared");
}

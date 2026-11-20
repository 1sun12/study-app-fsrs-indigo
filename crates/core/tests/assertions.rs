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

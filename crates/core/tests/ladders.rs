//! §7.8 ladders as insta snapshots: any library, config or guard change is a visible diff.

use indigo_core::sim::{LadderOpts, format_table, parse_grades, run_ladder};

fn snap(name: &str, opts: LadderOpts) {
    let rows = run_ladder(&opts).expect("ladder runs");
    insta::assert_snapshot!(name, format_table(&rows));
}

fn repeat(letter: char, n: usize) -> LadderOpts {
    LadderOpts::new(parse_grades(&letter.to_string().repeat(n)).unwrap())
}

#[test]
fn all_good() {
    snap("all_good", repeat('g', 12));
}

#[test]
fn all_easy() {
    snap("all_easy", repeat('e', 12));
}

#[test]
fn all_hard() {
    snap("all_hard", repeat('h', 12));
}

#[test]
fn all_again() {
    snap("all_again", repeat('a', 12));
}

#[test]
fn alternating_good_again() {
    snap(
        "alternating_good_again",
        LadderOpts::new(parse_grades("g,a,g,a,g,a,g,a,g,a,g,a").unwrap()),
    );
}

#[test]
fn lapse_then_recover() {
    snap(
        "lapse_then_recover",
        LadderOpts::new(parse_grades("g,g,g,a,g,g,g").unwrap()),
    );
}

#[test]
fn timing_variants() {
    let mut overdue_2x = repeat('g', 8);
    overdue_2x.overdue = 2.0;
    snap("overdue_2x", overdue_2x);

    let mut overdue_5x = repeat('g', 8);
    overdue_5x.overdue = 5.0;
    snap("overdue_5x", overdue_5x);

    let mut early = repeat('g', 8);
    early.overdue = 0.5;
    snap("early_half", early);

    let mut idle = repeat('g', 6);
    idle.idle_days = 30;
    snap("idle_30_days", idle);
}

#[test]
fn caps_disabled_shows_raw_growth() {
    let mut raw = repeat('g', 8);
    raw.cfg.streak_caps.clear();
    snap("all_good_no_caps", raw);
}

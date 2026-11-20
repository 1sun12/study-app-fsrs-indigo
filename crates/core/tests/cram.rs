//! Cram mode (owner ruling 2026-11-12): practice any card without touching due dates or
//! weak-step flags; it still counts as activity (heatmap, streak, totals, time).

use chrono::{DateTime, Duration, Utc};
use indigo_core::authoring::{save_card, save_category};
use indigo_core::db::Db;
use indigo_core::domain::{
    Card, CardState, Category, Difficulty, Grade, Response, ReviewKind, Step,
};
use indigo_core::planner::{self, PlanKind};
use indigo_core::sched::SchedConfig;
use indigo_core::stats;
use indigo_core::study;

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

/// A one-step recall card in its own category, saved.
fn saved_card(db: &Db, title: &str, t0: DateTime<Utc>) -> Card {
    let cat = save_category(
        db,
        Category {
            id: String::new(),
            name: title.into(),
            syntax: None,
            color: None,
            position: 0,
            modified_at: t0,
            deleted_at: None,
        },
        t0,
    )
    .unwrap()
    .id;
    let card = Card {
        id: String::new(),
        title: title.into(),
        category_id: cat,
        difficulty: Difficulty::Beginner,
        tags: vec![],
        est_minutes: None,
        steps: vec![Step {
            id: String::new(),
            prompt_md: "p".into(),
            response: Response::Recall,
            solution_md: "a".into(),
            solution_text: None,
            criteria: vec![],
            points: 1.0,
            media: vec![],
        }],
        source: None,
        pack_id: None,
        pack_card_id: None,
        state: CardState::Shelved,
        created_at: t0,
        modified_at: t0,
        modified_by: String::new(),
        deleted_at: None,
    };
    save_card(db, card, t0, "dev", &SchedConfig::default(), false)
        .unwrap()
        .card
}

/// Runs one whole attempt (single recall step) with the given grade.
fn run(db: &Db, card: &Card, cram: bool, grade: Grade, t: DateTime<Utc>) -> study::Summary {
    let v = if cram {
        study::start_cram(db, &card.id, t).unwrap()
    } else {
        study::start(db, &card.id, t, "dev").unwrap()
    };
    assert_eq!(v.attempt.cram, cram);
    let a = v.attempt.id;
    let s = &card.steps[0].id;
    study::step_show(db, &a, s, t).unwrap();
    study::step_reveal(db, &a, s, None, t + Duration::seconds(30)).unwrap();
    study::step_complete(db, &a, s, vec![]).unwrap();
    study::finish(db, &a, grade, t + Duration::seconds(40), "dev", 0).unwrap()
}

#[test]
fn cram_never_moves_the_schedule_or_flags_but_counts_as_activity() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("i.sqlite")).unwrap();
    let cfg = SchedConfig::default();
    let t0 = utc("2026-06-01T10:00:00Z");
    let studied = saved_card(&db, "Sidechain", t0);
    let shelved = saved_card(&db, "Never studied", t0);

    let real = run(&db, &studied, false, Grade::Good, t0);
    assert!(!real.cram);
    let sched_before = db.card_sched(&studied.id).unwrap();
    let flags_before = db.step_stats(&studied.id).unwrap();

    // Cram both, badly, a day later: nothing about either schedule changes.
    let plan = planner::cram(
        &db,
        &[studied.id.clone(), shelved.id.clone(), "gone".into()],
    )
    .unwrap();
    assert_eq!(plan.items.len(), 2, "unknown ids are skipped");
    assert!(plan.items.iter().all(|i| i.kind == PlanKind::Cram));
    let t1 = t0 + Duration::days(1);
    let summary = run(&db, &studied, true, Grade::Again, t1);
    assert!(summary.cram);
    assert_eq!(summary.due_day, sched_before.as_ref().unwrap().due_day);
    run(&db, &shelved, true, Grade::Again, t1);

    assert_eq!(
        db.card_sched(&studied.id).unwrap(),
        sched_before,
        "schedule untouched"
    );
    assert_eq!(
        db.step_stats(&studied.id).unwrap(),
        flags_before,
        "weak-step stats untouched"
    );
    assert_eq!(db.card_sched(&shelved.id).unwrap(), None);
    assert_eq!(
        db.get_card(&shelved.id).unwrap().unwrap().state,
        CardState::Shelved,
        "cramming does not introduce a shelf card"
    );
    // A full replay agrees (cram reviews are skipped, not just ignored once).
    assert_eq!(
        db.replay_card(&studied.id, &cfg).unwrap().sched,
        sched_before
    );

    let reviews = db.reviews_all().unwrap();
    assert_eq!(
        reviews
            .iter()
            .filter(|r| r.kind == ReviewKind::Cram)
            .count(),
        2
    );
    let o = stats::overview(&db, t1.date_naive(), &cfg).unwrap();
    assert_eq!(o.reviews_total, 3, "cram counts as activity");
    assert_eq!(o.streak_days, 2);
}

#[test]
fn grade_preview_matches_what_finish_schedules() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("i.sqlite")).unwrap();
    let t0 = utc("2026-06-01T10:00:00Z");
    let card = saved_card(&db, "Preview", t0);
    for (i, grade) in [Grade::Good, Grade::Hard, Grade::Easy, Grade::Again]
        .into_iter()
        .enumerate()
    {
        let t = t0 + Duration::days(10 * i as i64);
        let preview = study::grade_preview(&db, &card.id, t, 0).unwrap();
        assert!(preview[0] <= preview[1] && preview[1] <= preview[2] && preview[2] <= preview[3]);
        let summary = run(&db, &card, false, grade, t);
        assert_eq!(summary.interval_days, preview[grade as usize], "{grade:?}");
    }
}

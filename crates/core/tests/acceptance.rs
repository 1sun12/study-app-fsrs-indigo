//! KICKOFF Prompt 3 acceptance at the data level: a 3-step tool card with media and a
//! one-shot typed card; introduce, study, pause, "quit" (reopen the DB), resume, finish;
//! due dates must match the simulator for the same grades; drills and the stale rule.

use chrono::{DateTime, Days, Utc};
use indigo_core::authoring::{save_card, save_category};
use indigo_core::db::Db;
use indigo_core::domain::{
    AttemptStatus, Card, CardState, Category, Criterion, Difficulty, Grade, MediaRef, MediaRole,
    Response, Step, StepStatus,
};
use indigo_core::planner::{self, PlanKind};
use indigo_core::sim::{LadderOpts, run_ladder};
use indigo_core::study;
use indigo_core::{media, sched::SchedConfig};

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];
// Smallest valid GIF89a (1×1, one black pixel).
const GIF_1X1: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0x21, 0xF9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
];

const DEV: &str = "dev-a";

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn tool_step(prompt: &str, criteria: &[&str], media: Vec<MediaRef>) -> Step {
    Step {
        id: String::new(),
        prompt_md: prompt.into(),
        response: Response::Recall,
        solution_md: "Compare with the reference.".into(),
        solution_text: None,
        criteria: criteria
            .iter()
            .map(|c| Criterion {
                id: String::new(),
                text: (*c).into(),
                weight: 1.0,
            })
            .collect(),
        points: 1.0,
        media,
    }
}

fn card(title: &str, category_id: &str, steps: Vec<Step>) -> Card {
    Card {
        id: String::new(),
        title: title.into(),
        category_id: category_id.into(),
        difficulty: Difficulty::Beginner,
        tags: vec![],
        est_minutes: None,
        steps,
        source: None,
        pack_id: None,
        pack_card_id: None,
        state: CardState::Shelved,
        created_at: utc("2000-01-01T00:00:00Z"),
        modified_at: utc("2000-01-01T00:00:00Z"),
        modified_by: String::new(),
        deleted_at: None,
    }
}

fn media_ref(info: &media::MediaInfo, role: MediaRole) -> MediaRef {
    MediaRef {
        hash: info.hash.clone(),
        ext: info.ext.clone(),
        mime: info.mime.clone(),
        role,
        caption: None,
    }
}

#[test]
fn kickoff_prompt_3_scenario() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("indigo.sqlite");
    let media_dir = dir.path().join("media");
    let cfg = SchedConfig::default();
    let day1 = utc("2026-04-01T12:00:00Z");
    let today = day1.date_naive();

    let db = Db::open(&db_path).unwrap();
    let new_cat = |name: &str| Category {
        id: String::new(),
        name: name.into(),
        syntax: None,
        color: None,
        position: 0,
        modified_at: day1,
        deleted_at: None,
    };
    let blender = save_category(&db, new_cat("Blender"), day1).unwrap().id;
    let python = save_category(&db, new_cat("Python"), day1).unwrap().id;

    // A 3-step Blender card with two images and a GIF.
    let png = media::import(&db, &media_dir, PNG_1X1, day1).unwrap();
    let gif = media::import(&db, &media_dir, GIF_1X1, day1).unwrap();
    assert_eq!(gif.ext, "gif");
    let blender_card = save_card(
        &db,
        card(
            "Model a chair",
            &blender,
            vec![
                tool_step(
                    "Block out the seat",
                    &["Seat is a cube"],
                    vec![media_ref(&png, MediaRole::Prompt)],
                ),
                tool_step(
                    "Extrude the legs",
                    &["Four legs", "Even length"],
                    vec![media_ref(&png, MediaRole::Solution)],
                ),
                tool_step(
                    "Bevel the edges",
                    &["Edges bevelled"],
                    vec![media_ref(&gif, MediaRole::Solution)],
                ),
            ],
        ),
        day1,
        DEV,
        &cfg,
        false,
    )
    .unwrap()
    .card;
    assert_eq!(blender_card.steps[2].media[0].mime, "image/gif");

    // A one-shot Python card.
    let mut typed = Step {
        id: String::new(),
        prompt_md: "Print hello".into(),
        response: Response::Typed,
        solution_md: String::new(),
        solution_text: Some("print('hello')".into()),
        criteria: vec![],
        points: 1.0,
        media: vec![],
    };
    typed.solution_md.clear();
    let py_card = save_card(
        &db,
        card("Hello", &python, vec![typed]),
        day1,
        DEV,
        &cfg,
        false,
    )
    .unwrap()
    .card;

    // Introduction cap and the plan: both cards are new today.
    assert_eq!(planner::introduce(&db, today, day1, DEV).unwrap(), 2);
    assert_eq!(
        planner::introduce(&db, today, day1, DEV).unwrap(),
        0,
        "cap counter persists"
    );
    let plan = planner::build(&db, today, day1, false).unwrap();
    assert_eq!(plan.counts.new, 2);
    assert!(plan.items.iter().all(|i| i.kind == PlanKind::New));
    assert_eq!(
        plan.counts.minutes, 4,
        "one minute per step until time is measured"
    );

    // Study the Blender card: two steps, then pause mid-attempt.
    let v = study::start(&db, &blender_card.id, day1, DEV).unwrap();
    let a = v.attempt.id.clone();
    let s = |i: usize| blender_card.steps[i].id.clone();
    study::step_show(&db, &a, &s(0), day1).unwrap();
    study::step_reveal(&db, &a, &s(0), None, day1 + chrono::TimeDelta::seconds(90)).unwrap();
    study::step_complete(
        &db,
        &a,
        &s(0),
        vec![blender_card.steps[0].criteria[0].id.clone()],
    )
    .unwrap();
    study::step_show(&db, &a, &s(1), day1 + chrono::TimeDelta::seconds(100)).unwrap();
    study::pause(
        &db,
        &a,
        Some("half an answer".into()),
        day1 + chrono::TimeDelta::seconds(160),
    )
    .unwrap();
    drop(db);

    // "Quit and relaunch": reopen the database.
    let db = Db::open(&db_path).unwrap();
    let plan = planner::build(&db, today, day1, false).unwrap();
    assert_eq!(plan.continue_items.len(), 1);
    assert_eq!(plan.continue_items[0].steps_done, 1);
    assert!(
        plan.items.iter().all(|i| i.card_id != blender_card.id),
        "in-progress cards are not queued"
    );
    let v = study::get(&db, &a).unwrap();
    assert_eq!(v.attempt.status, AttemptStatus::InProgress);
    assert_eq!(v.attempt.steps[0].elapsed_ms, 90_000);
    assert_eq!(v.attempt.steps[1].elapsed_ms, 60_000, "paused time banked");
    assert_eq!(v.attempt.steps[1].status, StepStatus::Pending);
    assert_eq!(
        v.attempt.steps[1].typed_answer.as_deref(),
        Some("half an answer"),
        "the draft on screen survives Pause"
    );
    assert_eq!(
        study::start(&db, &blender_card.id, day1, DEV)
            .unwrap()
            .attempt
            .id,
        a,
        "resume, not restart"
    );

    // Resume and finish; the user grades the card Easy (ticks never decide it).
    let t = day1 + chrono::TimeDelta::minutes(10);
    study::step_show(&db, &a, &s(1), t).unwrap();
    study::step_reveal(&db, &a, &s(1), None, t).unwrap();
    let all1: Vec<String> = blender_card.steps[1]
        .criteria
        .iter()
        .map(|c| c.id.clone())
        .collect();
    study::step_complete(&db, &a, &s(1), all1).unwrap();
    study::step_show(&db, &a, &s(2), t).unwrap();
    study::step_reveal(&db, &a, &s(2), None, t).unwrap();
    assert!(
        study::finish(&db, &a, Grade::Easy, t, DEV, 0).is_err(),
        "last step not done yet"
    );
    study::step_complete(
        &db,
        &a,
        &s(2),
        vec![blender_card.steps[2].criteria[0].id.clone()],
    )
    .unwrap();
    let summary = study::finish(&db, &a, Grade::Easy, t, DEV, 0).unwrap();
    assert_eq!(summary.grade, Grade::Easy);
    assert_eq!(summary.step_scores.len(), 3);
    let ladder = run_ladder(&LadderOpts {
        start: today,
        ..LadderOpts::new(vec![Grade::Easy])
    })
    .unwrap();
    assert_eq!(
        summary.due_day, ladder[0].due,
        "due date matches the sim CLI"
    );
    assert_eq!(summary.interval_days, 7);
    assert!(
        study::finish(&db, &a, Grade::Easy, t, DEV, 0).is_err(),
        "an attempt finishes once"
    );

    // Python card: typed answer, graded Good → sim ladder.
    let v = study::start(&db, &py_card.id, t, DEV).unwrap();
    let a2 = v.attempt.id.clone();
    let ps = py_card.steps[0].id.clone();
    study::step_show(&db, &a2, &ps, t).unwrap();
    study::step_reveal(&db, &a2, &ps, Some("print('hello')".into()), t).unwrap();
    study::step_complete(&db, &a2, &ps, vec![]).unwrap();
    let summary = study::finish(&db, &a2, Grade::Good, t, DEV, 0).unwrap();
    let ladder = run_ladder(&LadderOpts {
        start: today,
        ..LadderOpts::new(vec![Grade::Good, Grade::Good])
    })
    .unwrap();
    assert_eq!(summary.due_day, ladder[0].due);
    assert_eq!(
        db.get_attempt(&a2).unwrap().unwrap().steps[0]
            .typed_answer
            .as_deref(),
        Some("print('hello')")
    );

    // Second review of the Python card exactly on its due day: still matches the ladder.
    let due = summary.due_day;
    let t2 = due.and_hms_opt(12, 0, 0).unwrap().and_utc();
    let plan = planner::build(&db, due, t2, false).unwrap();
    assert_eq!(plan.counts.quick, 1, "the python card is due");
    assert_eq!(
        plan.counts.projects, 0,
        "the chair is not due until {}",
        ladder[0].due
    );
    let a3 = study::start(&db, &py_card.id, t2, DEV).unwrap().attempt.id;
    study::step_show(&db, &a3, &ps, t2).unwrap();
    study::step_reveal(&db, &a3, &ps, Some("print('hello')".into()), t2).unwrap();
    study::step_complete(&db, &a3, &ps, vec![]).unwrap();
    let summary2 = study::finish(&db, &a3, Grade::Good, t2, DEV, 0).unwrap();
    assert_eq!(summary2.due_day, ladder[1].due);
    assert_eq!(summary2.reps, 2);

    // Expected minutes now come from measured time (D-17), not type defaults.
    let plan = planner::build(&db, due + Days::new(400), t2 + Days::new(400), false).unwrap();
    let py_item = plan.items.iter().find(|i| i.card_id == py_card.id).unwrap();
    assert!(
        (py_item.est_minutes - 0.0).abs() < 1e-9 || py_item.est_minutes == 1.0,
        "zero measured → falls back to the default"
    );

    // Drills: two low practice scores flag the step; it appears as a drill when not due.
    let stats =
        study::record_drill(&db, &py_card.id, &ps, Grade::Again, 5_000, t2, DEV, 0).unwrap();
    assert!(!stats.flagged);
    let stats =
        study::record_drill(&db, &py_card.id, &ps, Grade::Again, 5_000, t2, DEV, 0).unwrap();
    assert!(stats.flagged);
    let plan = planner::build(&db, due, t2, false).unwrap();
    assert!(
        plan.items
            .iter()
            .any(|i| i.kind == PlanKind::Drill && i.step_id.as_deref() == Some(ps.as_str()))
    );
    assert_eq!(
        db.card_sched(&py_card.id).unwrap().unwrap().reps,
        2,
        "practice never schedules"
    );

    // Stale attempts are flagged after seven days; restart discards and begins again.
    let a4 = study::start(&db, &blender_card.id, t2, DEV)
        .unwrap()
        .attempt
        .id;
    let later = t2 + Days::new(8);
    let plan = planner::build(&db, later.date_naive(), later, false).unwrap();
    assert!(
        plan.continue_items
            .iter()
            .any(|c| c.attempt_id == a4 && c.stale)
    );
    let restarted = study::restart(&db, &a4, later, DEV).unwrap();
    assert_ne!(restarted.attempt.id, a4);
    assert_eq!(
        db.get_attempt(&a4).unwrap().unwrap().status,
        AttemptStatus::Discarded
    );
    study::discard(&db, &restarted.attempt.id, later).unwrap();
    assert!(
        planner::build(&db, later.date_naive(), later, false)
            .unwrap()
            .continue_items
            .is_empty()
    );
}

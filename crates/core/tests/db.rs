//! Database: migrations, backup-before-migrate with rotation, and replay persistence.

use chrono::{DateTime, Utc};
use indigo_core::db::{BACKUPS_KEEP, Db};
use indigo_core::domain::{
    Card, CardState, Category, Criterion, Difficulty, Grade, Response, Review, ReviewKind,
    ReviewStep, SchedMeta, Step,
};
use indigo_core::replay;
use indigo_core::sched::SchedConfig;

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn fixture_card() -> (Category, Card) {
    let cat = Category {
        id: "cat1".into(),
        name: "Blender".into(),
        syntax: None,
        color: None,
        position: 0,
        modified_at: utc("2026-01-01T00:00:00Z"),
        deleted_at: None,
    };
    let step = |id: &str, criteria: Vec<Criterion>| Step {
        id: id.into(),
        prompt_md: format!("prompt {id}"),
        response: Response::Recall,
        solution_md: "sol".into(),
        solution_text: Some("x = 1".into()),
        criteria,
        points: 2.0,
        media: vec![],
    };
    let card = Card {
        id: "card1".into(),
        title: "Extrude a cube".into(),
        category_id: "cat1".into(),
        difficulty: Difficulty::Beginner,
        tags: vec!["modeling".into()],
        est_minutes: Some(4),
        steps: vec![
            step(
                "s1",
                vec![Criterion {
                    id: "k1".into(),
                    text: "Cube extruded".into(),
                    weight: 1.0,
                }],
            ),
            step("s2", vec![]),
        ],
        source: None,
        pack_id: None,
        pack_card_id: None,
        state: CardState::Active,
        created_at: utc("2026-01-01T00:00:00Z"),
        modified_at: utc("2026-01-02T00:00:00Z"),
        modified_by: "dev-a".into(),
        deleted_at: None,
    };
    (cat, card)
}

fn review(id: &str, ts: &str, grade: Grade, kind: ReviewKind, scores: [f64; 2]) -> Review {
    Review {
        id: id.into(),
        card_id: "card1".into(),
        ts_utc: utc(ts),
        tz_offset_min: -360,
        device_id: "dev-a".into(),
        kind,
        grade,
        score: (scores[0] + scores[1]) / 2.0,
        steps: vec![
            ReviewStep {
                step_id: "s1".into(),
                score: scores[0],
                elapsed_ms: 1000,
            },
            ReviewStep {
                step_id: "s2".into(),
                score: scores[1],
                elapsed_ms: 2000,
            },
        ],
        sched: SchedMeta {
            algo: "fsrs7".into(),
            lib_version: "7.0.0".into(),
            config_hash: SchedConfig::default().config_hash(),
        },
    }
}

#[test]
fn fresh_database_migrates_without_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("indigo.sqlite");
    let db = Db::open(&path).unwrap();
    assert!(path.exists());
    assert!(!dir.path().join("backups").exists());
    assert!(db.get_card("nope").unwrap().is_none());
}

#[test]
fn existing_database_is_backed_up_before_migrating() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("indigo.sqlite");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE junk(x INTEGER); INSERT INTO junk VALUES (42);")
            .unwrap();
    }
    Db::open(&path).unwrap();
    let backups: Vec<_> = std::fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1);
    let copy = rusqlite::Connection::open(&backups[0]).unwrap();
    let x: i64 = copy
        .query_row("SELECT x FROM junk", [], |r| r.get(0))
        .unwrap();
    assert_eq!(x, 42);
    let migrated: i64 = copy
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'reviews'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(migrated, 0, "the backup predates the migration");

    // Reopening an up-to-date database makes no second backup.
    Db::open(&path).unwrap();
    assert_eq!(
        std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn backups_rotate_to_the_newest_ten() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    for i in 0..12 {
        std::fs::write(
            backups.join(format!("indigo-2000010{i:02}T000000Z.sqlite")),
            b"old",
        )
        .unwrap();
    }
    std::fs::write(backups.join("unrelated.txt"), b"keep").unwrap();
    let db = Db::open(&dir.path().join("indigo.sqlite")).unwrap();
    let made = db.backup().unwrap();
    let mut names: Vec<String> = std::fs::read_dir(&backups)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("indigo-"))
        .collect();
    names.sort();
    assert_eq!(names.len(), BACKUPS_KEEP);
    assert!(made.exists());
    assert!(
        !backups.join("indigo-20000100T000000Z.sqlite").exists(),
        "oldest removed"
    );
    assert!(backups.join("unrelated.txt").exists());
}

#[test]
fn card_round_trips_and_replay_card_matches_pure_replay() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("indigo.sqlite")).unwrap();
    let (cat, card) = fixture_card();
    db.save_category(&cat).unwrap();
    db.save_card(&card).unwrap();
    assert_eq!(db.get_card("card1").unwrap().unwrap(), card);

    let reviews = vec![
        review(
            "r1",
            "2026-01-01T15:00:00Z",
            Grade::Good,
            ReviewKind::Review,
            [0.9, 0.5],
        ),
        review(
            "r3",
            "2026-01-20T15:00:00Z",
            Grade::Again,
            ReviewKind::Review,
            [0.2, 0.5],
        ),
        review(
            "r2",
            "2026-01-06T15:00:00Z",
            Grade::Good,
            ReviewKind::Review,
            [1.0, 0.5],
        ),
        review(
            "r4",
            "2026-01-21T15:00:00Z",
            Grade::Good,
            ReviewKind::Practice,
            [1.0, 1.0],
        ),
    ];
    for r in &reviews {
        db.insert_review(r).unwrap();
    }
    let cfg = SchedConfig::default();
    let state = db.replay_card("card1", &cfg).unwrap();
    let ids = vec!["s1".to_string(), "s2".to_string()];
    assert_eq!(state, replay::replay(&ids, &reviews, &cfg).unwrap());

    let sched = db.card_sched("card1").unwrap().unwrap();
    assert_eq!(Some(&sched), state.sched.as_ref());
    assert_eq!(sched.reps, 3, "practice does not schedule");
    assert_eq!(sched.lapses, 1);
    assert_eq!(db.step_stats("card1").unwrap(), state.steps);
    assert!(state.steps[1].flagged, "s2 scored 0.5 three times");
    assert_eq!(state.steps[1].recent_scores, vec![0.5, 0.5, 1.0]);

    let mut back = db.reviews_for_card("card1").unwrap();
    back.sort_by(|a, b| a.id.cmp(&b.id));
    let mut expected = reviews.clone();
    expected.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(back, expected);

    // Replaying again is idempotent.
    assert_eq!(db.replay_card("card1", &cfg).unwrap(), state);
}

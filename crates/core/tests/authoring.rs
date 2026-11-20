//! Phase 2: categories, card round trips with media, queries, guard rails.

use chrono::{DateTime, Utc};
use indigo_core::authoring::{save_card, save_category};
use indigo_core::db::Db;
use indigo_core::domain::{
    Card, CardFilter, CardState, Category, Criterion, Difficulty, MediaRef, MediaRole, Response,
    Step,
};
use indigo_core::media;
use indigo_core::sched::SchedConfig;

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn new_category(name: &str) -> Category {
    Category {
        id: String::new(),
        name: name.into(),
        syntax: None,
        color: None,
        position: 0,
        modified_at: utc("2026-01-01T00:00:00Z"),
        deleted_at: None,
    }
}

fn step(prompt: &str, response: Response) -> Step {
    Step {
        id: String::new(),
        prompt_md: prompt.into(),
        response,
        solution_md: String::new(),
        solution_text: None,
        criteria: vec![],
        points: 1.0,
        media: vec![],
    }
}

fn new_card(title: &str, category_id: &str, steps: Vec<Step>) -> Card {
    Card {
        id: String::new(),
        title: title.into(),
        category_id: category_id.into(),
        difficulty: Difficulty::Intermediate,
        tags: vec!["t".into()],
        est_minutes: None,
        steps,
        source: None,
        pack_id: None,
        pack_card_id: None,
        state: CardState::Active, // ignored: new cards start shelved
        created_at: utc("2000-01-01T00:00:00Z"),
        modified_at: utc("2000-01-01T00:00:00Z"),
        modified_by: String::new(),
        deleted_at: None,
    }
}

#[test]
fn categories_start_empty_autofill_syntax_and_revive_deleted_names() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("i.sqlite")).unwrap();
    let now = utc("2026-01-01T00:00:00Z");
    assert!(db.categories_list().unwrap().is_empty(), "nothing seeded");
    let python = save_category(&db, new_category("Python"), now).unwrap();
    let gd = save_category(&db, new_category("GDScript"), now).unwrap();
    let art = save_category(&db, new_category("Pixel art"), now).unwrap();
    assert_eq!(python.syntax.as_deref(), Some("python"));
    assert_eq!(gd.syntax.as_deref(), Some("gdscript"));
    assert_eq!(art.syntax, None);

    db.delete_category(&art.id, now).unwrap();
    let revived = save_category(&db, new_category("pixel art"), now).unwrap();
    assert_eq!(revived.id, art.id, "same row, undeleted");
    assert!(
        save_category(&db, new_category("Python"), now).is_err(),
        "duplicate name"
    );
    assert!(save_category(&db, new_category("  "), now).is_err());
}

#[test]
fn card_round_trip_with_media_query_and_delete_guard() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("i.sqlite")).unwrap();
    let now = utc("2026-03-01T10:00:00Z");
    let cfg = SchedConfig::default();
    let blender = save_category(&db, new_category("Blender"), now).unwrap();
    let python = save_category(&db, new_category("Python"), now).unwrap();

    let png = media::import(&db, &dir.path().join("media"), PNG_1X1, now).unwrap();
    let mut s1 = step("Extrude the top face", Response::Recall);
    s1.media.push(MediaRef {
        hash: png.hash.clone(),
        ext: png.ext.clone(),
        mime: png.mime.clone(),
        role: MediaRole::Solution,
        caption: Some("reference".into()),
    });
    s1.criteria.push(Criterion {
        id: String::new(),
        text: "Face extruded".into(),
        weight: 2.0,
    });
    let mut s2 = step("Bevel it", Response::Recall);
    s2.criteria.push(Criterion {
        id: String::new(),
        text: String::new(), // blank criteria are dropped
        weight: 1.0,
    });
    let saved = save_card(
        &db,
        new_card("Extrude", &blender.id, vec![s1, s2]),
        now,
        "dev",
        &cfg,
        true,
    )
    .unwrap();
    assert_eq!(saved.card.state, CardState::Shelved);
    assert_eq!(saved.card.created_at, now);
    assert!(!saved.card.id.is_empty() && !saved.card.steps[0].id.is_empty());
    assert!(saved.card.steps[1].criteria.is_empty());
    assert_eq!(
        saved.warnings.len(),
        2,
        "step 2 has no solution and no criteria: {:?}",
        saved.warnings
    );
    assert_eq!(db.get_card(&saved.card.id).unwrap().unwrap(), saved.card);
    let remembered = db.settings().unwrap().card_defaults;
    assert_eq!(remembered.category_id.as_deref(), Some(blender.id.as_str()));
    assert_eq!(remembered.difficulty, Difficulty::Intermediate);

    // Unknown media hash is refused; empty title too.
    let mut bad = new_card("Bad", &blender.id, vec![step("p", Response::Recall)]);
    bad.steps[0].media.push(MediaRef {
        hash: "0".repeat(64),
        ext: "png".into(),
        mime: "image/png".into(),
        role: MediaRole::Prompt,
        caption: None,
    });
    assert!(save_card(&db, bad, now, "dev", &cfg, false).is_err());
    assert!(
        save_card(
            &db,
            new_card(" ", &blender.id, vec![step("p", Response::Recall)]),
            now,
            "dev",
            &cfg,
            false
        )
        .is_err()
    );

    let mut typed = step("print hello", Response::Typed);
    typed.solution_text = Some("print('hello')".into());
    let py = save_card(
        &db,
        new_card("Hello", &python.id, vec![typed]),
        now,
        "dev",
        &cfg,
        false,
    )
    .unwrap();
    assert!(py.warnings.is_empty());

    // Edit: rename, drop a step; replay keeps working with the new step list.
    let mut edited = saved.card.clone();
    edited.title = "Extrude (edited)".into();
    edited.steps.truncate(1);
    let edited = save_card(&db, edited, utc("2026-03-02T10:00:00Z"), "dev", &cfg, false).unwrap();
    assert_eq!(edited.card.id, saved.card.id);
    assert_eq!(edited.card.created_at, now, "created_at survives edits");
    assert_eq!(
        db.settings().unwrap().card_defaults,
        remembered,
        "edits and bulk moves never change the new-card memory"
    );
    assert_eq!(
        db.get_card(&edited.card.id).unwrap().unwrap().steps.len(),
        1
    );

    let all = db.cards_query(&CardFilter::default()).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(
        all[0].title, "Extrude (edited)",
        "most recently modified first"
    );
    assert_eq!(all[0].step_count, 1);
    assert_eq!(all[0].category_name, "Blender");
    let hello = db
        .cards_query(&CardFilter {
            text: Some("hel".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hello.len(), 1);
    let by_cat = db
        .cards_query(&CardFilter {
            category_id: Some(python.id.clone()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_cat.len(), 1);
    let due = db
        .cards_query(&CardFilter {
            due_only: true,
            today: Some(now.date_naive()),
            ..Default::default()
        })
        .unwrap();
    assert!(due.is_empty(), "shelved cards are not due");

    assert!(
        db.delete_category(&python.id, now).is_err(),
        "a card still uses it"
    );
    db.delete_card(&py.card.id, now).unwrap();
    assert_eq!(db.cards_query(&CardFilter::default()).unwrap().len(), 1);
    db.delete_category(&python.id, now).unwrap();
    assert_eq!(db.categories_list().unwrap().len(), 1);
}

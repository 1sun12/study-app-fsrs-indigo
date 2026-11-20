//! Phase 5: full backup round trip, vault merge (LWW, conflicts, tombstones), packs with
//! re-import skip/update, and archives that must be refused.

use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use indigo_core::authoring::{save_card, save_category};
use indigo_core::db::Db;
use indigo_core::domain::{
    Card, CardState, Category, Difficulty, Grade, MediaRef, MediaRole, Response, Review,
    ReviewKind, ReviewStep, SchedMeta, Step,
};
use indigo_core::sched::SchedConfig;
use indigo_core::settings::KEY_SETTINGS;
use indigo_core::{backup, media, pack, vault, zipsafe};
use zip::write::SimpleFileOptions;

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

struct Profile {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    db: Db,
    media: std::path::PathBuf,
}

fn profile() -> Profile {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let db = Db::open(&root.join("indigo.sqlite")).unwrap();
    let media = vault::media_dir(&root).unwrap();
    Profile {
        _dir: dir,
        root,
        db,
        media,
    }
}

fn category(p: &Profile, name: &str, now: DateTime<Utc>) -> String {
    save_category(
        &p.db,
        Category {
            id: String::new(),
            name: name.into(),
            syntax: None,
            color: None,
            position: 0,
            modified_at: now,
            deleted_at: None,
        },
        now,
    )
    .unwrap()
    .id
}

fn step(prompt: &str, media: Vec<MediaRef>) -> Step {
    Step {
        id: String::new(),
        prompt_md: prompt.into(),
        response: Response::Recall,
        solution_md: "answer".into(),
        solution_text: None,
        criteria: vec![],
        points: 1.0,
        media,
    }
}

fn new_card(p: &Profile, title: &str, cat: &str, steps: Vec<Step>, now: DateTime<Utc>) -> Card {
    let card = Card {
        id: String::new(),
        title: title.into(),
        category_id: cat.into(),
        difficulty: Difficulty::Intermediate,
        tags: vec!["t".into()],
        est_minutes: Some(3),
        steps,
        source: None,
        pack_id: None,
        pack_card_id: None,
        state: CardState::Shelved,
        created_at: now,
        modified_at: now,
        modified_by: String::new(),
        deleted_at: None,
    };
    save_card(&p.db, card, now, "dev-a", &SchedConfig::default(), false)
        .unwrap()
        .card
}

fn review(card: &Card, device: &str, ts: DateTime<Utc>, grade: Grade) -> Review {
    Review {
        id: uuid::Uuid::now_v7().to_string(),
        card_id: card.id.clone(),
        ts_utc: ts,
        tz_offset_min: -360,
        device_id: device.into(),
        kind: ReviewKind::Review,
        grade,
        score: 0.9,
        steps: card
            .steps
            .iter()
            .map(|s| ReviewStep {
                step_id: s.id.clone(),
                score: 0.9,
                elapsed_ms: 5_000,
            })
            .collect(),
        sched: SchedMeta {
            algo: "fsrs".into(),
            lib_version: "7.0.0".into(),
            config_hash: "x".into(),
        },
    }
}

fn png_ref(p: &Profile, now: DateTime<Utc>) -> MediaRef {
    let info = media::import(&p.db, &p.media, PNG_1X1, now).unwrap();
    MediaRef {
        hash: info.hash,
        ext: info.ext,
        mime: info.mime,
        role: MediaRole::Prompt,
        caption: Some("pic".into()),
    }
}

fn snapshot(db: &Db) -> (Vec<Category>, Vec<Card>, Vec<Review>, String) {
    let cards = db
        .card_ids_all()
        .unwrap()
        .iter()
        .map(|id| db.get_card(id).unwrap().unwrap())
        .collect();
    let mut reviews = db.reviews_all().unwrap();
    reviews.sort_by(|a, b| a.id.cmp(&b.id));
    let mut scheds = db.card_scheds_all().unwrap();
    scheds.sort_by(|a, b| a.card_id.cmp(&b.card_id));
    (
        db.categories_all().unwrap(),
        cards,
        reviews,
        serde_json::to_string(&scheds).unwrap(),
    )
}

#[test]
fn backup_round_trip_restores_everything() {
    let a = profile();
    let t0 = utc("2026-03-01T10:00:00Z");
    let art = category(&a, "Pixel art", t0);
    let gone = category(&a, "Old", t0);
    a.db.delete_category(&gone, t0).unwrap();
    let pic = png_ref(&a, t0);
    let c1 = new_card(
        &a,
        "Shade a sphere",
        &art,
        vec![step("p1", vec![pic]), step("p2", vec![])],
        t0,
    );
    let c2 = new_card(&a, "Doomed", &art, vec![step("p", vec![])], t0);
    a.db.delete_card(&c2.id, t0).unwrap();
    a.db.set_card_state(&c1.id, CardState::Active, t0, "dev-a")
        .unwrap();
    a.db.insert_review(&review(&c1, "dev-a", t0 + Duration::days(1), Grade::Good))
        .unwrap();
    a.db.insert_foreign_review(&review(&c1, "dev-b", t0 + Duration::days(6), Grade::Hard))
        .unwrap();
    let cfg = SchedConfig {
        desired_retention: 0.85,
        ..SchedConfig::default()
    };
    vault::set_config(&a.db, &cfg, t0, "dev-a").unwrap();
    let mut settings = a.db.settings().unwrap();
    settings.daily_minutes = 45;
    a.db.setting_set(KEY_SETTINGS, &settings).unwrap();

    let zip_path = a.root.join("backup.zip");
    let rep = backup::export(&a.db, &a.media, &zip_path, t0).unwrap();
    assert_eq!(
        (rep.cards, rep.reviews, rep.media, rep.missing_media),
        (2, 2, 1, 0)
    );
    assert!(!a.root.join("backup.zip.part").exists());

    let b = profile();
    let merged = backup::import(&b.db, &b.media, &zip_path, &b.root, t0).unwrap();
    assert_eq!((merged.cards, merged.reviews, merged.media), (2, 2, 1));
    assert!(merged.skipped.is_empty(), "{:?}", merged.skipped);
    assert_eq!(
        snapshot(&b.db),
        snapshot(&a.db),
        "restored profile equals the source"
    );
    assert_eq!(b.db.sched_config().unwrap(), cfg);
    assert_eq!(
        b.db.settings().unwrap().daily_minutes,
        45,
        "empty profile: settings restored"
    );
    assert_eq!(
        vault::meta(&b.db, t0).unwrap(),
        vault::meta(&a.db, t0).unwrap()
    );
    assert!(b.media.join(media::rel_path(&pic_hash(&a), "png")).exists());
    assert!(
        std::fs::read_dir(b.root.join("backups")).unwrap().count() >= 1,
        "the database was backed up before the merge"
    );
    assert!(
        !std::fs::read_dir(&b.root).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("restore-")),
        "extracted copy removed"
    );

    // Importing again changes nothing and records no conflicts.
    let again = backup::import(&b.db, &b.media, &zip_path, &b.root, t0).unwrap();
    assert_eq!((again.cards, again.reviews, again.conflicts), (0, 0, 0));
    assert_eq!(snapshot(&b.db), snapshot(&a.db));
}

fn pic_hash(p: &Profile) -> String {
    p.db.media_all().unwrap()[0].hash.clone()
}

fn write_json(dir: &Path, rel: &str, value: &impl serde::Serialize) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

#[test]
fn merge_is_last_writer_wins_with_conflicts_and_tombstones() {
    let p = profile();
    let t0 = utc("2026-03-01T10:00:00Z");
    let cat = category(&p, "Audio", t0);
    let card = new_card(&p, "Local title", &cat, vec![step("p", vec![])], t0);
    let src = p.root.join("mirror");

    // Remote newer: remote wins.
    let mut remote = card.clone();
    remote.title = "Remote title".into();
    remote.modified_at = t0 + Duration::hours(1);
    remote.modified_by = "dev-b".into();
    write_json(&src, &vault::card_path(&card.id), &remote);
    let r = vault::merge(&p.db, &src, &p.media, t0).unwrap();
    assert_eq!(r.cards, 1);
    assert_eq!(
        r.conflicts, 1,
        "never exported: both sides count as changed"
    );
    assert_eq!(
        p.db.get_card(&card.id).unwrap().unwrap().title,
        "Remote title"
    );

    // Same version again: nothing to do.
    let r = vault::merge(&p.db, &src, &p.media, t0).unwrap();
    assert_eq!((r.cards, r.conflicts), (0, 0));

    // Local edited later than an older remote file: local wins, the remote loses.
    let mut local = p.db.get_card(&card.id).unwrap().unwrap();
    local.title = "Edited here".into();
    local.modified_at = t0 + Duration::hours(3);
    local.modified_by = "dev-a".into();
    p.db.save_card(&local).unwrap();
    let mut stale = remote.clone();
    stale.title = "Stale edit".into();
    stale.modified_at = t0 + Duration::hours(2);
    write_json(&src, &vault::card_path(&card.id), &stale);
    let r = vault::merge(&p.db, &src, &p.media, t0).unwrap();
    assert_eq!((r.cards, r.conflicts), (0, 1));
    assert_eq!(
        p.db.get_card(&card.id).unwrap().unwrap().title,
        "Edited here"
    );

    // Equal timestamps: the greater device id wins.
    let mut tie = local.clone();
    tie.title = "From dev-z".into();
    tie.modified_by = "dev-z".into();
    write_json(&src, &vault::card_path(&card.id), &tie);
    vault::merge(&p.db, &src, &p.media, t0).unwrap();
    assert_eq!(
        p.db.get_card(&card.id).unwrap().unwrap().title,
        "From dev-z"
    );

    // A newer tombstone deletes the card.
    let mut tomb = tie.clone();
    tomb.modified_at = t0 + Duration::hours(5);
    tomb.deleted_at = Some(t0 + Duration::hours(5));
    write_json(&src, &vault::card_path(&card.id), &tomb);
    vault::merge(&p.db, &src, &p.media, t0).unwrap();
    assert!(
        p.db.get_card(&card.id)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_some()
    );

    // Unknown fields, bad ids and missing media are skipped, not fatal.
    let mut evil = card.clone();
    evil.id = "../../escape".into();
    write_json(&src, &vault::card_path("x"), &evil);
    let mut ghost = card.clone();
    ghost.id = uuid::Uuid::now_v7().to_string();
    ghost.steps[0].media.push(MediaRef {
        hash: "ab".repeat(32),
        ext: "png".into(),
        mime: "image/png".into(),
        role: MediaRole::Prompt,
        caption: None,
    });
    write_json(&src, &vault::card_path(&ghost.id), &ghost);
    let r = vault::merge(&p.db, &src, &p.media, t0).unwrap();
    assert_eq!(r.skipped.len(), 2, "{:?}", r.skipped);
    assert!(p.db.get_card(&ghost.id).unwrap().is_none());

    // Reviews from another device are inserted once and replayed.
    let active = new_card(&p, "Reviewed", &cat, vec![step("p", vec![])], t0);
    let rv = review(&active, "dev-b", t0 + Duration::days(1), Grade::Good);
    let line = serde_json::to_string(&rv).unwrap() + "\n";
    let rp = src.join(vault::review_path("dev-b", rv.ts_utc));
    std::fs::create_dir_all(rp.parent().unwrap()).unwrap();
    std::fs::write(&rp, line.repeat(2)).unwrap();
    let r = vault::merge(&p.db, &src, &p.media, t0).unwrap();
    assert_eq!(r.reviews, 1);
    assert!(p.db.card_sched(&active.id).unwrap().is_some(), "replayed");
}

#[test]
fn pack_round_trip_with_skip_and_update() {
    let a = profile();
    let t0 = utc("2026-03-01T10:00:00Z");
    let cat = category(&a, "Shaders", t0);
    let pic = png_ref(&a, t0);
    let c1 = new_card(
        &a,
        "Fresnel",
        &cat,
        vec![step("p1", vec![pic]), step("p2", vec![])],
        t0,
    );
    let c2 = new_card(&a, "Dither", &cat, vec![step("p", vec![])], t0);
    let pack_path = a.root.join("shaders.indigopack");
    let n = pack::export(
        &a.db,
        &a.media,
        &[c1.id.clone(), c2.id.clone()],
        "Shaders",
        &pack_path,
        t0,
    )
    .unwrap();
    assert_eq!(n, 2);

    let b = profile();
    let mut settings = b.db.settings().unwrap();
    settings.card_defaults.difficulty = Difficulty::Master;
    b.db.setting_set(KEY_SETTINGS, &settings).unwrap();
    let preview = pack::inspect(&b.db, &pack_path).unwrap();
    assert_eq!(preview.meta.name, "Shaders");
    assert_eq!(preview.cards.len(), 2);
    assert!(
        preview
            .cards
            .iter()
            .all(|c| !c.exists && c.category == "Shaders")
    );

    let cfg = SchedConfig::default();
    let r = pack::import(&b.db, &b.media, &pack_path, &[], t0, "dev-b", &cfg).unwrap();
    assert_eq!((r.added, r.updated, r.skipped), (2, 0, 0));
    assert_eq!(
        b.db.settings().unwrap().card_defaults.difficulty,
        Difficulty::Master,
        "editor memory kept"
    );
    let imported_id = b.db.card_for_pack_card(&c1.id).unwrap().unwrap();
    let imported = b.db.get_card(&imported_id).unwrap().unwrap();
    assert_eq!(imported.state, CardState::Shelved);
    assert_eq!(imported.pack_card_id.as_deref(), Some(c1.id.as_str()));
    assert_eq!(imported.steps[0].media[0].hash, c1.steps[0].media[0].hash);
    assert!(
        b.media
            .join(media::rel_path(&imported.steps[0].media[0].hash, "png"))
            .exists()
    );
    assert_eq!(b.db.categories_list().unwrap()[0].name, "Shaders");

    // Re-import: everything exists; default is skip, listed cards are updated in place.
    assert!(
        pack::inspect(&b.db, &pack_path)
            .unwrap()
            .cards
            .iter()
            .all(|c| c.exists)
    );
    b.db.set_card_state(&imported_id, CardState::Active, t0, "dev-b")
        .unwrap();
    let r = pack::import(
        &b.db,
        &b.media,
        &pack_path,
        std::slice::from_ref(&c1.id),
        t0,
        "dev-b",
        &cfg,
    )
    .unwrap();
    assert_eq!((r.added, r.updated, r.skipped), (0, 1, 1));
    let updated = b.db.get_card(&imported_id).unwrap().unwrap();
    assert_eq!(
        updated.state,
        CardState::Active,
        "update keeps the schedule state"
    );
    assert_eq!(
        updated.steps[0].id, imported.steps[0].id,
        "step ids kept by position"
    );
    assert_eq!(b.db.card_ids_all().unwrap().len(), 2);
}

fn raw_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in entries {
        z.start_file(*name, SimpleFileOptions::default()).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

#[test]
fn malicious_archives_are_refused() {
    let p = profile();
    let t0 = utc("2026-03-01T10:00:00Z");
    let dir = &p.root;
    let meta = br#"{"format":1,"pack_id":"p1","name":"n","author":"","description":"","created_at":"2026-01-01T00:00:00Z"}"#;

    for (i, name) in [
        "../evil.json",
        "/abs.json",
        "a\\b.json",
        "cards/../../x.json",
    ]
    .iter()
    .enumerate()
    {
        let path = dir.join(format!("bad{i}.zip"));
        raw_zip(&path, &[("pack.json", meta), (name, b"{}")]);
        assert!(zipsafe::SafeZip::open(&path).is_err(), "{name}");
        assert!(backup::import(&p.db, &p.media, &path, dir, t0).is_err());
    }

    let link = dir.join("link.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&link).unwrap());
    z.add_symlink("media/x", "/etc/passwd", SimpleFileOptions::default())
        .unwrap();
    z.finish().unwrap();
    assert!(zipsafe::SafeZip::open(&link).is_err(), "symlink");

    let big = dir.join("big.zip");
    raw_zip(&big, &[("a.json", &[b' '; 100])]);
    let mut sz = zipsafe::SafeZip::open(&big).unwrap();
    assert!(sz.read("a.json", 10).is_err(), "decompressed size cap");

    let not_zip = dir.join("plain.zip");
    std::fs::write(&not_zip, b"hello").unwrap();
    assert!(pack::inspect(&p.db, &not_zip).is_err());

    let extra = dir.join("extra.indigopack");
    let meta_extra = br#"{"format":1,"pack_id":"p1","name":"n","author":"","description":"","created_at":"2026-01-01T00:00:00Z","run":"x"}"#;
    raw_zip(&extra, &[("pack.json", meta_extra)]);
    assert!(
        pack::inspect(&p.db, &extra).is_err(),
        "unknown pack.json field"
    );

    // Media whose content does not match its name is rejected before any card lands.
    let card = format!(
        r#"{{"id":"c1","title":"t","category":"c","difficulty":"beginner","tags":[],"est_minutes":null,"source":null,
        "steps":[{{"id":"s","prompt_md":"p","response":"recall","solution_md":"","solution_text":null,"criteria":[],"points":1,
        "media":[{{"hash":"{h}","ext":"png","mime":"image/png","role":"prompt","caption":null}}]}}]}}"#,
        h = "ab".repeat(32)
    );
    let forged = dir.join("forged.indigopack");
    let media_name = format!("media/{}.png", "ab".repeat(32));
    raw_zip(
        &forged,
        &[
            ("pack.json", meta),
            ("cards/c1.json", card.as_bytes()),
            (&media_name, PNG_1X1),
        ],
    );
    let err = pack::import(
        &p.db,
        &p.media,
        &forged,
        &[],
        t0,
        "d",
        &SchedConfig::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("does not match"), "{err}");
    assert!(p.db.card_ids_all().unwrap().is_empty());

    let not_backup = dir.join("nb.zip");
    raw_zip(&not_backup, &[("pack.json", meta)]);
    assert!(backup::import(&p.db, &p.media, &not_backup, dir, t0).is_err());
}

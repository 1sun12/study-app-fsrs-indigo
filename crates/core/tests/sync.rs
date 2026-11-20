//! Sync over git (bare repositories standing in for GitHub): two devices converge; concurrent
//! edits leave exactly one conflict; foreign review files are never written here; a push that
//! loses a race is retried; a foreign vault is refused.

use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use indigo_core::authoring::{save_card, save_category};
use indigo_core::db::Db;
use indigo_core::domain::{
    Card, CardState, Category, Difficulty, Grade, MediaRef, MediaRole, Response, Review,
    ReviewKind, ReviewStep, SchedMeta, Step,
};
use indigo_core::git::GitTransport;
use indigo_core::sched::SchedConfig;
use indigo_core::sync::{self, Paths, SyncOutcome};
use indigo_core::{Result, media, vault};

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

/// A bare repository standing in for the private GitHub repo.
fn bare(remote: &Path) -> String {
    if !remote.join("HEAD").exists() {
        std::fs::create_dir_all(remote).unwrap();
        let ok = std::process::Command::new("git")
            .args(["init", "-q", "--bare", "-b", "main"])
            .current_dir(remote)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git init --bare");
    }
    remote.display().to_string()
}

/// A file as the remote's main branch holds it.
fn remote_file(remote: &Path, path: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["show", &format!("main:{path}")])
        .current_dir(remote)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

struct Device {
    _dir: tempfile::TempDir,
    db: Db,
    paths: Paths,
    id: String,
}

fn device() -> Device {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("indigo.sqlite")).unwrap();
    vault::media_dir(dir.path()).unwrap();
    let paths = Paths::new(dir.path());
    let id = db.device_id().unwrap();
    Device {
        _dir: dir,
        db,
        paths,
        id,
    }
}

fn sync_with(d: &Device, remote: &Path, now: DateTime<Utc>) -> Result<SyncOutcome> {
    let transport = GitTransport {
        dir: d.paths.vault.clone(),
        url: bare(remote),
        device_name: "test".into(),
    };
    let out = sync::run(&d.db, &d.paths, &transport, &d.id, "test", now);
    sync::record(&d.db, &out, now).unwrap();
    out
}

fn step(prompt: &str, media: Vec<MediaRef>) -> Step {
    Step {
        id: String::new(),
        prompt_md: prompt.into(),
        response: Response::Recall,
        solution_md: "a".into(),
        solution_text: None,
        criteria: vec![],
        points: 1.0,
        media,
    }
}

fn add_card(d: &Device, title: &str, cat: &str, media: Vec<MediaRef>, now: DateTime<Utc>) -> Card {
    let card = Card {
        id: String::new(),
        title: title.into(),
        category_id: cat.into(),
        difficulty: Difficulty::Beginner,
        tags: vec![],
        est_minutes: None,
        steps: vec![step("p", media)],
        source: None,
        pack_id: None,
        pack_card_id: None,
        state: CardState::Shelved,
        created_at: now,
        modified_at: now,
        modified_by: String::new(),
        deleted_at: None,
    };
    save_card(&d.db, card, now, &d.id, &SchedConfig::default(), false)
        .unwrap()
        .card
}

fn retitle(d: &Device, id: &str, title: &str, now: DateTime<Utc>) {
    let mut c = d.db.get_card(id).unwrap().unwrap();
    c.title = title.into();
    save_card(&d.db, c, now, &d.id, &SchedConfig::default(), false).unwrap();
}

fn study(d: &Device, card_id: &str, now: DateTime<Utc>, grade: Grade) {
    let card = d.db.get_card(card_id).unwrap().unwrap();
    d.db.insert_review(&Review {
        id: uuid::Uuid::now_v7().to_string(),
        card_id: card.id.clone(),
        ts_utc: now,
        tz_offset_min: 60,
        device_id: d.id.clone(),
        kind: ReviewKind::Review,
        grade,
        score: 0.9,
        steps: vec![ReviewStep {
            step_id: card.steps[0].id.clone(),
            score: 0.9,
            elapsed_ms: 1000,
        }],
        sched: SchedMeta {
            algo: "fsrs".into(),
            lib_version: "7.0.0".into(),
            config_hash: "x".into(),
        },
    })
    .unwrap();
    d.db.replay_card(card_id, &SchedConfig::default()).unwrap();
}

fn snapshot(db: &Db) -> String {
    let mut cards: Vec<Card> = db
        .card_ids_all()
        .unwrap()
        .iter()
        .map(|id| db.get_card(id).unwrap().unwrap())
        .collect();
    cards.sort_by(|a, b| a.id.cmp(&b.id));
    let mut reviews = db.reviews_all().unwrap();
    reviews.sort_by(|a, b| a.id.cmp(&b.id));
    let mut scheds = db.card_scheds_all().unwrap();
    scheds.sort_by(|a, b| a.card_id.cmp(&b.card_id));
    serde_json::to_string(&(db.categories_all().unwrap(), cards, reviews, scheds)).unwrap()
}

#[test]
fn two_devices_converge_and_concurrent_edits_leave_one_conflict() {
    let remote_dir = tempfile::tempdir().unwrap();
    let remote = remote_dir.path().join("Indigo");
    let a = device();
    let b = device();
    let t = utc("2026-05-01T08:00:00Z");

    let cat = save_category(
        &a.db,
        Category {
            id: String::new(),
            name: "Rigging".into(),
            syntax: None,
            color: None,
            position: 0,
            modified_at: t,
            deleted_at: None,
        },
        t,
    )
    .unwrap()
    .id;
    let info = media::import(&a.db, &a.paths.media, PNG_1X1, t).unwrap();
    let pic = MediaRef {
        hash: info.hash.clone(),
        ext: info.ext,
        mime: info.mime,
        role: MediaRole::Prompt,
        caption: None,
    };
    let c1 = add_card(&a, "IK chain", &cat, vec![pic], t);
    let c2 = add_card(&a, "Weight paint", &cat, vec![], t);
    a.db.set_card_state(&c1.id, CardState::Active, t, &a.id)
        .unwrap();
    study(&a, &c1.id, t, Grade::Good);

    assert!(matches!(
        sync_with(&a, &remote, t),
        Ok(SyncOutcome::Synced { .. })
    ));
    let out = sync_with(&b, &remote, t + Duration::minutes(1)).unwrap();
    let SyncOutcome::Synced { merged, .. } = out else {
        panic!("{out:?}")
    };
    assert_eq!((merged.cards, merged.reviews, merged.media), (2, 1, 1));
    assert_eq!(
        vault::meta(&b.db, t).unwrap().vault_id,
        vault::meta(&a.db, t).unwrap().vault_id,
        "a never-synced device joins the remote vault"
    );
    assert!(
        b.paths
            .media
            .join(media::rel_path(&info.hash, "png"))
            .exists()
    );
    assert_eq!(snapshot(&a.db), snapshot(&b.db));

    // Both devices study and edit different things.
    let t2 = t + Duration::days(2);
    study(&b, &c1.id, t2, Grade::Hard);
    retitle(&b, &c2.id, "Weight paint (B)", t2);
    study(&a, &c1.id, t2 + Duration::hours(1), Grade::Easy);
    for (d, dt) in [(&b, 0), (&a, 1), (&b, 2)] {
        sync_with(d, &remote, t2 + Duration::hours(2 + dt)).unwrap();
    }
    assert_eq!(snapshot(&a.db), snapshot(&b.db), "converged");
    assert!(
        remote_file(&remote, &vault::card_path(&c2.id)).contains("Weight paint (B)"),
        "a device that only received the edit never pushes the old version back"
    );
    assert_eq!(a.db.reviews_all().unwrap().len(), 3);
    assert!(
        !a.paths.vault.join("reviews").join(&b.id).exists(),
        "another device's review file is never written here"
    );

    // Concurrent edits of the same card: newer wins everywhere, the loser is kept once.
    let t3 = t + Duration::days(3);
    retitle(&a, &c2.id, "A version", t3);
    sync_with(&a, &remote, t3 + Duration::minutes(5)).unwrap();
    retitle(&b, &c2.id, "B version", t3 + Duration::minutes(10));
    sync_with(&b, &remote, t3 + Duration::minutes(15)).unwrap();
    sync_with(&a, &remote, t3 + Duration::minutes(20)).unwrap();
    for d in [&a, &b] {
        assert_eq!(d.db.get_card(&c2.id).unwrap().unwrap().title, "B version");
    }
    assert_eq!(sync::status(&a.db).unwrap().conflicts, 0);
    let conflicts = sync::conflicts(&b.db).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].title, "A version");
    assert_eq!(snapshot(&a.db), snapshot(&b.db));

    // Restoring the loser makes it the newest edit, and it spreads.
    sync::restore_conflict(
        &b.db,
        &conflicts[0].id,
        t3 + Duration::hours(1),
        &b.id,
        &SchedConfig::default(),
    )
    .unwrap();
    sync_with(&b, &remote, t3 + Duration::hours(2)).unwrap();
    sync_with(&a, &remote, t3 + Duration::hours(3)).unwrap();
    assert_eq!(a.db.get_card(&c2.id).unwrap().unwrap().title, "A version");
    assert_eq!(sync::status(&b.db).unwrap().conflicts, 0);
    assert_eq!(snapshot(&a.db), snapshot(&b.db));
}

#[test]
fn foreign_vault_aborts() {
    let remote_dir = tempfile::tempdir().unwrap();
    let remote = remote_dir.path().join("Indigo");
    let t = utc("2026-05-01T08:00:00Z");
    let a = device();
    sync_with(&a, &remote, t).unwrap();

    // A device that already synced elsewhere refuses a different library.
    let other_dir = tempfile::tempdir().unwrap();
    let c = device();
    sync_with(&c, &other_dir.path().join("Other"), t).unwrap();
    let err = sync_with(&c, &remote, t + Duration::hours(1)).unwrap_err();
    assert!(
        err.to_string().contains("different Indigo library"),
        "{err}"
    );
    let s = sync::status(&c.db).unwrap();
    assert!(s.failed && s.last_success_at.is_some());
}

/// Runs `between` once, after the first fetch and before the first push.
struct Racing<'a> {
    inner: GitTransport,
    between: std::cell::RefCell<Option<Box<dyn FnOnce() + 'a>>>,
}

impl indigo_core::sync::Transport for Racing<'_> {
    fn fetch(&self, mirror: &Path, media_dir: &Path) -> indigo_core::Result<()> {
        self.inner.fetch(mirror, media_dir)
    }
    fn push(&self, vault_dir: &Path) -> indigo_core::Result<bool> {
        if let Some(f) = self.between.borrow_mut().take() {
            f();
        }
        self.inner.push(vault_dir)
    }
}

#[test]
fn a_push_that_loses_the_race_is_merged_and_retried() {
    let remote_dir = tempfile::tempdir().unwrap();
    let remote = remote_dir.path().join("Indigo");
    let t = utc("2026-05-01T08:00:00Z");
    let (a, b) = (device(), device());
    let cat = save_category(
        &a.db,
        Category {
            id: String::new(),
            name: "Audio".into(),
            syntax: None,
            color: None,
            position: 0,
            modified_at: t,
            deleted_at: None,
        },
        t,
    )
    .unwrap()
    .id;
    let c = add_card(&a, "Compressor", &cat, vec![], t);
    sync_with(&a, &remote, t).unwrap();
    sync_with(&b, &remote, t + Duration::minutes(1)).unwrap();

    retitle(&a, &c.id, "Compressor (A)", t + Duration::minutes(2));
    let d = add_card(&b, "Limiter", &cat, vec![], t + Duration::minutes(3));
    let racing = Racing {
        inner: GitTransport {
            dir: a.paths.vault.clone(),
            url: bare(&remote),
            device_name: "a".into(),
        },
        between: std::cell::RefCell::new(Some(Box::new(|| {
            sync_with(&b, &remote, t + Duration::minutes(4)).unwrap();
        }))),
    };
    let out = sync::run(
        &a.db,
        &a.paths,
        &racing,
        &a.id,
        "test",
        t + Duration::minutes(5),
    )
    .unwrap();
    assert!(matches!(out, SyncOutcome::Synced { .. }), "{out:?}");
    assert!(
        a.db.get_card(&d.id).unwrap().is_some(),
        "B's card merged on the retry"
    );
    sync_with(&b, &remote, t + Duration::minutes(6)).unwrap();
    assert_eq!(snapshot(&a.db), snapshot(&b.db));
    assert!(remote_file(&remote, &vault::card_path(&c.id)).contains("Compressor (A)"));

    // A sync with nothing new adds no commit.
    let commits = |r: &Path| {
        let out = std::process::Command::new("git")
            .args(["rev-list", "--count", "main"])
            .current_dir(r)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    let before = commits(&remote);
    sync_with(&a, &remote, t + Duration::minutes(7)).unwrap();
    sync_with(&b, &remote, t + Duration::minutes(8)).unwrap();
    assert_eq!(commits(&remote), before);
}

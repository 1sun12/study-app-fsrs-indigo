//! Media compression for the sync repository (owner ruling 2026-11-12): video becomes VP9 +
//! Opus WebM with the long side ≤ 1280 px, audio becomes Opus; cards are repointed once.
//! Skipped where ffmpeg is not installed.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Utc};
use indigo_core::authoring::{save_card, save_category};
use indigo_core::db::Db;
use indigo_core::domain::{
    Card, CardState, Category, Difficulty, MediaRef, MediaRole, Response, Step,
};
use indigo_core::media;
use indigo_core::sched::SchedConfig;

fn ffmpeg(args: &[&str]) {
    let ok = Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "ffmpeg {args:?}");
}

/// `codec,width,height` of the first video stream, or the audio codec.
fn probe(path: &Path, stream: &str) -> String {
    let entries = if stream == "v:0" {
        "stream=codec_name,width,height"
    } else {
        "stream=codec_name"
    };
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            stream,
            "-show_entries",
            entries,
        ])
        .args(["-of", "csv=p=0"])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn skip() -> bool {
    if media::ffmpeg_available() {
        return false;
    }
    eprintln!("ffmpeg not installed; skipped");
    true
}

#[test]
fn video_shrinks_to_720p_vp9_and_audio_to_opus() {
    if skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("wide.mp4");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1920x1080:rate=30",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440",
        "-t",
        "2",
        "-c:v",
        "mpeg4",
        "-q:v",
        "1",
        "-c:a",
        "aac",
        src.to_str().unwrap(),
    ]);
    let out = media::compress(&src, dir.path()).unwrap().expect("smaller");
    assert_eq!(probe(&out, "v:0"), "vp9,1280,720");
    assert_eq!(probe(&out, "a:0"), "opus");
    assert!(std::fs::metadata(&out).unwrap().len() < std::fs::metadata(&src).unwrap().len());

    let tall = dir.path().join("tall.mp4");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1080x1920:rate=30",
        "-t",
        "1",
        "-c:v",
        "mpeg4",
        "-q:v",
        "1",
        tall.to_str().unwrap(),
    ]);
    let out = media::compress(&tall, dir.path())
        .unwrap()
        .expect("smaller");
    assert_eq!(
        probe(&out, "v:0"),
        "vp9,720,1280",
        "portrait keeps its long side ≤ 1280"
    );

    let wav = dir.path().join("tone.wav");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440",
        "-t",
        "5",
        wav.to_str().unwrap(),
    ]);
    let out = media::compress(&wav, dir.path()).unwrap().expect("smaller");
    assert_eq!(probe(&out, "a:0"), "opus");

    let png = dir.path().join("x.png");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=64x64",
        "-frames:v",
        "1",
        png.to_str().unwrap(),
    ]);
    assert!(
        media::compress(&png, dir.path()).unwrap().is_none(),
        "images are left alone"
    );
}

fn card_with(db: &Db, m: MediaRef, now: DateTime<Utc>) -> Card {
    let cat = save_category(
        db,
        Category {
            id: String::new(),
            name: format!("Video {}", &m.hash[..8]),
            syntax: None,
            color: None,
            position: 0,
            modified_at: now,
            deleted_at: None,
        },
        now,
    )
    .unwrap()
    .id;
    let card = Card {
        id: String::new(),
        title: "Clip".into(),
        category_id: cat,
        difficulty: Difficulty::Beginner,
        tags: vec![],
        est_minutes: None,
        steps: vec![Step {
            id: String::new(),
            prompt_md: "watch".into(),
            response: Response::Recall,
            solution_md: String::new(),
            solution_text: None,
            criteria: vec![],
            points: 1.0,
            media: vec![m],
        }],
        source: None,
        pack_id: None,
        pack_card_id: None,
        state: CardState::Shelved,
        created_at: now,
        modified_at: now,
        modified_by: String::new(),
        deleted_at: None,
    };
    save_card(db, card, now, "dev", &SchedConfig::default(), false)
        .unwrap()
        .card
}

#[test]
fn existing_videos_are_compressed_once_and_cards_repointed() {
    if skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("i.sqlite")).unwrap();
    let media_dir = dir.path().join("media");
    let src = dir.path().join("big.mp4");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1920x1080:rate=30",
        "-t",
        "2",
        "-c:v",
        "mpeg4",
        "-q:v",
        "1",
        src.to_str().unwrap(),
    ]);
    let t0: DateTime<Utc> = "2026-05-01T08:00:00Z".parse().unwrap();
    // Imported the old way (no compression), as a library from before this change would be.
    let old = media::import(&db, &media_dir, &std::fs::read(&src).unwrap(), t0).unwrap();
    let card = card_with(
        &db,
        MediaRef {
            hash: old.hash.clone(),
            ext: old.ext.clone(),
            mime: old.mime.clone(),
            role: MediaRole::Prompt,
            caption: None,
        },
        t0,
    );

    let t1 = t0 + chrono::Duration::hours(1);
    let work = dir.path().join("tmp");
    assert_eq!(
        media::compress_library(&db, &media_dir, &work, t1, "dev").unwrap(),
        1
    );
    let after = db.get_card(&card.id).unwrap().unwrap();
    let m = &after.steps[0].media[0];
    assert_ne!(m.hash, old.hash);
    assert_eq!((m.ext.as_str(), m.mime.as_str()), ("webm", "video/webm"));
    assert_eq!(after.modified_at, t1, "the edit syncs to other devices");
    assert!(media_dir.join(media::rel_path(&m.hash, &m.ext)).is_file());
    assert_eq!(
        media::compress_library(&db, &media_dir, &work, t1, "dev").unwrap(),
        0,
        "never twice"
    );

    // Media that arrives later (sync, pack, backup) is never re-encoded by the pass.
    let other = dir.path().join("other.mp4");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=1920x1080:rate=30",
        "-t",
        "1",
        "-c:v",
        "mpeg4",
        "-q:v",
        "1",
        other.to_str().unwrap(),
    ]);
    let arrived = media::import(&db, &media_dir, &std::fs::read(&other).unwrap(), t1).unwrap();
    let later = card_with(
        &db,
        MediaRef {
            hash: arrived.hash.clone(),
            ext: arrived.ext.clone(),
            mime: arrived.mime.clone(),
            role: MediaRole::Prompt,
            caption: None,
        },
        t1,
    );
    assert_eq!(
        media::compress_library(&db, &media_dir, &work, t1, "dev").unwrap(),
        0
    );
    let kept = db.get_card(&later.id).unwrap().unwrap();
    assert_eq!(kept.steps[0].media[0].hash, arrived.hash);

    // A newly attached file is compressed on the way in and never revisited.
    let info = media::import_file(&db, &media_dir, &src, &work, t1).unwrap();
    assert_eq!(info.ext, "webm");
}

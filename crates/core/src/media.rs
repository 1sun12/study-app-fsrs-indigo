//! Media import (SPEC §10): type by magic bytes, size limits, content-addressed storage.
//! Video and audio attached by the user are compressed first (owner ruling 2026-11-12) so the
//! sync repository stays small: see `compress`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::error::invalid;
use crate::settings::MediaLimits;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MediaInfo {
    pub hash: String,
    pub mime: String,
    pub ext: String,
    #[ts(type = "number")]
    pub bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    pub created_at: DateTime<Utc>,
}

/// Accepted types: sniffed MIME → stored extension.
const ALLOWED: &[(&str, &str)] = &[
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
    ("audio/mpeg", "mp3"),
    ("audio/ogg", "ogg"),
    ("audio/x-wav", "wav"),
    ("audio/wav", "wav"),
    ("audio/x-flac", "flac"),
    ("audio/flac", "flac"),
    ("video/mp4", "mp4"),
    ("video/webm", "webm"),
];

pub fn is_allowed_ext(ext: &str) -> bool {
    ALLOWED.iter().any(|(_, e)| *e == ext)
}

/// `<hh>/<sha256>.<ext>` relative to the media dir; the frontend appends it to the base URL.
pub fn rel_path(hash: &str, ext: &str) -> String {
    format!("{}/{hash}.{ext}", &hash[..2])
}

/// Sniffs, checks limits, hashes and stores the bytes. Returns the row to insert.
pub fn import_bytes(
    media_dir: &Path,
    bytes: &[u8],
    limits: &MediaLimits,
    now: DateTime<Utc>,
) -> Result<MediaInfo> {
    let kind = infer::get(bytes).ok_or_else(|| invalid("Unrecognised file type"))?;
    let mime = kind.mime_type();
    let ext = ALLOWED
        .iter()
        .find(|(m, _)| *m == mime)
        .map(|(_, e)| *e)
        .ok_or_else(|| invalid(format!("{mime} is not a supported media type")))?;
    let (label, limit_mb) = if mime.starts_with("image/") {
        ("Image", limits.image_mb)
    } else if mime.starts_with("audio/") {
        ("Audio", limits.audio_mb)
    } else {
        ("Video", limits.video_mb)
    };
    // GitHub refuses files of 100 MB or more; nothing larger may enter the synced vault.
    let limit = (u64::from(limit_mb) * 1024 * 1024).min(crate::git::MAX_FILE_BYTES - 1);
    if bytes.len() as u64 > limit {
        return Err(invalid(format!(
            "{label} is {:.1} MB; the limit is {:.0} MB (Settings → Cards)",
            bytes.len() as f64 / 1_048_576.0,
            limit as f64 / 1_048_576.0
        )));
    }
    let hash: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let (width, height) = if mime.starts_with("image/") {
        imagesize::blob_size(bytes)
            .map(|s| (Some(s.width as u32), Some(s.height as u32)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };
    let target: PathBuf = media_dir.join(rel_path(&hash, ext));
    if !target.exists() {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write-then-rename so a crash never leaves a partial file under the final name.
        let tmp = target.with_extension(format!("{ext}.part"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &target)?;
    }
    Ok(MediaInfo {
        hash,
        mime: mime.to_string(),
        ext: ext.to_string(),
        bytes: bytes.len() as u64,
        width,
        height,
        duration_ms: None,
        created_at: now,
    })
}

/// RGBA pixels (as a clipboard hands them over) to PNG bytes, ready for `import`.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return Err(invalid("The clipboard image is empty or malformed"));
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc
        .write_header()
        .map_err(|e| invalid(format!("png: {e}")))?;
    w.write_image_data(rgba)
        .map_err(|e| invalid(format!("png: {e}")))?;
    w.finish().map_err(|e| invalid(format!("png: {e}")))?;
    Ok(out)
}

/// Import and record in one call; the normal entry point for commands.
pub fn import(db: &Db, media_dir: &Path, bytes: &[u8], now: DateTime<Utc>) -> Result<MediaInfo> {
    let limits = db.settings()?.media_limits;
    let info = import_bytes(media_dir, bytes, &limits, now)?;
    db.media_insert(&info)?;
    Ok(info)
}

/// Compressed video is no wider (or, portrait, no taller) than this: 720p for 16:9.
pub const VIDEO_MAX_SIDE: u32 = 1280;
/// VP9 constant quality (0–63, higher = smaller); 36 keeps screen text readable at 720p.
pub const VIDEO_CRF: u32 = 36;
pub const AUDIO_KBPS: u32 = 64;
/// Longest one compression may run before it is stopped.
const FFMPEG_WALL_CLOCK: Duration = Duration::from_secs(30 * 60);
/// Device-local flag: the one-time pass over the existing library has run.
const KEY_LIBRARY_COMPRESSED: &str = "media_library_compressed";

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn sniff(path: &Path) -> Result<Option<&'static str>> {
    let mut head = vec![0u8; 8192];
    let n = std::fs::File::open(path)?.read(&mut head)?;
    Ok(infer::get(&head[..n]).map(|k| k.mime_type()))
}

/// Re-encodes video to VP9 + Opus WebM (long side ≤ `VIDEO_MAX_SIDE`) and audio to Opus in
/// Ogg: royalty-free formats that WebKitGTK plays without extra codecs. Returns the new file
/// in `work`, or None for images, when ffmpeg is missing, or when the result is not smaller.
pub fn compress(src: &Path, work: &Path) -> Result<Option<PathBuf>> {
    let Some(mime) = sniff(src)? else {
        return Ok(None);
    };
    let video = mime.starts_with("video/");
    if !video && !mime.starts_with("audio/") {
        return Ok(None);
    }
    if !ffmpeg_available() {
        log::warn!("ffmpeg not found; {} is stored uncompressed", src.display());
        return Ok(None);
    }
    std::fs::create_dir_all(work)?;
    let out = work.join(format!(
        "compress-{}.{}",
        uuid::Uuid::now_v7(),
        if video { "webm" } else { "ogg" }
    ));
    let side = VIDEO_MAX_SIDE;
    let scale = format!(
        "scale='if(gte(iw,ih),trunc(min({side},iw)/2)*2,-2)':'if(gte(iw,ih),-2,trunc(min({side},ih)/2)*2)'"
    );
    let crf = VIDEO_CRF.to_string();
    let abr = format!("{AUDIO_KBPS}k");
    let mut args: Vec<&std::ffi::OsStr> = vec![
        "-nostdin".as_ref(),
        "-hide_banner".as_ref(),
        "-loglevel".as_ref(),
        "error".as_ref(),
        "-y".as_ref(),
        "-i".as_ref(),
        src.as_os_str(),
    ];
    let tail: Vec<&str> = if video {
        vec![
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-vf",
            &scale,
            "-pix_fmt",
            "yuv420p",
            "-c:v",
            "libvpx-vp9",
            "-crf",
            &crf,
            "-b:v",
            "0",
            "-deadline",
            "good",
            "-cpu-used",
            "4",
            "-row-mt",
            "1",
            "-c:a",
            "libopus",
            "-b:a",
            &abr,
            "-f",
            "webm",
        ]
    } else {
        vec![
            "-map", "0:a:0", "-vn", "-c:a", "libopus", "-b:a", &abr, "-f", "ogg",
        ]
    };
    args.extend(tail.iter().map(|a| std::ffi::OsStr::new(*a)));
    args.push(out.as_os_str());

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child.stderr.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > FFMPEG_WALL_CLOCK {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&out);
            return Err(invalid("Compressing took too long and was stopped"));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let err = reader.join().unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(&out);
        let last = err
            .lines()
            .last()
            .unwrap_or("ffmpeg failed")
            .trim()
            .to_string();
        return Err(invalid(format!("Could not compress this file: {last}")));
    }
    if std::fs::metadata(&out)?.len() >= std::fs::metadata(src)?.len() {
        let _ = std::fs::remove_file(&out);
        return Ok(None);
    }
    Ok(Some(out))
}

/// Imports a file the user attached: compressed first when it is video or audio, then
/// stored like any other media.
pub fn import_file(
    db: &Db,
    media_dir: &Path,
    src: &Path,
    work: &Path,
    now: DateTime<Utc>,
) -> Result<MediaInfo> {
    let compressed = compress(src, work)?;
    let bytes = std::fs::read(compressed.as_deref().unwrap_or(src));
    if let Some(c) = &compressed {
        let _ = std::fs::remove_file(c);
    }
    import(db, media_dir, &bytes?, now)
}

/// Once per device, before its first sync after this feature: compresses the video and audio
/// cards already use and points those cards at the smaller files. Runs once only, so media
/// that arrives later by sync, pack or backup (already compressed, or deliberately kept as
/// imported) is never re-encoded. Old files stay until "Clean up media". Returns how many
/// files were replaced.
pub fn compress_library(
    db: &Db,
    media_dir: &Path,
    work: &Path,
    now: DateTime<Utc>,
    device_id: &str,
) -> Result<u32> {
    if db
        .setting_get::<bool>(KEY_LIBRARY_COMPRESSED)?
        .unwrap_or(false)
    {
        return Ok(0);
    }
    let mut cards = Vec::new();
    let mut todo: BTreeSet<(String, String)> = BTreeSet::new();
    for id in db.card_ids_all()? {
        let Some(card) = db.get_card(&id)? else {
            continue;
        };
        for m in card.steps.iter().flat_map(|s| s.media.iter()) {
            if m.mime.starts_with("video/") || m.mime.starts_with("audio/") {
                todo.insert((m.hash.clone(), m.ext.clone()));
            }
        }
        cards.push(card);
    }
    if todo.is_empty() || !ffmpeg_available() {
        db.setting_set(KEY_LIBRARY_COMPRESSED, &true)?;
        return Ok(0);
    }
    db.backup()?;
    let mut replaced: BTreeMap<String, MediaInfo> = BTreeMap::new();
    for (hash, ext) in todo {
        let path = media_dir.join(rel_path(&hash, &ext));
        if !path.is_file() {
            continue;
        }
        match compress(&path, work) {
            Ok(Some(out)) => {
                let bytes = std::fs::read(&out);
                let _ = std::fs::remove_file(&out);
                let info = import(db, media_dir, &bytes?, now)?;
                replaced.insert(hash.clone(), info);
            }
            Ok(None) => {}
            Err(e) => log::warn!("media {hash} not compressed: {e}"),
        }
    }
    for mut card in cards {
        let mut changed = false;
        for m in card.steps.iter_mut().flat_map(|s| s.media.iter_mut()) {
            if let Some(new) = replaced.get(&m.hash) {
                m.hash = new.hash.clone();
                m.ext = new.ext.clone();
                m.mime = new.mime.clone();
                changed = true;
            }
        }
        if changed {
            card.modified_at = now;
            card.modified_by = device_id.to_string();
            db.save_card(&card)?;
        }
    }
    db.setting_set(KEY_LIBRARY_COMPRESSED, &true)?;
    Ok(replaced.len() as u32)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CleanupReport {
    pub removed_files: u32,
    pub removed_rows: u32,
    #[ts(type = "number")]
    pub freed_bytes: u64,
}

/// Explicit "Clean up media" (SPEC §10): deletes files and rows no step references. Never runs
/// implicitly; soft-deleted cards still count as references.
pub fn cleanup(db: &Db, media_dir: &Path) -> Result<CleanupReport> {
    let referenced = db.referenced_media_hashes()?;
    let mut report = CleanupReport::default();
    for row in db.media_all()? {
        if referenced.contains(&row.hash) {
            continue;
        }
        let path = media_dir.join(rel_path(&row.hash, &row.ext));
        if path.exists() {
            report.freed_bytes += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            std::fs::remove_file(&path)?;
            report.removed_files += 1;
        }
        db.media_delete(&row.hash)?;
        report.removed_rows += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn png_is_sniffed_hashed_measured_and_stored_once() {
        let dir = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let info = import_bytes(dir.path(), PNG_1X1, &MediaLimits::default(), now).unwrap();
        assert_eq!(info.mime, "image/png");
        assert_eq!(info.ext, "png");
        assert_eq!((info.width, info.height), (Some(1), Some(1)));
        assert_eq!(info.hash.len(), 64);
        let path = dir.path().join(rel_path(&info.hash, "png"));
        assert!(path.exists());
        let again = import_bytes(dir.path(), PNG_1X1, &MediaLimits::default(), now).unwrap();
        assert_eq!(again.hash, info.hash);
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn clipboard_pixels_become_a_stored_png() {
        let dir = tempfile::tempdir().unwrap();
        let rgba = [255u8, 0, 0, 255, 0, 255, 0, 255]; // 2×1: red, green
        let png = encode_png(2, 1, &rgba).unwrap();
        let info = import_bytes(dir.path(), &png, &MediaLimits::default(), Utc::now()).unwrap();
        assert_eq!(
            (info.mime.as_str(), info.width, info.height),
            ("image/png", Some(2), Some(1))
        );
        assert!(encode_png(2, 2, &rgba).is_err(), "size mismatch");
        assert!(encode_png(0, 0, &[]).is_err());
    }

    #[test]
    fn unknown_types_and_oversize_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let now = Utc::now();
        assert!(import_bytes(dir.path(), b"plain text", &MediaLimits::default(), now).is_err());
        let tiny = MediaLimits {
            image_mb: 0,
            ..MediaLimits::default()
        };
        let err = import_bytes(dir.path(), PNG_1X1, &tiny, now).unwrap_err();
        assert!(err.to_string().contains("limit"));
        // A PDF has a real magic number but is not media.
        assert!(import_bytes(dir.path(), b"%PDF-1.4 ...", &MediaLimits::default(), now).is_err());
    }
}

//! Vault format (SPEC §11.1) and the merge of a vault-shaped directory into the database
//! (§11.2 step 5). Full backups use the same layout, so restoring one is a merge.
//!
//! Every file read here is untrusted: ids and hashes are checked before they are used in a
//! path or stored, and counts are capped.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::domain::{Card, Category, MediaRef, Review};
use crate::error::invalid;
use crate::media;
use crate::sched::SchedConfig;
use crate::settings::KEY_SCHED_CONFIG;
use crate::settings::MediaLimits;
use crate::zipsafe::MAX_JSON_BYTES;

pub const FORMAT: u32 = 1;
/// Cards (or categories, or review files) accepted from one vault or archive.
pub const MAX_ITEMS: usize = 20_000;
pub const VAULT_JSON: &str = "vault.json";
pub const CONFIG_PATH: &str = "config/sched.json";
const KEY_VAULT: &str = "vault";
const KEY_CONFIG_META: &str = "sched_config_meta";

/// Media from one's own vault or backup: type and hash are still verified, but the user's
/// current import limits do not apply (they may have been raised when the file came in).
const RESTORE_LIMITS: MediaLimits = MediaLimits {
    image_mb: 1024,
    audio_mb: 1024,
    video_mb: 1024,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultMeta {
    pub format: u32,
    pub vault_id: String,
    pub created_at: DateTime<Utc>,
}

/// `config/sched.json`: the synced scheduling constants with their LWW stamp.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub sched: SchedConfig,
    pub modified_at: DateTime<Utc>,
    pub modified_by: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct ConfigMeta {
    modified_at: Option<DateTime<Utc>>,
    modified_by: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergeReport {
    pub categories: u32,
    pub cards: u32,
    pub reviews: u32,
    pub media: u32,
    pub conflicts: u32,
    pub config_changed: bool,
    /// Items left out, one line each (missing media, unknown category, bad file).
    pub skipped: Vec<String>,
}

pub fn card_path(id: &str) -> String {
    format!("cards/{id}.json")
}

pub fn category_path(id: &str) -> String {
    format!("categories/{id}.json")
}

pub fn review_path(device_id: &str, ts: DateTime<Utc>) -> String {
    format!("reviews/{device_id}/{}.jsonl", ts.format("%Y-%m"))
}

pub fn media_path(hash: &str, ext: &str) -> String {
    format!("media/{}", media::rel_path(hash, ext))
}

/// Ids become file names, so only UUID-like text is accepted from outside.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn check_id(what: &str, id: &str) -> Result<()> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(invalid(format!("{what} has an invalid id {id:?}")))
    }
}

pub fn check_media_ref(m: &MediaRef) -> Result<()> {
    if valid_hash(&m.hash) && media::is_allowed_ext(&m.ext) {
        Ok(())
    } else {
        Err(invalid(format!("invalid media reference {:?}", m.hash)))
    }
}

pub fn check_card(c: &Card) -> Result<()> {
    check_id("card", &c.id)?;
    check_id("card category", &c.category_id)?;
    for s in &c.steps {
        check_id("step", &s.id)?;
        for cr in &s.criteria {
            check_id("criterion", &cr.id)?;
        }
        for m in &s.media {
            check_media_ref(m)?;
        }
    }
    Ok(())
}

pub fn check_review(r: &Review) -> Result<()> {
    check_id("review", &r.id)?;
    check_id("review card", &r.card_id)?;
    check_id("review device", &r.device_id)?;
    r.steps
        .iter()
        .try_for_each(|s| check_id("review step", &s.step_id))
}

/// This profile's vault identity, created once.
pub fn meta(db: &Db, now: DateTime<Utc>) -> Result<VaultMeta> {
    if let Some(m) = db.sync_get::<VaultMeta>(KEY_VAULT)? {
        return Ok(m);
    }
    let m = VaultMeta {
        format: FORMAT,
        vault_id: uuid::Uuid::now_v7().to_string(),
        created_at: now,
    };
    db.sync_set(KEY_VAULT, &m)?;
    Ok(m)
}

pub fn set_meta(db: &Db, meta: &VaultMeta) -> Result<()> {
    if !valid_id(&meta.vault_id) {
        return Err(invalid("invalid vault id"));
    }
    db.sync_set(KEY_VAULT, meta)
}

/// The local scheduling config with its LWW stamp; never edited locally = the epoch.
pub fn config_file(db: &Db) -> Result<ConfigFile> {
    let m: ConfigMeta = db.setting_get(KEY_CONFIG_META)?.unwrap_or_default();
    Ok(ConfigFile {
        sched: db.sched_config()?,
        modified_at: m.modified_at.unwrap_or(DateTime::UNIX_EPOCH),
        modified_by: m.modified_by,
    })
}

/// Saves the config, stamps it for sync, and replays every card (§7.6).
pub fn set_config(db: &Db, cfg: &SchedConfig, now: DateTime<Utc>, device_id: &str) -> Result<()> {
    cfg.validate()?;
    db.setting_set(KEY_SCHED_CONFIG, cfg)?;
    db.setting_set(
        KEY_CONFIG_META,
        &ConfigMeta {
            modified_at: Some(now),
            modified_by: device_id.to_string(),
        },
    )?;
    for id in db.card_ids_all()? {
        db.replay_card(&id, cfg)?;
    }
    Ok(())
}

/// `<app data>/vault/media`, the one media directory (SPEC §11.2: media exists once per
/// device). A media directory from before the vault existed is moved in.
pub fn media_dir(data_dir: &Path) -> Result<PathBuf> {
    let dir = data_dir.join("vault").join("media");
    let legacy = data_dir.join("media");
    if !dir.exists() && legacy.is_dir() {
        std::fs::create_dir_all(data_dir.join("vault"))?;
        std::fs::rename(&legacy, &dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn read_capped(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_JSON_BYTES {
        return Err(invalid(format!("{} is too large", path.display())));
    }
    Ok(buf)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&read_capped(path)?)
        .map_err(|e| invalid(format!("{}: {e}", path.display())))
}

/// Regular `*.<ext>` files directly in `dir` (none if it is missing), capped.
fn files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .collect();
    if out.len() > MAX_ITEMS {
        return Err(invalid(format!("{} holds too many files", dir.display())));
    }
    out.sort();
    Ok(out)
}

/// Makes sure the media row and file exist, taking the file from `src` or from the local
/// media directory (where sync downloads land), and checking its content hash.
fn ensure_media(
    db: &Db,
    src: &Path,
    media_dir: &Path,
    m: &MediaRef,
    now: DateTime<Utc>,
) -> Result<bool> {
    if db.media_get(&m.hash)?.is_some() {
        return Ok(false);
    }
    let rel = media::rel_path(&m.hash, &m.ext);
    let path = [src.join("media").join(&rel), media_dir.join(&rel)]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| invalid(format!("media {} is missing", m.hash)))?;
    let bytes = std::fs::read(&path)?;
    let info = media::import_bytes(media_dir, &bytes, &RESTORE_LIMITS, now)?;
    if info.hash != m.hash {
        return Err(invalid(format!(
            "media {} does not match its content",
            m.hash
        )));
    }
    db.media_insert(&info)?;
    Ok(true)
}

/// Last writer wins on `(modified_at, modified_by)`; the greater device id breaks ties.
fn newer(a: (DateTime<Utc>, &str), b: (DateTime<Utc>, &str)) -> bool {
    a > b
}

/// Merges `src` (a vault directory, a sync mirror or an extracted backup) into the database.
/// The database is backed up first. Every step is idempotent, so a merge interrupted by a
/// crash is completed by running it again.
pub fn merge(db: &Db, src: &Path, media_dir: &Path, now: DateTime<Utc>) -> Result<MergeReport> {
    let vault_json = src.join(VAULT_JSON);
    if vault_json.exists() {
        let m: VaultMeta = read_json(&vault_json)?;
        if m.format > FORMAT {
            return Err(invalid(
                "This data was written by a newer version of Indigo; update the app first",
            ));
        }
    }
    db.backup()?;
    let mut report = MergeReport::default();
    let mut touched: BTreeSet<String> = BTreeSet::new();

    // Categories first: cards reference them.
    for path in files(&src.join("categories"), "json")? {
        let mut cat: Category = match read_json(&path) {
            Ok(c) => c,
            Err(e) => {
                report.skipped.push(e.to_string());
                continue;
            }
        };
        if !valid_id(&cat.id) || cat.name.trim().is_empty() {
            report
                .skipped
                .push(format!("{}: invalid category", path.display()));
            continue;
        }
        let local = db.get_category(&cat.id)?;
        if local
            .as_ref()
            .is_some_and(|l| !newer((cat.modified_at, ""), (l.modified_at, "")))
        {
            continue;
        }
        // Names are unique: a same-named category made independently elsewhere keeps both.
        if let Some(other) = db.category_by_name(&cat.name)?
            && other.id != cat.id
        {
            cat.name = format!(
                "{} ({})",
                cat.name,
                &cat.id[cat.id.len().saturating_sub(4)..]
            );
        }
        db.save_category(&cat)?;
        report.categories += 1;
    }

    for path in files(&src.join("cards"), "json")? {
        let card: Card = match read_json(&path).and_then(|c: Card| check_card(&c).map(|_| c)) {
            Ok(c) => c,
            Err(e) => {
                report.skipped.push(e.to_string());
                continue;
            }
        };
        let local = db.get_card(&card.id)?;
        let remote_wins = match &local {
            None => true,
            Some(l) => newer(
                (card.modified_at, &card.modified_by),
                (l.modified_at, &l.modified_by),
            ),
        };
        if let Some(l) = &local {
            // `exported_at` holds the `modified_at` of the version last written to the vault:
            // a conflict needs both sides to have moved away from it.
            let base = db.card_exported_at(&l.id)?;
            let same_version =
                l.modified_at == card.modified_at && l.modified_by == card.modified_by;
            let both_changed = base != Some(l.modified_at) && base != Some(card.modified_at);
            if !same_version && both_changed {
                let loser = if remote_wins { l } else { &card };
                db.insert_conflict(
                    "card",
                    &card.id,
                    &serde_json::to_string(loser)?,
                    &loser.modified_by,
                    now,
                )?;
                report.conflicts += 1;
            }
        }
        if !remote_wins {
            continue;
        }
        if db.get_category(&card.category_id)?.is_none() {
            report
                .skipped
                .push(format!("card {:?}: its category is missing", card.title));
            continue;
        }
        let mut card = card;
        let mut media_ok = true;
        for m in card.steps.iter_mut().flat_map(|s| s.media.iter_mut()) {
            match ensure_media(db, src, media_dir, m, now) {
                Ok(added) => {
                    report.media += u32::from(added);
                    if let Some(row) = db.media_get(&m.hash)? {
                        m.ext = row.ext;
                        m.mime = row.mime;
                    }
                }
                Err(e) => {
                    report.skipped.push(format!("card {:?}: {e}", card.title));
                    media_ok = false;
                    break;
                }
            }
        }
        if !media_ok {
            continue;
        }
        db.save_card(&card)?;
        touched.insert(card.id.clone());
        report.cards += 1;
    }

    let review_dirs = match std::fs::read_dir(src.join("reviews")) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect(),
        Err(_) => Vec::new(),
    };
    for dir in review_dirs {
        for path in files(&dir, "jsonl")? {
            let text = String::from_utf8(read_capped(&path)?)
                .map_err(|_| invalid(format!("{} is not UTF-8", path.display())))?;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let review: Review =
                    match serde_json::from_str(line).map_err(|e| invalid(e.to_string())) {
                        Ok(r) => r,
                        Err(e) => {
                            report.skipped.push(format!("{}: {e}", path.display()));
                            continue;
                        }
                    };
                if let Err(e) = check_review(&review) {
                    report.skipped.push(e.to_string());
                    continue;
                }
                if db.insert_foreign_review(&review)? {
                    touched.insert(review.card_id.clone());
                    report.reviews += 1;
                }
            }
        }
    }

    let cfg_path = src.join(CONFIG_PATH);
    if cfg_path.is_file() {
        let remote: ConfigFile = read_json(&cfg_path)?;
        let local = config_file(db)?;
        if newer(
            (remote.modified_at, &remote.modified_by),
            (local.modified_at, &local.modified_by),
        ) {
            db.setting_set(KEY_SCHED_CONFIG, &remote.sched)?;
            db.setting_set(
                KEY_CONFIG_META,
                &ConfigMeta {
                    modified_at: Some(remote.modified_at),
                    modified_by: remote.modified_by,
                },
            )?;
            report.config_changed = true;
        }
    }

    let cfg = db.sched_config()?;
    let replay: Vec<String> = if report.config_changed {
        db.card_ids_all()?
    } else {
        let known: BTreeSet<String> = db.card_ids_all()?.into_iter().collect();
        touched
            .into_iter()
            .filter(|id| known.contains(id))
            .collect()
    };
    for id in replay {
        db.replay_card(&id, &cfg)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_hashes_are_path_safe() {
        assert!(valid_id("0196f2a4-7c3b-7d1e-9a8b-0123456789ab"));
        for bad in ["", "../x", "a/b", "a.b", &"x".repeat(65)] {
            assert!(!valid_id(bad), "{bad}");
        }
        assert!(valid_hash(&"ab".repeat(32)));
        assert!(!valid_hash(&"AB".repeat(32)));
        assert!(!valid_hash("ab/"));
    }

    #[test]
    fn legacy_media_dir_moves_into_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("media/ab")).unwrap();
        std::fs::write(dir.path().join("media/ab/x.png"), b"x").unwrap();
        let m = media_dir(dir.path()).unwrap();
        assert_eq!(m, dir.path().join("vault/media"));
        assert!(m.join("ab/x.png").exists());
        assert!(!dir.path().join("media").exists());
        assert_eq!(media_dir(dir.path()).unwrap(), m, "idempotent");
    }
}

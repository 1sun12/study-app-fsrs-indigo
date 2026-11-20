//! Full backup (SPEC §12): one zip in the vault layout plus `settings.json`. Restoring is the
//! §11 merge, so a non-empty profile is merged into, never overwritten.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::error::invalid;
use crate::media;
use crate::settings::{KEY_SETTINGS, Settings};
use crate::vault::{self, MergeReport};
use crate::zipsafe::{SafeZip, ZipOut};

pub const SETTINGS_JSON: &str = "settings.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackupReport {
    pub cards: u32,
    pub reviews: u32,
    pub media: u32,
    /// Media rows whose file is gone from disk; the cards using them will not restore.
    pub missing_media: u32,
}

/// Writes everything durable: categories and cards (tombstones included), the scheduling
/// config, every device's reviews, media, and this device's settings.
pub fn export(db: &Db, media_dir: &Path, out: &Path, now: DateTime<Utc>) -> Result<BackupReport> {
    let mut report = BackupReport::default();
    let mut z = ZipOut::create(out)?;
    z.json(vault::VAULT_JSON, &vault::meta(db, now)?)?;
    for c in db.categories_all()? {
        z.json(&vault::category_path(&c.id), &c)?;
    }
    for id in db.card_ids_all()? {
        if let Some(card) = db.get_card(&id)? {
            z.json(&vault::card_path(&id), &card)?;
            report.cards += 1;
        }
    }
    z.json(vault::CONFIG_PATH, &vault::config_file(db)?)?;

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    let mut reviews = db.reviews_all()?;
    reviews.sort_by(|a, b| (a.ts_utc, &a.id).cmp(&(b.ts_utc, &b.id)));
    for r in &reviews {
        let file = files
            .entry(vault::review_path(&r.device_id, r.ts_utc))
            .or_default();
        file.push_str(&serde_json::to_string(r)?);
        file.push('\n');
        report.reviews += 1;
    }
    for (path, text) in &files {
        z.bytes(path, text.as_bytes())?;
    }

    for m in db.media_all()? {
        let file = media_dir.join(media::rel_path(&m.hash, &m.ext));
        if file.is_file() {
            z.file(&vault::media_path(&m.hash, &m.ext), &file)?;
            report.media += 1;
        } else {
            report.missing_media += 1;
        }
    }
    z.json(SETTINGS_JSON, &db.settings()?)?;
    z.finish()?;
    Ok(report)
}

/// Validates and extracts the archive under `work_dir`, merges it, and restores settings when
/// the profile was empty. The extracted copy is removed afterwards, success or not.
pub fn import(
    db: &Db,
    media_dir: &Path,
    archive: &Path,
    work_dir: &Path,
    now: DateTime<Utc>,
) -> Result<MergeReport> {
    let mut z = SafeZip::open(archive)?;
    if !z.has(vault::VAULT_JSON) {
        return Err(invalid("This file is not an Indigo backup"));
    }
    let tmp = work_dir.join(format!("restore-{}", uuid::Uuid::now_v7()));
    let result = (|| {
        z.extract_to(&tmp)?;
        let was_empty = db.is_empty_profile()?;
        let report = vault::merge(db, &tmp, media_dir, now)?;
        if was_empty {
            // A restored profile is the same vault, so it can keep syncing with it (§11).
            let meta: vault::VaultMeta =
                serde_json::from_slice(&std::fs::read(tmp.join(vault::VAULT_JSON))?)
                    .map_err(|e| invalid(format!("{}: {e}", vault::VAULT_JSON)))?;
            vault::set_meta(db, &meta)?;
        }
        let settings_path = tmp.join(SETTINGS_JSON);
        if was_empty && settings_path.is_file() {
            let settings: Settings = serde_json::from_slice(&std::fs::read(&settings_path)?)
                .map_err(|e| invalid(format!("{SETTINGS_JSON}: {e}")))?;
            db.setting_set(KEY_SETTINGS, &settings)?;
        }
        Ok(report)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

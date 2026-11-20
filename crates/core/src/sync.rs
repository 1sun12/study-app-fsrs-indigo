//! Sync (SPEC §11.2): fetch the remote into `mirror/`, merge, export what changed into
//! `vault/`, publish. The transport is a trait; the app and the tests both use
//! `git::GitTransport` (the tests against local bare repositories).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::domain::Card;
use crate::error::invalid;
use crate::sched::SchedConfig;
use crate::vault::{self, MergeReport, VaultMeta};

/// A push the remote rejects (another device published meanwhile) re-runs the sync this many
/// times before giving up until the next trigger.
pub const PUSH_ATTEMPTS: usize = 3;
const KEY_STATUS: &str = "last_sync";

/// Moves files between this device and the remote.
pub trait Transport {
    /// Remote JSON (all but `media/`) into `mirror`, and remote media into `media_dir`.
    fn fetch(&self, mirror: &Path, media_dir: &Path) -> Result<()>;
    /// Publishes the vault over the fetched remote state. Ok(false): the remote moved since
    /// `fetch`, so sync again.
    fn push(&self, vault_dir: &Path) -> Result<bool>;
}

pub struct Paths {
    pub vault: PathBuf,
    pub mirror: PathBuf,
    pub media: PathBuf,
}

impl Paths {
    pub fn new(data_dir: &Path) -> Paths {
        Paths {
            vault: data_dir.join("vault"),
            mirror: data_dir.join("mirror"),
            media: data_dir.join("vault").join("media"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportReport {
    pub categories: u32,
    pub cards: u32,
    pub reviews: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SyncOutcome {
    Synced {
        merged: MergeReport,
        exported: ExportReport,
    },
    Skipped {
        reason: String,
    },
}

/// What the status dot and Settings → Sync show; device-local.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SyncStatus {
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    /// Why the last attempt did not sync (skipped or failed); None after a success.
    pub message: Option<String>,
    /// The last attempt failed (as opposed to being skipped).
    pub failed: bool,
    pub conflicts: u32,
}

/// Changes not yet in the vault: cards edited since export plus this device's unexported
/// reviews. Zero after a successful sync with no edits since.
pub fn pending(db: &Db, device_id: &str) -> Result<u32> {
    Ok((db.cards_to_export()?.len() + db.unexported_reviews(device_id)?.len()) as u32)
}

/// `sync_status` for the frontend.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SyncView {
    pub configured: bool,
    pub syncing: bool,
    pub status: SyncStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceFile {
    pub name: String,
    pub app_version: String,
    pub last_sync_at: DateTime<Utc>,
}

/// Writes `bytes` unless the file already holds exactly them; write-then-rename.
fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<bool> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(true)
}

fn json(value: &impl Serialize) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(value)?)
}

/// §11.2 step 7: writes what changed into the vault. Only this device's review file is ever
/// written; another device's file here would overwrite theirs on upload.
pub fn export(
    db: &Db,
    vault_dir: &Path,
    device_id: &str,
    device_name: &str,
    now: DateTime<Utc>,
) -> Result<ExportReport> {
    let mut report = ExportReport::default();
    write_if_changed(
        &vault_dir.join(vault::VAULT_JSON),
        &json(&vault::meta(db, now)?)?,
    )?;
    for c in db.categories_all()? {
        if write_if_changed(&vault_dir.join(vault::category_path(&c.id)), &json(&c)?)? {
            report.categories += 1;
        }
    }
    for id in db.cards_to_export()? {
        if let Some(card) = db.get_card(&id)? {
            write_if_changed(&vault_dir.join(vault::card_path(&id)), &json(&card)?)?;
            db.set_card_exported(&id, card.modified_at)?;
            report.cards += 1;
        }
    }
    write_if_changed(
        &vault_dir.join(vault::CONFIG_PATH),
        &json(&vault::config_file(db)?)?,
    )?;

    let reviews = db.unexported_reviews(device_id)?;
    let mut by_file: BTreeMap<String, String> = BTreeMap::new();
    for r in &reviews {
        let text = by_file
            .entry(vault::review_path(device_id, r.ts_utc))
            .or_default();
        text.push_str(&serde_json::to_string(r)?);
        text.push('\n');
    }
    for (rel, text) in &by_file {
        let path = vault_dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Append-only. A crash before the rows are marked repeats lines; merge ignores known ids.
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    db.mark_reviews_exported(&reviews.iter().map(|r| r.id.clone()).collect::<Vec<_>>())?;
    report.reviews = reviews.len() as u32;

    // Rewritten only when the name or version changes: a sync with nothing new then commits
    // nothing (git history already records when each sync happened).
    let device_path = vault_dir.join(format!("devices/{device_id}.json"));
    let current = std::fs::read(&device_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<DeviceFile>(&b).ok());
    if current.is_none_or(|d| d.name != device_name || d.app_version != env!("CARGO_PKG_VERSION")) {
        write_if_changed(
            &device_path,
            &json(&DeviceFile {
                name: device_name.to_string(),
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                last_sync_at: now,
            })?,
        )?;
    }
    Ok(report)
}

pub fn status(db: &Db) -> Result<SyncStatus> {
    let mut s: SyncStatus = db.sync_get(KEY_STATUS)?.unwrap_or_default();
    s.conflicts = db.conflicts_all()?.len() as u32;
    Ok(s)
}

/// Stores the outcome of an attempt for the status dot.
pub fn record(db: &Db, outcome: &Result<SyncOutcome>, now: DateTime<Utc>) -> Result<SyncStatus> {
    let mut s = status(db)?;
    s.last_attempt_at = Some(now);
    match outcome {
        Ok(SyncOutcome::Synced { .. }) => {
            s.last_success_at = Some(now);
            s.message = None;
            s.failed = false;
        }
        Ok(SyncOutcome::Skipped { reason }) => {
            s.message = Some(reason.clone());
            s.failed = false;
        }
        Err(e) => {
            s.message = Some(e.to_string());
            s.failed = true;
        }
    }
    db.sync_set(KEY_STATUS, &s)?;
    status(db)
}

/// One sync (§11.2). Preconditions (git present, repository configured) are the caller's;
/// everything here either syncs, skips with a reason, or fails without data loss.
pub fn run(
    db: &Db,
    paths: &Paths,
    transport: &dyn Transport,
    device_id: &str,
    device_name: &str,
    now: DateTime<Utc>,
) -> Result<SyncOutcome> {
    std::fs::create_dir_all(&paths.media)?;
    for _ in 0..PUSH_ATTEMPTS {
        if paths.mirror.exists() {
            std::fs::remove_dir_all(&paths.mirror)?;
        }
        std::fs::create_dir_all(&paths.mirror)?;
        transport.fetch(&paths.mirror, &paths.media)?;

        let local = vault::meta(db, now)?;
        let remote_meta = paths.mirror.join(vault::VAULT_JSON);
        if remote_meta.is_file() {
            let remote: VaultMeta = serde_json::from_slice(&std::fs::read(&remote_meta)?)
                .map_err(|e| invalid(format!("remote vault.json: {e}")))?;
            if remote.vault_id != local.vault_id {
                if status(db)?.last_success_at.is_some() {
                    return Err(invalid(
                        "The repository holds a different Indigo library. Check the repository in Settings → Sync.",
                    ));
                }
                // First sync of this device: join the existing vault; merge never deletes.
                vault::set_meta(db, &remote)?;
            }
        }

        // Merge and export are idempotent, so a rejected push simply goes round again.
        let merged = vault::merge(db, &paths.mirror, &paths.media, now)?;
        let exported = export(db, &paths.vault, device_id, device_name, now)?;
        if transport.push(&paths.vault)? {
            return Ok(SyncOutcome::Synced { merged, exported });
        }
    }
    Ok(SyncOutcome::Skipped {
        reason: "The repository kept changing; trying again later".into(),
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConflictView {
    pub id: String,
    pub entity: String,
    pub entity_id: String,
    pub title: String,
    pub detected_at: DateTime<Utc>,
    pub losing_device: String,
}

pub fn conflicts(db: &Db) -> Result<Vec<ConflictView>> {
    Ok(db
        .conflicts_all()?
        .into_iter()
        .map(
            |(id, entity, entity_id, detected_at, json, losing_device)| {
                let title = serde_json::from_str::<Card>(&json)
                    .map(|c| c.title)
                    .unwrap_or_else(|_| entity_id.clone());
                ConflictView {
                    id,
                    entity,
                    entity_id,
                    title,
                    detected_at,
                    losing_device,
                }
            },
        )
        .collect())
}

/// Brings back the version that lost: it is saved as a fresh edit, so it also wins the next
/// sync on every device. The conflict record is removed.
pub fn restore_conflict(
    db: &Db,
    id: &str,
    now: DateTime<Utc>,
    device_id: &str,
    cfg: &SchedConfig,
) -> Result<()> {
    let (_, _, _, _, json, _) = db
        .conflicts_all()?
        .into_iter()
        .find(|c| c.0 == id)
        .ok_or_else(|| invalid("That conflict is gone"))?;
    let mut card: Card = serde_json::from_str(&json)?;
    vault::check_card(&card)?;
    if db.get_category(&card.category_id)?.is_none() {
        return Err(invalid("The category of that version no longer exists"));
    }
    card.modified_at = now;
    card.modified_by = device_id.to_string();
    db.save_card(&card)?;
    db.replay_card(&card.id, cfg)?;
    db.conflict_delete(id)
}

pub fn dismiss_conflict(db: &Db, id: &str) -> Result<()> {
    db.conflict_delete(id)
}

//! SQLite access (SPEC §6): WAL, foreign keys, one writer behind a mutex, migrations via
//! `rusqlite_migration`, and a backup copy before any migration of an existing database.

pub mod migrations;
pub mod repo;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use rusqlite::Connection;
use rusqlite::types::Type;
use serde::de::DeserializeOwned;

use crate::error::invalid;
use crate::{CoreError, Result};

/// Backups kept in `<db dir>/backups/`, oldest removed first.
pub const BACKUPS_KEEP: usize = 10;

pub struct Db {
    conn: Mutex<Connection>,
    pub path: PathBuf,
}

impl Db {
    /// Opens (creating if needed) and migrates. An existing file with pending migrations is
    /// copied to `backups/` first.
    pub fn open(path: &Path) -> Result<Db> {
        let existed = path.exists();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // The app and `indigo-cli` may write at the same moment: wait instead of failing.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if existed && version < migrations::COUNT as i64 {
            backup(&conn, path)?;
        }
        migrations::migrations().to_latest(&mut conn)?;
        Ok(Db {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }

    /// Consistent copy of the live database into `backups/`, then rotation. Also used before
    /// sync merges.
    pub fn backup(&self) -> Result<PathBuf> {
        self.with(|conn| backup(conn, &self.path))
    }

    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| invalid("database mutex poisoned"))?;
        f(&mut conn)
    }
}

fn backup(conn: &Connection, db_path: &Path) -> Result<PathBuf> {
    let dir = db_path
        .parent()
        .ok_or_else(|| invalid("database path has no parent"))?
        .join("backups");
    std::fs::create_dir_all(&dir)?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut target = dir.join(format!("indigo-{stamp}.sqlite"));
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("indigo-{stamp}-{n}.sqlite"));
        n += 1;
    }
    let target_str = target
        .to_str()
        .ok_or_else(|| invalid("backup path is not valid UTF-8"))?;
    // VACUUM INTO writes a complete, consistent copy even with WAL pages not yet checkpointed.
    conn.execute("VACUUM INTO ?1", [target_str])?;
    rotate_backups(&dir)?;
    Ok(target)
}

fn rotate_backups(dir: &Path) -> Result<()> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("indigo-") && n.ends_with(".sqlite"))
        })
        .collect();
    files.sort(); // the timestamp in the name sorts chronologically
    let excess = files.len().saturating_sub(BACKUPS_KEEP);
    for old in files.into_iter().take(excess) {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

// Text encodings shared by the repos.

pub(crate) fn ts_text(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn opt_ts_text(ts: Option<DateTime<Utc>>) -> Option<String> {
    ts.map(ts_text)
}

pub(crate) fn day_text(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

pub(crate) fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| invalid(format!("bad timestamp {s:?}: {e}")))
}

pub(crate) fn parse_day(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| invalid(format!("bad day {s:?}: {e}")))
}

// Column readers usable inside `query_map` closures (they return rusqlite errors).

fn conv(idx: usize, e: CoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(e))
}

pub(crate) fn ts_col(row: &rusqlite::Row, idx: usize) -> rusqlite::Result<DateTime<Utc>> {
    let s: String = row.get(idx)?;
    parse_ts(&s).map_err(|e| conv(idx, e))
}

pub(crate) fn opt_ts_col(
    row: &rusqlite::Row,
    idx: usize,
) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let s: Option<String> = row.get(idx)?;
    s.map(|s| parse_ts(&s).map_err(|e| conv(idx, e)))
        .transpose()
}

pub(crate) fn day_col(row: &rusqlite::Row, idx: usize) -> rusqlite::Result<NaiveDate> {
    let s: String = row.get(idx)?;
    parse_day(&s).map_err(|e| conv(idx, e))
}

pub(crate) fn opt_day_col(row: &rusqlite::Row, idx: usize) -> rusqlite::Result<Option<NaiveDate>> {
    let s: Option<String> = row.get(idx)?;
    s.map(|s| parse_day(&s).map_err(|e| conv(idx, e)))
        .transpose()
}

pub(crate) fn enum_col<T: std::str::FromStr<Err = String>>(
    row: &rusqlite::Row,
    idx: usize,
) -> rusqlite::Result<T> {
    let s: String = row.get(idx)?;
    s.parse::<T>().map_err(|e| conv(idx, CoreError::Invalid(e)))
}

pub(crate) fn opt_enum_col<T: std::str::FromStr<Err = String>>(
    row: &rusqlite::Row,
    idx: usize,
) -> rusqlite::Result<Option<T>> {
    let s: Option<String> = row.get(idx)?;
    s.map(|s| s.parse::<T>().map_err(|e| conv(idx, CoreError::Invalid(e))))
        .transpose()
}

pub(crate) fn json_col<T: DeserializeOwned>(
    row: &rusqlite::Row,
    idx: usize,
) -> rusqlite::Result<T> {
    let s: String = row.get(idx)?;
    serde_json::from_str(&s).map_err(|e| conv(idx, CoreError::Json(e)))
}

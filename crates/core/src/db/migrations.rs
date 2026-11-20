//! Schema migrations. Append a new `M::up` for every change; never edit a shipped one.

use rusqlite_migration::{M, Migrations};

pub const COUNT: usize = 2;

const V1: &str = r#"
CREATE TABLE categories(
  id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, syntax TEXT, color TEXT,
  position INTEGER NOT NULL DEFAULT 0, modified_at TEXT NOT NULL, deleted_at TEXT);
CREATE TABLE cards(
  id TEXT PRIMARY KEY, title TEXT NOT NULL, category_id TEXT NOT NULL REFERENCES categories(id),
  difficulty TEXT NOT NULL, tags_json TEXT NOT NULL DEFAULT '[]', est_minutes INTEGER, source TEXT,
  pack_id TEXT, pack_card_id TEXT, state TEXT NOT NULL, created_at TEXT NOT NULL,
  modified_at TEXT NOT NULL, modified_by TEXT NOT NULL, deleted_at TEXT, exported_at TEXT);
CREATE TABLE steps(
  id TEXT PRIMARY KEY, card_id TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
  position INTEGER NOT NULL, prompt_md TEXT NOT NULL, response TEXT NOT NULL,
  solution_md TEXT NOT NULL DEFAULT '', solution_text TEXT, points REAL NOT NULL DEFAULT 1);
CREATE INDEX steps_card ON steps(card_id, position);
CREATE TABLE criteria(
  id TEXT PRIMARY KEY, step_id TEXT NOT NULL REFERENCES steps(id) ON DELETE CASCADE,
  position INTEGER NOT NULL, text TEXT NOT NULL, weight REAL NOT NULL DEFAULT 1);
CREATE INDEX criteria_step ON criteria(step_id, position);
CREATE TABLE media(
  hash TEXT PRIMARY KEY, mime TEXT NOT NULL, ext TEXT NOT NULL, bytes INTEGER NOT NULL,
  width INTEGER, height INTEGER, duration_ms INTEGER, created_at TEXT NOT NULL);
CREATE TABLE step_media(
  step_id TEXT NOT NULL REFERENCES steps(id) ON DELETE CASCADE,
  hash TEXT NOT NULL REFERENCES media(hash), role TEXT NOT NULL, position INTEGER NOT NULL,
  caption TEXT, PRIMARY KEY(step_id, hash, role));
CREATE TABLE attempts(
  id TEXT PRIMARY KEY, card_id TEXT NOT NULL REFERENCES cards(id), status TEXT NOT NULL,
  started_at TEXT NOT NULL, completed_at TEXT);
CREATE TABLE attempt_steps(
  attempt_id TEXT NOT NULL REFERENCES attempts(id) ON DELETE CASCADE, step_id TEXT NOT NULL,
  status TEXT NOT NULL, shown_at TEXT, elapsed_ms INTEGER NOT NULL DEFAULT 0, typed_answer TEXT,
  checked_json TEXT NOT NULL DEFAULT '[]', grade4 TEXT, score REAL,
  PRIMARY KEY(attempt_id, step_id));
CREATE TABLE reviews(
  id TEXT PRIMARY KEY, card_id TEXT NOT NULL, ts_utc TEXT NOT NULL,
  tz_offset_min INTEGER NOT NULL, device_id TEXT NOT NULL, kind TEXT NOT NULL,
  grade TEXT NOT NULL, score REAL NOT NULL, steps_json TEXT NOT NULL, sched_json TEXT NOT NULL,
  exported INTEGER NOT NULL DEFAULT 0);
CREATE INDEX reviews_card ON reviews(card_id, ts_utc);
CREATE TABLE card_sched(
  card_id TEXT PRIMARY KEY, stability REAL NOT NULL, difficulty REAL NOT NULL,
  stability_fast REAL NOT NULL, last_review_ts TEXT NOT NULL, due_day TEXT NOT NULL,
  interval_days INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL,
  success_streak INTEGER NOT NULL);
CREATE INDEX card_sched_due ON card_sched(due_day);
CREATE TABLE step_stats(
  step_id TEXT PRIMARY KEY, recent_scores_json TEXT NOT NULL, flagged INTEGER NOT NULL,
  flag_count INTEGER NOT NULL);
CREATE TABLE settings(key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
CREATE TABLE sync_state(key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
CREATE TABLE conflicts(
  id TEXT PRIMARY KEY, entity TEXT NOT NULL, entity_id TEXT NOT NULL, detected_at TEXT NOT NULL,
  losing_json TEXT NOT NULL, losing_device TEXT NOT NULL);
"#;

/// Cram attempts (2026-11-12).
const V2: &str = "ALTER TABLE attempts ADD COLUMN cram INTEGER NOT NULL DEFAULT 0;";

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(V1), M::up(V2)])
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_are_valid() {
        assert!(super::migrations().validate().is_ok());
    }
}

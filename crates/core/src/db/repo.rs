//! Repositories: SQL only. Orchestration (ids, replay, settings side effects) lives in
//! `authoring.rs`, `study.rs` and `planner.rs`.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    Db, day_col, day_text, enum_col, json_col, opt_day_col, opt_enum_col, opt_ts_col, opt_ts_text,
    ts_col, ts_text,
};
use crate::Result;
use crate::domain::{
    Attempt, AttemptStep, Card, CardFilter, CardSched, CardState, CardSummary, Category, Criterion,
    MediaRef, Review, Step, StepStats,
};
use crate::error::invalid;
use crate::media::MediaInfo;
use crate::replay::{self, ReplayState};
use crate::sched::SchedConfig;
use crate::settings::{KEY_SCHED_CONFIG, KEY_SETTINGS, Settings};

// ---------------------------------------------------------------- settings / device

impl Db {
    pub fn setting_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.with(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT value_json FROM settings WHERE key = ?1",
                    [key],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
        })
    }

    pub fn setting_set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.with(|conn| {
            conn.execute(
                "INSERT INTO settings(key, value_json) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
                params![key, json],
            )?;
            Ok(())
        })
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(self.setting_get(KEY_SETTINGS)?.unwrap_or_default())
    }

    pub fn sched_config(&self) -> Result<SchedConfig> {
        Ok(self
            .setting_get::<SchedConfig>(KEY_SCHED_CONFIG)?
            .unwrap_or_default()
            .clamped())
    }

    /// Stable per-installation id, created on first call.
    pub fn device_id(&self) -> Result<String> {
        self.with(|conn| {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT value_json FROM sync_state WHERE key = 'device_id'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(json) = existing {
                return Ok(serde_json::from_str(&json)?);
            }
            let id = uuid::Uuid::now_v7().to_string();
            conn.execute(
                "INSERT INTO sync_state(key, value_json) VALUES ('device_id', ?1)",
                [serde_json::to_string(&id)?],
            )?;
            Ok(id)
        })
    }
}

// ---------------------------------------------------------------- categories

const CATEGORY_COLS: &str = "id, name, syntax, color, position, modified_at, deleted_at";

fn category_row(r: &rusqlite::Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: r.get(0)?,
        name: r.get(1)?,
        syntax: r.get(2)?,
        color: r.get(3)?,
        position: r.get(4)?,
        modified_at: ts_col(r, 5)?,
        deleted_at: opt_ts_col(r, 6)?,
    })
}

impl Db {
    pub fn categories_list(&self) -> Result<Vec<Category>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {CATEGORY_COLS} FROM categories WHERE deleted_at IS NULL ORDER BY position, name"
            ))?;
            let rows = stmt.query_map([], category_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn get_category(&self, id: &str) -> Result<Option<Category>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    &format!("SELECT {CATEGORY_COLS} FROM categories WHERE id = ?1"),
                    [id],
                    category_row,
                )
                .optional()?)
        })
    }

    /// Including soft-deleted rows: names are unique, so a deleted category can be revived.
    pub fn category_by_name(&self, name: &str) -> Result<Option<Category>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    &format!(
                        "SELECT {CATEGORY_COLS} FROM categories WHERE name = ?1 COLLATE NOCASE"
                    ),
                    [name],
                    category_row,
                )
                .optional()?)
        })
    }

    pub fn save_category(&self, c: &Category) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "INSERT INTO categories(id, name, syntax, color, position, modified_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, syntax = excluded.syntax,
                   color = excluded.color, position = excluded.position,
                   modified_at = excluded.modified_at, deleted_at = excluded.deleted_at",
                params![
                    c.id,
                    c.name,
                    c.syntax,
                    c.color,
                    c.position,
                    ts_text(c.modified_at),
                    opt_ts_text(c.deleted_at)
                ],
            )?;
            Ok(())
        })
    }

    pub fn category_card_count(&self, id: &str) -> Result<u32> {
        self.with(|conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM cards WHERE category_id = ?1 AND deleted_at IS NULL",
                [id],
                |r| r.get(0),
            )?)
        })
    }

    /// Soft delete; refused while live cards still use the category.
    pub fn delete_category(&self, id: &str, now: DateTime<Utc>) -> Result<()> {
        let n = self.category_card_count(id)?;
        if n > 0 {
            return Err(invalid(format!(
                "{n} card(s) still use this category; move or delete them first"
            )));
        }
        self.with(|conn| {
            conn.execute(
                "UPDATE categories SET deleted_at = ?2, modified_at = ?2 WHERE id = ?1",
                params![id, ts_text(now)],
            )?;
            Ok(())
        })
    }
}

// ---------------------------------------------------------------- media

impl Db {
    pub fn media_insert(&self, m: &MediaInfo) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO media(hash, mime, ext, bytes, width, height, duration_ms, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    m.hash,
                    m.mime,
                    m.ext,
                    m.bytes as i64,
                    m.width,
                    m.height,
                    m.duration_ms.map(|d| d as i64),
                    ts_text(m.created_at)
                ],
            )?;
            Ok(())
        })
    }

    pub fn media_get(&self, hash: &str) -> Result<Option<MediaInfo>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT hash, mime, ext, bytes, width, height, duration_ms, created_at
                     FROM media WHERE hash = ?1",
                    [hash],
                    |r| {
                        Ok(MediaInfo {
                            hash: r.get(0)?,
                            mime: r.get(1)?,
                            ext: r.get(2)?,
                            bytes: r.get::<_, i64>(3)? as u64,
                            width: r.get(4)?,
                            height: r.get(5)?,
                            duration_ms: r.get::<_, Option<i64>>(6)?.map(|d| d as u64),
                            created_at: ts_col(r, 7)?,
                        })
                    },
                )
                .optional()?)
        })
    }
}

// ---------------------------------------------------------------- cards

impl Db {
    /// Upserts the card row and replaces its steps, criteria and media refs in one transaction.
    pub fn save_card(&self, card: &Card) -> Result<()> {
        self.with(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO cards(id, title, category_id, difficulty, tags_json, est_minutes, source,
                   pack_id, pack_card_id, state, created_at, modified_at, modified_by, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title,
                   category_id = excluded.category_id, difficulty = excluded.difficulty,
                   tags_json = excluded.tags_json, est_minutes = excluded.est_minutes,
                   source = excluded.source, pack_id = excluded.pack_id,
                   pack_card_id = excluded.pack_card_id, state = excluded.state,
                   modified_at = excluded.modified_at, modified_by = excluded.modified_by,
                   deleted_at = excluded.deleted_at",
                params![
                    card.id,
                    card.title,
                    card.category_id,
                    card.difficulty.as_str(),
                    serde_json::to_string(&card.tags)?,
                    card.est_minutes,
                    card.source,
                    card.pack_id,
                    card.pack_card_id,
                    card.state.as_str(),
                    ts_text(card.created_at),
                    ts_text(card.modified_at),
                    card.modified_by,
                    opt_ts_text(card.deleted_at),
                ],
            )?;
            tx.execute("DELETE FROM steps WHERE card_id = ?1", [&card.id])?;
            for (pos, step) in card.steps.iter().enumerate() {
                tx.execute(
                    "INSERT INTO steps(id, card_id, position, prompt_md, response, solution_md,
                       solution_text, points) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        step.id,
                        card.id,
                        pos as i64,
                        step.prompt_md,
                        step.response.as_str(),
                        step.solution_md,
                        step.solution_text,
                        step.points
                    ],
                )?;
                for (cpos, c) in step.criteria.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO criteria(id, step_id, position, text, weight)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![c.id, step.id, cpos as i64, c.text, c.weight],
                    )?;
                }
                for (mpos, m) in step.media.iter().enumerate() {
                    let known: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM media WHERE hash = ?1)",
                        [&m.hash],
                        |r| r.get(0),
                    )?;
                    if !known {
                        return Err(invalid(format!("media {} was never imported", m.hash)));
                    }
                    tx.execute(
                        "INSERT OR IGNORE INTO step_media(step_id, hash, role, position, caption)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![step.id, m.hash, m.role.as_str(), mpos as i64, m.caption],
                    )?;
                }
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn get_card(&self, id: &str) -> Result<Option<Card>> {
        self.with(|conn| {
            let card = conn
                .query_row(
                    "SELECT id, title, category_id, difficulty, tags_json, est_minutes, source,
                       pack_id, pack_card_id, state, created_at, modified_at, modified_by, deleted_at
                     FROM cards WHERE id = ?1",
                    [id],
                    |r| {
                        Ok(Card {
                            id: r.get(0)?,
                            title: r.get(1)?,
                            category_id: r.get(2)?,
                            difficulty: enum_col(r, 3)?,
                            tags: json_col(r, 4)?,
                            est_minutes: r.get(5)?,
                            source: r.get(6)?,
                            pack_id: r.get(7)?,
                            pack_card_id: r.get(8)?,
                            state: enum_col(r, 9)?,
                            created_at: ts_col(r, 10)?,
                            modified_at: ts_col(r, 11)?,
                            modified_by: r.get(12)?,
                            deleted_at: opt_ts_col(r, 13)?,
                            steps: Vec::new(),
                        })
                    },
                )
                .optional()?;
            let Some(mut card) = card else {
                return Ok(None);
            };
            card.steps = load_steps(conn, id)?;
            Ok(Some(card))
        })
    }

    pub fn cards_query(&self, f: &CardFilter) -> Result<Vec<CardSummary>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.title, c.category_id, cat.name, cat.color, c.difficulty, c.state,
                   (SELECT count(*) FROM steps s WHERE s.card_id = c.id),
                   sc.due_day,
                   EXISTS(SELECT 1 FROM steps s JOIN step_stats ss ON ss.step_id = s.id
                          WHERE s.card_id = c.id AND ss.flagged = 1),
                   c.modified_at, c.tags_json
                 FROM cards c
                 JOIN categories cat ON cat.id = c.category_id
                 LEFT JOIN card_sched sc ON sc.card_id = c.id
                 WHERE c.deleted_at IS NULL
                   AND (?1 IS NULL OR c.title LIKE '%' || ?1 || '%')
                   AND (?2 IS NULL OR c.category_id = ?2)
                   AND (?3 IS NULL OR c.state = ?3)
                   AND (?4 = 0 OR (c.state = 'active' AND (sc.due_day IS NULL OR sc.due_day <= ?5)))
                   AND (?6 = 0 OR EXISTS(SELECT 1 FROM steps s JOIN step_stats ss ON ss.step_id = s.id
                                         WHERE s.card_id = c.id AND ss.flagged = 1))
                 ORDER BY c.modified_at DESC",
            )?;
            let rows = stmt.query_map(
                params![
                    f.text.as_deref().map(str::trim).filter(|t| !t.is_empty()),
                    f.category_id,
                    f.state.map(|s| s.as_str()),
                    f.due_only,
                    f.today.map(day_text),
                    f.flagged_only,
                ],
                |r| {
                    Ok(CardSummary {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        category_id: r.get(2)?,
                        category_name: r.get(3)?,
                        category_color: r.get(4)?,
                        difficulty: enum_col(r, 5)?,
                        state: enum_col(r, 6)?,
                        step_count: r.get(7)?,
                        due_day: opt_day_col(r, 8)?,
                        flagged: r.get(9)?,
                        modified_at: ts_col(r, 10)?,
                        tags: json_col(r, 11)?,
                    })
                },
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Soft delete; any in-progress attempt on the card is discarded.
    pub fn delete_card(&self, id: &str, now: DateTime<Utc>) -> Result<()> {
        self.with(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE cards SET deleted_at = ?2, modified_at = ?2 WHERE id = ?1",
                params![id, ts_text(now)],
            )?;
            tx.execute(
                "UPDATE attempts SET status = 'discarded', completed_at = ?2
                 WHERE card_id = ?1 AND status = 'in_progress'",
                params![id, ts_text(now)],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn set_card_state(
        &self,
        id: &str,
        state: CardState,
        now: DateTime<Utc>,
        device_id: &str,
    ) -> Result<()> {
        self.with(|conn| {
            let n = conn.execute(
                "UPDATE cards SET state = ?2, modified_at = ?3, modified_by = ?4
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![id, state.as_str(), ts_text(now), device_id],
            )?;
            if n == 0 {
                return Err(invalid(format!("no card {id}")));
            }
            Ok(())
        })
    }

    /// Shelf in introduction order: user-created before pack-imported, oldest first (§9.3).
    pub fn shelf_fifo(&self) -> Result<Vec<String>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM cards WHERE state = 'shelved' AND deleted_at IS NULL
                 ORDER BY (pack_id IS NOT NULL), created_at",
            )?;
            let ids = stmt.query_map([], |r| r.get::<_, String>(0))?;
            Ok(ids.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Active, live cards with their schedule (None = never reviewed), for the planner.
    pub fn planning_cards(&self) -> Result<Vec<(Card, Option<CardSched>)>> {
        let ids: Vec<String> = self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM cards WHERE state = 'active' AND deleted_at IS NULL ORDER BY created_at",
            )?;
            let ids = stmt.query_map([], |r| r.get::<_, String>(0))?;
            Ok(ids.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(card) = self.get_card(&id)? {
                let sched = self.card_sched(&id)?;
                out.push((card, sched));
            }
        }
        Ok(out)
    }

    /// `(card_id, step_id)` of flagged steps on active cards, in card order.
    pub fn flagged_steps(&self) -> Result<Vec<(String, String)>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT s.card_id, s.id FROM steps s
                 JOIN step_stats ss ON ss.step_id = s.id
                 JOIN cards c ON c.id = s.card_id
                 WHERE ss.flagged = 1 AND c.state = 'active' AND c.deleted_at IS NULL
                 ORDER BY c.created_at, s.position",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

fn load_steps(conn: &Connection, card_id: &str) -> Result<Vec<Step>> {
    let mut stmt = conn.prepare(
        "SELECT id, prompt_md, response, solution_md, solution_text, points
         FROM steps WHERE card_id = ?1 ORDER BY position",
    )?;
    let mut steps = stmt
        .query_map([card_id], |r| {
            Ok(Step {
                id: r.get(0)?,
                prompt_md: r.get(1)?,
                response: enum_col(r, 2)?,
                solution_md: r.get(3)?,
                solution_text: r.get(4)?,
                points: r.get(5)?,
                criteria: Vec::new(),
                media: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut cstmt =
        conn.prepare("SELECT id, text, weight FROM criteria WHERE step_id = ?1 ORDER BY position")?;
    let mut mstmt = conn.prepare(
        "SELECT sm.hash, m.ext, m.mime, sm.role, sm.caption
         FROM step_media sm JOIN media m ON m.hash = sm.hash
         WHERE sm.step_id = ?1 ORDER BY sm.position",
    )?;
    for step in &mut steps {
        step.criteria = cstmt
            .query_map([&step.id], |r| {
                Ok(Criterion {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    weight: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        step.media = mstmt
            .query_map([&step.id], |r| {
                Ok(MediaRef {
                    hash: r.get(0)?,
                    ext: r.get(1)?,
                    mime: r.get(2)?,
                    role: enum_col(r, 3)?,
                    caption: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(steps)
}

// ---------------------------------------------------------------- stats support

/// A flagged step with enough context for the Stats list (§7.5, §14).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct FlaggedStep {
    pub card_id: String,
    pub title: String,
    pub step_index: u32,
    pub flag_count: u32,
}

impl Db {
    pub fn reviews_all(&self) -> Result<Vec<Review>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, card_id, ts_utc, tz_offset_min, device_id, kind, grade, score,
                   steps_json, sched_json FROM reviews",
            )?;
            let rows = stmt.query_map([], review_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn card_scheds_all(&self) -> Result<Vec<CardSched>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT card_id, stability, difficulty, stability_fast, last_review_ts, due_day,
                   interval_days, reps, lapses, success_streak FROM card_sched",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(CardSched {
                    card_id: r.get(0)?,
                    stability: r.get(1)?,
                    difficulty: r.get(2)?,
                    stability_fast: r.get(3)?,
                    last_review_ts: ts_col(r, 4)?,
                    due_day: day_col(r, 5)?,
                    interval_days: r.get(6)?,
                    reps: r.get(7)?,
                    lapses: r.get(8)?,
                    success_streak: r.get(9)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn flagged_step_details(&self) -> Result<Vec<FlaggedStep>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.title, s.position, ss.flag_count FROM steps s
                 JOIN step_stats ss ON ss.step_id = s.id
                 JOIN cards c ON c.id = s.card_id
                 WHERE ss.flagged = 1 AND c.deleted_at IS NULL AND c.state != 'suspended'
                 ORDER BY ss.flag_count DESC, c.title, s.position",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(FlaggedStep {
                    card_id: r.get(0)?,
                    title: r.get(1)?,
                    step_index: r.get::<_, i64>(2)? as u32,
                    flag_count: r.get(3)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Hashes any step (of any card, deleted or not) still points at.
    pub fn referenced_media_hashes(&self) -> Result<std::collections::HashSet<String>> {
        self.with(|conn| {
            let mut stmt = conn.prepare("SELECT DISTINCT hash FROM step_media")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn media_all(&self) -> Result<Vec<MediaInfo>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT hash, mime, ext, bytes, width, height, duration_ms, created_at FROM media",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(MediaInfo {
                    hash: r.get(0)?,
                    mime: r.get(1)?,
                    ext: r.get(2)?,
                    bytes: r.get::<_, i64>(3)? as u64,
                    width: r.get(4)?,
                    height: r.get(5)?,
                    duration_ms: r.get::<_, Option<i64>>(6)?.map(|d| d as u64),
                    created_at: ts_col(r, 7)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn media_delete(&self, hash: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM media WHERE hash = ?1", [hash])?;
            Ok(())
        })
    }
}

// ---------------------------------------------------------------- attempts

impl Db {
    pub fn save_attempt(&self, a: &Attempt) -> Result<()> {
        self.with(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO attempts(id, card_id, status, started_at, completed_at, cram)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                   completed_at = excluded.completed_at",
                params![
                    a.id,
                    a.card_id,
                    a.status.as_str(),
                    ts_text(a.started_at),
                    opt_ts_text(a.completed_at),
                    a.cram
                ],
            )?;
            tx.execute("DELETE FROM attempt_steps WHERE attempt_id = ?1", [&a.id])?;
            for s in &a.steps {
                tx.execute(
                    "INSERT INTO attempt_steps(attempt_id, step_id, status, shown_at, elapsed_ms,
                       typed_answer, checked_json, grade4, score)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        a.id,
                        s.step_id,
                        s.status.as_str(),
                        opt_ts_text(s.shown_at),
                        s.elapsed_ms,
                        s.typed_answer,
                        serde_json::to_string(&s.checked)?,
                        s.grade4.map(|g| g.as_str()),
                        s.score
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn get_attempt(&self, id: &str) -> Result<Option<Attempt>> {
        self.with(|conn| {
            let attempt = conn
                .query_row(
                    "SELECT id, card_id, status, started_at, completed_at, cram FROM attempts WHERE id = ?1",
                    [id],
                    attempt_row,
                )
                .optional()?;
            let Some(mut attempt) = attempt else {
                return Ok(None);
            };
            attempt.steps = load_attempt_steps(conn, id)?;
            Ok(Some(attempt))
        })
    }

    pub fn attempts_in_progress(&self) -> Result<Vec<Attempt>> {
        let heads: Vec<Attempt> = self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT a.id, a.card_id, a.status, a.started_at, a.completed_at, a.cram FROM attempts a
                 JOIN cards c ON c.id = a.card_id
                 WHERE a.status = 'in_progress' AND c.deleted_at IS NULL
                 ORDER BY a.started_at",
            )?;
            let rows = stmt.query_map([], attempt_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut out = Vec::with_capacity(heads.len());
        for mut a in heads {
            a.steps = self.with(|conn| load_attempt_steps(conn, &a.id))?;
            out.push(a);
        }
        Ok(out)
    }

    /// The unfinished attempt of this card in this mode (a cram run never resumes a real one).
    pub fn attempt_in_progress_for_card(
        &self,
        card_id: &str,
        cram: bool,
    ) -> Result<Option<Attempt>> {
        Ok(self
            .attempts_in_progress()?
            .into_iter()
            .find(|a| a.card_id == card_id && a.cram == cram))
    }
}

fn attempt_row(r: &rusqlite::Row) -> rusqlite::Result<Attempt> {
    Ok(Attempt {
        id: r.get(0)?,
        card_id: r.get(1)?,
        status: enum_col(r, 2)?,
        started_at: ts_col(r, 3)?,
        completed_at: opt_ts_col(r, 4)?,
        steps: Vec::new(),
        cram: r.get(5)?,
    })
}

fn load_attempt_steps(conn: &Connection, attempt_id: &str) -> Result<Vec<AttemptStep>> {
    let mut stmt = conn.prepare(
        "SELECT step_id, status, shown_at, elapsed_ms, typed_answer, checked_json, grade4, score
         FROM attempt_steps WHERE attempt_id = ?1 ORDER BY rowid",
    )?;
    let rows = stmt.query_map([attempt_id], |r| {
        Ok(AttemptStep {
            step_id: r.get(0)?,
            status: enum_col(r, 1)?,
            shown_at: opt_ts_col(r, 2)?,
            elapsed_ms: r.get(3)?,
            typed_answer: r.get(4)?,
            checked: json_col(r, 5)?,
            grade4: opt_enum_col(r, 6)?,
            score: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------- reviews and caches

impl Db {
    pub fn insert_review(&self, r: &Review) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "INSERT INTO reviews(id, card_id, ts_utc, tz_offset_min, device_id, kind, grade,
                   score, steps_json, sched_json, exported)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
                params![
                    r.id,
                    r.card_id,
                    ts_text(r.ts_utc),
                    r.tz_offset_min,
                    r.device_id,
                    r.kind.as_str(),
                    r.grade.as_str(),
                    r.score,
                    serde_json::to_string(&r.steps)?,
                    serde_json::to_string(&r.sched)?,
                ],
            )?;
            Ok(())
        })
    }

    /// All reviews of a card in insertion order; `replay` sorts them itself.
    pub fn reviews_for_card(&self, card_id: &str) -> Result<Vec<Review>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, card_id, ts_utc, tz_offset_min, device_id, kind, grade, score,
                   steps_json, sched_json FROM reviews WHERE card_id = ?1",
            )?;
            let rows = stmt.query_map([card_id], review_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn card_sched(&self, card_id: &str) -> Result<Option<CardSched>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT card_id, stability, difficulty, stability_fast, last_review_ts, due_day,
                       interval_days, reps, lapses, success_streak FROM card_sched WHERE card_id = ?1",
                    [card_id],
                    |r| {
                        Ok(CardSched {
                            card_id: r.get(0)?,
                            stability: r.get(1)?,
                            difficulty: r.get(2)?,
                            stability_fast: r.get(3)?,
                            last_review_ts: ts_col(r, 4)?,
                            due_day: day_col(r, 5)?,
                            interval_days: r.get(6)?,
                            reps: r.get(7)?,
                            lapses: r.get(8)?,
                            success_streak: r.get(9)?,
                        })
                    },
                )
                .optional()?)
        })
    }

    /// Stats for the card's current steps, in step order (missing rows = fresh stats).
    pub fn step_stats(&self, card_id: &str) -> Result<Vec<StepStats>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT s.id, t.recent_scores_json, t.flagged, t.flag_count
                 FROM steps s LEFT JOIN step_stats t ON t.step_id = s.id
                 WHERE s.card_id = ?1 ORDER BY s.position",
            )?;
            let rows = stmt.query_map([card_id], |r| {
                let step_id: String = r.get(0)?;
                let scores: Option<String> = r.get(1)?;
                Ok(match scores {
                    Some(_) => StepStats {
                        step_id,
                        recent_scores: json_col(r, 1)?,
                        flagged: r.get::<_, Option<bool>>(2)?.unwrap_or(false),
                        flag_count: r.get::<_, Option<u32>>(3)?.unwrap_or(0),
                    },
                    None => StepStats::new(step_id),
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Writes both caches in one transaction.
    pub fn save_replay(&self, card_id: &str, state: &ReplayState) -> Result<()> {
        self.with(|conn| {
            let tx = conn.transaction()?;
            match &state.sched {
                Some(s) => {
                    tx.execute(
                        "INSERT INTO card_sched(card_id, stability, difficulty, stability_fast,
                           last_review_ts, due_day, interval_days, reps, lapses, success_streak)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                         ON CONFLICT(card_id) DO UPDATE SET stability = excluded.stability,
                           difficulty = excluded.difficulty, stability_fast = excluded.stability_fast,
                           last_review_ts = excluded.last_review_ts, due_day = excluded.due_day,
                           interval_days = excluded.interval_days, reps = excluded.reps,
                           lapses = excluded.lapses, success_streak = excluded.success_streak",
                        params![
                            s.card_id,
                            s.stability,
                            s.difficulty,
                            s.stability_fast,
                            ts_text(s.last_review_ts),
                            day_text(s.due_day),
                            s.interval_days,
                            s.reps,
                            s.lapses,
                            s.success_streak
                        ],
                    )?;
                }
                None => {
                    tx.execute("DELETE FROM card_sched WHERE card_id = ?1", [card_id])?;
                }
            }
            for st in &state.steps {
                tx.execute(
                    "INSERT INTO step_stats(step_id, recent_scores_json, flagged, flag_count)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(step_id) DO UPDATE SET
                       recent_scores_json = excluded.recent_scores_json,
                       flagged = excluded.flagged, flag_count = excluded.flag_count",
                    params![
                        st.step_id,
                        serde_json::to_string(&st.recent_scores)?,
                        st.flagged,
                        st.flag_count
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Load reviews and steps, replay, write the caches (SPEC §7.6).
    pub fn replay_card(&self, card_id: &str, cfg: &SchedConfig) -> Result<ReplayState> {
        let step_ids: Vec<String> = self.with(|conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM steps WHERE card_id = ?1 ORDER BY position")?;
            let ids = stmt.query_map([card_id], |r| r.get::<_, String>(0))?;
            Ok(ids.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let reviews = self.reviews_for_card(card_id)?;
        let state = replay::replay(&step_ids, &reviews, cfg)?;
        self.save_replay(card_id, &state)?;
        Ok(state)
    }
}

fn review_row(r: &rusqlite::Row) -> rusqlite::Result<Review> {
    Ok(Review {
        id: r.get(0)?,
        card_id: r.get(1)?,
        ts_utc: ts_col(r, 2)?,
        tz_offset_min: r.get(3)?,
        device_id: r.get(4)?,
        kind: enum_col(r, 5)?,
        grade: enum_col(r, 6)?,
        score: r.get(7)?,
        steps: json_col(r, 8)?,
        sched: json_col(r, 9)?,
    })
}

// ---------------------------------------------------------------- vault, backup, packs

impl Db {
    pub fn sync_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.with(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT value_json FROM sync_state WHERE key = ?1",
                    [key],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
        })
    }

    pub fn sync_set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.with(|conn| {
            conn.execute(
                "INSERT INTO sync_state(key, value_json) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
                params![key, json],
            )?;
            Ok(())
        })
    }

    /// Every category, soft-deleted ones included (tombstones travel too).
    pub fn categories_all(&self) -> Result<Vec<Category>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {CATEGORY_COLS} FROM categories ORDER BY position, name"
            ))?;
            let rows = stmt.query_map([], category_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Every card id, soft-deleted ones included.
    pub fn card_ids_all(&self) -> Result<Vec<String>> {
        self.with(|conn| {
            let mut stmt = conn.prepare("SELECT id FROM cards ORDER BY created_at")?;
            let ids = stmt.query_map([], |r| r.get::<_, String>(0))?;
            Ok(ids.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn card_exported_at(&self, id: &str) -> Result<Option<DateTime<Utc>>> {
        self.with(|conn| {
            let ts: Option<Option<String>> = conn
                .query_row("SELECT exported_at FROM cards WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            ts.flatten().map(|s| super::parse_ts(&s)).transpose()
        })
    }

    /// Inserts a review written elsewhere (another device's vault file or a backup). Returns
    /// false when it is already known. `exported = 1`: this device never re-writes it.
    pub fn insert_foreign_review(&self, r: &Review) -> Result<bool> {
        self.with(|conn| {
            let n = conn.execute(
                "INSERT OR IGNORE INTO reviews(id, card_id, ts_utc, tz_offset_min, device_id, kind,
                   grade, score, steps_json, sched_json, exported)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1)",
                params![
                    r.id,
                    r.card_id,
                    ts_text(r.ts_utc),
                    r.tz_offset_min,
                    r.device_id,
                    r.kind.as_str(),
                    r.grade.as_str(),
                    r.score,
                    serde_json::to_string(&r.steps)?,
                    serde_json::to_string(&r.sched)?,
                ],
            )?;
            Ok(n == 1)
        })
    }

    pub fn insert_conflict(
        &self,
        entity: &str,
        entity_id: &str,
        losing_json: &str,
        losing_device: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "INSERT INTO conflicts(id, entity, entity_id, detected_at, losing_json, losing_device)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    uuid::Uuid::now_v7().to_string(),
                    entity,
                    entity_id,
                    ts_text(now),
                    losing_json,
                    losing_device
                ],
            )?;
            Ok(())
        })
    }

    /// True before the first card or review exists: a backup may then restore settings too.
    pub fn is_empty_profile(&self) -> Result<bool> {
        self.with(|conn| {
            Ok(conn.query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM cards) AND NOT EXISTS(SELECT 1 FROM reviews)",
                [],
                |r| r.get(0),
            )?)
        })
    }

    /// The live card a pack card was imported as (or, for one's own export, the original).
    pub fn card_for_pack_card(&self, pack_card_id: &str) -> Result<Option<String>> {
        self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT id FROM cards WHERE deleted_at IS NULL
                       AND (pack_card_id = ?1 OR id = ?1) ORDER BY (id = ?1) LIMIT 1",
                    [pack_card_id],
                    |r| r.get(0),
                )
                .optional()?)
        })
    }
}

// ---------------------------------------------------------------- sync

impl Db {
    /// Cards whose current version has not been written to the vault yet.
    pub fn cards_to_export(&self) -> Result<Vec<String>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM cards WHERE exported_at IS NULL OR exported_at <> modified_at",
            )?;
            let ids = stmt.query_map([], |r| r.get::<_, String>(0))?;
            Ok(ids.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Records which version was exported: `exported_at` = that version's `modified_at`.
    pub fn set_card_exported(&self, id: &str, version: DateTime<Utc>) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "UPDATE cards SET exported_at = ?2 WHERE id = ?1",
                params![id, ts_text(version)],
            )?;
            Ok(())
        })
    }

    pub fn unexported_reviews(&self, device_id: &str) -> Result<Vec<Review>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, card_id, ts_utc, tz_offset_min, device_id, kind, grade, score,
                   steps_json, sched_json FROM reviews
                 WHERE exported = 0 AND device_id = ?1 ORDER BY ts_utc, id",
            )?;
            let rows = stmt.query_map([device_id], review_row)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn mark_reviews_exported(&self, ids: &[String]) -> Result<()> {
        self.with(|conn| {
            let tx = conn.transaction()?;
            for id in ids {
                tx.execute("UPDATE reviews SET exported = 1 WHERE id = ?1", [id])?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// `(id, entity, entity_id, detected_at, losing_json, losing_device)`, newest first.
    #[allow(clippy::type_complexity)]
    pub fn conflicts_all(
        &self,
    ) -> Result<Vec<(String, String, String, DateTime<Utc>, String, String)>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, entity, entity_id, detected_at, losing_json, losing_device
                 FROM conflicts ORDER BY detected_at DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    ts_col(r, 3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn conflict_delete(&self, id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM conflicts WHERE id = ?1", [id])?;
            Ok(())
        })
    }
}

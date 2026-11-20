//! `.indigopack` (SPEC §9.4): shareable cards without reviews or scheduling. Packs come from
//! strangers, so everything is validated: strict JSON, ids, sizes, media type and hash.
//!
//! A pack card's id is the original card's UUID, which stays globally unique across
//! re-exports; that is what a re-import matches on.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::authoring;
use crate::db::Db;
use crate::domain::{Card, CardState, Category, Difficulty, Step};
use crate::error::invalid;
use crate::media;
use crate::sched::SchedConfig;
use crate::vault::{self, MAX_ITEMS};
use crate::zipsafe::{MAX_FILE_BYTES, SafeZip, ZipOut};

pub const FORMAT: u32 = 1;
pub const PACK_JSON: &str = "pack.json";
const MAX_STEPS: usize = 100;
const MAX_CRITERIA: usize = 50;
const MAX_MEDIA_PER_STEP: usize = 20;
const MAX_TAGS: usize = 50;
const MAX_SHORT: usize = 500;
const MAX_TEXT: usize = 200_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct PackMeta {
    pub format: u32,
    pub pack_id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

/// `cards/<id>.json`. The category travels by name; ids are meaningless elsewhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackCard {
    pub id: String,
    pub title: String,
    pub category: String,
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
    pub est_minutes: Option<u32>,
    pub source: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackPreviewCard {
    pub pack_card_id: String,
    pub title: String,
    pub category: String,
    /// Already in the library: import skips it unless it is listed for update.
    pub exists: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackPreview {
    pub meta: PackMeta,
    pub cards: Vec<PackPreviewCard>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackReport {
    pub added: u32,
    pub updated: u32,
    pub skipped: u32,
}

fn too_long(s: &str, max: usize) -> bool {
    s.chars().count() > max
}

fn check(c: &PackCard) -> Result<()> {
    let bad = |what: &str| Err(invalid(format!("Pack card {:?}: {what}", c.title)));
    if !vault::valid_id(&c.id) {
        return bad("invalid id");
    }
    if too_long(&c.title, MAX_SHORT) || too_long(&c.category, MAX_SHORT) {
        return bad("title or category too long");
    }
    if c.tags.len() > MAX_TAGS || c.tags.iter().any(|t| too_long(t, MAX_SHORT)) {
        return bad("too many or too long tags");
    }
    if c.source.as_deref().is_some_and(|s| too_long(s, MAX_TEXT)) {
        return bad("source too long");
    }
    if c.steps.is_empty() || c.steps.len() > MAX_STEPS {
        return bad("wrong number of steps");
    }
    for s in &c.steps {
        let texts = [&s.prompt_md, &s.solution_md];
        if texts.iter().any(|t| too_long(t, MAX_TEXT))
            || s.solution_text
                .as_deref()
                .is_some_and(|t| too_long(t, MAX_TEXT))
            || s.criteria.len() > MAX_CRITERIA
            || s.criteria
                .iter()
                .any(|cr| too_long(&cr.text, MAX_SHORT * 4))
            || s.media.len() > MAX_MEDIA_PER_STEP
        {
            return bad("a step is too large");
        }
        for m in &s.media {
            vault::check_media_ref(m)?;
            if m.caption.as_deref().is_some_and(|t| too_long(t, MAX_SHORT)) {
                return bad("caption too long");
            }
        }
    }
    Ok(())
}

fn media_entry(hash: &str, ext: &str) -> String {
    format!("media/{hash}.{ext}")
}

/// Exports the given cards (a selection, or a whole category selected in the Library).
pub fn export(
    db: &Db,
    media_dir: &Path,
    card_ids: &[String],
    name: &str,
    out: &Path,
    now: DateTime<Utc>,
) -> Result<u32> {
    let categories: Vec<Category> = db.categories_all()?;
    let mut z = ZipOut::create(out)?;
    z.json(
        PACK_JSON,
        &PackMeta {
            format: FORMAT,
            pack_id: uuid::Uuid::now_v7().to_string(),
            name: name.trim().to_string(),
            author: String::new(),
            description: String::new(),
            created_at: now,
        },
    )?;
    let mut written_media: BTreeSet<String> = BTreeSet::new();
    let mut n = 0;
    for id in card_ids {
        let Some(card) = db.get_card(id)?.filter(|c| c.deleted_at.is_none()) else {
            continue;
        };
        let pc = PackCard {
            id: card.pack_card_id.clone().unwrap_or(card.id.clone()),
            title: card.title,
            category: categories
                .iter()
                .find(|c| c.id == card.category_id)
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            difficulty: card.difficulty,
            tags: card.tags,
            est_minutes: card.est_minutes,
            source: card.source,
            steps: card.steps,
        };
        for m in pc.steps.iter().flat_map(|s| &s.media) {
            if written_media.insert(m.hash.clone()) {
                z.file(
                    &media_entry(&m.hash, &m.ext),
                    &media_dir.join(media::rel_path(&m.hash, &m.ext)),
                )?;
            }
        }
        z.json(&vault::card_path(&pc.id), &pc)?;
        n += 1;
    }
    z.finish()?;
    Ok(n)
}

fn read_pack(path: &Path) -> Result<(SafeZip, PackMeta, Vec<PackCard>)> {
    let mut z = SafeZip::open(path)?;
    if !z.has(PACK_JSON) {
        return Err(invalid("This file is not an Indigo pack"));
    }
    let meta: PackMeta = z.read_json(PACK_JSON)?;
    if meta.format > FORMAT {
        return Err(invalid("This pack needs a newer version of Indigo"));
    }
    if !vault::valid_id(&meta.pack_id) || too_long(&meta.name, MAX_SHORT) {
        return Err(invalid("pack.json is invalid"));
    }
    let names: Vec<String> = z
        .names()
        .iter()
        .filter(|n| n.starts_with("cards/") && n.ends_with(".json"))
        .cloned()
        .collect();
    if names.len() > MAX_ITEMS {
        return Err(invalid("This pack holds too many cards"));
    }
    let mut cards = Vec::with_capacity(names.len());
    for name in names {
        let c: PackCard = z.read_json(&name)?;
        check(&c)?;
        cards.push(c);
    }
    Ok((z, meta, cards))
}

/// What the import dialog shows: every card and whether it is already in the library.
pub fn inspect(db: &Db, path: &Path) -> Result<PackPreview> {
    let (_, meta, cards) = read_pack(path)?;
    let cards = cards
        .into_iter()
        .map(|c| {
            Ok(PackPreviewCard {
                exists: db.card_for_pack_card(&c.id)?.is_some(),
                pack_card_id: c.id,
                title: c.title,
                category: c.category,
            })
        })
        .collect::<Result<_>>()?;
    Ok(PackPreview { meta, cards })
}

/// The live category with this name, reviving or creating it as needed.
fn category_for(db: &Db, name: &str, now: DateTime<Utc>) -> Result<String> {
    let name = if name.trim().is_empty() {
        "Imported"
    } else {
        name
    };
    if let Some(c) = db.category_by_name(name)?
        && c.deleted_at.is_none()
    {
        return Ok(c.id);
    }
    let cat = Category {
        id: String::new(),
        name: name.to_string(),
        syntax: None,
        color: None,
        position: 0,
        modified_at: now,
        deleted_at: None,
    };
    Ok(authoring::save_category(db, cat, now)?.id)
}

/// New cards go to the shelf; cards already present are updated only when listed in
/// `update`, keeping their schedule, state and local step ids (matched by position).
pub fn import(
    db: &Db,
    media_dir: &Path,
    path: &Path,
    update: &[String],
    now: DateTime<Utc>,
    device_id: &str,
    cfg: &SchedConfig,
) -> Result<PackReport> {
    let (mut z, meta, cards) = read_pack(path)?;
    let mut report = PackReport::default();
    for pc in cards {
        let existing = db.card_for_pack_card(&pc.id)?;
        if existing.is_some() && !update.contains(&pc.id) {
            report.skipped += 1;
            continue;
        }
        let mut steps = pc.steps;
        for m in steps.iter_mut().flat_map(|s| s.media.iter_mut()) {
            let entry = media_entry(&m.hash, &m.ext);
            if !z.has(&entry) {
                return Err(invalid(format!("The pack is missing {entry}")));
            }
            let bytes = z.read(&entry, MAX_FILE_BYTES)?;
            let info = media::import(db, media_dir, &bytes, now)?;
            if info.hash != m.hash {
                return Err(invalid(format!(
                    "Pack media {} does not match its content",
                    m.hash
                )));
            }
            m.ext = info.ext;
            m.mime = info.mime;
        }
        let category_id = category_for(db, &pc.category, now)?;
        let local = match &existing {
            Some(id) => db.get_card(id)?,
            None => None,
        };
        let local_step_ids: Vec<String> = local
            .as_ref()
            .map(|c| c.steps.iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default();
        for (i, s) in steps.iter_mut().enumerate() {
            s.id = local_step_ids.get(i).cloned().unwrap_or_default();
            for c in &mut s.criteria {
                c.id = String::new();
            }
        }
        let card = Card {
            title: pc.title,
            category_id,
            difficulty: pc.difficulty,
            tags: pc.tags,
            est_minutes: pc.est_minutes,
            source: pc.source,
            steps,
            ..local.clone().unwrap_or(Card {
                id: String::new(),
                title: String::new(),
                category_id: String::new(),
                difficulty: Difficulty::Beginner,
                tags: Vec::new(),
                est_minutes: None,
                steps: Vec::new(),
                source: None,
                pack_id: Some(meta.pack_id.clone()),
                pack_card_id: Some(pc.id.clone()),
                state: CardState::Shelved,
                created_at: now,
                modified_at: now,
                modified_by: String::new(),
                deleted_at: None,
            })
        };
        authoring::save_card(db, card, now, device_id, cfg, false)?;
        if local.is_some() {
            report.updated += 1;
        } else {
            report.added += 1;
        }
    }
    Ok(report)
}

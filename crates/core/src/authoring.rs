//! Authoring orchestration (SPEC §9): ids, validation, replay after structural edits,
//! editor memory. Repos do the SQL. Categories are only ever created by the user (D-18).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::domain::{Card, CardState, Category};
use crate::error::invalid;
use crate::sched::SchedConfig;
use crate::settings::KEY_SETTINGS;
use crate::syntax::known_syntax;
use crate::warnings::card_warnings;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SaveResult {
    pub card: Card,
    pub warnings: Vec<String>,
}

fn ensure_id(id: &mut String) {
    if id.trim().is_empty() {
        *id = uuid::Uuid::now_v7().to_string();
    }
}

/// Validates, assigns ids, stores, and replays the card (its steps may have changed). With
/// `remember` (only the Add dialog passes it) the card's fields seed the next new card;
/// bulk edits never touch that memory.
pub fn save_card(
    db: &Db,
    mut card: Card,
    now: DateTime<Utc>,
    device_id: &str,
    cfg: &SchedConfig,
    remember: bool,
) -> Result<SaveResult> {
    card.title = card.title.trim().to_string();
    if card.title.is_empty() {
        return Err(invalid("Give the card a title"));
    }
    match db.get_category(&card.category_id)? {
        Some(c) if c.deleted_at.is_none() => {}
        _ => return Err(invalid("Pick a category")),
    }
    if card.steps.is_empty() {
        return Err(invalid("A card needs at least one step"));
    }
    for (i, step) in card.steps.iter_mut().enumerate() {
        if step.prompt_md.trim().is_empty()
            && !step
                .media
                .iter()
                .any(|m| m.role == crate::domain::MediaRole::Prompt)
        {
            return Err(invalid(format!("Step {} needs a prompt", i + 1)));
        }
        if !step.points.is_finite() || step.points <= 0.0 {
            return Err(invalid(format!("Step {} needs points above zero", i + 1)));
        }
        ensure_id(&mut step.id);
        step.criteria.retain(|c| !c.text.trim().is_empty());
        for c in &mut step.criteria {
            ensure_id(&mut c.id);
            if !c.weight.is_finite() || c.weight <= 0.0 {
                return Err(invalid(format!(
                    "Step {}: criterion weights must be above zero",
                    i + 1
                )));
            }
        }
    }
    let is_new = card.id.trim().is_empty() || db.get_card(&card.id)?.is_none();
    ensure_id(&mut card.id);
    if is_new {
        card.created_at = now;
        card.state = CardState::Shelved;
    }
    card.modified_at = now;
    card.modified_by = device_id.to_string();
    card.deleted_at = None;
    db.save_card(&card)?;
    db.replay_card(&card.id, cfg)?;

    if remember {
        let mut settings = db.settings()?;
        settings.card_defaults.remember(&card);
        db.setting_set(KEY_SETTINGS, &settings)?;
    }

    let warnings = card_warnings(&card);
    Ok(SaveResult { card, warnings })
}

/// Creates or renames a category. A new category whose name matches a known language gets
/// that syntax; a deleted category with the same name is revived instead of duplicated.
pub fn save_category(db: &Db, mut cat: Category, now: DateTime<Utc>) -> Result<Category> {
    cat.name = cat.name.trim().to_string();
    if cat.name.is_empty() {
        return Err(invalid("Give the category a name"));
    }
    let existing = if cat.id.trim().is_empty() {
        None
    } else {
        db.get_category(&cat.id)?
    };
    if existing.is_none() {
        if let Some(same_name) = db.category_by_name(&cat.name)? {
            if same_name.deleted_at.is_none() {
                return Err(invalid(format!(
                    "A category named {} already exists",
                    cat.name
                )));
            }
            cat.id = same_name.id;
        }
        if cat.syntax.is_none() {
            cat.syntax = known_syntax(&cat.name).map(str::to_string);
        }
        if cat.position == 0 {
            cat.position = db.categories_list()?.len() as i64;
        }
    } else if let Some(other) = db.category_by_name(&cat.name)?
        && other.id != cat.id
        && other.deleted_at.is_none()
    {
        return Err(invalid(format!(
            "A category named {} already exists",
            cat.name
        )));
    }
    ensure_id(&mut cat.id);
    cat.syntax = cat.syntax.filter(|s| !s.trim().is_empty());
    cat.modified_at = now;
    cat.deleted_at = None;
    db.save_category(&cat)?;
    Ok(cat)
}

/// Renames a tag on every card that has it (onto an existing tag = merge). Each changed card is
/// stamped so the edit syncs. Returns how many cards changed.
pub fn rename_tag(
    db: &Db,
    from: &str,
    to: &str,
    now: DateTime<Utc>,
    device_id: &str,
) -> Result<u32> {
    let to = to.trim();
    if to.is_empty() {
        return Err(invalid("A tag needs a name"));
    }
    retag(db, from, Some(to), now, device_id)
}

/// Removes a tag from every card that has it. Returns how many cards changed.
pub fn delete_tag(db: &Db, tag: &str, now: DateTime<Utc>, device_id: &str) -> Result<u32> {
    retag(db, tag, None, now, device_id)
}

fn retag(
    db: &Db,
    from: &str,
    to: Option<&str>,
    now: DateTime<Utc>,
    device_id: &str,
) -> Result<u32> {
    let mut changed = 0;
    for id in db.card_ids_all()? {
        let Some(mut card) = db.get_card(&id)? else {
            continue;
        };
        if card.deleted_at.is_some() || !card.tags.iter().any(|t| t == from) {
            continue;
        }
        let mut tags: Vec<String> = Vec::with_capacity(card.tags.len());
        for t in card
            .tags
            .iter()
            .map(|t| if t == from { to } else { Some(t.as_str()) })
        {
            if let Some(t) = t
                && !tags.iter().any(|x| x == t)
            {
                tags.push(t.to_string());
            }
        }
        card.tags = tags;
        card.modified_at = now;
        card.modified_by = device_id.to_string();
        db.save_card(&card)?;
        changed += 1;
    }
    Ok(changed)
}

#[cfg(test)]
mod tag_tests {
    use super::*;
    use crate::domain::{Difficulty, Response, Step};

    #[test]
    fn rename_merges_and_delete_removes() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("i.sqlite")).unwrap();
        let now: DateTime<Utc> = "2026-05-01T08:00:00Z".parse().unwrap();
        let cat = save_category(
            &db,
            Category {
                id: String::new(),
                name: "Blender".into(),
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
        let card = |tags: &[&str]| Card {
            id: String::new(),
            title: "c".into(),
            category_id: cat.clone(),
            difficulty: Difficulty::Beginner,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            est_minutes: None,
            steps: vec![Step {
                id: String::new(),
                prompt_md: "p".into(),
                response: Response::Recall,
                solution_md: "a".into(),
                solution_text: None,
                criteria: vec![],
                points: 1.0,
                media: vec![],
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
        let cfg = SchedConfig::default();
        let a = save_card(&db, card(&["model", "uv"]), now, "d", &cfg, false)
            .unwrap()
            .card;
        let b = save_card(&db, card(&["rig"]), now, "d", &cfg, false)
            .unwrap()
            .card;
        let later = now + chrono::Duration::minutes(1);

        assert_eq!(
            rename_tag(&db, "uv", "model", later, "d").unwrap(),
            1,
            "merge"
        );
        let a2 = db.get_card(&a.id).unwrap().unwrap();
        assert_eq!(a2.tags, vec!["model".to_string()]);
        assert_eq!(a2.modified_at, later, "the edit syncs");
        assert_eq!(
            db.get_card(&b.id).unwrap().unwrap().modified_at,
            now,
            "untouched"
        );
        assert!(rename_tag(&db, "rig", "  ", later, "d").is_err());

        assert_eq!(delete_tag(&db, "model", later, "d").unwrap(), 1);
        assert!(db.get_card(&a.id).unwrap().unwrap().tags.is_empty());
    }
}

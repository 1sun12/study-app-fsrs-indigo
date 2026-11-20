//! Device-local settings (SPEC §6 `settings`, §14). Every field has a default (D-11).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::domain::{Card, CardState, Difficulty, Response};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct MediaLimits {
    pub image_mb: u32,
    pub audio_mb: u32,
    pub video_mb: u32,
}

impl Default for MediaLimits {
    fn default() -> Self {
        MediaLimits {
            image_mb: 10,
            audio_mb: 25,
            video_mb: 90,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct EditorSettings {
    pub vim: bool,
    pub tab_width: u32,
}

impl Default for EditorSettings {
    fn default() -> Self {
        EditorSettings {
            vim: false,
            tab_width: 4,
        }
    }
}

/// Sync repository (SPEC §11.4, owner ruling 2026-11-12): the URL of a private GitHub repo.
/// Never credentials; git's own SSH agent or credential helper signs in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct SyncSettings {
    /// e.g. `https://github.com/you/indigo-vault.git` or `git@github.com:you/indigo-vault.git`.
    pub repo_url: String,
}

impl SyncSettings {
    /// The repository URL, or None when unset or unsafe to pass to git.
    pub fn remote(&self) -> Option<String> {
        let url = self.repo_url.trim();
        crate::git::valid_url(url).then(|| url.to_string())
    }
}

/// Fields a new card starts with, taken from the last card added (owner ruling 2026-11-12:
/// several cards in a row should only need a title and a prompt). Off = plain defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct CardDefaults {
    pub enabled: bool,
    pub category_id: Option<String>,
    pub difficulty: Difficulty,
    pub response: Response,
    pub tags: Vec<String>,
    pub est_minutes: Option<u32>,
    pub source: Option<String>,
}

impl Default for CardDefaults {
    fn default() -> Self {
        CardDefaults {
            enabled: true,
            category_id: None,
            difficulty: Difficulty::Beginner,
            response: Response::Recall,
            tags: Vec::new(),
            est_minutes: None,
            source: None,
        }
    }
}

impl CardDefaults {
    /// Remembers the fields of a card that was just added (no-op when disabled).
    pub fn remember(&mut self, card: &Card) {
        if !self.enabled {
            return;
        }
        self.category_id = Some(card.category_id.clone());
        self.difficulty = card.difficulty;
        self.response = card.steps.first().map_or(Response::Recall, |s| s.response);
        self.tags = card.tags.clone();
        self.est_minutes = card.est_minutes;
        self.source = card.source.clone();
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct Settings {
    /// Soft session budget (§8.1).
    pub daily_minutes: u32,
    /// Shelf cards introduced per day (§9.3, owner default 20).
    pub new_per_day: u32,
    /// Editor memory (§9.1): what the next new card starts with.
    pub card_defaults: CardDefaults,
    pub media_limits: MediaLimits,
    pub theme: Theme,
    /// Minimal UI mode: sidebar locked collapsed, helper text hidden (owner ruling 2026-11-12).
    pub minimal_ui: bool,
    pub editor: EditorSettings,
    pub sync: SyncSettings,
    /// Tags created in Settings → Tags & filters before any card uses them (a tag on a card
    /// syncs with the card). shortcut: device-local, so an unused tag does not reach other
    /// devices; give tags their own vault file if that ever matters.
    pub tags: Vec<String>,
    /// The user's named Library filters (device-local, like all settings).
    pub saved_filters: Vec<SavedFilter>,
}

/// A named Library filter. Empty fields match everything.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct SavedFilter {
    pub id: String,
    pub name: String,
    pub text: Option<String>,
    pub category_id: Option<String>,
    pub state: Option<CardState>,
    pub difficulty: Option<Difficulty>,
    pub tag: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            daily_minutes: 30,
            new_per_day: 20,
            card_defaults: CardDefaults::default(),
            media_limits: MediaLimits::default(),
            theme: Theme::System,
            minimal_ui: false,
            editor: EditorSettings::default(),
            sync: SyncSettings::default(),
            tags: Vec::new(),
            saved_filters: Vec::new(),
        }
    }
}

/// How many shelf cards were introduced on which local day (device-local, §9.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Introduction {
    pub day: NaiveDate,
    pub count: u32,
}

pub const KEY_SETTINGS: &str = "settings";
pub const KEY_SCHED_CONFIG: &str = "sched_config";
pub const KEY_INTRODUCTION: &str = "introduction";

/// File the installer may leave in the data directory with the repository it set up (it cannot
/// write the database). Adopted once, only if no remote is configured yet, then removed.
pub const SYNC_SETUP_FILE: &str = "sync-setup.json";

pub fn adopt_sync_setup(db: &crate::db::Db, data_dir: &std::path::Path) -> crate::Result<bool> {
    let path = data_dir.join(SYNC_SETUP_FILE);
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(false);
    };
    let _ = std::fs::remove_file(&path);
    let Ok(setup) = serde_json::from_slice::<SyncSettings>(&bytes) else {
        return Ok(false);
    };
    let mut settings = db.settings()?;
    if settings.sync.remote().is_some() || setup.remote().is_none() {
        return Ok(false);
    }
    settings.sync = setup;
    db.setting_set(KEY_SETTINGS, &settings)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::SyncSettings;

    #[test]
    fn installer_sync_setup_is_adopted_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Db::open(&dir.path().join("i.sqlite")).unwrap();
        let file = dir.path().join(super::SYNC_SETUP_FILE);
        std::fs::write(&file, br#"{"repo_url":"--upload-pack=x"}"#).unwrap();
        assert!(!super::adopt_sync_setup(&db, dir.path()).unwrap());
        assert!(!file.exists(), "consumed either way");
        let url = "https://github.com/me/indigo-vault.git";
        std::fs::write(&file, format!(r#"{{"repo_url":"{url}"}}"#)).unwrap();
        assert!(super::adopt_sync_setup(&db, dir.path()).unwrap());
        assert_eq!(db.settings().unwrap().sync.remote().as_deref(), Some(url));
        std::fs::write(&file, br#"{"repo_url":"git@github.com:other/x.git"}"#).unwrap();
        assert!(
            !super::adopt_sync_setup(&db, dir.path()).unwrap(),
            "never overrides"
        );
    }

    #[test]
    fn old_rclone_settings_read_as_unconfigured() {
        let old: SyncSettings =
            serde_json::from_str(r#"{"remote_name":"gdrive","path":"Indigo","rclone_path":""}"#)
                .unwrap();
        assert_eq!(old.remote(), None);
    }
}

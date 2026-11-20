//! Domain types (SPEC §5). IDs are UUIDv7 strings, timestamps UTC, days local calendar dates.
//! Every type that crosses the IPC boundary derives `TS`; `npm run bindings` exports them.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Enum stored as plain text in SQLite and as snake_case in JSON.
macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
        #[ts(export)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> std::result::Result<Self, String> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    other => Err(format!("unknown {}: {other}", stringify!($name))),
                }
            }
        }
    };
}

text_enum!(Grade { Again => "again", Hard => "hard", Good => "good", Easy => "easy" });
text_enum!(Difficulty {
    Beginner => "beginner",
    Intermediate => "intermediate",
    Advanced => "advanced",
    Expert => "expert",
    Master => "master",
});
/// How a step is answered: typed into the app, or recalled. The old "tool" (do it, then tick a
/// checklist that graded the card) is gone (owner ruling 2026-11-14: no auto-grading); it reads
/// as Recall wherever it still appears (database rows, packs, other devices' vault files).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Typed,
    Recall,
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl Response {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Typed => "typed",
            Self::Recall => "recall",
        }
    }
}

impl std::str::FromStr for Response {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "typed" => Ok(Self::Typed),
            "recall" | "tool" => Ok(Self::Recall),
            other => Err(format!("unknown Response: {other}")),
        }
    }
}
text_enum!(CardState { Shelved => "shelved", Active => "active", Suspended => "suspended" });
text_enum!(ReviewKind { Review => "review", Practice => "practice", Cram => "cram" });
text_enum!(AttemptStatus { InProgress => "in_progress", Completed => "completed", Discarded => "discarded" });
text_enum!(StepStatus { Pending => "pending", Revealed => "revealed", Graded => "graded" });
text_enum!(MediaRole { Prompt => "prompt", Solution => "solution" });

impl Grade {
    pub const ALL: [Grade; 4] = [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy];

    /// `a`, `h`, `g`, `e` as used by the simulator.
    pub fn from_letter(c: char) -> Option<Grade> {
        match c.to_ascii_lowercase() {
            'a' => Some(Grade::Again),
            'h' => Some(Grade::Hard),
            'g' => Some(Grade::Good),
            'e' => Some(Grade::Easy),
            _ => None,
        }
    }
}

// The enum comes from a macro, so `#[default]` cannot be attached to the variant.
#[allow(clippy::derivable_impls)]
impl Default for Difficulty {
    fn default() -> Self {
        Difficulty::Beginner
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub syntax: Option<String>,
    pub color: Option<String>,
    #[ts(type = "number")]
    pub position: i64,
    pub modified_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Criterion {
    pub id: String,
    pub text: String,
    pub weight: f64,
}

/// Reference to a stored media file. `ext`/`mime` are copied from the media table so the
/// frontend can build `<hh>/<hash>.<ext>` and pick the right element without another call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MediaRef {
    pub hash: String,
    pub ext: String,
    pub mime: String,
    pub role: MediaRole,
    pub caption: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Step {
    pub id: String,
    pub prompt_md: String,
    pub response: Response,
    pub solution_md: String,
    pub solution_text: Option<String>,
    pub criteria: Vec<Criterion>,
    pub points: f64,
    pub media: Vec<MediaRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub category_id: String,
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
    pub est_minutes: Option<u32>,
    pub steps: Vec<Step>,
    pub source: Option<String>,
    pub pack_id: Option<String>,
    pub pack_card_id: Option<String>,
    pub state: CardState,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub modified_by: String,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// One row of the Library list (SPEC §14).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CardSummary {
    pub id: String,
    pub title: String,
    pub category_id: String,
    pub category_name: String,
    pub category_color: Option<String>,
    pub difficulty: Difficulty,
    pub state: CardState,
    pub step_count: u32,
    pub due_day: Option<NaiveDate>,
    pub flagged: bool,
    pub modified_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct CardFilter {
    pub text: Option<String>,
    pub category_id: Option<String>,
    pub state: Option<CardState>,
    /// Active cards due on or before `today`, or active with no schedule yet.
    pub due_only: bool,
    pub flagged_only: bool,
    pub today: Option<NaiveDate>,
}

/// Per-step result inside an attempt. `elapsed_ms` follows D-17: time the step was on
/// screen before Reveal, from the app's own timestamps only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AttemptStep {
    pub step_id: String,
    pub status: StepStatus,
    pub shown_at: Option<DateTime<Utc>>,
    #[ts(type = "number")]
    pub elapsed_ms: i64,
    pub typed_answer: Option<String>,
    pub checked: Vec<String>,
    pub grade4: Option<Grade>,
    pub score: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Attempt {
    pub id: String,
    pub card_id: String,
    pub status: AttemptStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub steps: Vec<AttemptStep>,
    /// Cram run (owner ruling 2026-11-12): finishes as a `cram` review that never moves due
    /// dates or weak-step flags.
    #[serde(default)]
    pub cram: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewStep {
    pub step_id: String,
    pub score: f64,
    #[ts(type = "number")]
    pub elapsed_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SchedMeta {
    pub algo: String,
    pub lib_version: String,
    pub config_hash: String,
}

/// Immutable scheduling event (SPEC §5). `tz_offset_min` is the device's UTC offset when the
/// review was written, so replay reproduces the same local day boundaries anywhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Review {
    pub id: String,
    pub card_id: String,
    pub ts_utc: DateTime<Utc>,
    pub tz_offset_min: i32,
    pub device_id: String,
    pub kind: ReviewKind,
    pub grade: Grade,
    pub score: f64,
    pub steps: Vec<ReviewStep>,
    pub sched: SchedMeta,
}

/// Scheduling cache for one card (SPEC §6 `card_sched`); a pure function of its reviews.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CardSched {
    pub card_id: String,
    pub stability: f32,
    pub difficulty: f32,
    pub stability_fast: f32,
    pub last_review_ts: DateTime<Utc>,
    pub due_day: NaiveDate,
    pub interval_days: u32,
    pub reps: u32,
    pub lapses: u32,
    pub success_streak: u32,
}

/// Weak-step cache (SPEC §7.5); newest score last.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StepStats {
    pub step_id: String,
    pub recent_scores: Vec<f64>,
    pub flagged: bool,
    pub flag_count: u32,
}

impl StepStats {
    pub fn new(step_id: impl Into<String>) -> Self {
        StepStats {
            step_id: step_id.into(),
            recent_scores: Vec::new(),
            flagged: false,
            flag_count: 0,
        }
    }
}

#[cfg(test)]
mod response_tests {
    use super::Response;

    #[test]
    fn old_tool_steps_read_as_recall() {
        let r: Response = serde_json::from_str("\"tool\"").unwrap();
        assert_eq!(r, Response::Recall);
        assert_eq!("tool".parse::<Response>(), Ok(Response::Recall));
        assert_eq!(
            serde_json::to_string(&Response::Recall).unwrap(),
            "\"recall\""
        );
        assert!("do".parse::<Response>().is_err());
    }
}

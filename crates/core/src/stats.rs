//! Stats (SPEC §14, owner-expanded 2026-11-11): everything derives from reviews, the schedule
//! cache and card summaries. `compute` is pure; `overview` loads and calls it.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Days, NaiveDate};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Result;
use crate::db::Db;
use crate::db::repo::FlaggedStep;
use crate::domain::{CardFilter, CardSched, CardState, CardSummary, Grade, Review, ReviewKind};
use crate::replay;
use crate::sched::SchedConfig;
use crate::sched::day::local_day;

/// A card counts as mature once its interval reaches this many days.
pub const MATURE_DAYS: u32 = 21;
pub const HEATMAP_DAYS: u64 = 365;
pub const FORECAST_DAYS: u64 = 14;
pub const PROJECTION_WEEKS: u64 = 8;
pub const MIX_WEEKS: u64 = 12;
pub const TREND_WINDOW_DAYS: i64 = 30;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayCount {
    pub day: NaiveDate,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NamedStat {
    pub name: String,
    /// Mean measured minutes per review (D-17).
    pub minutes: f64,
    /// Mean of the last 30 days minus the mean of the 30 before; 0 when either is empty.
    pub trend: f64,
    pub reviews: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WeekMix {
    pub week_start: NaiveDate,
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
}

/// Medians over cards with at least two reviews: first attempt vs latest attempt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Improvement {
    pub cards: u32,
    pub first_score: f64,
    pub latest_score: f64,
    pub first_minutes: f64,
    pub latest_minutes: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CategoryCount {
    pub name: String,
    pub color: Option<String>,
    pub total: u32,
    pub active: u32,
    pub shelved: u32,
    pub suspended: u32,
    pub due: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatsOverview {
    pub reviews_total: u32,
    pub streak_days: u32,
    pub longest_streak_days: u32,
    /// Measured study time (D-17) over every study act, and over today's.
    #[ts(type = "number")]
    pub time_total_ms: i64,
    #[ts(type = "number")]
    pub time_today_ms: i64,
    pub heatmap: Vec<DayCount>,
    pub forecast: Vec<DayCount>,
    pub projection_weeks: Vec<DayCount>,
    pub matured_now: u32,
    pub matured_30d_ago: u32,
    pub time_by_category: Vec<NamedStat>,
    pub time_by_difficulty: Vec<NamedStat>,
    pub grade_mix: Vec<WeekMix>,
    pub improvement: Improvement,
    pub flagged: Vec<FlaggedStep>,
    pub categories: Vec<CategoryCount>,
}

pub fn overview(db: &Db, today: NaiveDate, cfg: &SchedConfig) -> Result<StatsOverview> {
    let reviews = db.reviews_all()?;
    let cards = db.cards_query(&CardFilter::default())?;
    let scheds = db.card_scheds_all()?;
    let flagged = db.flagged_step_details()?;
    compute(today, cfg, &reviews, &cards, &scheds, flagged)
}

fn ms_of(r: &Review) -> i64 {
    r.steps.iter().map(|s| s.elapsed_ms).sum()
}

fn minutes_of(r: &Review) -> f64 {
    ms_of(r) as f64 / 60_000.0
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn median(mut xs: Vec<f64>) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    let n = xs.len();
    if n % 2 == 1 {
        xs[n / 2]
    } else {
        (xs[n / 2 - 1] + xs[n / 2]) / 2.0
    }
}

fn week_start(day: NaiveDate) -> NaiveDate {
    day - Days::new(u64::from(day.weekday().num_days_from_monday()))
}

fn named_stats(groups: BTreeMap<String, Vec<(i64, f64)>>) -> Vec<NamedStat> {
    groups
        .into_iter()
        .map(|(name, rows)| {
            let all: Vec<f64> = rows.iter().map(|(_, m)| *m).collect();
            let recent: Vec<f64> = rows
                .iter()
                .filter(|(age, _)| *age < TREND_WINDOW_DAYS)
                .map(|(_, m)| *m)
                .collect();
            let previous: Vec<f64> = rows
                .iter()
                .filter(|(age, _)| (TREND_WINDOW_DAYS..2 * TREND_WINDOW_DAYS).contains(age))
                .map(|(_, m)| *m)
                .collect();
            NamedStat {
                name,
                minutes: mean(&all),
                trend: if recent.is_empty() || previous.is_empty() {
                    0.0
                } else {
                    mean(&recent) - mean(&previous)
                },
                reviews: all.len() as u32,
            }
        })
        .collect()
}

pub fn compute(
    today: NaiveDate,
    cfg: &SchedConfig,
    reviews: &[Review],
    cards: &[CardSummary],
    scheds: &[CardSched],
    flagged: Vec<FlaggedStep>,
) -> Result<StatsOverview> {
    let by_id: HashMap<&str, &CardSummary> = cards.iter().map(|c| (c.id.as_str(), c)).collect();
    let day_of = |r: &Review| local_day(r.ts_utc, r.tz_offset_min, cfg.rollover_hour);

    // Heatmap and streak count every study act (reviews and drills).
    let mut per_day: HashMap<NaiveDate, u32> = HashMap::new();
    for r in reviews {
        *per_day.entry(day_of(r)).or_default() += 1;
    }
    let heatmap: Vec<DayCount> = (0..HEATMAP_DAYS)
        .rev()
        .map(|back| {
            let day = today - Days::new(back);
            DayCount {
                day,
                count: per_day.get(&day).copied().unwrap_or(0),
            }
        })
        .collect();
    let mut streak_days = 0;
    let mut cursor = if per_day.contains_key(&today) {
        today
    } else {
        today - Days::new(1)
    };
    while per_day.contains_key(&cursor) {
        streak_days += 1;
        cursor = cursor - Days::new(1);
    }
    let mut days: Vec<NaiveDate> = per_day.keys().copied().collect();
    days.sort();
    let (mut longest_streak_days, mut run) = (0, 0);
    for (i, d) in days.iter().enumerate() {
        run = if i > 0 && days[i - 1] + Days::new(1) == *d {
            run + 1
        } else {
            1
        };
        longest_streak_days = longest_streak_days.max(run);
    }
    let time_total_ms = reviews.iter().map(ms_of).sum();
    let time_today_ms = reviews
        .iter()
        .filter(|r| day_of(r) == today)
        .map(ms_of)
        .sum();

    // Due forecast and projection from the live schedule of active cards.
    let active: HashMap<&str, &CardSummary> = cards
        .iter()
        .filter(|c| c.state == CardState::Active)
        .map(|c| (c.id.as_str(), c))
        .collect();
    let mut forecast: Vec<DayCount> = (0..FORECAST_DAYS)
        .map(|i| DayCount {
            day: today + Days::new(i),
            count: 0,
        })
        .collect();
    let mut projection_weeks: Vec<DayCount> = (0..PROJECTION_WEEKS)
        .map(|i| DayCount {
            day: today + Days::new(i * 7),
            count: 0,
        })
        .collect();
    let sched_by_id: HashMap<&str, &CardSched> =
        scheds.iter().map(|s| (s.card_id.as_str(), s)).collect();
    for card in active.values() {
        let due = sched_by_id
            .get(card.id.as_str())
            .map_or(today, |s| s.due_day);
        let offset = (due - today).num_days().max(0) as u64;
        if offset < FORECAST_DAYS {
            forecast[offset as usize].count += 1;
        }
        if offset / 7 < PROJECTION_WEEKS {
            projection_weeks[(offset / 7) as usize].count += 1;
        }
    }
    let matured_now = scheds
        .iter()
        .filter(|s| active.contains_key(s.card_id.as_str()) && s.interval_days >= MATURE_DAYS)
        .count() as u32;
    let cutoff = today - Days::new(TREND_WINDOW_DAYS as u64);
    let mut per_card: HashMap<&str, Vec<&Review>> = HashMap::new();
    for r in reviews.iter().filter(|r| r.kind == ReviewKind::Review) {
        per_card.entry(r.card_id.as_str()).or_default().push(r);
    }
    let mut matured_30d_ago = 0;
    for (card_id, rs) in &per_card {
        if !active.contains_key(card_id) {
            continue;
        }
        let older: Vec<Review> = rs
            .iter()
            .filter(|r| day_of(r) < cutoff)
            .map(|r| (*r).clone())
            .collect();
        if older.is_empty() {
            continue;
        }
        if let Some(s) = replay::replay(&[], &older, cfg)?.sched
            && s.interval_days >= MATURE_DAYS
        {
            matured_30d_ago += 1;
        }
    }

    // Measured time by category and difficulty, with a 30-day trend.
    let mut by_category: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
    let mut by_difficulty: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
    let mut mix: BTreeMap<NaiveDate, WeekMix> = BTreeMap::new();
    let mix_cutoff = week_start(today) - Days::new(7 * (MIX_WEEKS - 1));
    for r in reviews.iter().filter(|r| r.kind == ReviewKind::Review) {
        let day = day_of(r);
        let age = (today - day).num_days();
        let minutes = minutes_of(r);
        if let Some(card) = by_id.get(r.card_id.as_str())
            && minutes > 0.0
        {
            by_category
                .entry(card.category_name.clone())
                .or_default()
                .push((age, minutes));
            by_difficulty
                .entry(card.difficulty.as_str().to_string())
                .or_default()
                .push((age, minutes));
        }
        let ws = week_start(day);
        if ws >= mix_cutoff {
            let m = mix.entry(ws).or_insert(WeekMix {
                week_start: ws,
                again: 0,
                hard: 0,
                good: 0,
                easy: 0,
            });
            match r.grade {
                Grade::Again => m.again += 1,
                Grade::Hard => m.hard += 1,
                Grade::Good => m.good += 1,
                Grade::Easy => m.easy += 1,
            }
        }
    }
    let grade_mix: Vec<WeekMix> = (0..MIX_WEEKS)
        .map(|i| {
            let ws = mix_cutoff + Days::new(7 * i);
            mix.remove(&ws).unwrap_or(WeekMix {
                week_start: ws,
                again: 0,
                hard: 0,
                good: 0,
                easy: 0,
            })
        })
        .collect();

    // Improvement: first vs latest attempt per card with ≥ 2 reviews.
    let mut first_scores = Vec::new();
    let mut latest_scores = Vec::new();
    let mut first_minutes = Vec::new();
    let mut latest_minutes = Vec::new();
    for rs in per_card.values_mut() {
        if rs.len() < 2 {
            continue;
        }
        rs.sort_by_key(|r| r.ts_utc);
        let (first, latest) = (rs[0], rs[rs.len() - 1]);
        first_scores.push(first.score);
        latest_scores.push(latest.score);
        first_minutes.push(minutes_of(first));
        latest_minutes.push(minutes_of(latest));
    }
    let improvement = Improvement {
        cards: first_scores.len() as u32,
        first_score: median(first_scores),
        latest_score: median(latest_scores),
        first_minutes: median(first_minutes),
        latest_minutes: median(latest_minutes),
    };

    let mut categories: BTreeMap<String, CategoryCount> = BTreeMap::new();
    for c in cards {
        let e = categories
            .entry(c.category_name.clone())
            .or_insert(CategoryCount {
                name: c.category_name.clone(),
                color: c.category_color.clone(),
                total: 0,
                active: 0,
                shelved: 0,
                suspended: 0,
                due: 0,
            });
        e.total += 1;
        match c.state {
            CardState::Active => {
                e.active += 1;
                if c.due_day.is_none_or(|d| d <= today) {
                    e.due += 1;
                }
            }
            CardState::Shelved => e.shelved += 1,
            CardState::Suspended => e.suspended += 1,
        }
    }

    Ok(StatsOverview {
        reviews_total: reviews.len() as u32,
        streak_days,
        longest_streak_days,
        time_total_ms,
        time_today_ms,
        heatmap,
        forecast,
        projection_weeks,
        matured_now,
        matured_30d_ago,
        time_by_category: named_stats(by_category),
        time_by_difficulty: named_stats(by_difficulty),
        grade_mix,
        improvement,
        flagged,
        categories: categories.into_values().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Difficulty, ReviewStep, SchedMeta};
    use chrono::{TimeDelta, Utc};

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn review(card: &str, d: &str, grade: Grade, score: f64, ms: i64) -> Review {
        Review {
            id: format!("{card}-{d}-{ms}"),
            card_id: card.into(),
            ts_utc: day(d).and_hms_opt(12, 0, 0).unwrap().and_utc(),
            tz_offset_min: 0,
            device_id: "d".into(),
            kind: ReviewKind::Review,
            grade,
            score,
            steps: vec![ReviewStep {
                step_id: "s".into(),
                score,
                elapsed_ms: ms,
            }],
            sched: SchedMeta {
                algo: "fsrs7".into(),
                lib_version: "7.0.0".into(),
                config_hash: String::new(),
            },
        }
    }

    fn summary(
        id: &str,
        cat: &str,
        difficulty: Difficulty,
        state: CardState,
        due: Option<&str>,
    ) -> CardSummary {
        CardSummary {
            id: id.into(),
            title: id.into(),
            category_id: cat.into(),
            category_name: cat.into(),
            category_color: None,
            difficulty,
            state,
            step_count: 1,
            due_day: due.map(day),
            flagged: false,
            modified_at: Utc::now(),
            tags: vec![],
        }
    }

    fn sched(card: &str, due: &str, interval: u32) -> CardSched {
        CardSched {
            card_id: card.into(),
            stability: 1.0,
            difficulty: 1.0,
            stability_fast: 1.0,
            last_review_ts: day(due).and_hms_opt(0, 0, 0).unwrap().and_utc()
                - TimeDelta::days(i64::from(interval)),
            due_day: day(due),
            interval_days: interval,
            reps: 1,
            lapses: 0,
            success_streak: 1,
        }
    }

    #[test]
    fn aggregates_on_a_synthetic_log() {
        let today = day("2026-06-30"); // a Tuesday
        let cfg = SchedConfig::default();
        let reviews = vec![
            review("a", "2026-06-30", Grade::Good, 0.9, 120_000),
            review("a", "2026-06-29", Grade::Again, 0.5, 240_000),
            review("a", "2026-05-20", Grade::Good, 0.85, 300_000),
            review("b", "2026-06-28", Grade::Easy, 1.0, 60_000),
            review("b", "2026-06-01", Grade::Hard, 0.7, 90_000),
            review("c", "2026-06-30", Grade::Good, 0.9, 0),
        ];
        let cards = vec![
            summary(
                "a",
                "Blender",
                Difficulty::Beginner,
                CardState::Active,
                Some("2026-06-30"),
            ),
            summary(
                "b",
                "Python",
                Difficulty::Advanced,
                CardState::Active,
                Some("2026-07-10"),
            ),
            summary(
                "c",
                "Blender",
                Difficulty::Beginner,
                CardState::Shelved,
                None,
            ),
            summary(
                "d",
                "Blender",
                Difficulty::Beginner,
                CardState::Active,
                None,
            ),
        ];
        let scheds = vec![sched("a", "2026-06-30", 1), sched("b", "2026-07-10", 30)];
        let o = compute(today, &cfg, &reviews, &cards, &scheds, vec![]).unwrap();

        assert_eq!(o.reviews_total, 6);
        assert_eq!(o.streak_days, 3, "28th, 29th, 30th");
        assert_eq!(o.longest_streak_days, 3);
        assert_eq!(o.time_total_ms, 810_000);
        assert_eq!(o.time_today_ms, 120_000);
        assert_eq!(o.heatmap.len(), 365);
        assert_eq!(o.heatmap.last().unwrap().count, 2);
        assert_eq!(o.forecast[0].count, 2, "a due today + d never reviewed");
        assert_eq!(o.forecast[10].count, 1, "b in ten days");
        assert_eq!(o.projection_weeks[0].count, 2);
        assert_eq!(o.projection_weeks[1].count, 1);
        assert_eq!(o.matured_now, 1, "only b has a 30-day interval");

        let blender = o
            .time_by_category
            .iter()
            .find(|s| s.name == "Blender")
            .unwrap();
        assert_eq!(blender.reviews, 3, "c's zero-minute review is excluded");
        assert!((blender.minutes - (2.0 + 4.0 + 5.0) / 3.0).abs() < 1e-9);
        assert!(
            (blender.trend - (3.0 - 5.0)).abs() < 1e-9,
            "recent 3 min vs 5 min before"
        );
        let adv = o
            .time_by_difficulty
            .iter()
            .find(|s| s.name == "advanced")
            .unwrap();
        assert_eq!(adv.reviews, 2);

        assert_eq!(o.grade_mix.len(), 12);
        let this_week = o.grade_mix.last().unwrap();
        assert_eq!(this_week.week_start, day("2026-06-29"));
        assert_eq!((this_week.again, this_week.good), (1, 2));

        assert_eq!(o.improvement.cards, 2);
        assert!((o.improvement.first_score - (0.85 + 0.7) / 2.0).abs() < 1e-9);
        assert!((o.improvement.latest_score - (0.9 + 1.0) / 2.0).abs() < 1e-9);

        let cat = o.categories.iter().find(|c| c.name == "Blender").unwrap();
        assert_eq!((cat.total, cat.active, cat.shelved, cat.due), (3, 2, 1, 2));
    }

    #[test]
    fn longest_streak_outlives_the_current_one() {
        let reviews: Vec<Review> = [
            "2026-03-01",
            "2026-03-02",
            "2026-03-03",
            "2026-03-04",
            "2026-03-10",
        ]
        .iter()
        .map(|d| review("a", d, Grade::Good, 0.9, 1000))
        .collect();
        let o = compute(
            day("2026-03-10"),
            &SchedConfig::default(),
            &reviews,
            &[],
            &[],
            vec![],
        )
        .unwrap();
        assert_eq!((o.streak_days, o.longest_streak_days), (1, 4));
    }

    #[test]
    fn empty_log_is_all_zeros() {
        let o = compute(
            day("2026-01-01"),
            &SchedConfig::default(),
            &[],
            &[],
            &[],
            vec![],
        )
        .unwrap();
        assert_eq!(
            (o.streak_days, o.longest_streak_days, o.time_total_ms),
            (0, 0, 0)
        );
        assert_eq!(o.improvement, Improvement::default());
        assert!(o.forecast.iter().all(|d| d.count == 0));
    }
}

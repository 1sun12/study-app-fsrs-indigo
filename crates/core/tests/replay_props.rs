//! Replay properties (SPEC §7.6): the incremental fold equals a full replay, and replay is
//! independent of the order rows were inserted.

use chrono::{Days, TimeDelta};
use indigo_core::domain::{Grade, Review, ReviewKind, ReviewStep, SchedMeta};
use indigo_core::replay::{ReplayState, apply, replay};
use indigo_core::sched::SchedConfig;
use proptest::prelude::*;

type Item = (u32, u8, Vec<f64>, u8, bool);

fn reviews_from(items: &[Item], n_steps: usize) -> Vec<Review> {
    let base = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    let mut day = 0u64;
    items
        .iter()
        .enumerate()
        .map(|(i, (gap, grade, scores, device, practice))| {
            day += u64::from(*gap);
            let grade = Grade::ALL[usize::from(*grade % 4)];
            let steps: Vec<ReviewStep> = scores
                .iter()
                .take(n_steps)
                .enumerate()
                .map(|(s, score)| ReviewStep {
                    step_id: format!("s{s}"),
                    score: *score,
                    elapsed_ms: 0,
                })
                .collect();
            Review {
                id: format!("r{i:03}"),
                card_id: "p".into(),
                ts_utc: base + Days::new(day) + TimeDelta::seconds(i as i64),
                tz_offset_min: -360,
                device_id: format!("d{device}"),
                kind: if *practice {
                    ReviewKind::Practice
                } else {
                    ReviewKind::Review
                },
                grade,
                score: steps.iter().map(|s| s.score).sum::<f64>() / steps.len().max(1) as f64,
                steps,
                sched: SchedMeta {
                    algo: "fsrs7".into(),
                    lib_version: "7.0.0".into(),
                    config_hash: String::new(),
                },
            }
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn fold_equals_replay_and_order_does_not_matter(
        n_steps in 1usize..=6,
        items in prop::collection::vec(
            (0u32..=90, 0u8..4, prop::collection::vec(0.0f64..=1.0, 6), 0u8..3, prop::bool::weighted(0.2)),
            1..40,
        ),
        perm in prop::collection::vec(any::<u32>(), 40),
    ) {
        let cfg = SchedConfig::default();
        let ids: Vec<String> = (0..n_steps).map(|i| format!("s{i}")).collect();
        let reviews = reviews_from(&items, n_steps);

        let full = replay(&ids, &reviews, &cfg).unwrap();

        let mut sorted = reviews.clone();
        sorted.sort_by(|a, b| (a.ts_utc, &a.device_id, &a.id).cmp(&(b.ts_utc, &b.device_id, &b.id)));
        let mut incremental = ReplayState::new(&ids);
        for r in &sorted {
            apply(&mut incremental, r, &cfg).unwrap();
        }
        prop_assert_eq!(&incremental, &full);

        let mut shuffled: Vec<(u32, Review)> = perm.iter().copied().zip(reviews).collect();
        shuffled.sort_by_key(|(k, _)| *k);
        let shuffled: Vec<Review> = shuffled.into_iter().map(|(_, r)| r).collect();
        prop_assert_eq!(replay(&ids, &shuffled, &cfg).unwrap(), full);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Cram reviews (any number, anywhere in the history) never change the replayed state.
    #[test]
    fn cram_reviews_are_invisible_to_replay(
        n_steps in 1usize..=4,
        items in prop::collection::vec(
            (0u32..=60, 0u8..4, prop::collection::vec(0.0f64..=1.0, 4), 0u8..2, prop::bool::weighted(0.2)),
            1..25,
        ),
        cram_at in prop::collection::vec((0usize..25, 0u8..4), 0..10),
    ) {
        let cfg = SchedConfig::default();
        let ids: Vec<String> = (0..n_steps).map(|i| format!("s{i}")).collect();
        let reviews = reviews_from(&items, n_steps);
        let without = replay(&ids, &reviews, &cfg).unwrap();

        let mut with = reviews.clone();
        for (k, (at, grade)) in cram_at.iter().enumerate() {
            let mut c = reviews[at % reviews.len()].clone();
            c.id = format!("cram{k}");
            c.kind = ReviewKind::Cram;
            c.grade = Grade::ALL[usize::from(*grade)];
            for s in &mut c.steps {
                s.score = 0.0;
            }
            with.push(c);
        }
        prop_assert_eq!(replay(&ids, &with, &cfg).unwrap(), without);
    }
}

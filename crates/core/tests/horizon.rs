//! 26- and 52-week simulations with a seeded learner whose recall probability is the
//! library's own predicted retrievability. Bounds must hold on every review.

use chrono::{Days, NaiveDate};
use indigo_core::domain::{CardSched, Grade};
use indigo_core::sched::day::{elapsed_days, review_day};
use indigo_core::sched::fsrs::{memory_of, retrievability};
use indigo_core::sched::{SchedConfig, next_sched};

struct XorShift(u64);

impl XorShift {
    fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn simulate(weeks: u32, seed: u64) -> Vec<CardSched> {
    let cfg = SchedConfig::default();
    let mut rng = XorShift(seed.max(1));
    let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let mut sched: Option<CardSched> = None;
    let mut history = Vec::new();
    for d in 0..weeks * 7 {
        let day = start + Days::new(u64::from(d));
        if sched.as_ref().is_some_and(|s| s.due_day > day) {
            continue;
        }
        let grade = match &sched {
            None => Grade::Good,
            Some(s) => {
                let p = f64::from(retrievability(
                    memory_of(s),
                    elapsed_days(review_day(s), day),
                ));
                let r = rng.next_f64();
                if r >= p {
                    Grade::Again
                } else if r < p * 0.15 {
                    Grade::Easy
                } else if r < p * 0.90 {
                    Grade::Good
                } else {
                    Grade::Hard
                }
            }
        };
        let ts = day.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let out = next_sched(sched.as_ref(), "h", grade, ts, 0, &cfg).unwrap();
        assert!(
            (1..=365).contains(&out.sched.interval_days),
            "seed {seed} day {d}: interval {}",
            out.sched.interval_days
        );
        if grade == Grade::Again {
            if let Some(prev) = &sched {
                assert!(
                    out.sched.interval_days <= prev.interval_days,
                    "lapse lengthened"
                );
            }
            assert_eq!(out.sched.success_streak, 0);
        }
        assert!(out.sched.due_day > day, "due day must be in the future");
        sched = Some(out.sched.clone());
        history.push(out.sched);
    }
    history
}

#[test]
fn horizons_hold_the_bounds() {
    for weeks in [26, 52] {
        for seed in 1..=8 {
            let h = simulate(weeks, seed);
            assert!(
                h.len() >= 2,
                "seed {seed}: only {} reviews in {weeks} weeks",
                h.len()
            );
            let last = h.last().unwrap();
            assert_eq!(last.reps as usize, h.len());
            assert_eq!(
                last.lapses as usize,
                h.iter().filter(|s| s.success_streak == 0).count()
            );
        }
    }
}

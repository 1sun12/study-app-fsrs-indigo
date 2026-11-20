//! Interval guards (SPEC §7.4): applied to the due date only, never to memory state.

use super::SchedConfig;

/// `round(raw)` clamped to `[min, max]`, then the early-growth cap for the streak after
/// this review (streak 1 → first cap, …); streaks past the table get no extra cap.
pub fn guarded_interval(raw_days: f32, success_streak: u32, cfg: &SchedConfig) -> u32 {
    let rounded = if raw_days.is_finite() {
        raw_days.round().max(0.0) as u32
    } else {
        0
    };
    let mut days = rounded.clamp(cfg.min_interval_days, cfg.max_interval_days);
    if let Some(cap) = success_streak
        .checked_sub(1)
        .and_then(|i| cfg.streak_caps.get(i as usize))
    {
        days = days.min(*cap);
    }
    days.max(cfg.min_interval_days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_and_caps() {
        let cfg = SchedConfig::default();
        assert_eq!(guarded_interval(0.0, 0, &cfg), 1); // again → tomorrow
        assert_eq!(guarded_interval(0.6, 1, &cfg), 1); // sub-day hard → tomorrow
        assert_eq!(guarded_interval(53.9, 1, &cfg), 7); // first easy capped
        assert_eq!(guarded_interval(26.5, 2, &cfg), 21);
        assert_eq!(guarded_interval(94.2, 3, &cfg), 60);
        assert_eq!(guarded_interval(298.1, 4, &cfg), 298); // no cap past the table
        assert_eq!(guarded_interval(900.0, 5, &cfg), 365);
        assert_eq!(guarded_interval(f32::NAN, 2, &cfg), 1);
    }

    #[test]
    fn caps_can_be_disabled() {
        let cfg = SchedConfig {
            streak_caps: vec![],
            ..SchedConfig::default()
        };
        assert_eq!(guarded_interval(53.9, 1, &cfg), 54);
    }
}

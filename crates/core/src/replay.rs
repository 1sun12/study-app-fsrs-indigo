//! Replay (SPEC §7.6, P-05): `card_sched` and `step_stats` are a pure fold over the card's
//! reviews ordered by `(ts_utc, device_id, id)`. Incremental grading uses the same `apply`.

use crate::Result;
use crate::domain::{CardSched, Review, ReviewKind, StepStats};
use crate::sched::{self, SchedConfig};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ReplayState {
    pub sched: Option<CardSched>,
    /// One entry per current card step, in card order. Review steps for unknown ids are ignored.
    pub steps: Vec<StepStats>,
}

impl ReplayState {
    pub fn new(step_ids: &[String]) -> Self {
        ReplayState {
            sched: None,
            steps: step_ids.iter().map(StepStats::new).collect(),
        }
    }
}

/// Applies one review. Practice reviews update step stats only; cram reviews change nothing
/// (they count as activity in stats). Returns the raw library interval when the card was
/// scheduled.
pub fn apply(state: &mut ReplayState, review: &Review, cfg: &SchedConfig) -> Result<Option<f32>> {
    if review.kind == ReviewKind::Cram {
        return Ok(None);
    }
    for rs in &review.steps {
        if let Some(stats) = state.steps.iter_mut().find(|s| s.step_id == rs.step_id) {
            push_score(stats, rs.score, cfg);
        }
    }
    if review.kind != ReviewKind::Review {
        return Ok(None);
    }
    let out = sched::next_sched(
        state.sched.as_ref(),
        &review.card_id,
        review.grade,
        review.ts_utc,
        review.tz_offset_min,
        cfg,
    )?;
    state.sched = Some(out.sched);
    Ok(Some(out.raw_interval))
}

/// Full rebuild from any order of rows.
pub fn replay(step_ids: &[String], reviews: &[Review], cfg: &SchedConfig) -> Result<ReplayState> {
    let mut sorted: Vec<&Review> = reviews.iter().collect();
    sorted.sort_by(|a, b| (a.ts_utc, &a.device_id, &a.id).cmp(&(b.ts_utc, &b.device_id, &b.id)));
    let mut state = ReplayState::new(step_ids);
    for review in sorted {
        apply(&mut state, review, cfg)?;
    }
    Ok(state)
}

/// Weak-step rule (§7.5): flag on ≥ `flag_low_count` of the last `flag_window` scores below
/// `flag_low_score`; unflag when the last `unflag_high_count` are all ≥ `unflag_high_score`.
fn push_score(stats: &mut StepStats, score: f64, cfg: &SchedConfig) {
    stats.recent_scores.push(score);
    if stats.recent_scores.len() > cfg.flag_window {
        stats.recent_scores.remove(0);
    }
    let low = stats
        .recent_scores
        .iter()
        .filter(|&&s| s < cfg.flag_low_score)
        .count();
    if !stats.flagged && low >= cfg.flag_low_count {
        stats.flagged = true;
        stats.flag_count += 1;
    } else if stats.flagged {
        let n = cfg.unflag_high_count;
        let tail = &stats.recent_scores[stats.recent_scores.len().saturating_sub(n)..];
        if tail.len() >= n && tail.iter().all(|&s| s >= cfg.unflag_high_score) {
            stats.flagged = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_and_unflag() {
        let cfg = SchedConfig::default();
        let mut s = StepStats::new("s");
        push_score(&mut s, 0.5, &cfg);
        assert!(!s.flagged);
        push_score(&mut s, 0.9, &cfg);
        push_score(&mut s, 0.5, &cfg);
        assert!(s.flagged);
        assert_eq!(s.flag_count, 1);
        push_score(&mut s, 0.85, &cfg);
        assert!(s.flagged, "one good score is not enough");
        push_score(&mut s, 0.85, &cfg);
        assert!(!s.flagged);
        assert_eq!(s.recent_scores.len(), 3);
        push_score(&mut s, 0.1, &cfg);
        push_score(&mut s, 0.1, &cfg);
        assert!(s.flagged);
        assert_eq!(s.flag_count, 2);
    }
}

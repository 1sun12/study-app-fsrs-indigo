//! Scores (SPEC §7.1–7.2). The card's grade is always the user's own (owner ruling 2026-11-14);
//! these numbers are recorded for Stats and weak-step flags only. Pure functions.

use crate::domain::{Criterion, Grade};
use crate::sched::SchedConfig;

/// `Σ(weight · checked) / Σ weight`; 0 when the criteria carry no weight.
pub fn checklist_score(criteria: &[Criterion], checked: &[String]) -> f64 {
    let total: f64 = criteria.iter().map(|c| c.weight).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let got: f64 = criteria
        .iter()
        .filter(|c| checked.contains(&c.id))
        .map(|c| c.weight)
        .sum();
    got / total
}

pub fn grade4_score(grade: Grade, cfg: &SchedConfig) -> f64 {
    cfg.grade4_scores[grade as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crit(id: &str, w: f64) -> Criterion {
        Criterion {
            id: id.into(),
            text: id.into(),
            weight: w,
        }
    }

    #[test]
    fn checklist_weights() {
        let c = [crit("a", 1.0), crit("b", 3.0)];
        assert_eq!(checklist_score(&c, &["b".to_string()]), 0.75);
        assert_eq!(checklist_score(&c, &[]), 0.0);
        assert_eq!(checklist_score(&[], &[]), 0.0);
    }

    #[test]
    fn grade4_map() {
        let cfg = SchedConfig::default();
        assert_eq!(grade4_score(Grade::Again, &cfg), 0.0);
        assert_eq!(grade4_score(Grade::Hard, &cfg), 0.70);
        assert_eq!(grade4_score(Grade::Good, &cfg), 0.85);
        assert_eq!(grade4_score(Grade::Easy, &cfg), 1.0);
    }
}

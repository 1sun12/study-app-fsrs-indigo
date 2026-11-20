//! Every §7 constant in one synced struct (P-04 defaults). Exposed in Settings → Advanced.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

/// Owner rulings: desired retention may be set anywhere in 40–99% (0.2: 40–95; 0.3: up to 99).
pub const RETENTION_RANGE: std::ops::RangeInclusive<f32> = 0.40..=0.99;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct SchedConfig {
    /// FSRS desired retention, within `RETENTION_RANGE`.
    pub desired_retention: f32,
    /// Step score for the four buttons, indexed by `Grade` (again, hard, good, easy).
    pub grade4_scores: [f64; 4],
    pub min_interval_days: u32,
    pub max_interval_days: u32,
    /// Early-growth caps indexed by `success_streak - 1`; empty disables the guard.
    pub streak_caps: Vec<u32>,
    /// Local hour at which the study day rolls over.
    pub rollover_hour: u32,
    /// Weak-step flagging (§7.5): window of recent scores, and the flag/unflag rules.
    pub flag_window: usize,
    pub flag_low_score: f64,
    pub flag_low_count: usize,
    pub unflag_high_score: f64,
    pub unflag_high_count: usize,
}

impl Default for SchedConfig {
    fn default() -> Self {
        SchedConfig {
            desired_retention: 0.90,
            grade4_scores: [0.0, 0.70, 0.85, 1.0],
            min_interval_days: 1,
            max_interval_days: 365,
            streak_caps: vec![7, 21, 60],
            rollover_hour: 4,
            flag_window: 3,
            flag_low_score: 0.60,
            flag_low_count: 2,
            unflag_high_score: 0.80,
            unflag_high_count: 2,
        }
    }
}

impl SchedConfig {
    /// Rejects a retention outside `RETENTION_RANGE`.
    pub fn validate(&self) -> crate::Result<()> {
        if !RETENTION_RANGE.contains(&self.desired_retention) {
            return Err(crate::error::invalid(format!(
                "desired retention {} is outside 0.40–0.99",
                self.desired_retention
            )));
        }
        Ok(())
    }

    /// The same config with retention pulled into `RETENTION_RANGE` (for configs from sync).
    pub fn clamped(mut self) -> Self {
        self.desired_retention = self
            .desired_retention
            .clamp(*RETENTION_RANGE.start(), *RETENTION_RANGE.end());
        self
    }

    /// SHA-256 of the canonical JSON; stored in every review so a config change is visible.
    pub fn config_hash(&self) -> String {
        let json = serde_json::to_string(self).expect("SchedConfig serializes");
        Sha256::digest(json.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_changes_with_config() {
        let a = SchedConfig::default();
        let b = SchedConfig {
            desired_retention: 0.85,
            ..SchedConfig::default()
        };
        assert_eq!(a.config_hash(), SchedConfig::default().config_hash());
        assert_ne!(a.config_hash(), b.config_hash());
        assert_eq!(a.config_hash().len(), 64);
    }

    #[test]
    fn retention_bounds() {
        let at = |r| SchedConfig {
            desired_retention: r,
            ..SchedConfig::default()
        };
        assert!(at(0.40).validate().is_ok());
        assert!(at(0.99).validate().is_ok());
        assert!(at(0.39).validate().is_err());
        assert!(at(0.995).validate().is_err());
        assert_eq!(at(0.2).clamped().desired_retention, 0.40);
        assert_eq!(at(1.0).clamped().desired_retention, 0.99);
    }
}

//! Thin wrapper over the pinned `fsrs` crate (D-04). Nothing else touches the library.

use fsrs::{FSRS, ItemState, MemoryState};

use crate::domain::{CardSched, Grade};
use crate::{CoreError, Result};

/// Pinned in Cargo.toml (`=7.0.0`); recorded in every review's `sched.lib_version`.
pub const LIB_VERSION: &str = "7.0.0";

/// e.g. `fsrs7` — which algorithm the crate's default parameters select.
pub fn algo_name() -> String {
    format!("{:?}", FSRS::default().version()).to_lowercase()
}

pub fn memory_of(sched: &CardSched) -> MemoryState {
    MemoryState {
        stability: sched.stability,
        difficulty: sched.difficulty,
        stability_fast: sched.stability_fast,
    }
}

/// New card: `prev = None`, `elapsed_days = 0` (SPEC §7.3).
pub fn next_memory(
    prev: Option<MemoryState>,
    grade: Grade,
    elapsed_days: u32,
    desired_retention: f32,
) -> Result<ItemState> {
    let next = FSRS::default()
        .next_states(prev, desired_retention, elapsed_days)
        .map_err(|e| CoreError::Fsrs(format!("{e:?}")))?;
    Ok(match grade {
        Grade::Again => next.again,
        Grade::Hard => next.hard,
        Grade::Good => next.good,
        Grade::Easy => next.easy,
    })
}

/// Probability of recall after `elapsed_days`; drives the simulated learner.
pub fn retrievability(state: MemoryState, elapsed_days: u32) -> f32 {
    FSRS::default().current_retrievability(state, elapsed_days as f32)
}

//! Phase 0 spike (b): prove the pinned `fsrs` API shape and print a ladder.
//! Run with `cargo test -p indigo-core --test fsrs_smoke -- --nocapture`.

use fsrs::{DEFAULT_PARAMETERS, FSRS, MemoryState};

const RETENTION: f32 = 0.9;

#[test]
fn fsrs7_default_schedule_is_sane() {
    let fsrs = FSRS::default();
    println!(
        "model: {:?}, default parameter count: {}",
        fsrs.version(),
        DEFAULT_PARAMETERS.len()
    );

    let first = fsrs.next_states(None, RETENTION, 0).unwrap();
    println!(
        "new card intervals: again {:.2} | hard {:.2} | good {:.2} | easy {:.2}",
        first.again.interval, first.hard.interval, first.good.interval, first.easy.interval
    );
    assert!(first.again.interval <= first.hard.interval);
    assert!(first.hard.interval <= first.good.interval);
    assert!(first.good.interval <= first.easy.interval);

    println!("step | stability | difficulty | stab_fast | interval");
    let mut state: Option<MemoryState> = None;
    let mut elapsed = 0u32;
    for step in 1..=12 {
        let next = fsrs.next_states(state, RETENTION, elapsed).unwrap().good;
        println!(
            "{step:>4} | {:>9.2} | {:>10.2} | {:>9.2} | {:>8.2}",
            next.memory.stability,
            next.memory.difficulty,
            next.memory.stability_fast,
            next.interval
        );
        assert!(next.interval > 0.0);
        // Review exactly on the due day: the spec rounds and clamps to >= 1 day.
        elapsed = next.interval.round().max(1.0) as u32;
        state = Some(next.memory);
    }
}

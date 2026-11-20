//! Schedule simulator for human tuning (SPEC §7.8).
//!
//! `cargo run -p indigo-core --bin sim -- --grades g,g,g,e --retention 0.9`

use chrono::NaiveDate;
use clap::Parser;
use indigo_core::sched::SchedConfig;
use indigo_core::sim::{LadderOpts, format_table, parse_grades, run_ladder};

#[derive(Parser)]
#[command(about = "Prints one scheduling ladder: review on the due day, grade by grade.")]
struct Args {
    /// Grades in order, letters a/h/g/e, e.g. `g,g,g,e`.
    #[arg(long)]
    grades: String,
    #[arg(long, default_value_t = 0.9)]
    retention: f32,
    #[arg(long, default_value = "2026-01-01")]
    start: NaiveDate,
    /// Multiplier on the scheduled gap: 2.0 reviews twice as late, 0.5 early.
    #[arg(long, default_value_t = 1.0)]
    overdue: f64,
    /// Days between the first and second review instead of the scheduled gap.
    #[arg(long, default_value_t = 0)]
    idle_days: u32,
    /// Disable the early-growth streak caps (MIN/MAX clamps still apply).
    #[arg(long)]
    no_caps: bool,
}

fn main() {
    let args = Args::parse();
    let grades = match parse_grades(&args.grades) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let mut cfg = SchedConfig {
        desired_retention: args.retention,
        ..SchedConfig::default()
    };
    if args.no_caps {
        cfg.streak_caps.clear();
    }
    let opts = LadderOpts {
        grades,
        cfg,
        start: args.start,
        overdue: args.overdue,
        idle_days: args.idle_days,
    };
    match run_ladder(&opts) {
        Ok(rows) => print!("{}", format_table(&rows)),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

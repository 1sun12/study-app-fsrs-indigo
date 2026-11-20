//! Process-wide state and the device clock (local offset + study day with rollover).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Local, NaiveDate, Offset, Utc};
use indigo_core::db::Db;
use indigo_core::sched::day::local_day;

use crate::error::CmdResult;

pub struct AppState {
    pub db: Db,
    pub data_dir: PathBuf,
    pub media_dir: PathBuf,
    pub device_id: String,
    /// One sync at a time (launch, summary, Sync now, quit).
    pub syncing: AtomicBool,
    /// Set on the first close request; the second one closes for real.
    pub quitting: AtomicBool,
}

pub struct Clock {
    pub now: DateTime<Utc>,
    pub tz_offset_min: i32,
    pub today: NaiveDate,
}

impl AppState {
    /// `now` plus the device's current UTC offset and the study day it falls in.
    pub fn clock(&self) -> CmdResult<Clock> {
        let now = Utc::now();
        let tz_offset_min = Local::now().offset().fix().local_minus_utc() / 60;
        let rollover = self.db.sched_config()?.rollover_hour;
        Ok(Clock {
            now,
            tz_offset_min,
            today: local_day(now, tz_offset_min, rollover),
        })
    }
}

//! Process-wide state and the device clock (local offset + study day with rollover).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use indigo_core::db::Db;
use indigo_core::sched::day::Clock;

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

impl AppState {
    /// `now` plus the device's current UTC offset and the study day it falls in.
    pub fn clock(&self) -> CmdResult<Clock> {
        Ok(Clock::now(self.db.sched_config()?.rollover_hour))
    }
}

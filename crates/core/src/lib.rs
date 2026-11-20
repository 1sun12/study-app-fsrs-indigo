//! Indigo core: domain, scoring, scheduling, replay, attempt rules, database, authoring,
//! simulator. No Tauri dependencies; everything here is testable without a webview.

pub mod authoring;
pub mod backup;
pub mod db;
pub mod domain;
pub mod error;
pub mod git;
pub mod media;
pub mod pack;
pub mod planner;
pub mod replay;
pub mod sched;
pub mod scoring;
pub mod session;
pub mod settings;
pub mod sim;
pub mod stats;
pub mod study;
pub mod sync;
pub mod syntax;
pub mod vault;
pub mod warnings;
pub mod zipsafe;

pub use error::{CoreError, Result};

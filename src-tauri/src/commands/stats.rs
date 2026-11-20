use indigo_core::stats::{self, StatsOverview};
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn stats_overview(state: State<'_, AppState>) -> CmdResult<StatsOverview> {
    let cfg = state.db.sched_config()?;
    Ok(stats::overview(&state.db, state.clock()?.today, &cfg)?)
}

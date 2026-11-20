use indigo_core::sched::SchedConfig;
use indigo_core::settings::{KEY_SETTINGS, Settings};
use indigo_core::{sim, vault};
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.db.settings()?)
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    Ok(state.db.setting_set(KEY_SETTINGS, &settings)?)
}

#[tauri::command]
pub fn sched_config_get(state: State<'_, AppState>) -> CmdResult<SchedConfig> {
    Ok(state.db.sched_config()?)
}

/// Saving the config stamps it for sync and replays every card (§7.6).
#[tauri::command]
pub fn sched_config_set(state: State<'_, AppState>, config: SchedConfig) -> CmdResult<()> {
    let now = state.clock()?.now;
    Ok(vault::set_config(
        &state.db,
        &config,
        now,
        &state.device_id,
    )?)
}

/// Intervals after a run of good reviews at this retention (the Settings slider preview).
#[tauri::command]
pub fn sched_preview(state: State<'_, AppState>, retention: f32) -> CmdResult<Vec<u32>> {
    Ok(sim::retention_preview(
        &state.db.sched_config()?,
        retention,
    )?)
}

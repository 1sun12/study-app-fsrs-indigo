use indigo_core::sync::{self, ConflictView, SyncView};
use tauri::{AppHandle, State};

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn sync_status(app: AppHandle) -> CmdResult<SyncView> {
    Ok(crate::sync::view(&app)?)
}

/// Starts a background sync; progress arrives as `sync-status` events.
#[tauri::command]
pub fn sync_now(app: AppHandle) -> CmdResult<bool> {
    Ok(crate::sync::spawn(&app))
}

#[tauri::command]
pub fn conflicts_list(state: State<'_, AppState>) -> CmdResult<Vec<ConflictView>> {
    Ok(sync::conflicts(&state.db)?)
}

#[tauri::command]
pub fn conflict_restore(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let cfg = state.db.sched_config()?;
    Ok(sync::restore_conflict(
        &state.db,
        &id,
        state.clock()?.now,
        &state.device_id,
        &cfg,
    )?)
}

#[tauri::command]
pub fn conflict_dismiss(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(sync::dismiss_conflict(&state.db, &id)?)
}

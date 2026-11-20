//! Settings → Data and the Library's pack export (SPEC §12). Paths come from native dialogs.
//! Async so a large archive runs off the main thread and the window stays responsive.

use std::path::Path;

use indigo_core::backup::{self, BackupReport};
use indigo_core::pack::{self, PackPreview, PackReport};
use indigo_core::vault::MergeReport;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub async fn backup_export(state: State<'_, AppState>, path: String) -> CmdResult<BackupReport> {
    let now = state.clock()?.now;
    Ok(backup::export(
        &state.db,
        &state.media_dir,
        Path::new(&path),
        now,
    )?)
}

#[tauri::command]
pub async fn backup_import(state: State<'_, AppState>, path: String) -> CmdResult<MergeReport> {
    let now = state.clock()?.now;
    Ok(backup::import(
        &state.db,
        &state.media_dir,
        Path::new(&path),
        &state.data_dir,
        now,
    )?)
}

#[tauri::command]
pub async fn pack_export(
    state: State<'_, AppState>,
    path: String,
    card_ids: Vec<String>,
    name: String,
) -> CmdResult<u32> {
    let now = state.clock()?.now;
    Ok(pack::export(
        &state.db,
        &state.media_dir,
        &card_ids,
        &name,
        Path::new(&path),
        now,
    )?)
}

#[tauri::command]
pub async fn pack_inspect(state: State<'_, AppState>, path: String) -> CmdResult<PackPreview> {
    Ok(pack::inspect(&state.db, Path::new(&path))?)
}

#[tauri::command]
pub async fn pack_import(
    state: State<'_, AppState>,
    path: String,
    update: Vec<String>,
) -> CmdResult<PackReport> {
    let now = state.clock()?.now;
    let cfg = state.db.sched_config()?;
    Ok(pack::import(
        &state.db,
        &state.media_dir,
        Path::new(&path),
        &update,
        now,
        &state.device_id,
        &cfg,
    )?)
}

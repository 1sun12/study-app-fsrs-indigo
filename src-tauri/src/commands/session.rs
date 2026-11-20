use indigo_core::domain::Grade;
use indigo_core::domain::StepStats;
use indigo_core::planner::{self, SessionPlan};
use indigo_core::study::{self, AttemptView, Summary};
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn session_build(state: State<'_, AppState>, include_all: bool) -> CmdResult<SessionPlan> {
    let c = state.clock()?;
    planner::introduce(&state.db, c.today, c.now, &state.device_id)?;
    Ok(planner::build(&state.db, c.today, c.now, include_all)?)
}

/// A cram session over chosen cards (due dates never change).
#[tauri::command]
pub fn session_cram(state: State<'_, AppState>, card_ids: Vec<String>) -> CmdResult<SessionPlan> {
    Ok(planner::cram(&state.db, &card_ids)?)
}

#[tauri::command]
pub fn attempt_start(
    state: State<'_, AppState>,
    card_id: String,
    cram: Option<bool>,
) -> CmdResult<AttemptView> {
    let now = state.clock()?.now;
    if cram.unwrap_or(false) {
        return Ok(study::start_cram(&state.db, &card_id, now)?);
    }
    Ok(study::start(&state.db, &card_id, now, &state.device_id)?)
}

#[tauri::command]
pub fn attempt_get(state: State<'_, AppState>, attempt_id: String) -> CmdResult<AttemptView> {
    Ok(study::get(&state.db, &attempt_id)?)
}

#[tauri::command]
pub fn attempt_step_show(
    state: State<'_, AppState>,
    attempt_id: String,
    step_id: String,
) -> CmdResult<AttemptView> {
    Ok(study::step_show(
        &state.db,
        &attempt_id,
        &step_id,
        state.clock()?.now,
    )?)
}

#[tauri::command]
pub fn attempt_step_reveal(
    state: State<'_, AppState>,
    attempt_id: String,
    step_id: String,
    typed_answer: Option<String>,
) -> CmdResult<AttemptView> {
    Ok(study::step_reveal(
        &state.db,
        &attempt_id,
        &step_id,
        typed_answer,
        state.clock()?.now,
    )?)
}

#[tauri::command]
pub fn attempt_step_complete(
    state: State<'_, AppState>,
    attempt_id: String,
    step_id: String,
    checked: Vec<String>,
) -> CmdResult<AttemptView> {
    Ok(study::step_complete(
        &state.db,
        &attempt_id,
        &step_id,
        checked,
    )?)
}

/// Days until the card returns for Forgot, Hard, Good, Easy (shown on the grade buttons).
#[tauri::command]
pub fn grade_preview(state: State<'_, AppState>, card_id: String) -> CmdResult<[u32; 4]> {
    let c = state.clock()?;
    Ok(study::grade_preview(
        &state.db,
        &card_id,
        c.now,
        c.tz_offset_min,
    )?)
}

#[tauri::command]
pub fn attempt_pause(state: State<'_, AppState>, attempt_id: String) -> CmdResult<AttemptView> {
    Ok(study::pause(&state.db, &attempt_id, state.clock()?.now)?)
}

#[tauri::command]
pub fn attempt_discard(state: State<'_, AppState>, attempt_id: String) -> CmdResult<()> {
    Ok(study::discard(&state.db, &attempt_id, state.clock()?.now)?)
}

#[tauri::command]
pub fn attempt_restart(state: State<'_, AppState>, attempt_id: String) -> CmdResult<AttemptView> {
    Ok(study::restart(
        &state.db,
        &attempt_id,
        state.clock()?.now,
        &state.device_id,
    )?)
}

#[tauri::command]
pub fn attempt_finish(
    state: State<'_, AppState>,
    attempt_id: String,
    grade: Grade,
) -> CmdResult<Summary> {
    let c = state.clock()?;
    Ok(study::finish(
        &state.db,
        &attempt_id,
        grade,
        c.now,
        &state.device_id,
        c.tz_offset_min,
    )?)
}

#[tauri::command]
pub fn drill_record(
    state: State<'_, AppState>,
    card_id: String,
    step_id: String,
    grade: Grade,
    elapsed_ms: i64,
) -> CmdResult<StepStats> {
    let c = state.clock()?;
    Ok(study::record_drill(
        &state.db,
        &card_id,
        &step_id,
        grade,
        elapsed_ms,
        c.now,
        &state.device_id,
        c.tz_offset_min,
    )?)
}

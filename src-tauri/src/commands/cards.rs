use indigo_core::authoring::{self, SaveResult};
use indigo_core::domain::{Card, CardFilter, CardState, CardSummary};
use indigo_core::warnings;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn cards_query(state: State<'_, AppState>, filter: CardFilter) -> CmdResult<Vec<CardSummary>> {
    let mut filter = filter;
    if filter.today.is_none() {
        filter.today = Some(state.clock()?.today);
    }
    Ok(state.db.cards_query(&filter)?)
}

#[tauri::command]
pub fn card_get(state: State<'_, AppState>, id: String) -> CmdResult<Option<Card>> {
    Ok(state.db.get_card(&id)?.filter(|c| c.deleted_at.is_none()))
}

#[tauri::command]
pub fn card_save(
    state: State<'_, AppState>,
    card: Card,
    remember: Option<bool>,
) -> CmdResult<SaveResult> {
    let cfg = state.db.sched_config()?;
    Ok(authoring::save_card(
        &state.db,
        card,
        state.clock()?.now,
        &state.device_id,
        &cfg,
        remember.unwrap_or(false),
    )?)
}

/// Guard rails for the editor while typing (never blocks).
#[tauri::command]
pub fn card_warnings(card: Card) -> Vec<String> {
    warnings::card_warnings(&card)
}

#[tauri::command]
pub fn card_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.db.delete_card(&id, state.clock()?.now)?)
}

#[tauri::command]
pub fn card_set_state(
    state: State<'_, AppState>,
    id: String,
    card_state: CardState,
) -> CmdResult<()> {
    Ok(state
        .db
        .set_card_state(&id, card_state, state.clock()?.now, &state.device_id)?)
}

/// Renames (or merges) a tag on every card; returns how many cards changed.
#[tauri::command]
pub fn tag_rename(state: State<'_, AppState>, from: String, to: String) -> CmdResult<u32> {
    Ok(authoring::rename_tag(
        &state.db,
        &from,
        &to,
        state.clock()?.now,
        &state.device_id,
    )?)
}

/// Removes a tag from every card; returns how many cards changed.
#[tauri::command]
pub fn tag_delete(state: State<'_, AppState>, tag: String) -> CmdResult<u32> {
    Ok(authoring::delete_tag(
        &state.db,
        &tag,
        state.clock()?.now,
        &state.device_id,
    )?)
}

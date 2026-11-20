use indigo_core::authoring;
use indigo_core::domain::Category;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn categories_list(state: State<'_, AppState>) -> CmdResult<Vec<Category>> {
    Ok(state.db.categories_list()?)
}

#[tauri::command]
pub fn category_save(state: State<'_, AppState>, category: Category) -> CmdResult<Category> {
    Ok(authoring::save_category(
        &state.db,
        category,
        state.clock()?.now,
    )?)
}

#[tauri::command]
pub fn category_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.db.delete_category(&id, state.clock()?.now)?)
}

use indigo_core::media::{self, CleanupReport, MediaInfo};
use std::path::PathBuf;

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, CmdResult};
use crate::state::AppState;

/// Imports a file on a worker thread: video and audio are compressed first, which can take a
/// while, so the window stays responsive.
async fn import_file(app: AppHandle, src: PathBuf, remove_src: bool) -> CmdResult<MediaInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let out = media::import_file(
            &state.db,
            &state.media_dir,
            &src,
            &state.data_dir.join("tmp"),
            state.clock()?.now,
        );
        if remove_src {
            let _ = std::fs::remove_file(&src);
        }
        Ok::<_, AppError>(out?)
    })
    .await
    .map_err(|e| AppError {
        message: e.to_string(),
    })?
}

/// Pasted or dropped file: the frontend sends the raw bytes as the invoke body.
#[tauri::command]
pub async fn media_import_bytes(app: AppHandle, request: Request<'_>) -> CmdResult<MediaInfo> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError {
            message: "expected raw bytes".into(),
        });
    };
    let tmp = app.state::<AppState>().data_dir.join("tmp");
    std::fs::create_dir_all(&tmp)?;
    let src = tmp.join(format!("drop-{}", uuid::Uuid::new_v4()));
    std::fs::write(&src, bytes)?;
    import_file(app, src, true).await
}

/// Ctrl+V of an image: WebKitGTK does not expose clipboard images to the page, so the
/// backend reads it once, on this explicit paste (never polled). None = no image there.
#[tauri::command]
pub fn media_import_clipboard(state: State<'_, AppState>) -> CmdResult<Option<MediaInfo>> {
    let Ok(img) = arboard::Clipboard::new().and_then(|mut c| c.get_image()) else {
        return Ok(None);
    };
    let png = media::encode_png(img.width as u32, img.height as u32, &img.bytes)?;
    Ok(Some(media::import(
        &state.db,
        &state.media_dir,
        &png,
        state.clock()?.now,
    )?))
}

/// File picker: the dialog hands back a path.
#[tauri::command]
pub async fn media_import_path(app: AppHandle, path: String) -> CmdResult<MediaInfo> {
    import_file(app, PathBuf::from(path), false).await
}

/// Settings → Data → "Clean up media" (SPEC §10): explicit only.
#[tauri::command]
pub fn media_cleanup(state: State<'_, AppState>) -> CmdResult<CleanupReport> {
    Ok(media::cleanup(&state.db, &state.media_dir)?)
}

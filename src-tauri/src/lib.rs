mod commands;
mod error;
mod media_server;
mod state;
mod sync;

use indigo_core::db::Db;
use indigo_core::vault;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Manager, WindowEvent};

use state::AppState;

/// `indigo --sync-state --data-dir DIR`: prints JSON for `uninstall.sh` and exits, so the
/// uninstaller can tell whether deleting local data would lose anything (SPEC §17).
fn print_sync_state(args: &[String]) -> Option<i32> {
    args.iter().position(|a| a == "--sync-state")?;
    let dir = args
        .iter()
        .position(|a| a == "--data-dir")
        .and_then(|i| args.get(i + 1))?;
    let result = (|| -> indigo_core::Result<serde_json::Value> {
        let db = Db::open(&std::path::Path::new(dir).join("indigo.sqlite"))?;
        let status = indigo_core::sync::status(&db)?;
        Ok(serde_json::json!({
            "configured": db.settings()?.sync.remote().is_some(),
            "last_success_at": status.last_success_at,
            "failed": status.failed,
            "pending": indigo_core::sync::pending(&db, &db.device_id()?)?,
        }))
    })();
    match result {
        Ok(v) => {
            println!("{v}");
            Some(0)
        }
        Err(e) => {
            eprintln!("{e}");
            Some(1)
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = print_sync_state(&args) {
        std::process::exit(code);
    }
    // `indigo --quit` (used by install.sh) asks the running instance to sync and exit; with no
    // running instance there is nothing to quit.
    if args.iter().any(|a| a == "--quit") && cfg!(debug_assertions) {
        return;
    }
    let mut builder = tauri::Builder::default();
    // Release only, so a dev build can run beside the installed app on its own XDG profile.
    if !cfg!(debug_assertions) {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--quit") {
                let state = app.state::<AppState>();
                if !state.quitting.swap(true, Ordering::SeqCst) {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.hide();
                    }
                    sync::sync_then_exit(app.clone());
                }
            } else if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            if args.iter().any(|a| a == "--quit") {
                // We are the only instance: nothing was running, so do not start.
                app.handle().exit(0);
                return Ok(());
            }
            // Release builds log too (to `<app data>/logs`), so sync and compression problems
            // leave a trace on the owner's machine.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db = Db::open(&data_dir.join("indigo.sqlite"))?;
            let device_id = db.device_id()?;
            indigo_core::settings::adopt_sync_setup(&db, &data_dir)?;
            let media_dir = vault::media_dir(&data_dir)?;
            app.manage(media_server::start(media_dir.clone())?);
            app.manage(AppState {
                db,
                data_dir,
                media_dir,
                device_id,
                syncing: AtomicBool::new(false),
                quitting: AtomicBool::new(false),
            });
            sync::spawn(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Quit trigger (SPEC §11.3): hide, sync, then exit. Unconfigured sync closes at once.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let configured = state.db.settings().is_ok_and(|s| s.sync.remote().is_some());
                if !configured || state.quitting.swap(true, Ordering::SeqCst) {
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                sync::sync_then_exit(app.clone());
            }
        })
        .invoke_handler(tauri::generate_handler![
            media_server::media_base_url,
            commands::categories::categories_list,
            commands::categories::category_save,
            commands::categories::category_delete,
            commands::cards::cards_query,
            commands::cards::card_get,
            commands::cards::card_save,
            commands::cards::card_warnings,
            commands::cards::card_delete,
            commands::cards::card_set_state,
            commands::cards::tag_rename,
            commands::cards::tag_delete,
            commands::media::media_import_bytes,
            commands::media::media_import_path,
            commands::media::media_import_clipboard,
            commands::media::media_cleanup,
            commands::data::backup_export,
            commands::data::backup_import,
            commands::data::pack_export,
            commands::data::pack_inspect,
            commands::data::pack_import,
            commands::sync::sync_status,
            commands::sync::sync_now,
            commands::sync::conflicts_list,
            commands::sync::conflict_restore,
            commands::sync::conflict_dismiss,
            commands::stats::stats_overview,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::sched_config_get,
            commands::settings::sched_config_set,
            commands::settings::sched_preview,
            commands::session::session_build,
            commands::session::session_cram,
            commands::session::attempt_start,
            commands::session::attempt_get,
            commands::session::attempt_step_show,
            commands::session::attempt_step_reveal,
            commands::session::attempt_step_complete,
            commands::session::grade_preview,
            commands::session::attempt_pause,
            commands::session::attempt_discard,
            commands::session::attempt_restart,
            commands::session::attempt_finish,
            commands::session::drill_record,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

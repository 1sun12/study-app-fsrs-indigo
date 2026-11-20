//! Sync triggers (SPEC §11.3) over the git transport in core: the vault directory is a git
//! repository pushed to the user's private GitHub repo. Before a sync, video and audio that
//! were never compressed here are compressed once (`media::compress_library`).

use std::sync::atomic::Ordering;
use std::time::Duration;

use indigo_core::git::{self, GitTransport};
use indigo_core::media;
use indigo_core::sync::{self, Paths, SyncStatus, SyncView};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

fn device_name() -> String {
    let name = gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string();
    if name.is_empty() {
        "device".into()
    } else {
        name
    }
}

pub fn view(app: &AppHandle) -> indigo_core::Result<SyncView> {
    let state = app.state::<AppState>();
    Ok(SyncView {
        configured: state.db.settings()?.sync.remote().is_some(),
        syncing: state.syncing.load(Ordering::SeqCst),
        status: sync::status(&state.db)?,
    })
}

fn emit(app: &AppHandle) {
    if let Ok(v) = view(app) {
        let _ = app.emit("sync-status", v);
    }
}

/// One sync on the calling thread. Unconfigured = nothing happens; a missing git is a skip,
/// not an error (§11.2 step 1).
fn sync_blocking(app: &AppHandle) -> indigo_core::Result<SyncStatus> {
    let state = app.state::<AppState>();
    let now = chrono::Utc::now();
    let Some(url) = state.db.settings()?.sync.remote() else {
        return sync::status(&state.db);
    };
    let outcome = if git::is_available() {
        let paths = Paths::new(&state.data_dir);
        media::compress_library(
            &state.db,
            &paths.media,
            &state.data_dir.join("tmp"),
            now,
            &state.device_id,
        )
        .and_then(|_| {
            let transport = GitTransport {
                dir: paths.vault.clone(),
                url,
                device_name: device_name(),
            };
            sync::run(
                &state.db,
                &paths,
                &transport,
                &state.device_id,
                &device_name(),
                now,
            )
        })
    } else {
        Ok(sync::SyncOutcome::Skipped {
            reason: "git is not installed; see docs/SYNC.md".into(),
        })
    };
    let outcome = match outcome {
        Err(e) if git::is_offline(&e.to_string()) => Ok(sync::SyncOutcome::Skipped {
            reason: "Offline; will sync when connected".into(),
        }),
        other => other,
    };
    if let Err(e) = &outcome {
        log::warn!("sync failed: {e}");
    }
    sync::record(&state.db, &outcome, now)
}

/// Starts a background sync unless one is running. Returns false if one already was.
pub fn spawn(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    if state.syncing.swap(true, Ordering::SeqCst) {
        return false;
    }
    emit(app);
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = sync_blocking(&app);
        app.state::<AppState>()
            .syncing
            .store(false, Ordering::SeqCst);
        emit(&app);
    });
    true
}

/// Quit trigger: waits for a running sync, runs a final one, then exits.
pub fn sync_then_exit(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        while state.syncing.swap(true, Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = sync_blocking(&app);
        state.syncing.store(false, Ordering::SeqCst);
        app.exit(0);
    });
}

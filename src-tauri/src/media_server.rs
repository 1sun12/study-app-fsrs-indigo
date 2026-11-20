//! Loopback HTTP server for media playback.
//!
//! WebKitGTK's GStreamer cannot fetch Tauri's `asset://` scheme (tauri#3725; confirmed in the
//! Phase 0 spike), so media is served from `127.0.0.1` on a random port with HTTP Range support,
//! under a per-launch random path token so other local processes cannot guess the URLs.

use std::path::PathBuf;

use axum::Router;
use tower_http::services::ServeDir;

/// Managed Tauri state: the base URL every media file is relative to.
pub struct MediaServer {
    pub base_url: String,
}

/// Binds the listener synchronously (so the port is known at setup time) and serves `dir`
/// on the Tauri async runtime. Only files inside `dir` are reachable; `ServeDir` rejects
/// path traversal.
pub fn start(dir: PathBuf) -> std::io::Result<MediaServer> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let app = Router::new().nest_service(&format!("/{token}"), ServeDir::new(dir));
    tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => {
                if let Err(e) = axum::serve(listener, app).await {
                    log::error!("media server stopped: {e}");
                }
            }
            Err(e) => log::error!("media server listener: {e}"),
        }
    });
    Ok(MediaServer {
        base_url: format!("http://127.0.0.1:{port}/{token}/"),
    })
}

/// Base URL for `<img>`, `<audio>` and `<video>` sources; append `<hh>/<sha256>.<ext>`.
#[tauri::command]
pub fn media_base_url(server: tauri::State<'_, MediaServer>) -> String {
    server.base_url.clone()
}

//! Serializable error for every command (SPEC §15: all return `Result<T, AppError>`).

#[derive(Debug, serde::Serialize)]
pub struct AppError {
    pub message: String,
}

impl From<indigo_core::CoreError> for AppError {
    fn from(e: indigo_core::CoreError) -> Self {
        AppError {
            message: e.to_string(),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError {
            message: e.to_string(),
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError {
            message: e.to_string(),
        }
    }
}

pub type CmdResult<T> = Result<T, AppError>;

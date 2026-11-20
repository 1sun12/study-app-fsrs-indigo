#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("migration: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("fsrs: {0}")]
    Fsrs(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

pub(crate) fn invalid(msg: impl Into<String>) -> CoreError {
    CoreError::Invalid(msg.into())
}

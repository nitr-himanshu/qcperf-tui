pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("libqcperf error {code}: {message}")]
    QcPerf { code: i32, message: String },
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid dashboard file: {0}")]
    Toml(String),
    #[error("invalid snapshot: {0}")]
    Json(String),
}

impl Error {
    pub fn message(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

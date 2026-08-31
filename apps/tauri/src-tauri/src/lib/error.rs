#[derive(Debug, Error)]
pub enum AppError {
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("window error: {0}")]
    Window(String),
    #[error("command error: {0}")]
    Command(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

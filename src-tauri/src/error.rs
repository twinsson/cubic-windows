use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Keyring error: {0}")]
    Keyring(String),

    #[error("Download cancelled")]
    Cancelled,

    #[error("Hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("Not signed in")]
    NotSignedIn,

    #[error(
        "Cubic needs its own Microsoft app ID. Create an Entra app named \"Cubic\", enable public client flows, paste the Application (client) ID in Settings, then submit it at https://aka.ms/mce-reviewappid"
    )]
    MissingClientId,

    #[error("This Microsoft account does not own Minecraft: Java Edition.")]
    GameNotOwned,

    #[error("No suitable Java runtime found (need major version {required})")]
    JavaNotFound { required: u32 },

    #[error("Instance not found: {0}")]
    InstanceNotFound(String),

    #[error("Version not found: {0}")]
    VersionNotFound(String),

    #[error("OAuth failed: {0}")]
    OAuth(String),

    #[error("Authentication failed: {0}")]
    Auth(String),
}

impl AppError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<keyring::Error> for AppError {
    fn from(value: keyring::Error) -> Self {
        Self::Keyring(value.to_string())
    }
}

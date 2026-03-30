use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization/deserialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("CSRF token missing from response headers")]
    CsrfTokenMissing,

    #[error("Rate limited by Roblox. Retry after backoff.")]
    RateLimited,

    #[error("Encryption/Decryption error: {0}")]
    Crypto(String),

    #[error("Keyring error: {0}")]
    Keyring(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Process error: {0}")]
    Process(String),

    #[error("Roblox API error ({status}): {message}")]
    RobloxApi { status: u16, message: String },
}

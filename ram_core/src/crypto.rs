//! Secure storage for `.ROBLOSECURITY` cookies.
//!
//! Two backends:
//! 1. **File-based AES-256-GCM** — encrypts an `AccountStore` JSON blob with a
//!    key derived from a user-supplied master password (PBKDF2-like via SHA-256
//!    stretching). The encrypted payload is stored as a single `.dat` file.
//! 2. **Windows Credential Manager** — stores each cookie individually via the
//!    `keyring` crate, keyed by Roblox user ID.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::error::CoreError;
use crate::models::AccountStore;

// ---------------------------------------------------------------------------
// File-based AES-256-GCM encryption
// ---------------------------------------------------------------------------

/// Derive a 256-bit key from a password using iterated SHA-256.
/// This is intentionally simple; swap for `argon2` if you want stronger KDF.
fn derive_key(password: &str) -> [u8; 32] {
    let mut hash = Sha256::digest(password.as_bytes());
    for _ in 0..100_000 {
        hash = Sha256::digest(&hash);
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    key
}

/// Encrypt the `AccountStore` to bytes: `nonce (12) || ciphertext`.
pub fn encrypt_store(store: &AccountStore, password: &str) -> Result<Vec<u8>, CoreError> {
    let plaintext = serde_json::to_vec(store)?;
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt bytes produced by `encrypt_store` back into an `AccountStore`.
pub fn decrypt_store(data: &[u8], password: &str) -> Result<AccountStore, CoreError> {
    if data.len() < 13 {
        return Err(CoreError::Crypto("encrypted data too short".into()));
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CoreError::Crypto("decryption failed - wrong password?".into()))?;

    let store: AccountStore = serde_json::from_slice(&plaintext)?;
    Ok(store)
}

/// Save encrypted store to a file.
pub fn save_encrypted(
    path: &Path,
    store: &AccountStore,
    password: &str,
) -> Result<(), CoreError> {
    let bytes = encrypt_store(store, password)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Load and decrypt store from a file.
pub fn load_encrypted(path: &Path, password: &str) -> Result<AccountStore, CoreError> {
    let bytes = std::fs::read(path)?;
    decrypt_store(&bytes, password)
}

// ---------------------------------------------------------------------------
// Windows Credential Manager backend
// ---------------------------------------------------------------------------

const SERVICE_NAME: &str = "RM-Rust";

/// Store a single cookie in the OS credential store.
pub fn credential_store(user_id: u64, cookie: &str) -> Result<(), CoreError> {
    let entry = keyring::Entry::new(SERVICE_NAME, &user_id.to_string())
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    entry
        .set_password(cookie)
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    Ok(())
}

/// Retrieve a cookie from the OS credential store.
pub fn credential_load(user_id: u64) -> Result<String, CoreError> {
    let entry = keyring::Entry::new(SERVICE_NAME, &user_id.to_string())
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    entry
        .get_password()
        .map_err(|e| CoreError::Keyring(e.to_string()))
}

/// Delete a cookie from the OS credential store.
pub fn credential_delete(user_id: u64) -> Result<(), CoreError> {
    let entry = keyring::Entry::new(SERVICE_NAME, &user_id.to_string())
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    entry
        .delete_credential()
        .map_err(|e| CoreError::Keyring(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Cookie encryption helpers (for in-memory Account struct serialization)
// ---------------------------------------------------------------------------

/// Encrypt a single cookie string, returning a Base64 blob.
pub fn encrypt_cookie(cookie: &str, password: &str) -> Result<String, CoreError> {
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, cookie.as_bytes())
        .map_err(|e| CoreError::Crypto(e.to_string()))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypt a Base64-encoded cookie blob.
pub fn decrypt_cookie(encoded: &str, password: &str) -> Result<String, CoreError> {
    let data = B64
        .decode(encoded)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;
    if data.len() < 13 {
        return Err(CoreError::Crypto("encrypted cookie too short".into()));
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Crypto(e.to_string()))?;
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CoreError::Crypto("cookie decryption failed".into()))?;

    String::from_utf8(plaintext).map_err(|e| CoreError::Crypto(e.to_string()))
}

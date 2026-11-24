//! Claude API key storage in the OS keychain. The key never touches SQLite or logs.

use crate::{Error, Result};

const SERVICE: &str = "aroh";
const CLAUDE_USER: &str = "claude_api_key";

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, CLAUDE_USER).map_err(keychain_error)
}

fn keychain_error(err: keyring::Error) -> Error {
    Error::Keychain(match err {
        keyring::Error::NoStorageAccess(_) => "no access to keychain storage".into(),
        keyring::Error::PlatformFailure(_) => "keychain platform failure".into(),
        other => other.to_string(),
    })
}

/// Returns the stored Claude API key, or `None` when no key is saved.
pub fn claude_api_key() -> Result<Option<String>> {
    match entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(keychain_error(err)),
    }
}

/// Saves `key` as the Claude API key, replacing any previous one. Rejects empty keys.
pub fn set_claude_api_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() || key.len() > 512 {
        return Err(Error::Keychain("api key must be 1 to 512 characters".into()));
    }
    entry()?.set_password(key).map_err(keychain_error)
}

/// Deletes the stored Claude API key. Succeeds when no key was saved.
pub fn delete_claude_api_key() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(keychain_error(err)),
    }
}

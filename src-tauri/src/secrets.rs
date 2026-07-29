//! Storage for sync credentials.
//!
//! Passwords never go in `device.json` or `settings.json` on desktop — those
//! are plaintext files that end up in backups and screen shares. They live in
//! the OS keychain instead (Keychain on macOS, Credential Manager on Windows,
//! Secret Service on Linux).
//!
//! Android has no keyring backend, so there credentials fall back to a file in
//! the app-private data directory. That directory is sandboxed per-app and not
//! readable by other apps, which is a comparable threat model for this use —
//! it is not, however, hardware-backed the way Keystore would be.

const SERVICE: &str = "com.mh968.note-manager";

#[cfg(not(target_os = "android"))]
pub fn set_password(account: &str, password: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|e| format!("Failed to open keychain: {e}"))?;

    if password.is_empty() {
        return match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Failed to clear password: {e}")),
        };
    }

    entry
        .set_password(password)
        .map_err(|e| format!("Failed to store password: {e}"))
}

#[cfg(not(target_os = "android"))]
pub fn get_password(account: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, account)
        .ok()?
        .get_password()
        .ok()
}

#[cfg(target_os = "android")]
fn fallback_path(account: &str) -> Result<std::path::PathBuf, String> {
    // Android's app data dir is resolved lazily here rather than passed in, so
    // the desktop path doesn't have to carry an AppHandle it never uses.
    let base = std::env::var("HOME").map_err(|_| "No home directory".to_string())?;
    let dir = std::path::PathBuf::from(base).join(".note-manager-credentials");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create credential dir: {e}"))?;
    Ok(dir.join(format!("{}.secret", crate::notes::sanitize_id(account))))
}

#[cfg(target_os = "android")]
pub fn set_password(account: &str, password: &str) -> Result<(), String> {
    let path = fallback_path(account)?;
    if password.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::write(&path, password).map_err(|e| format!("Failed to store password: {e}"))
}

#[cfg(target_os = "android")]
pub fn get_password(account: &str) -> Option<String> {
    std::fs::read_to_string(fallback_path(account).ok()?).ok()
}

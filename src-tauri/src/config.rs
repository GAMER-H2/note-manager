use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

/// Directory (inside the vault) holding everything the app maintains *about*
/// the notes: revision history, sync bookkeeping, and the export manifest.
/// It lives in the vault so it travels with the notes over any sync transport,
/// and it starts with a `.` so the folder scanners skip it — see
/// `is_hidden_path` in `notes.rs`.
pub const META_DIR: &str = ".notemanager";

/// Machine-local configuration: which vault this install points at, and the
/// identity it stamps onto note ids and history entries.
///
/// Deliberately kept out of the vault (and out of `settings.json`) because it
/// describes *this device*. If it ever synced, every device would adopt the
/// same id and note ids would start colliding again — the exact problem the
/// device suffix exists to prevent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    /// Transport discriminator: "folder" today, "webdav" in Phase 5.
    pub kind: String,
    /// Folder remotes: absolute path to the shared directory (an rclone or
    /// Syncthing folder, an NFS/SMB mount, a docker bind mount).
    #[serde(default)]
    pub path: String,
    /// WebDAV remotes: base URL and username. The password is never stored
    /// here — see the keychain handling in Phase 5.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub device_id: String,
    /// `None` means "use the built-in default" (`app_data_dir/notes`), so the
    /// default keeps working if the app data dir ever moves.
    #[serde(default)]
    pub vault_path: Option<String>,
    #[serde(default)]
    pub sync: Option<SyncConfig>,
}

/// What `set_vault_root` actually did, so the UI can report it accurately.
#[derive(Debug, Serialize, Deserialize)]
pub struct VaultChange {
    pub path: String,
    /// Existing notes were moved from the old vault into the new location.
    pub migrated: bool,
    /// The target already held a vault, so it was adopted as-is and nothing
    /// was moved (avoids silently merging two sets of notes).
    pub adopted: bool,
}

pub struct AppState {
    pub config: Mutex<DeviceConfig>,
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Six hex characters derived from install-time entropy (clock + pid + data
/// dir). Generated once and persisted, so it only has to be unique across the
/// handful of devices one person syncs — not globally — which is why this is
/// enough without pulling in a uuid/rand dependency.
fn generate_device_id(seed_path: &Path) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seed = format!("{nanos}-{}-{}", std::process::id(), seed_path.display());
    format!("{:06x}", fnv1a64(seed.as_bytes()) & 0x00ff_ffff)
}

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("device.json"))
}

/// The vault location used when the user hasn't chosen one.
pub fn default_vault_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("notes"))
}

/// Reads `device.json`, creating it (with a fresh device id) on first run.
pub fn load_or_init(app: &tauri::AppHandle) -> Result<DeviceConfig, String> {
    let path = config_path(app)?;

    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<DeviceConfig>(&raw) {
            return Ok(cfg);
        }
        // A corrupt config would otherwise wedge startup permanently. Fall
        // through and rebuild it; a regenerated device id is harmless (it only
        // affects the suffix on *future* note ids).
    }

    let cfg = DeviceConfig {
        device_id: generate_device_id(&path),
        vault_path: None,
        sync: None,
    };
    save(app, &cfg)?;
    Ok(cfg)
}

pub fn sync_config(app: &tauri::AppHandle) -> Option<SyncConfig> {
    app.state::<AppState>()
        .config
        .lock()
        .ok()
        .and_then(|cfg| cfg.sync.clone())
}

#[tauri::command]
pub fn get_sync_config(app: tauri::AppHandle) -> Result<Option<SyncConfig>, String> {
    Ok(sync_config(&app))
}

#[tauri::command]
pub fn set_sync_config(
    app: tauri::AppHandle,
    sync: Option<SyncConfig>,
) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "Device config lock poisoned".to_string())?;
        cfg.sync = sync;
        save(&app, &cfg)?;
    }
    Ok(())
}

pub fn save(app: &tauri::AppHandle, cfg: &DeviceConfig) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create data dir: {e}"))?;
    }
    let data = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Failed to serialize device config: {e}"))?;
    fs::write(&path, data).map_err(|e| format!("Failed to write device config: {e}"))
}

pub fn device_id(app: &tauri::AppHandle) -> String {
    app.state::<AppState>()
        .config
        .lock()
        .map(|cfg| cfg.device_id.clone())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Absolute path to the vault root. Every note/folder path in the app is
/// resolved against this, so changing it repoints the entire app.
pub fn vault_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let configured = app
        .state::<AppState>()
        .config
        .lock()
        .map_err(|_| "Device config lock poisoned".to_string())?
        .vault_path
        .clone();

    match configured {
        Some(p) if !p.trim().is_empty() => Ok(PathBuf::from(p)),
        _ => default_vault_root(app),
    }
}

/// Path to the vault's metadata directory, creating it on demand.
pub fn meta_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = vault_root(app)?.join(META_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create metadata dir: {e}"))?;
    Ok(dir)
}

/// Settings that describe the *vault* rather than the device, so they live in
/// the vault's metadata directory and travel over sync. A device joining a
/// vault adopts its conventions instead of imposing its own.
///
/// `updated_at` exists because a boolean has no merge: without it, two devices
/// disagreeing would each keep rewriting the other's choice on every sync.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSettings {
    /// Name note files `Title--id.md` rather than `id.md`, locally and on the
    /// remote. Off keeps the original id-only naming.
    #[serde(default)]
    pub titled_filenames: bool,
    /// Milliseconds since the epoch, stamped whenever a device changes this.
    #[serde(default)]
    pub updated_at: u64,
}

fn vault_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(meta_dir(app)?.join("vault-settings.json"))
}

pub fn read_vault_settings(app: &tauri::AppHandle) -> VaultSettings {
    vault_settings_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Raw JSON for the sync layer, which merges these as a file like any other.
pub fn read_vault_settings_raw(app: &tauri::AppHandle) -> Result<String, String> {
    let settings = read_vault_settings(app);
    serde_json::to_string(&settings).map_err(|e| format!("Failed to serialize vault settings: {e}"))
}

pub fn write_vault_settings_raw(app: &tauri::AppHandle, data: &str) -> Result<(), String> {
    let path = vault_settings_path(app)?;
    fs::write(&path, data).map_err(|e| format!("Failed to write vault settings: {e}"))
}

/// Whether note files carry their title. Read straight off disk rather than
/// cached: sync can change it mid-session when another device's newer choice
/// arrives.
pub fn titled_filenames(app: &tauri::AppHandle) -> bool {
    read_vault_settings(app).titled_filenames
}

#[tauri::command]
pub fn get_titled_filenames(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(titled_filenames(&app))
}

#[tauri::command]
pub fn set_titled_filenames(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let settings = VaultSettings {
        titled_filenames: enabled,
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };
    let json = serde_json::to_string(&settings)
        .map_err(|e| format!("Failed to serialize vault settings: {e}"))?;
    write_vault_settings_raw(&app, &json)
}

/// True when `dir` already holds a vault (notes or app metadata), meaning we
/// must not move another vault's contents on top of it.
fn holds_vault(dir: &Path) -> bool {
    if dir.join(META_DIR).is_dir() {
        return true;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    // A folder of notes looks like directories and/or `.md` files; anything
    // else (a stray `.DS_Store`, say) shouldn't count as "occupied".
    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_dir()
            || path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
    })
}

fn move_dir_contents(from: &Path, to: &Path) -> Result<(), String> {
    let entries = fs::read_dir(from).map_err(|e| format!("Failed to read old vault: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read old vault entry: {e}"))?;
        let src = entry.path();
        let Some(name) = src.file_name() else { continue };
        let dest = to.join(name);
        if dest.exists() {
            continue;
        }
        // `rename` fails across filesystems (a real case here: app data dir on
        // `/` and the new vault on an external/mounted drive), so fall back to
        // a recursive copy-then-remove.
        if fs::rename(&src, &dest).is_err() {
            copy_recursive(&src, &dest)?;
            let removed = if src.is_dir() {
                fs::remove_dir_all(&src)
            } else {
                fs::remove_file(&src)
            };
            removed.map_err(|e| format!("Failed to clear old vault entry: {e}"))?;
        }
    }
    Ok(())
}

fn copy_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if src.is_dir() {
        fs::create_dir_all(dest).map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
        let entries = fs::read_dir(src).map_err(|e| format!("Failed to read {}: {e}", src.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read dir entry: {e}"))?;
            let Some(name) = entry.path().file_name().map(|n| n.to_owned()) else {
                continue;
            };
            copy_recursive(&entry.path(), &dest.join(name))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest)
            .map(|_| ())
            .map_err(|e| format!("Failed to copy {}: {e}", src.display()))
    }
}

#[tauri::command]
pub fn get_vault_root(app: tauri::AppHandle) -> Result<String, String> {
    Ok(vault_root(&app)?.to_string_lossy().to_string())
}

/// Points the app at a different vault directory.
///
/// If the target is empty, the current vault's contents are moved into it. If
/// the target already holds notes, it's adopted as-is — merging two vaults
/// blindly is how you end up with duplicated or clobbered notes, so that case
/// is left to the import flow instead.
#[tauri::command]
pub fn set_vault_root(app: tauri::AppHandle, path: String) -> Result<VaultChange, String> {
    let target = PathBuf::from(path.trim());
    if target.as_os_str().is_empty() {
        return Err("Vault path cannot be empty.".to_string());
    }
    if target.is_relative() {
        return Err("Vault path must be absolute.".to_string());
    }

    fs::create_dir_all(&target).map_err(|e| format!("Failed to create vault directory: {e}"))?;

    let current = vault_root(&app)?;
    let canonical_target = target.canonicalize().unwrap_or_else(|_| target.clone());
    let canonical_current = current.canonicalize().unwrap_or_else(|_| current.clone());

    let mut change = VaultChange {
        path: canonical_target.to_string_lossy().to_string(),
        migrated: false,
        adopted: false,
    };

    if canonical_target != canonical_current {
        if holds_vault(&canonical_target) {
            change.adopted = true;
        } else if canonical_current.is_dir() {
            move_dir_contents(&canonical_current, &canonical_target)?;
            change.migrated = true;
        }
    }

    {
        let state = app.state::<AppState>();
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "Device config lock poisoned".to_string())?;
        cfg.vault_path = Some(change.path.clone());
        save(&app, &cfg)?;
    }

    Ok(change)
}

/// Resets the vault location back to the app data directory, moving notes back.
#[tauri::command]
pub fn reset_vault_root(app: tauri::AppHandle) -> Result<VaultChange, String> {
    let default = default_vault_root(&app)?;
    fs::create_dir_all(&default).map_err(|e| format!("Failed to create vault directory: {e}"))?;
    set_vault_root(app, default.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_device_id(app: tauri::AppHandle) -> Result<String, String> {
    Ok(device_id(&app))
}

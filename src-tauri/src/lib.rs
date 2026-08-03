mod archive;
mod config;
mod diff;
mod history;
mod notes;
mod secrets;
mod sync;
mod webdav;

use std::{fs, path::PathBuf, sync::Mutex};
use tauri::Manager;

fn reminders_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    Ok(base.join("reminders.json"))
}

/// Returns the raw JSON string of the reminders map (noteId -> config), or
/// "{}" if none have been saved yet. The frontend owns the JSON shape.
#[tauri::command]
fn get_reminders(app: tauri::AppHandle) -> Result<String, String> {
    let path = reminders_path(&app)?;
    match fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("{}".to_string()),
        Err(e) => Err(format!("Failed to read reminders: {e}")),
    }
}

/// Persists the reminders map as-is. `data` is the JSON produced by the frontend.
#[tauri::command]
fn set_reminders(app: tauri::AppHandle, data: String) -> Result<(), String> {
    let path = reminders_path(&app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create data dir: {e}"))?;
    }
    fs::write(&path, data).map_err(|e| format!("Failed to write reminders: {e}"))?;
    Ok(())
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    Ok(base.join("settings.json"))
}

/// Returns the raw JSON string of user settings, or "{}" if none saved yet.
#[tauri::command]
fn get_settings(app: tauri::AppHandle) -> Result<String, String> {
    let path = settings_path(&app)?;
    match fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("{}".to_string()),
        Err(e) => Err(format!("Failed to read settings: {e}")),
    }
}

/// Persists the settings object as-is. `data` is the JSON produced by the frontend.
#[tauri::command]
fn set_settings(app: tauri::AppHandle, data: String) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create data dir: {e}"))?;
    }
    fs::write(&path, data).map_err(|e| format!("Failed to write settings: {e}"))?;
    Ok(())
}

fn pinned_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    Ok(base.join("pinned.json"))
}

/// Returns the raw JSON string of pinned note ids (a JSON array), or "[]" if
/// none have been saved yet. The frontend owns the JSON shape.
#[tauri::command]
fn get_pinned(app: tauri::AppHandle) -> Result<String, String> {
    let path = pinned_path(&app)?;
    match fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("[]".to_string()),
        Err(e) => Err(format!("Failed to read pinned notes: {e}")),
    }
}

/// Persists the pinned note ids as-is. `data` is the JSON produced by the frontend.
#[tauri::command]
fn set_pinned(app: tauri::AppHandle, data: String) -> Result<(), String> {
    let path = pinned_path(&app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create data dir: {e}"))?;
    }
    fs::write(&path, data).map_err(|e| format!("Failed to write pinned notes: {e}"))?;
    Ok(())
}

/// Raw JSON of the pinned/reminder stores, for the archive exporter.
pub(crate) fn read_pinned_raw(app: &tauri::AppHandle) -> Result<String, String> {
    get_pinned(app.clone())
}

pub(crate) fn read_reminders_raw(app: &tauri::AppHandle) -> Result<String, String> {
    get_reminders(app.clone())
}

pub(crate) fn write_pinned_raw(app: &tauri::AppHandle, data: &str) -> Result<(), String> {
    set_pinned(app.clone(), data.to_string())
}

pub(crate) fn write_reminders_raw(app: &tauri::AppHandle, data: &str) -> Result<(), String> {
    set_reminders(app.clone(), data.to_string())
}

/// Folds an imported pinned list into the existing one. Union rather than
/// replace: importing an archive shouldn't unpin notes it didn't know about.
pub(crate) fn merge_pinned(
    app: &tauri::AppHandle,
    incoming: &serde_json::Value,
) -> Result<(), String> {
    let Some(incoming) = incoming.as_array() else {
        return Ok(());
    };

    let existing_raw = get_pinned(app.clone())?;
    let mut merged: Vec<serde_json::Value> = serde_json::from_str(&existing_raw).unwrap_or_default();

    for id in incoming {
        if !merged.iter().any(|e| e == id) {
            merged.push(id.clone());
        }
    }

    set_pinned(
        app.clone(),
        serde_json::to_string(&merged).map_err(|e| format!("Failed to serialize pinned: {e}"))?,
    )
}

/// Folds imported reminders into the existing map. Existing entries win, since
/// a reminder already scheduled on this device is live and the imported one is
/// not.
pub(crate) fn merge_reminders(
    app: &tauri::AppHandle,
    incoming: &serde_json::Value,
) -> Result<(), String> {
    let Some(incoming) = incoming.as_object() else {
        return Ok(());
    };

    let existing_raw = get_reminders(app.clone())?;
    let mut merged: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&existing_raw).unwrap_or_default();

    for (note_id, cfg) in incoming {
        merged.entry(note_id.clone()).or_insert_with(|| cfg.clone());
    }

    set_reminders(
        app.clone(),
        serde_json::to_string(&merged).map_err(|e| format!("Failed to serialize reminders: {e}"))?,
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            notes::create_note,
            notes::update_note,
            notes::delete_note,
            notes::list_notes,
            notes::move_note,
            notes::list_folders,
            notes::create_folder,
            notes::restyle_note_filenames,
            config::get_vault_root,
            config::set_vault_root,
            config::reset_vault_root,
            config::get_device_id,
            history::list_revisions,
            history::get_revision,
            history::restore_revision,
            history::diff_revisions,
            history::clear_history,
            archive::export_vault,
            archive::export_preview,
            archive::import_vault,
            config::get_sync_config,
            config::set_sync_config,
            config::get_titled_filenames,
            config::set_titled_filenames,
            sync::sync_now,
            sync::test_sync_remote,
            sync::has_stored_password,
            sync::get_last_sync,
            get_reminders,
            set_reminders,
            get_settings,
            set_settings,
            get_pinned,
            set_pinned
        ])
        .setup(|app| {
            let handle = app.handle();

            // Device config first — every path below resolves through it, since
            // the vault may live outside the app data dir.
            let device_config = config::load_or_init(handle)?;
            app.manage(config::AppState {
                config: Mutex::new(device_config),
            });

            // Ensure notes directory + default folder exist, and migrate any
            // pre-existing flat notes into it, at startup.
            let dir = notes::notes_dir(handle)?;
            fs::create_dir_all(&dir).map_err(tauri::Error::Io)?;
            notes::ensure_default_folder(handle)?;
            notes::migrate_stray_notes(handle)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

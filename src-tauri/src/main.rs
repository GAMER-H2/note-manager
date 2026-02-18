#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::Manager;
use tauri_plugin_notification::init as notification_init;

#[derive(Debug, Serialize, Deserialize)]
struct CreateNoteResponse {
    /// Stable identifier for the note (used as filename stem).
    id: String,
    /// Absolute path to the created markdown file on disk.
    path: String,
    /// Initial content written to disk.
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateNoteRequest {
    id: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeleteNoteRequest {
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NoteRecord {
    id: String,
    path: String,
    content: String,
}

fn notes_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    Ok(base.join("notes"))
}

fn sanitize_id(raw: &str) -> String {
    // Keep it simple and filesystem-friendly. Also prevents path traversal.
    // Allow only: a-z A-Z 0-9 _ -
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "note".to_string()
    } else {
        out
    }
}

fn note_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.md"))
}

fn is_md_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

fn generate_id() -> String {
    // No external crate: use milliseconds since UNIX_EPOCH.
    // This is "unique enough" for a local notes app; frontend can treat as opaque.
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("note_{ms}")
}

#[tauri::command]
fn list_notes(app: tauri::AppHandle) -> Result<Vec<NoteRecord>, String> {
    let dir = notes_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create notes dir: {e}"))?;

    let mut notes = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read notes dir: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read notes dir entry: {e}"))?;
        let path = entry.path();

        if !path.is_file() || !is_md_file(&path) {
            continue;
        }

        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        if id.is_empty() {
            continue;
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read note content ({}): {e}", path.display()))?;

        notes.push(NoteRecord {
            id,
            path: path.to_string_lossy().to_string(),
            content,
        });
    }

    // Deterministic order: newest-looking first (assuming your ids are note_<ms>)
    notes.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(notes)
}

#[tauri::command]
fn create_note(app: tauri::AppHandle) -> Result<CreateNoteResponse, String> {
    let dir = notes_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create notes dir: {e}"))?;

    let id = sanitize_id(&generate_id());
    let path = note_path(&dir, &id);

    // Default content (empty note). You can change this to include a title/frontmatter.
    let content = String::new();

    // Create exclusively; if collision (very unlikely), try a few more times.
    // We avoid adding rand/uuid crates to keep it minimal.
    const MAX_TRIES: usize = 5;
    let mut attempt = 0usize;
    let final_path = loop {
        let candidate = if attempt == 0 {
            path.clone()
        } else {
            note_path(&dir, &sanitize_id(&format!("{id}_{attempt}")))
        };

        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(content.as_bytes())
                    .map_err(|e| format!("Failed to write note: {e}"))?;
                break candidate;
            }
            Err(e) => {
                attempt += 1;
                if attempt >= MAX_TRIES {
                    return Err(format!(
                        "Failed to create note file after {MAX_TRIES} tries: {e}"
                    ));
                }
            }
        }
    };

    Ok(CreateNoteResponse {
        id: final_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&id)
            .to_string(),
        path: final_path.to_string_lossy().to_string(),
        content,
    })
}

#[tauri::command]
fn update_note(app: tauri::AppHandle, req: UpdateNoteRequest) -> Result<(), String> {
    let dir = notes_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create notes dir: {e}"))?;

    let id = sanitize_id(&req.id);
    let path = note_path(&dir, &id);

    fs::write(&path, req.content).map_err(|e| format!("Failed to write note file: {e}"))?;
    Ok(())
}

#[tauri::command]
fn delete_note(app: tauri::AppHandle, req: DeleteNoteRequest) -> Result<(), String> {
    let dir = notes_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create notes dir: {e}"))?;

    let id = sanitize_id(&req.id);
    let path = note_path(&dir, &id);

    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete note file: {e}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn main() {
    tauri::Builder::default()
        .plugin(notification_init())
        .invoke_handler(tauri::generate_handler![
            create_note,
            update_note,
            delete_note,
            list_notes
        ])
        .setup(|app| {
            // Ensure notes directory exists at startup.
            let dir = notes_dir(app.handle())?;
            fs::create_dir_all(&dir).map_err(|e| tauri::Error::Io(e))?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

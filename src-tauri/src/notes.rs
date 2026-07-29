use crate::config;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// The folder every new note falls into unless another is specified, and the
/// only folder guaranteed to exist.
pub const DEFAULT_FOLDER: &str = "General";

#[derive(Debug, Serialize, Deserialize)]
pub struct NoteRecord {
    pub id: String,
    pub path: String,
    pub content: String,
    pub folder: String,
    /// Last modified time, milliseconds since the epoch.
    pub mtime: u64,
    /// SHA-256 of the content, hex encoded. Sync uses this to tell "this note
    /// changed" from "this note was merely rewritten with identical bytes".
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteNoteRequest {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoveNoteRequest {
    pub id: String,
    pub folder: String,
}

/// Hex SHA-256 of note content. Shared by the note list, history, and sync so
/// they all agree on what "unchanged" means.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn mtime_millis(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn notes_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    config::vault_root(app)
}

/// Dot-prefixed entries are app/system bookkeeping (`.notemanager`, `.git`,
/// `.DS_Store`), never user content. Every directory walk has to skip them or
/// the metadata directory shows up in the sidebar as a folder and its history
/// files get scanned as notes.
fn is_hidden_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

pub fn sanitize_id(raw: &str) -> String {
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

// Folder names become real directory names, so they're user-facing (unlike note
// ids) — keep spaces/punctuation but strip path-traversal and cross-platform
// reserved characters. Sanitizes a single path segment (no `/`).
pub fn sanitize_folder_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_control() {
            continue;
        }
        if matches!(ch, '/' | '\\' | '.' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            continue;
        }
        out.push(ch);
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        DEFAULT_FOLDER.to_string()
    } else {
        trimmed.to_string()
    }
}

// A folder is now potentially a nested path ("Work/Projects/ClientA"). Sanitize
// each segment individually and drop empty ones — this is what neutralizes `..`,
// leading/trailing slashes, and doubled slashes.
pub fn sanitize_folder_path(raw: &str) -> String {
    let segments: Vec<String> = raw
        .split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(sanitize_folder_name)
        .collect();

    if segments.is_empty() {
        DEFAULT_FOLDER.to_string()
    } else {
        segments.join("/")
    }
}

fn note_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.md"))
}

pub fn folder_dir(app: &tauri::AppHandle, folder: &str) -> Result<PathBuf, String> {
    Ok(notes_dir(app)?.join(sanitize_folder_path(folder)))
}

pub fn ensure_default_folder(app: &tauri::AppHandle) -> Result<(), String> {
    let dir = folder_dir(app, DEFAULT_FOLDER)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create default folder: {e}"))
}

// One-time migration: notes used to live flat in `notes_dir()`. Move any
// stragglers into `General/` so every note has a folder.
pub fn migrate_stray_notes(app: &tauri::AppHandle) -> Result<(), String> {
    let root = notes_dir(app)?;
    fs::create_dir_all(&root).map_err(|e| format!("Failed to create notes dir: {e}"))?;
    ensure_default_folder(app)?;
    let general = folder_dir(app, DEFAULT_FOLDER)?;

    let entries = fs::read_dir(&root).map_err(|e| format!("Failed to read notes dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read notes dir entry: {e}"))?;
        let path = entry.path();
        if !path.is_file() || !is_md_file(&path) || is_hidden_path(&path) {
            continue;
        }
        if let Some(file_name) = path.file_name() {
            let dest = general.join(file_name);
            if !dest.exists() {
                fs::rename(&path, &dest)
                    .map_err(|e| format!("Failed to migrate note {}: {e}", path.display()))?;
            }
        }
    }
    Ok(())
}

// Relative directory path (using `/`, regardless of platform) from `root` to `dir`.
fn relative_folder(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root)
        .ok()
        .map(|rel| {
            rel.components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect::<Vec<_>>()
                .join("/")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_FOLDER.to_string())
}

// Recursively scans subdirectories of `dir` (at any depth) for `<id>.md`. Notes
// are looked up this way (rather than requiring callers to track folders) so
// `update_note`/`delete_note` keep working after a note has been moved, and so
// notes in nested subfolders are found too.
fn find_note_in_dir(root: &Path, dir: &Path, id: &str) -> Result<Option<(PathBuf, String)>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read notes dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read notes dir entry: {e}"))?;
        let path = entry.path();
        if is_hidden_path(&path) {
            continue;
        }
        if path.is_dir() {
            if let Some(found) = find_note_in_dir(root, &path, id)? {
                return Ok(Some(found));
            }
        } else if path.is_file()
            && is_md_file(&path)
            && path.file_stem().and_then(|s| s.to_str()) == Some(id)
        {
            let folder = relative_folder(root, dir);
            return Ok(Some((path, folder)));
        }
    }
    Ok(None)
}

pub fn find_note_path(app: &tauri::AppHandle, id: &str) -> Result<(PathBuf, String), String> {
    let id = sanitize_id(id);
    let root = notes_dir(app)?;
    find_note_in_dir(&root, &root, &id)?.ok_or_else(|| format!("Note '{id}' not found"))
}

fn is_md_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

/// `note_<millis>_<device>`. The device suffix is what makes ids safe to sync:
/// without it, two devices creating a note in the same millisecond while
/// offline would produce the same filename, and sync would silently treat two
/// different notes as one.
fn generate_id(app: &tauri::AppHandle) -> String {
    use std::time::SystemTime;
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("note_{ms}_{}", config::device_id(app))
}

// Recursively walks `dir` (at any depth under `root`) collecting note records.
fn collect_notes(root: &Path, dir: &Path, out: &mut Vec<NoteRecord>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read notes dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read notes dir entry: {e}"))?;
        let path = entry.path();

        if is_hidden_path(&path) {
            continue;
        }

        if path.is_dir() {
            collect_notes(root, &path, out)?;
            continue;
        }

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

        out.push(NoteRecord {
            id,
            path: path.to_string_lossy().to_string(),
            hash: content_hash(&content),
            mtime: mtime_millis(&path),
            content,
            folder: relative_folder(root, dir),
        });
    }
    Ok(())
}

#[tauri::command]
pub fn list_notes(app: tauri::AppHandle) -> Result<Vec<NoteRecord>, String> {
    let root = notes_dir(&app)?;
    fs::create_dir_all(&root).map_err(|e| format!("Failed to create notes dir: {e}"))?;

    let mut notes = Vec::new();
    collect_notes(&root, &root, &mut notes)?;

    // Deterministic order: newest-looking first (assuming your ids are note_<ms>)
    notes.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(notes)
}

#[tauri::command]
pub fn create_note(app: tauri::AppHandle, folder: Option<String>) -> Result<NoteRecord, String> {
    let folder = sanitize_folder_path(folder.as_deref().unwrap_or(DEFAULT_FOLDER));
    let dir = folder_dir(&app, &folder)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create folder dir: {e}"))?;

    let id = sanitize_id(&generate_id(&app));
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

    Ok(NoteRecord {
        id: final_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&id)
            .to_string(),
        path: final_path.to_string_lossy().to_string(),
        hash: content_hash(&content),
        mtime: mtime_millis(&final_path),
        content,
        folder,
    })
}

#[tauri::command]
pub fn update_note(app: tauri::AppHandle, req: UpdateNoteRequest) -> Result<(), String> {
    let (path, _folder) = find_note_path(&app, &req.id)?;
    fs::write(&path, req.content).map_err(|e| format!("Failed to write note file: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn delete_note(app: tauri::AppHandle, req: DeleteNoteRequest) -> Result<(), String> {
    match find_note_path(&app, &req.id) {
        Ok((path, _folder)) => match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Failed to delete note file: {e}")),
        },
        Err(_) => Ok(()),
    }
}

#[tauri::command]
pub fn move_note(app: tauri::AppHandle, req: MoveNoteRequest) -> Result<NoteRecord, String> {
    let (src_path, current_folder) = find_note_path(&app, &req.id)?;
    let target_folder = sanitize_folder_path(&req.folder);

    if target_folder == current_folder {
        let content = fs::read_to_string(&src_path)
            .map_err(|e| format!("Failed to read note content: {e}"))?;
        return Ok(NoteRecord {
            id: sanitize_id(&req.id),
            path: src_path.to_string_lossy().to_string(),
            hash: content_hash(&content),
            mtime: mtime_millis(&src_path),
            content,
            folder: current_folder,
        });
    }

    let target_dir = folder_dir(&app, &target_folder)?;
    fs::create_dir_all(&target_dir).map_err(|e| format!("Failed to create folder dir: {e}"))?;

    let id = sanitize_id(&req.id);
    let dest_path = note_path(&target_dir, &id);
    fs::rename(&src_path, &dest_path).map_err(|e| format!("Failed to move note: {e}"))?;

    let content = fs::read_to_string(&dest_path)
        .map_err(|e| format!("Failed to read note content: {e}"))?;

    Ok(NoteRecord {
        id,
        path: dest_path.to_string_lossy().to_string(),
        hash: content_hash(&content),
        mtime: mtime_millis(&dest_path),
        content,
        folder: target_folder,
    })
}

// Recursively walks `dir` (at any depth under `root`) collecting every
// subfolder's full relative path (e.g. "Work", "Work/Projects").
fn collect_folders(
    root: &Path,
    dir: &Path,
    prefix: &str,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read notes dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read notes dir entry: {e}"))?;
        let path = entry.path();
        if !path.is_dir() || is_hidden_path(&path) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let full = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        out.push(full.clone());
        collect_folders(root, &path, &full, out)?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_folders(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    ensure_default_folder(&app)?;
    let root = notes_dir(&app)?;

    let mut paths = Vec::new();
    collect_folders(&root, &root, "", &mut paths)?;

    // "General" always first, then alphabetical (which also keeps every
    // subfolder path sorted directly after its parent, since a path is always
    // a string-prefix of its descendants).
    paths.sort_by(
        |a, b| match (a.as_str() == DEFAULT_FOLDER, b.as_str() == DEFAULT_FOLDER) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        },
    );

    Ok(paths)
}

#[tauri::command]
pub fn create_folder(
    app: tauri::AppHandle,
    name: String,
    parent: Option<String>,
) -> Result<String, String> {
    let sanitized_name = sanitize_folder_name(&name);
    let full_path = match parent.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => format!("{}/{}", sanitize_folder_path(p), sanitized_name),
        None => sanitized_name,
    };

    if full_path.split('/').any(|seg| seg.eq_ignore_ascii_case("pinned")) {
        return Err("\"Pinned\" is a reserved name.".to_string());
    }

    let dir = notes_dir(&app)?.join(&full_path);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create folder: {e}"))?;
    Ok(full_path)
}

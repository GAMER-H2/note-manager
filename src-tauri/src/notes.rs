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

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteFolderRequest {
    pub folder: String,
    /// "delete" removes the notes (and their history) along with the folder;
    /// "move" relocates every note into General before removing the folder.
    pub mode: String,
}

/// Hex SHA-256 of note content. Shared by the note list, history, and sync so
/// they all agree on what "unchanged" means.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Derives a note's title from its first non-empty line.
///
/// This MUST stay in step with `firstLineTitle` in `src/lib/notes.js` — that
/// function is what note-links resolve `folder/title` against, and this one is
/// what names files in a zip export and labels revisions. If they drift, an
/// exported vault's links stop matching its filenames.
pub fn first_line_title(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n");
    let text = normalized.trim();
    if text.is_empty() {
        return "Untitled".to_string();
    }

    let first_line = text.split('\n').next().unwrap_or("").trim();
    if first_line.is_empty() {
        return "Untitled".to_string();
    }

    // Mirrors /^#{1,6}\s+/: up to six hashes, and only when followed by
    // whitespace (so "#######x" and "#x" are both left alone).
    let hashes = first_line.chars().take_while(|c| *c == '#').count();
    let stripped = if (1..=6).contains(&hashes)
        && first_line
            .chars()
            .nth(hashes)
            .map(|c| c.is_whitespace())
            .unwrap_or(false)
    {
        first_line[hashes..].trim_start()
    } else {
        first_line
    };

    let title: String = stripped.chars().take(80).collect();
    if title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        title
    }
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

/// Separates a note's readable title from its id in a filename. Two dashes
/// because single ones are common inside titles; `sanitize_title_for_filename`
/// collapses dash runs so the separator can never appear on the left side, and
/// splitting on the last occurrence is therefore unambiguous.
const TITLE_ID_SEP: &str = "--";

/// Longest title fragment kept in a filename. Filesystems cap names near 255
/// bytes and the id plus separator plus `.md` has to fit alongside it.
const MAX_TITLE_LEN: usize = 60;

/// Reduces a note's title to something safe as one filename segment: no path
/// separators, no cross-platform reserved characters, no dash runs (which would
/// make `TITLE_ID_SEP` ambiguous), and no runaway length.
pub fn sanitize_title_for_filename(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_dash = false;

    for ch in raw.chars() {
        // Whitespace becomes a space rather than being dropped: a tab is a
        // control character, and discarding it would weld "b\tc" into "bc".
        if ch.is_whitespace() {
            out.push(' ');
            prev_dash = false;
            continue;
        }
        if ch.is_control() {
            continue;
        }
        if matches!(ch, '/' | '\\' | '.' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            continue;
        }
        if ch == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
        } else {
            prev_dash = false;
        }
        out.push(ch);
    }

    // Collapse internal whitespace runs too, so "a    b" doesn't become a
    // filename with a four-space gap in it.
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated: String = collapsed.chars().take(MAX_TITLE_LEN).collect();
    let trimmed = truncated.trim().trim_matches('-').trim();

    if trimmed.is_empty() {
        "Untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The filename stem a note should have: `Title--id` when titled filenames are
/// on, plain `id` when they're off.
fn note_file_stem(title: &str, id: &str, titled: bool) -> String {
    if titled {
        format!("{}{TITLE_ID_SEP}{id}", sanitize_title_for_filename(title))
    } else {
        id.to_string()
    }
}

/// Recovers the id from a filename stem written in either layout. The id is
/// the note's identity — sync, history, pins and reminders all key off it — so
/// this has to stay exact.
pub fn id_from_stem(stem: &str) -> &str {
    match stem.rsplit_once(TITLE_ID_SEP) {
        Some((_, id)) => id,
        None => stem,
    }
}

/// Whether a filename stem belongs to `id`.
///
/// Deliberately defined in terms of `id_from_stem` rather than a suffix test:
/// `collect_notes` derives the ids the rest of the app sees from that same
/// function, so anything looser would let `find_note_path` resolve an id the
/// note list never reports — and the two must not disagree about identity.
fn stem_matches_id(stem: &str, id: &str) -> bool {
    id_from_stem(stem) == id
}

fn note_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.md"))
}

/// Filename a note should carry under the current style. For callers that write
/// note files directly rather than going through `create_note` — the zip
/// importer — so imports land with the same naming as everything else.
pub fn note_file_name(app: &tauri::AppHandle, content: &str, id: &str) -> String {
    note_file_name_with(content, id, config::titled_filenames(app))
}

/// As `note_file_name`, but with the style passed in — for callers that resolve
/// it once and then name many files, like a sync pass.
pub fn note_file_name_with(content: &str, id: &str, titled: bool) -> String {
    format!("{}.md", note_file_stem(&first_line_title(content), id, titled))
}

/// Absolute path a note should live at, given its content and the current
/// filename style.
fn titled_note_path(dir: &Path, content: &str, id: &str, titled: bool) -> PathBuf {
    dir.join(format!(
        "{}.md",
        note_file_stem(&first_line_title(content), id, titled)
    ))
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
            && path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem_matches_id(stem, id))
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
            .map(id_from_stem)
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
    let titled = config::titled_filenames(&app);

    // Default content (empty note). You can change this to include a title/frontmatter.
    let content = String::new();
    let path = titled_note_path(&dir, &content, &id, titled);

    // Create exclusively; if collision (very unlikely), try a few more times.
    // We avoid adding rand/uuid crates to keep it minimal.
    const MAX_TRIES: usize = 5;
    let mut attempt = 0usize;
    let final_path = loop {
        let candidate = if attempt == 0 {
            path.clone()
        } else {
            titled_note_path(
                &dir,
                &content,
                &sanitize_id(&format!("{id}_{attempt}")),
                titled,
            )
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
            .map(id_from_stem)
            .unwrap_or(&id)
            .to_string(),
        path: final_path.to_string_lossy().to_string(),
        hash: content_hash(&content),
        mtime: mtime_millis(&final_path),
        content,
        folder,
    })
}

/// Returns the note's path, which may have changed: retitling a note renames
/// its file, and the caller is holding the old path until we say otherwise.
#[tauri::command]
pub fn update_note(app: tauri::AppHandle, req: UpdateNoteRequest) -> Result<String, String> {
    let (path, _folder) = find_note_path(&app, &req.id)?;

    // Capture what's on disk *before* overwriting it, so a note that predates
    // version history still gets its original content into the timeline.
    let previous = fs::read_to_string(&path).unwrap_or_default();
    let previous_mtime = mtime_millis(&path);
    crate::history::ensure_baseline(&app, &req.id, &previous, previous_mtime)?;

    fs::write(&path, &req.content).map_err(|e| format!("Failed to write note file: {e}"))?;

    // Editing the first line retitles the note, so the filename has to follow.
    // Best-effort for the same reason as history below: the content is already
    // safely on disk, and a stale filename is not worth failing the save over.
    let final_path = match rename_to_match_title(&app, &path, &sanitize_id(&req.id), &req.content) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to rename note {}: {e}", req.id);
            path
        }
    };

    // History is best-effort: a failure here must not make the user think their
    // note failed to save, because it did save.
    if let Err(e) = crate::history::snapshot(&app, &req.id, &req.content) {
        eprintln!("Failed to snapshot note {}: {e}", req.id);
    }
    Ok(final_path.to_string_lossy().to_string())
}

/// Renames `path` so its filename matches `content`'s title under the current
/// naming style. No-op when the name is already right, which is the common case
/// (only the first line of a note affects its filename).
fn rename_to_match_title(
    app: &tauri::AppHandle,
    path: &Path,
    id: &str,
    content: &str,
) -> Result<PathBuf, String> {
    let Some(dir) = path.parent() else {
        return Ok(path.to_path_buf());
    };
    let desired = titled_note_path(dir, content, id, config::titled_filenames(app));
    // A different note already owning the name would mean two ids collided,
    // which the id suffix is there to prevent — but never clobber if it does.
    if desired == path || desired.exists() {
        return Ok(path.to_path_buf());
    }
    fs::rename(path, &desired).map_err(|e| format!("Failed to rename note file: {e}"))?;
    Ok(desired)
}

/// Renames every note in the vault to match the current filename style, and
/// reports how many actually moved. Run after the setting is toggled so existing
/// notes adopt the new naming instead of only newly-created ones.
///
/// Safe to run repeatedly, and safe with sync configured: sync identifies notes
/// by id and writes remote paths from the id alone, so local renames don't
/// register as deletes on the remote.
#[tauri::command]
pub fn restyle_note_filenames(app: tauri::AppHandle) -> Result<usize, String> {
    let root = notes_dir(&app)?;
    let titled = config::titled_filenames(&app);

    let mut notes = Vec::new();
    collect_notes(&root, &root, &mut notes)?;

    let mut renamed = 0usize;
    for note in notes {
        let path = PathBuf::from(&note.path);
        let Some(dir) = path.parent() else { continue };

        let desired = titled_note_path(dir, &note.content, &note.id, titled);
        if desired == path || desired.exists() {
            continue;
        }
        match fs::rename(&path, &desired) {
            Ok(()) => renamed += 1,
            // One stubborn file shouldn't abort the whole pass.
            Err(e) => eprintln!("Failed to rename {}: {e}", path.display()),
        }
    }
    Ok(renamed)
}

#[tauri::command]
pub fn delete_note(app: tauri::AppHandle, req: DeleteNoteRequest) -> Result<(), String> {
    match find_note_path(&app, &req.id) {
        Ok((path, _folder)) => {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Failed to delete note file: {e}")),
            }
            // Otherwise the deleted note's full text lingers in the vault.
            crate::history::forget(&app, &req.id)
        }
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
    // Carry the existing filename across rather than rebuilding it: a move
    // shouldn't strip a note's title out of its name.
    let dest_path = match src_path.file_name() {
        Some(name) => target_dir.join(name),
        None => note_path(&target_dir, &id),
    };
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

/// Deletes a folder (and its subfolders). The notes it contains are either
/// deleted outright or flattened into General first, depending on `req.mode`.
/// Returns the ids of the notes that lived under the folder, so the frontend
/// can reconcile its pin/reminder stores.
///
/// Subfolders are never preserved: in "move" mode the notes are unpacked into
/// General and the (now note-free) directory tree is removed wholesale.
#[tauri::command]
pub fn delete_folder(app: tauri::AppHandle, req: DeleteFolderRequest) -> Result<Vec<String>, String> {
    let folder = sanitize_folder_path(&req.folder);
    if folder == DEFAULT_FOLDER {
        return Err("The General folder can't be deleted.".to_string());
    }

    let root = notes_dir(&app)?;
    let target = folder_dir(&app, &folder)?;
    if !target.is_dir() {
        return Err(format!("Folder '{folder}' not found"));
    }

    // Enumerate the notes under the folder subtree up front, both to report them
    // back and (in move mode) to relocate them.
    let mut notes = Vec::new();
    collect_notes(&root, &target, &mut notes)?;
    let ids: Vec<String> = notes.iter().map(|n| n.id.clone()).collect();

    match req.mode.as_str() {
        "move" => {
            let general = folder_dir(&app, DEFAULT_FOLDER)?;
            fs::create_dir_all(&general)
                .map_err(|e| format!("Failed to ensure General folder: {e}"))?;
            for note in &notes {
                let src = PathBuf::from(&note.path);
                let Some(name) = src.file_name() else { continue };
                let mut dest = general.join(name);
                // Note ids are unique, so a clash here would only be a note that
                // is somehow already in General — never overwrite it.
                if dest.exists() && dest != src {
                    dest = general.join(format!("{}.md", note.id));
                }
                fs::rename(&src, &dest)
                    .map_err(|e| format!("Failed to move note into General: {e}"))?;
            }
        }
        "delete" => {
            for id in &ids {
                // Best-effort: the note files go with the directory below either
                // way; a lingering history entry shouldn't abort the delete.
                if let Err(e) = crate::history::forget(&app, id) {
                    eprintln!("Failed to forget history for {id}: {e}");
                }
            }
        }
        other => return Err(format!("Unknown delete mode '{other}'")),
    }

    fs::remove_dir_all(&target).map_err(|e| format!("Failed to remove folder: {e}"))?;
    Ok(ids)
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

    if full_path.split('/').any(|seg| {
        seg.eq_ignore_ascii_case("pinned") || seg.eq_ignore_ascii_case("archive")
    }) {
        return Err("\"Pinned\" and \"Archive\" are reserved names.".to_string());
    }

    let dir = notes_dir(&app)?.join(&full_path);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create folder: {e}"))?;
    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titled_stems_round_trip_back_to_the_id() {
        let id = "note_1700000000000_a1b2c3";
        let stem = note_file_stem("Project Kickoff", id, true);
        assert_eq!(stem, format!("Project Kickoff--{id}"));
        assert_eq!(id_from_stem(&stem), id);
    }

    #[test]
    fn plain_id_stems_still_resolve() {
        // Notes written before the setting existed must keep working.
        let id = "note_1700000000000_a1b2c3";
        assert_eq!(id_from_stem(id), id);
        assert!(stem_matches_id(id, id));
    }

    #[test]
    fn dashes_in_titles_cannot_forge_the_separator() {
        // A title containing "--" would otherwise make the split ambiguous.
        let stem = note_file_stem("Draft -- v2 --- final", "abc123", true);
        assert!(!stem.trim_end_matches("--abc123").contains("--"));
        assert_eq!(id_from_stem(&stem), "abc123");
    }

    #[test]
    fn lookup_agrees_with_the_id_the_note_list_would_report() {
        // Generated ids never contain the separator, but if one somehow did,
        // resolution must still match what `collect_notes` derives — only the
        // segment after the final separator counts, for both.
        let stem = "Some Title--we--ird";
        assert_eq!(id_from_stem(stem), "ird");
        assert!(stem_matches_id(stem, "ird"));
        assert!(!stem_matches_id(stem, "we--ird"));
    }

    #[test]
    fn titles_are_stripped_of_path_and_reserved_characters() {
        assert_eq!(
            sanitize_title_for_filename("../etc/passwd:*?\"<>|"),
            "etcpasswd"
        );
        assert!(!sanitize_title_for_filename("a/b\\c").contains('/'));
    }

    #[test]
    fn blank_and_symbol_only_titles_fall_back_to_untitled() {
        assert_eq!(sanitize_title_for_filename(""), "Untitled");
        assert_eq!(sanitize_title_for_filename("   "), "Untitled");
        assert_eq!(sanitize_title_for_filename("---"), "Untitled");
        assert_eq!(sanitize_title_for_filename("..."), "Untitled");
    }

    #[test]
    fn long_titles_are_truncated_without_splitting_a_character() {
        let title = "é".repeat(200);
        let out = sanitize_title_for_filename(&title);
        assert_eq!(out.chars().count(), MAX_TITLE_LEN);
    }

    #[test]
    fn whitespace_runs_collapse() {
        assert_eq!(sanitize_title_for_filename("a    b\tc"), "a b c");
    }

    #[test]
    fn markdown_heading_titles_lose_their_hashes_before_naming() {
        let stem = note_file_stem(&first_line_title("# My Note\n\nbody"), "id1", true);
        assert_eq!(stem, "My Note--id1");
    }

    #[test]
    fn disabling_the_setting_yields_a_bare_id() {
        assert_eq!(note_file_stem("Anything", "id1", false), "id1");
    }
}

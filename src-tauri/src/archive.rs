use crate::{config, notes};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Read, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

/// Bumped when the archive layout changes in a way importers must know about.
const SCHEMA_VERSION: u32 = 1;

const MANIFEST_ENTRY: &str = ".notemanager/manifest.json";
const HISTORY_PREFIX: &str = ".notemanager/history/";

/// Refuse absurd archives rather than trying to expand them. A notes vault is
/// megabytes; anything near this is either not ours or a zip bomb.
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestNote {
    id: String,
    folder: String,
    /// Path of this note inside the archive, e.g. "Work/Project Kickoff.md".
    file: String,
    title: String,
    hash: String,
    mtime: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: u32,
    exported_at: u64,
    device: String,
    notes: Vec<ManifestNote>,
    /// Verbatim JSON from the pinned/reminder stores, so their shapes stay
    /// owned by the frontend exactly as they are at rest.
    pinned: serde_json::Value,
    reminders: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub notes: usize,
    pub bytes: usize,
    pub included_history: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub folders: usize,
    pub restored_history: usize,
    /// True when the archive had no manifest, so ids/pins/reminders were not
    /// recoverable and everything came in as new notes.
    pub without_manifest: bool,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Makes a note title usable as a filename.
///
/// Unlike `sanitize_folder_name` this keeps dots (titles like "v1.2 plan" are
/// normal), but strips *leading* dots so a title can never produce a hidden
/// file — which the vault scanners would then skip, silently losing the note
/// on a round-trip.
fn sanitize_file_stem(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_control() {
            continue;
        }
        if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            continue;
        }
        out.push(ch);
    }
    let trimmed = out.trim().trim_start_matches('.').trim();
    if trimmed.is_empty() {
        "Untitled".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// Picks a filename unique within its folder. Two notes can legitimately share
/// a title (the note-link feature resolves such a link to whichever it finds
/// first), so the archive has to disambiguate them itself.
fn unique_file_name(used: &mut HashSet<String>, folder: &str, stem: &str) -> String {
    let mut candidate = format!("{stem}.md");
    let mut n = 2;
    while !used.insert(format!("{folder}\u{0}{}", candidate.to_lowercase())) {
        candidate = format!("{stem} ({n}).md");
        n += 1;
    }
    candidate
}

#[tauri::command]
pub fn export_vault(
    app: tauri::AppHandle,
    include_history: bool,
) -> Result<tauri::ipc::Response, String> {
    let records = notes::list_notes(app.clone())?;
    let mut zip = ZipWriter::new(Cursor::new(Vec::<u8>::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut used_names: HashSet<String> = HashSet::new();
    let mut manifest_notes = Vec::with_capacity(records.len());

    for record in &records {
        let stem = sanitize_file_stem(&notes::first_line_title(&record.content));
        let file_name = unique_file_name(&mut used_names, &record.folder, &stem);
        let entry = format!("{}/{}", record.folder, file_name);

        zip.start_file(&entry, options)
            .map_err(|e| format!("Failed to add {entry} to archive: {e}"))?;
        zip.write_all(record.content.as_bytes())
            .map_err(|e| format!("Failed to write {entry}: {e}"))?;

        manifest_notes.push(ManifestNote {
            id: record.id.clone(),
            folder: record.folder.clone(),
            file: entry,
            title: notes::first_line_title(&record.content),
            hash: record.hash.clone(),
            mtime: record.mtime,
        });
    }

    // Empty folders would otherwise vanish on a round-trip, since a zip only
    // records the paths of the files in it.
    for folder in notes::list_folders(app.clone())? {
        let dir_entry = format!("{folder}/");
        if !manifest_notes.iter().any(|n| n.folder == folder) {
            zip.add_directory(&dir_entry, options)
                .map_err(|e| format!("Failed to add folder {folder}: {e}"))?;
        }
    }

    if include_history {
        let history_dir = config::meta_dir(&app)?.join("history");
        if let Ok(entries) = fs::read_dir(&history_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let Ok(data) = fs::read(&path) else { continue };
                let entry_name = format!("{HISTORY_PREFIX}{name}");
                zip.start_file(&entry_name, options)
                    .map_err(|e| format!("Failed to add history {name}: {e}"))?;
                zip.write_all(&data)
                    .map_err(|e| format!("Failed to write history {name}: {e}"))?;
            }
        }
    }

    let manifest = Manifest {
        schema: SCHEMA_VERSION,
        exported_at: now_ms(),
        device: config::device_id(&app),
        notes: manifest_notes,
        pinned: serde_json::from_str(&crate::read_pinned_raw(&app)?)
            .unwrap_or(serde_json::Value::Array(vec![])),
        reminders: serde_json::from_str(&crate::read_reminders_raw(&app)?)
            .unwrap_or(serde_json::Value::Object(Default::default())),
    };

    zip.start_file(MANIFEST_ENTRY, options)
        .map_err(|e| format!("Failed to add manifest: {e}"))?;
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
    zip.write_all(&manifest_json)
        .map_err(|e| format!("Failed to write manifest: {e}"))?;

    let buffer = zip
        .finish()
        .map_err(|e| format!("Failed to finalize archive: {e}"))?;

    Ok(tauri::ipc::Response::new(buffer.into_inner()))
}

/// Rebuilds a safe vault-relative path from an archive entry.
///
/// The archive's own path string is never used directly — each segment is
/// re-sanitized and `.`/`..` are dropped, which is what neutralizes zip-slip
/// (`../../.ssh/authorized_keys`) and absolute paths. Returns
/// `(folder, file_stem)`.
fn safe_entry_path(raw: &str) -> Option<(String, String)> {
    let normalized = raw.replace('\\', "/");
    let mut segments: Vec<&str> = normalized
        .split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect();

    let file = segments.pop()?;
    let stem = file.strip_suffix(".md").or_else(|| file.strip_suffix(".MD"))?;
    let stem = sanitize_file_stem(stem);

    let folder = if segments.is_empty() {
        notes::DEFAULT_FOLDER.to_string()
    } else {
        notes::sanitize_folder_path(&segments.join("/"))
    };

    Some((folder, stem))
}

#[tauri::command]
pub fn import_vault(
    app: tauri::AppHandle,
    data: Vec<u8>,
    replace: bool,
) -> Result<ImportSummary, String> {
    let reader = Cursor::new(data);
    let mut zip = ZipArchive::new(reader).map_err(|e| format!("Not a readable zip file: {e}"))?;

    let total: u64 = (0..zip.len())
        .filter_map(|i| zip.by_index_raw(i).ok().map(|f| f.size()))
        .sum();
    if total > MAX_UNCOMPRESSED_BYTES {
        return Err("Archive is too large to import.".to_string());
    }

    // Manifest first, so ids and metadata are known before any note is written.
    let manifest: Option<Manifest> = zip
        .by_name(MANIFEST_ENTRY)
        .ok()
        .and_then(|mut f| {
            let mut s = String::new();
            f.read_to_string(&mut s).ok().map(|_| s)
        })
        .and_then(|s| serde_json::from_str(&s).ok());

    let ids_by_file: HashMap<String, (String, String)> = manifest
        .as_ref()
        .map(|m| {
            m.notes
                .iter()
                .map(|n| (n.file.clone(), (n.id.clone(), n.folder.clone())))
                .collect()
        })
        .unwrap_or_default();

    let root = notes::notes_dir(&app)?;

    if replace {
        // Clear existing notes but keep .notemanager/, so history survives an
        // ill-judged replace and the sync state isn't invalidated.
        let entries = fs::read_dir(&root).map_err(|e| format!("Failed to read vault: {e}"))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false);
            if hidden {
                continue;
            }
            let removed = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            removed.map_err(|e| format!("Failed to clear vault: {e}"))?;
        }
    }

    let existing_ids: HashSet<String> = notes::list_notes(app.clone())?
        .into_iter()
        .map(|n| n.id)
        .collect();

    let mut summary = ImportSummary {
        imported: 0,
        skipped: 0,
        folders: 0,
        restored_history: 0,
        without_manifest: manifest.is_none(),
    };
    let mut folders_seen: HashSet<String> = HashSet::new();
    let mut history_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut taken_ids: HashSet<String> = existing_ids.clone();

    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| format!("Failed to read archive entry: {e}"))?;
        let raw_name = file.name().to_string();

        if raw_name == MANIFEST_ENTRY {
            continue;
        }

        if let Some(stripped) = raw_name.strip_prefix(HISTORY_PREFIX) {
            // Keyed by note id, so it's collected now and written only after
            // ids have been resolved.
            if stripped.ends_with(".jsonl") && !stripped.contains('/') {
                let mut buf = Vec::new();
                if file.read_to_end(&mut buf).is_ok() {
                    history_entries.push((stripped.trim_end_matches(".jsonl").to_string(), buf));
                }
            }
            continue;
        }

        if file.is_dir() {
            let folder = notes::sanitize_folder_path(raw_name.trim_end_matches('/'));
            if folders_seen.insert(folder.clone()) {
                fs::create_dir_all(root.join(&folder))
                    .map_err(|e| format!("Failed to create folder {folder}: {e}"))?;
            }
            continue;
        }

        let Some((folder, stem)) = safe_entry_path(&raw_name) else {
            summary.skipped += 1;
            continue;
        };

        let mut content = String::new();
        if file.read_to_string(&mut content).is_err() {
            summary.skipped += 1; // not UTF-8 text; not a note
            continue;
        }

        // Prefer the manifest's folder over the path we derived, since the
        // manifest records what the app actually had.
        let (id, folder) = match ids_by_file.get(&raw_name) {
            Some((id, manifest_folder)) => (id.clone(), manifest_folder.clone()),
            None => (String::new(), folder),
        };

        // A colliding id means a *different* note already owns it, so mint a
        // new one rather than overwriting someone's note.
        let final_id = if id.is_empty() || taken_ids.contains(&id) {
            let mut candidate = format!(
                "note_{}_{}_{}",
                now_ms(),
                config::device_id(&app),
                summary.imported
            );
            while taken_ids.contains(&candidate) {
                candidate.push('x');
            }
            candidate
        } else {
            id
        };
        taken_ids.insert(final_id.clone());

        let dir = root.join(&folder);
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create folder {folder}: {e}"))?;
        folders_seen.insert(folder.clone());

        let _ = stem; // the title comes back out of the content, not the entry name
        let file_name = notes::note_file_name(&app, &content, &notes::sanitize_id(&final_id));
        fs::write(dir.join(file_name), &content)
            .map_err(|e| format!("Failed to write imported note: {e}"))?;
        summary.imported += 1;
    }

    // History is only meaningful for ids that survived import unchanged.
    for (note_id, data) in history_entries {
        if !taken_ids.contains(&note_id) {
            continue;
        }
        let dir = config::meta_dir(&app)?.join("history");
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create history dir: {e}"))?;
        let path = dir.join(format!("{}.jsonl", notes::sanitize_id(&note_id)));
        if fs::write(&path, &data).is_ok() {
            summary.restored_history += 1;
        }
    }

    if let Some(manifest) = manifest {
        crate::merge_pinned(&app, &manifest.pinned)?;
        crate::merge_reminders(&app, &manifest.reminders)?;
    }

    summary.folders = folders_seen.len();
    Ok(summary)
}

/// Number of notes and whether history exists, for the export confirmation UI.
#[tauri::command]
pub fn export_preview(app: tauri::AppHandle) -> Result<ExportSummary, String> {
    let records = notes::list_notes(app.clone())?;
    let history_dir: PathBuf = config::meta_dir(&app)?.join("history");
    let has_history = fs::read_dir(&history_dir)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);

    Ok(ExportSummary {
        notes: records.len(),
        bytes: records.iter().map(|r| r.content.len()).sum(),
        included_history: has_history,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_traversal_segments_from_entry_paths() {
        let (folder, stem) = safe_entry_path("../../../etc/passwd.md").unwrap();
        assert!(!folder.contains(".."));
        assert_eq!(folder, "etc");
        assert_eq!(stem, "passwd");
    }

    #[test]
    fn rejects_absolute_entry_paths() {
        let (folder, _) = safe_entry_path("/etc/cron.d/evil.md").unwrap();
        assert!(!folder.starts_with('/'));
        assert_eq!(folder, "etc/crond");
    }

    #[test]
    fn treats_backslashes_as_separators() {
        let (folder, stem) = safe_entry_path("..\\..\\Windows\\System32\\note.md").unwrap();
        assert!(!folder.contains(".."));
        assert!(!folder.contains('\\'));
        assert_eq!(stem, "note");
    }

    #[test]
    fn non_markdown_entries_are_rejected() {
        assert!(safe_entry_path("Work/payload.sh").is_none());
        assert!(safe_entry_path("Work/photo.png").is_none());
    }

    #[test]
    fn bare_note_lands_in_the_default_folder() {
        let (folder, stem) = safe_entry_path("loose.md").unwrap();
        assert_eq!(folder, notes::DEFAULT_FOLDER);
        assert_eq!(stem, "loose");
    }

    #[test]
    fn titles_never_produce_hidden_files() {
        assert_eq!(sanitize_file_stem(".ssh"), "ssh");
        assert_eq!(sanitize_file_stem("..."), "Untitled");
        assert_eq!(sanitize_file_stem("  "), "Untitled");
    }

    #[test]
    fn keeps_dots_inside_titles() {
        assert_eq!(sanitize_file_stem("v1.2 release plan"), "v1.2 release plan");
    }

    #[test]
    fn deduplicates_names_within_a_folder_only() {
        let mut used = HashSet::new();
        assert_eq!(unique_file_name(&mut used, "Work", "Notes"), "Notes.md");
        assert_eq!(unique_file_name(&mut used, "Work", "Notes"), "Notes (2).md");
        // Same title in a different folder is not a collision.
        assert_eq!(unique_file_name(&mut used, "Home", "Notes"), "Notes.md");
    }
}

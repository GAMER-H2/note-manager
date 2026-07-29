use crate::{config, diff, history, notes};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Where the shared metadata lives on the remote, mirroring the vault layout.
const REMOTE_META: &str = ".notemanager";

/// A file on the remote, addressed by a vault-relative path using `/`.
#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub path: String,
    /// Reconciliation is hash-based, not clock-based — remote clocks lie and
    /// mtimes don't survive every transport. Kept because transports report it
    /// cheaply and it's useful for diagnostics.
    #[allow(dead_code)]
    pub mtime: u64,
}

/// The one interface every sync transport implements.
///
/// Deliberately dumb — list/get/put/delete over relative paths — so the
/// reconciliation below is written once and Phase 5's WebDAV client slots in
/// without the algorithm knowing anything changed.
pub trait SyncRemote {
    fn list(&self) -> Result<Vec<RemoteEntry>, String>;
    fn get(&self, path: &str) -> Result<Vec<u8>, String>;
    fn put(&self, path: &str, data: &[u8]) -> Result<(), String>;
    fn delete(&self, path: &str) -> Result<(), String>;
    /// Stable identity for the remote, so pointing the app at a different one
    /// invalidates the merge base rather than merging against a stranger.
    fn id(&self) -> String;
}

/// A plain directory: an rclone/Syncthing folder, an NFS or SMB mount, a
/// docker bind mount, or a second local disk.
pub struct FolderRemote {
    root: PathBuf,
}

impl FolderRemote {
    pub fn new(root: &str) -> Result<Self, String> {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err("Sync folder must be an absolute path.".to_string());
        }
        fs::create_dir_all(&root).map_err(|e| format!("Failed to open sync folder: {e}"))?;
        Ok(Self { root })
    }

    /// Rebuilds a path from sanitized segments. The remote is shared and may
    /// have been written by anything, so its names get the same distrust the
    /// zip importer applies.
    fn resolve(&self, rel: &str) -> Result<PathBuf, String> {
        let mut out = self.root.clone();
        for segment in rel.split('/') {
            let segment = segment.trim();
            if segment.is_empty() || segment == "." || segment == ".." {
                continue;
            }
            out.push(segment);
        }
        if !out.starts_with(&self.root) {
            return Err(format!("Refusing to touch {rel} outside the sync folder."));
        }
        Ok(out)
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<RemoteEntry>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `.notemanager` is ours and must be walked; other dot-entries are the
        // host filesystem's business (.DS_Store, .git, Syncthing's .stfolder).
        if name.starts_with('.') && name != REMOTE_META {
            continue;
        }
        let rel = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };

        if path.is_dir() {
            walk(&path, &rel, out);
        } else {
            let mtime = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            out.push(RemoteEntry { path: rel, mtime });
        }
    }
}

impl SyncRemote for FolderRemote {
    fn list(&self) -> Result<Vec<RemoteEntry>, String> {
        let mut out = Vec::new();
        walk(&self.root, "", &mut out);
        Ok(out)
    }

    fn get(&self, path: &str) -> Result<Vec<u8>, String> {
        fs::read(self.resolve(path)?).map_err(|e| format!("Failed to read {path}: {e}"))
    }

    fn put(&self, path: &str, data: &[u8]) -> Result<(), String> {
        let target = self.resolve(path)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create remote folder: {e}"))?;
        }
        fs::write(&target, data).map_err(|e| format!("Failed to write {path}: {e}"))
    }

    fn delete(&self, path: &str) -> Result<(), String> {
        match fs::remove_file(self.resolve(path)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Failed to delete {path}: {e}")),
        }
    }

    fn id(&self) -> String {
        format!("folder:{}", self.root.display())
    }
}

/// What this device believes both sides agreed on at the end of the last sync.
/// This is the merge base: without it, "they changed it" and "I deleted it" are
/// indistinguishable, which is how naive sync deletes everything.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncState {
    #[serde(default)]
    remote_id: String,
    #[serde(default)]
    last_sync_at: u64,
    /// noteId -> hash agreed at last sync.
    #[serde(default)]
    notes: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    pub merged: usize,
    pub conflicts: usize,
    pub deleted_local: usize,
    pub deleted_remote: usize,
    pub errors: Vec<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn state_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(config::meta_dir(app)?.join("sync-state.json"))
}

fn load_state(app: &tauri::AppHandle, remote_id: &str) -> Result<SyncState, String> {
    let raw = match fs::read_to_string(state_path(app)?) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SyncState::default()),
        Err(e) => return Err(format!("Failed to read sync state: {e}")),
    };

    let state: SyncState = serde_json::from_str(&raw).unwrap_or_default();
    // A different remote means the recorded base describes an agreement with
    // somebody else. Treating it as ours would merge against the wrong text.
    if state.remote_id != remote_id {
        return Ok(SyncState::default());
    }
    Ok(state)
}

fn save_state(app: &tauri::AppHandle, state: &SyncState) -> Result<(), String> {
    let data =
        serde_json::to_string_pretty(state).map_err(|e| format!("Failed to serialize state: {e}"))?;
    fs::write(state_path(app)?, data).map_err(|e| format!("Failed to write sync state: {e}"))
}

/// Remote path a note occupies, mirroring the local layout.
fn remote_note_path(folder: &str, id: &str) -> String {
    format!("{folder}/{id}.md")
}

fn note_id_from_remote(path: &str) -> Option<(String, String)> {
    let stem = path.strip_suffix(".md")?;
    let (folder, id) = stem.rsplit_once('/')?;
    if folder.starts_with(REMOTE_META) {
        return None;
    }
    Some((folder.to_string(), id.to_string()))
}

/// Writes the remote's copy of a note alongside ours under a new id, for when
/// a merge can't be done safely. Nothing is lost and the user reconciles by
/// hand.
fn write_conflict_copy(
    app: &tauri::AppHandle,
    folder: &str,
    remote_content: &str,
    device: &str,
) -> Result<(), String> {
    let dir = notes::folder_dir(app, folder)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create folder: {e}"))?;

    let id = notes::sanitize_id(&format!("note_{}_{}_conflict", now_ms(), device));
    let stamp = chrono_ish_stamp(now_ms());
    let title = notes::first_line_title(remote_content);
    // Prepending a heading is what makes the copy findable — the note list and
    // note-links both key off the first line.
    let body = format!("{title} (conflicted copy from {device}, {stamp})\n\n{remote_content}");

    fs::write(dir.join(format!("{id}.md")), body)
        .map_err(|e| format!("Failed to write conflict copy: {e}"))
}

/// Minimal UTC "YYYY-MM-DD HH:MM" without pulling in a date crate.
fn chrono_ish_stamp(ms: u64) -> String {
    let secs = ms / 1000;
    let days = secs / 86_400;
    let tod = secs % 86_400;

    // Civil-from-days (Howard Hinnant's algorithm), epoch-shifted to 0000-03-01.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        tod / 3600,
        (tod % 3600) / 60
    )
}

/// Unions two pinned lists.
///
/// Pins are a set and unpinning is rare and low-stakes, so union never loses a
/// pin at the cost of not propagating an unpin. Losing a pin silently is the
/// worse failure.
fn merge_pinned_json(local: &str, remote: &str) -> String {
    let mut out: Vec<serde_json::Value> = serde_json::from_str(local).unwrap_or_default();
    let incoming: Vec<serde_json::Value> = serde_json::from_str(remote).unwrap_or_default();
    for id in incoming {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_string())
}

/// Unions two reminder maps, local winning on overlap — the local entry has a
/// notification actually scheduled on this device behind it.
fn merge_reminders_json(local: &str, remote: &str) -> String {
    let mut out: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(local).unwrap_or_default();
    let incoming: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(remote).unwrap_or_default();
    for (k, v) in incoming {
        out.entry(k).or_insert(v);
    }
    serde_json::to_string(&out).unwrap_or_else(|_| "{}".to_string())
}

fn run_sync(app: &tauri::AppHandle, remote: &dyn SyncRemote) -> Result<SyncReport, String> {
    let mut report = SyncReport::default();
    let device = config::device_id(app);
    let remote_id = remote.id();
    let mut state = load_state(app, &remote_id)?;

    let local_notes = notes::list_notes(app.clone())?;
    let local_by_id: HashMap<String, &notes::NoteRecord> =
        local_notes.iter().map(|n| (n.id.clone(), n)).collect();

    let remote_entries = remote.list()?;
    let mut remote_by_id: HashMap<String, (String, String)> = HashMap::new(); // id -> (folder, path)
    for entry in &remote_entries {
        if let Some((folder, id)) = note_id_from_remote(&entry.path) {
            remote_by_id.insert(id, (folder, entry.path.clone()));
        }
    }

    let ids: HashSet<String> = local_by_id
        .keys()
        .chain(remote_by_id.keys())
        .chain(state.notes.keys())
        .cloned()
        .collect();

    let mut next_state_notes: HashMap<String, String> = HashMap::new();

    for id in ids {
        let local = local_by_id.get(&id);
        let remote_ref = remote_by_id.get(&id);
        let base_hash = state.notes.get(&id).cloned();

        let remote_content = match remote_ref {
            Some((_, path)) => match remote.get(path) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => Some(s),
                    Err(_) => {
                        report.errors.push(format!("{id}: remote copy is not text"));
                        continue;
                    }
                },
                Err(e) => {
                    report.errors.push(format!("{id}: {e}"));
                    continue;
                }
            },
            None => None,
        };
        let remote_hash = remote_content.as_deref().map(notes::content_hash);

        match (local, remote_content.as_deref()) {
            // Present on both sides.
            (Some(local), Some(rc)) => {
                let lh = &local.hash;
                let rh = remote_hash.clone().unwrap_or_default();

                if *lh == rh {
                    next_state_notes.insert(id.clone(), rh);
                    continue;
                }

                let base = base_hash
                    .as_ref()
                    .and_then(|h| history::content_by_hash(app, &id, h));

                match base {
                    // Only one side moved away from the base: take that side.
                    // Unchanged here, changed there: take theirs.
                    Some(base) if notes::content_hash(&base) == *lh => {
                        apply_incoming(app, &id, rc)?;
                        report.pulled += 1;
                        next_state_notes.insert(id.clone(), rh);
                    }
                    // Changed here, unchanged there: keep ours.
                    Some(base) if notes::content_hash(&base) == rh => {
                        let path = remote_note_path(&local.folder, &id);
                        remote.put(&path, local.content.as_bytes())?;
                        report.pushed += 1;
                        next_state_notes.insert(id.clone(), lh.clone());
                    }
                    // Both moved: merge them.
                    Some(base) => {
                        let merged = diff::merge3(&base, &local.content, rc);
                        if merged.conflicted {
                            write_conflict_copy(app, &local.folder, rc, &device)?;
                            let path = remote_note_path(&local.folder, &id);
                            remote.put(&path, local.content.as_bytes())?;
                            report.conflicts += 1;
                            next_state_notes.insert(id.clone(), lh.clone());
                        } else {
                            apply_incoming(app, &id, &merged.text)?;
                            let path = remote_note_path(&local.folder, &id);
                            remote.put(&path, merged.text.as_bytes())?;
                            report.merged += 1;
                            next_state_notes.insert(id.clone(), notes::content_hash(&merged.text));
                        }
                    }
                    // No usable base (never synced, or the revision was pruned).
                    // Keeping both is the only choice that can't lose an edit.
                    None => {
                        write_conflict_copy(app, &local.folder, rc, &device)?;
                        let path = remote_note_path(&local.folder, &id);
                        remote.put(&path, local.content.as_bytes())?;
                        report.conflicts += 1;
                        next_state_notes.insert(id.clone(), lh.clone());
                    }
                }
            }

            // Local only.
            (Some(local), None) => {
                if base_hash.as_deref() == Some(local.hash.as_str()) {
                    // Tracked, unchanged here, gone there: they deleted it.
                    notes::delete_note(
                        app.clone(),
                        notes::DeleteNoteRequest { id: id.clone() },
                    )?;
                    report.deleted_local += 1;
                } else {
                    // New here, or edited here after they deleted it — an edit
                    // outranks a delete, so it comes back.
                    let path = remote_note_path(&local.folder, &id);
                    remote.put(&path, local.content.as_bytes())?;
                    report.pushed += 1;
                    next_state_notes.insert(id.clone(), local.hash.clone());
                }
            }

            // Remote only.
            (None, Some(rc)) => {
                let rh = remote_hash.clone().unwrap_or_default();
                let folder = remote_ref
                    .map(|(f, _)| f.clone())
                    .unwrap_or_else(|| notes::DEFAULT_FOLDER.to_string());

                if base_hash.as_deref() == Some(rh.as_str()) {
                    // Tracked, unchanged there, gone here: we deleted it.
                    if let Some((_, path)) = remote_ref {
                        remote.delete(path)?;
                    }
                    report.deleted_remote += 1;
                } else {
                    create_from_remote(app, &id, &folder, rc)?;
                    report.pulled += 1;
                    next_state_notes.insert(id.clone(), rh);
                }
            }

            (None, None) => {} // tracked but gone from both sides
        }
    }

    sync_meta_file(app, remote, "pinned.json", &mut report, merge_pinned_json)?;
    sync_meta_file(app, remote, "reminders.json", &mut report, merge_reminders_json)?;
    sync_history(app, remote, &mut report);

    state.remote_id = remote_id;
    state.last_sync_at = now_ms();
    state.notes = next_state_notes;
    save_state(app, &state)?;

    Ok(report)
}

/// Writes incoming content through `update_note` so it goes through history —
/// an incoming change is exactly the kind you want to be able to undo.
/// Counting is left to callers, since the same write means "pulled" in one
/// branch and "merged" in another.
fn apply_incoming(app: &tauri::AppHandle, id: &str, content: &str) -> Result<(), String> {
    notes::update_note(
        app.clone(),
        notes::UpdateNoteRequest {
            id: id.to_string(),
            content: content.to_string(),
        },
    )
}

fn create_from_remote(
    app: &tauri::AppHandle,
    id: &str,
    folder: &str,
    content: &str,
) -> Result<(), String> {
    let dir = notes::folder_dir(app, folder)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create folder: {e}"))?;
    fs::write(dir.join(format!("{}.md", notes::sanitize_id(id))), content)
        .map_err(|e| format!("Failed to write pulled note: {e}"))?;
    history::snapshot_forced(app, id, content)
}

/// Syncs one shared JSON file both ways using a union merge, so neither side
/// can silently drop the other's entries.
fn sync_meta_file(
    app: &tauri::AppHandle,
    remote: &dyn SyncRemote,
    name: &str,
    report: &mut SyncReport,
    merge: fn(&str, &str) -> String,
) -> Result<(), String> {
    let remote_path = format!("{REMOTE_META}/{name}");
    let local = match name {
        "pinned.json" => crate::read_pinned_raw(app)?,
        _ => crate::read_reminders_raw(app)?,
    };

    let remote_raw = remote
        .get(&remote_path)
        .ok()
        .and_then(|b| String::from_utf8(b).ok());

    let merged = match remote_raw {
        Some(r) => merge(&local, &r),
        None => local.clone(),
    };

    if let Err(e) = remote.put(&remote_path, merged.as_bytes()) {
        report.errors.push(format!("{name}: {e}"));
    }

    if merged != local {
        let result = match name {
            "pinned.json" => crate::write_pinned_raw(app, &merged),
            _ => crate::write_reminders_raw(app, &merged),
        };
        if let Err(e) = result {
            report.errors.push(format!("{name}: {e}"));
        }
    }
    Ok(())
}

/// Pushes and pulls revision files, unioning entries so history converges
/// rather than one device's timeline overwriting the other's.
fn sync_history(app: &tauri::AppHandle, remote: &dyn SyncRemote, report: &mut SyncReport) {
    let Ok(dir) = config::meta_dir(app).map(|d| d.join("history")) else {
        return;
    };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }

    let remote_files: Vec<String> = remote
        .list()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| {
            e.path
                .strip_prefix(&format!("{REMOTE_META}/history/"))
                .filter(|s| s.ends_with(".jsonl") && !s.contains('/'))
                .map(|s| s.to_string())
        })
        .collect();

    let local_files: Vec<String> = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .filter(|n| n.ends_with(".jsonl"))
                .collect()
        })
        .unwrap_or_default();

    let all: HashSet<String> = remote_files.into_iter().chain(local_files).collect();

    for name in all {
        let remote_path = format!("{REMOTE_META}/history/{name}");
        let local_path = dir.join(&name);

        let local_raw = fs::read_to_string(&local_path).unwrap_or_default();
        let remote_raw = remote
            .get(&remote_path)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default();

        let merged = history::merge_jsonl(&local_raw, &remote_raw);
        if merged != local_raw {
            let _ = fs::write(&local_path, &merged);
        }
        if merged != remote_raw {
            if let Err(e) = remote.put(&remote_path, merged.as_bytes()) {
                report.errors.push(format!("history {name}: {e}"));
            }
        }
    }
}

/// Account key under which a remote's password is filed in the keychain.
/// Keyed by URL and username so several servers can coexist.
fn credential_account(cfg: &config::SyncConfig) -> String {
    format!("webdav:{}:{}", cfg.url.trim_end_matches('/'), cfg.username)
}

fn build_remote(cfg: &config::SyncConfig) -> Result<Box<dyn SyncRemote>, String> {
    build_remote_with(cfg, None)
}

/// `password` is supplied only while testing a not-yet-saved remote; otherwise
/// it comes from the keychain, since it's never held in the config.
fn build_remote_with(
    cfg: &config::SyncConfig,
    password: Option<&str>,
) -> Result<Box<dyn SyncRemote>, String> {
    match cfg.kind.as_str() {
        "folder" => Ok(Box::new(FolderRemote::new(&cfg.path)?)),
        "webdav" => {
            let pass = match password {
                Some(p) => p.to_string(),
                None => crate::secrets::get_password(&credential_account(cfg)).unwrap_or_default(),
            };
            Ok(Box::new(crate::webdav::WebDavRemote::new(
                &cfg.url,
                &cfg.username,
                &pass,
            )?))
        }
        other => Err(format!("Unknown sync type '{other}'.")),
    }
}

#[tauri::command]
pub fn sync_now(app: tauri::AppHandle) -> Result<SyncReport, String> {
    let cfg = config::sync_config(&app).ok_or("Sync isn't set up yet.")?;
    let remote = build_remote(&cfg)?;
    run_sync(&app, remote.as_ref())
}

/// Verifies a remote is reachable and writable before the user saves it, and
/// files the password on success so it's only persisted once proven.
#[tauri::command]
pub fn test_sync_remote(
    cfg: config::SyncConfig,
    password: Option<String>,
) -> Result<String, String> {
    let remote = build_remote_with(&cfg, password.as_deref())?;
    let probe = format!("{REMOTE_META}/.probe");
    remote.put(&probe, b"ok")?;
    remote.delete(&probe)?;

    if cfg.kind == "webdav" {
        if let Some(p) = password {
            crate::secrets::set_password(&credential_account(&cfg), &p)?;
        }
    }

    Ok(format!("Connected to {}", remote.id()))
}

/// Whether a password is already on file, so the UI can show "saved" rather
/// than an empty box that looks like the credential was lost.
#[tauri::command]
pub fn has_stored_password(cfg: config::SyncConfig) -> Result<bool, String> {
    Ok(crate::secrets::get_password(&credential_account(&cfg))
        .map(|p| !p.is_empty())
        .unwrap_or(false))
}

#[tauri::command]
pub fn get_last_sync(app: tauri::AppHandle) -> Result<u64, String> {
    let remote_id = config::sync_config(&app)
        .and_then(|c| build_remote(&c).ok())
        .map(|r| r.id())
        .unwrap_or_default();
    Ok(load_state(&app, &remote_id)?.last_sync_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_union_keeps_both_sides() {
        let merged = merge_pinned_json(r#"["a","b"]"#, r#"["b","c"]"#);
        let ids: Vec<String> = serde_json::from_str(&merged).unwrap();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"c".to_string()));
    }

    #[test]
    fn reminder_merge_prefers_the_local_entry() {
        let merged = merge_reminders_json(r#"{"n1":{"at":"local"}}"#, r#"{"n1":{"at":"remote"},"n2":{"at":"x"}}"#);
        let map: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(map["n1"]["at"], "local");
        assert_eq!(map["n2"]["at"], "x");
    }

    #[test]
    fn remote_paths_map_back_to_note_ids() {
        let (folder, id) = note_id_from_remote("Work/Projects/note_1_ab.md").unwrap();
        assert_eq!(folder, "Work/Projects");
        assert_eq!(id, "note_1_ab");
        assert!(note_id_from_remote(".notemanager/history/x.jsonl").is_none());
    }

    #[test]
    fn stamp_formats_a_known_instant() {
        // 2021-01-01T00:00:00Z
        assert_eq!(chrono_ish_stamp(1_609_459_200_000), "2021-01-01 00:00");
    }

    fn temp_remote() -> (tempfile::TempDir, FolderRemote) {
        let dir = tempfile::tempdir().unwrap();
        let remote = FolderRemote::new(dir.path().to_str().unwrap()).unwrap();
        (dir, remote)
    }

    #[test]
    fn round_trips_a_file() {
        let (_dir, remote) = temp_remote();
        remote.put("General/note_1.md", b"hello").unwrap();
        assert_eq!(remote.get("General/note_1.md").unwrap(), b"hello");

        let listed = remote.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "General/note_1.md");

        remote.delete("General/note_1.md").unwrap();
        assert!(remote.list().unwrap().is_empty());
    }

    #[test]
    fn deleting_a_missing_file_is_not_an_error() {
        let (_dir, remote) = temp_remote();
        assert!(remote.delete("General/gone.md").is_ok());
    }

    #[test]
    fn traversal_in_remote_paths_is_neutralized() {
        let (dir, remote) = temp_remote();
        remote.put("../escaped.md", b"nope").unwrap();
        // The `..` is dropped rather than followed, so the write lands inside.
        assert!(dir.path().join("escaped.md").exists());
        assert!(!dir.path().parent().unwrap().join("escaped.md").exists());
    }

    #[test]
    fn listing_skips_foreign_dot_entries_but_keeps_ours() {
        let (dir, remote) = temp_remote();
        fs::create_dir_all(dir.path().join(".stfolder")).unwrap();
        fs::write(dir.path().join(".stfolder/marker"), b"x").unwrap();
        remote.put(".notemanager/pinned.json", b"[]").unwrap();

        let paths: Vec<String> = remote.list().unwrap().into_iter().map(|e| e.path).collect();
        assert!(paths.contains(&".notemanager/pinned.json".to_string()));
        assert!(!paths.iter().any(|p| p.contains(".stfolder")));
    }

    #[test]
    fn relative_sync_folders_are_rejected() {
        assert!(FolderRemote::new("relative/path").is_err());
    }
}

use crate::{config, notes};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Edits by the same device inside this window collapse into a single
/// revision. Autosave is debounced at 400ms, so without coalescing a sustained
/// typing session would produce thousands of near-identical entries.
const COALESCE_WINDOW_MS: u64 = 10 * 60 * 1000;

/// Revisions older than this are pruned, unless keeping them is needed to stay
/// above `KEEP_MIN`.
const MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// Always retain at least this many revisions, however old.
const KEEP_MIN: usize = 20;
/// Hard ceiling, so a very busy month can't grow a note's history without bound.
const KEEP_MAX: usize = 200;

/// One stored version of a note. Full content rather than a diff — notes are
/// kilobytes, and diff/patch machinery isn't worth the complexity or the risk
/// of a corrupt chain making every later revision unreadable.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Revision {
    rev: u64,
    /// Milliseconds since the epoch.
    ts: u64,
    device: String,
    hash: String,
    content: String,
}

/// Revision listing entry. Deliberately excludes `content` so opening the
/// history panel doesn't ship every stored version of the note to the frontend.
#[derive(Debug, Serialize)]
pub struct RevisionMeta {
    pub rev: u64,
    pub ts: u64,
    pub device: String,
    pub hash: String,
    pub bytes: usize,
    /// The note's title as of this revision, for the history list.
    pub title: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn history_path(app: &tauri::AppHandle, id: &str) -> Result<PathBuf, String> {
    let dir = config::meta_dir(app)?.join("history");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create history dir: {e}"))?;
    Ok(dir.join(format!("{}.jsonl", notes::sanitize_id(id))))
}

/// Reads a note's revisions, oldest first. A malformed line is skipped rather
/// than failing the whole read — losing one revision beats making the entire
/// history unreadable.
fn read_revisions(app: &tauri::AppHandle, id: &str) -> Result<Vec<Revision>, String> {
    let path = history_path(app, id)?;
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Failed to read history: {e}")),
    };

    Ok(raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Revision>(line).ok())
        .collect())
}

fn write_revisions(app: &tauri::AppHandle, id: &str, revs: &[Revision]) -> Result<(), String> {
    let path = history_path(app, id)?;
    let mut out = String::new();
    for rev in revs {
        let line = serde_json::to_string(rev)
            .map_err(|e| format!("Failed to serialize revision: {e}"))?;
        out.push_str(&line);
        out.push('\n');
    }
    fs::write(&path, out).map_err(|e| format!("Failed to write history: {e}"))
}

/// Keeps everything from the last 30 days, but never fewer than `KEEP_MIN` and
/// never more than `KEEP_MAX`, always preferring the newest.
fn prune(mut revs: Vec<Revision>) -> Vec<Revision> {
    let now = now_ms();
    let recent = revs
        .iter()
        .filter(|r| now.saturating_sub(r.ts) <= MAX_AGE_MS)
        .count();

    let keep = recent.max(KEEP_MIN).min(KEEP_MAX).min(revs.len());
    if keep < revs.len() {
        revs.drain(..revs.len() - keep);
    }
    revs
}

/// Records `content` as a revision.
///
/// `force` bypasses coalescing, for moments where the pre-change state must
/// survive no matter how recently it was written (restoring, and sync
/// overwrites).
fn record(app: &tauri::AppHandle, id: &str, content: &str, force: bool) -> Result<(), String> {
    let mut revs = read_revisions(app, id)?;
    let hash = notes::content_hash(content);

    // Identical content is not a new version.
    if revs.last().map(|r| r.hash == hash).unwrap_or(false) {
        return Ok(());
    }

    let device = config::device_id(app);
    let now = now_ms();
    let coalesce = !force
        && revs
            .last()
            .map(|last| last.device == device && now.saturating_sub(last.ts) <= COALESCE_WINDOW_MS)
            .unwrap_or(false);

    if coalesce {
        if let Some(last) = revs.last_mut() {
            last.content = content.to_string();
            last.hash = hash;
            last.ts = now;
        }
    } else {
        let rev = revs.last().map(|r| r.rev + 1).unwrap_or(1);
        revs.push(Revision {
            rev,
            ts: now,
            device,
            hash,
            content: content.to_string(),
        });
    }

    write_revisions(app, id, &prune(revs))
}

pub fn snapshot(app: &tauri::AppHandle, id: &str, content: &str) -> Result<(), String> {
    record(app, id, content, false)
}

pub fn snapshot_forced(app: &tauri::AppHandle, id: &str, content: &str) -> Result<(), String> {
    record(app, id, content, true)
}

/// Seeds history with a note's pre-edit content the first time it's edited.
///
/// Without this, the first edit to any note that predates version history
/// would destroy the only copy of its original content: `snapshot` on an empty
/// history just records the *new* text, and the old text is already gone from
/// disk. Timestamped with the file's real mtime so it sits outside the
/// coalescing window and the very next edit can't absorb it.
pub fn ensure_baseline(
    app: &tauri::AppHandle,
    id: &str,
    content: &str,
    mtime: u64,
) -> Result<(), String> {
    if content.is_empty() {
        return Ok(());
    }
    if !read_revisions(app, id)?.is_empty() {
        return Ok(());
    }

    let revs = vec![Revision {
        rev: 1,
        ts: if mtime > 0 { mtime } else { now_ms() },
        device: config::device_id(app),
        hash: notes::content_hash(content),
        content: content.to_string(),
    }];
    write_revisions(app, id, &revs)
}

/// Looks up stored content by its hash.
///
/// This is how sync recovers the merge base: sync-state records only the hash
/// both sides agreed on, and the text behind it is already sitting in history,
/// so there's no need for a second copy of every note. Returns `None` if the
/// revision has since been pruned, which callers must treat as "no safe base".
pub fn content_by_hash(app: &tauri::AppHandle, id: &str, hash: &str) -> Option<String> {
    read_revisions(app, id)
        .ok()?
        .into_iter()
        .find(|r| r.hash == hash)
        .map(|r| r.content)
}

/// Unions two histories of the same note, so syncing converges both timelines
/// instead of one device's overwriting the other's.
///
/// Entries are identified by (timestamp, hash); `rev` is renumbered afterwards
/// because two devices working offline will both have minted the same numbers
/// for different content.
pub fn merge_jsonl(local: &str, remote: &str) -> String {
    let parse = |raw: &str| -> Vec<Revision> {
        raw.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<Revision>(l).ok())
            .collect()
    };

    let mut seen: std::collections::HashSet<(u64, String)> = std::collections::HashSet::new();
    let mut all: Vec<Revision> = Vec::new();
    for rev in parse(local).into_iter().chain(parse(remote)) {
        if seen.insert((rev.ts, rev.hash.clone())) {
            all.push(rev);
        }
    }

    all.sort_by_key(|r| r.ts);
    let merged = prune(all);

    let mut out = String::new();
    for (i, mut rev) in merged.into_iter().enumerate() {
        rev.rev = i as u64 + 1;
        if let Ok(line) = serde_json::to_string(&rev) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// Drops a note's history. Called when the note itself is deleted, so deleting
/// a note doesn't leave its full text behind in the vault.
pub fn forget(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let path = history_path(app, id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete history: {e}")),
    }
}

#[tauri::command]
pub fn list_revisions(app: tauri::AppHandle, id: String) -> Result<Vec<RevisionMeta>, String> {
    let mut revs = read_revisions(&app, &id)?;
    revs.reverse(); // newest first, matching how the UI lists them
    Ok(revs
        .into_iter()
        .map(|r| RevisionMeta {
            rev: r.rev,
            ts: r.ts,
            device: r.device,
            hash: r.hash,
            bytes: r.content.len(),
            title: notes::first_line_title(&r.content),
        })
        .collect())
}

#[tauri::command]
pub fn get_revision(app: tauri::AppHandle, id: String, rev: u64) -> Result<String, String> {
    read_revisions(&app, &id)?
        .into_iter()
        .find(|r| r.rev == rev)
        .map(|r| r.content)
        .ok_or_else(|| format!("Revision {rev} not found"))
}

/// Restores a past revision as the note's current content, returning it.
///
/// The current content is force-snapshotted first, so restoring is itself
/// undoable from the same history list.
#[tauri::command]
pub fn restore_revision(app: tauri::AppHandle, id: String, rev: u64) -> Result<String, String> {
    let target = read_revisions(&app, &id)?
        .into_iter()
        .find(|r| r.rev == rev)
        .ok_or_else(|| format!("Revision {rev} not found"))?;

    let (path, _folder) = notes::find_note_path(&app, &id)?;
    let current = fs::read_to_string(&path).unwrap_or_default();

    snapshot_forced(&app, &id, &current)?;
    fs::write(&path, &target.content).map_err(|e| format!("Failed to restore note: {e}"))?;
    snapshot_forced(&app, &id, &target.content)?;

    Ok(target.content)
}

/// Line diff of a stored revision against another one, or against the note's
/// current content when `to` is omitted.
#[tauri::command]
pub fn diff_revisions(
    app: tauri::AppHandle,
    id: String,
    from: u64,
    to: Option<u64>,
) -> Result<Vec<crate::diff::DiffLine>, String> {
    let revs = read_revisions(&app, &id)?;
    let find = |rev: u64| {
        revs.iter()
            .find(|r| r.rev == rev)
            .map(|r| r.content.clone())
            .ok_or_else(|| format!("Revision {rev} not found"))
    };

    let before = find(from)?;
    let after = match to {
        Some(rev) => find(rev)?,
        None => {
            let (path, _folder) = notes::find_note_path(&app, &id)?;
            fs::read_to_string(&path).unwrap_or_default()
        }
    };

    Ok(crate::diff::diff_lines(&before, &after))
}

#[tauri::command]
pub fn clear_history(app: tauri::AppHandle, id: String) -> Result<(), String> {
    forget(&app, &id)
}

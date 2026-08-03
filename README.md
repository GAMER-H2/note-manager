# Note Manager

A simple markdown note manager meant to replace the notification/reminder
side of Google Keep. Built with [Tauri](https://tauri.app) (Rust backend) and
a [Vue 3](https://vuejs.org) + [Vite](https://vitejs.dev) frontend. Runs on
desktop (Linux/macOS/Windows) and Android.

## Features

- Create, edit, and delete notes as plain markdown files
- Notes auto-save while you type (debounced) and on blur/close
- Notes are stored as individual `.md` files on disk, one per note
- Link notes to each other with `[label](folderName/noteTitle)`
- Per-note reminders: schedule a notification (one-time or repeating
  hourly/daily/weekly/monthly/yearly) that shows the note's title as the main
  text and the rest of the note as the body (via `tauri-plugin-notification`)
- Per-note version history with a diff view and one-click restore
- Sync a vault between devices via a shared folder or any WebDAV server
- Import and export the whole vault as a zip file
- Collapsible sidebar and a settings modal
  (general/notifications/appearance/sync)

## Prerequisites

- [Node.js](https://nodejs.org/) (for `npm`/`npx`)
- [Rust toolchain](https://www.rust-lang.org/tools/install) (`cargo`, `rustc`)
- Platform build dependencies for Tauri — see the
  [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)
  (on Linux this usually means `webkit2gtk`, `libayatana-appindicator3`, etc.)

## Setup

```bash
npm install
```

## Run in development (desktop)

```bash
npx tauri dev
```

This starts the Vite dev server (`npm run dev`) and opens the app in a native
window with hot-reload for the frontend (`src/`) and automatic rebuilds when
the Rust backend (`src-tauri/`) changes.

## Run in development (Android)

Requires the Android SDK + NDK, a JDK (17+), and the Rust Android targets:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi \
  i686-linux-android x86_64-linux-android
```

Set `ANDROID_HOME`, `NDK_HOME`, and `JAVA_HOME`, then connect a device with USB
debugging enabled (or start an emulator) and run:

```bash
npx tauri android dev
```

Notes:

- If installing fails with `INSTALL_FAILED_UPDATE_INCOMPATIBLE` (signature
  mismatch), uninstall the old copy first:
  `adb uninstall com.mh968.note_manager`.
- The Vite dev server listens on ports **1420** (app) and **1421** (HMR). If
  the device can't reach it, allow those ports through your firewall
  (e.g. `sudo ufw allow 1420/tcp`).

## Build for production

```bash
npx tauri build            # desktop bundle
npx tauri android build    # Android APK/AAB
```

Desktop bundles land in `src-tauri/target/release/bundle/`.

## GitHub releases

Publishing a GitHub release triggers `.github/workflows/release.yml`, which runs
these production builds and attaches their outputs to that release:

- Windows x86_64
- macOS ARM64
- Linux x86_64 and ARM64
- Android ARM64 APK and AAB

The Android job follows the Android setup above and builds with
`npx tauri android build --target aarch64 --split-per-abi --apk --aab`.
To publish installable, consistently signed Android files, configure these
repository secrets with a release keystore:

- `ANDROID_KEY_BASE64` — base64-encoded contents of the keystore
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

Without those secrets, the workflow still attaches unsigned Android build
outputs. The workflow can also be started manually from the Actions tab by
providing the existing release tag.

## Where notes live

Notes are saved as markdown files under the **vault root**, one file per note,
with folders as real directories:

```
<vault root>/
  General/note_1771439757406_a7f31c.md
  Work/Projects/note_....md
  .notemanager/
    history/<id>.jsonl      one JSON line per stored revision
    sync-state.json         the hash both sides agreed on last sync
```

The vault root defaults to `app_data_dir/notes` and can be pointed anywhere
from **Settings → Sync → Vault folder**. Everything the app maintains *about*
the notes lives in `.notemanager/` inside the vault, so it travels with them
over any sync transport and lands in a zip export automatically. Directory
scans skip dot-prefixed entries, which is why that folder never shows up in
the sidebar.

Note ids carry a per-device suffix (`note_<millis>_<device>`). Without it, two
devices creating a note in the same millisecond while offline would produce the
same filename and sync would merge two different notes into one.

The backend lives in `src-tauri/src/` as a shared library crate so desktop and
Android run identical code — `main.rs` just calls into it:

| Module | Responsibility |
| --- | --- |
| `notes.rs` | Note and folder CRUD |
| `config.rs` | Device identity, vault location, sync settings |
| `history.rs` | Revision storage, pruning, restore |
| `diff.rs` | Line diff and three-way merge |
| `archive.rs` | Zip import/export |
| `sync.rs` | Reconciliation + the `SyncRemote` trait, folder transport |
| `webdav.rs` | WebDAV transport |
| `secrets.rs` | Credentials via the OS keychain |

## Version history

Every save records a revision, coalescing edits made by the same device within
ten minutes — autosave is debounced at 400ms, so snapshotting each save
verbatim would produce thousands of near-identical entries. Open **History** in
the note editor to diff any revision against the current note and restore it.
Restoring snapshots the current content first, so it's itself undoable.

Pruning keeps the last 30 days, never fewer than 20 revisions and never more
than 200.

## Sync

Settings → Sync connects the vault to a **shared folder** or a **WebDAV
server**. Both implement the same `SyncRemote` trait, so they behave
identically.

Reconciliation is three-way: each device records the content hash both sides
agreed on at the last sync, and merges local and remote against that base.
Edits to different parts of a note merge silently; edits that overlap keep the
remote version as a separate note titled `... (conflicted copy from <device>)`,
so nothing is ever lost. An edit always outranks a delete. Comparison is
hash-based rather than mtime-based, because remote clocks lie and modification
times don't survive every transport.

Pinned notes and reminders sync too. Both merge as a union — an unpin or a
removed reminder won't propagate, which is the safe direction to fail.

### Automatic sync

Settings → Sync → **Automatic sync** picks how often the app syncs on its own:
manual only (the default), or every 5, 15, 30, or 60 minutes. With an interval
set, the app also syncs when it starts, when you switch back to it, and a few
seconds after you close a note — so in practice you rarely press the button.

Two deliberate limits:

- **Nothing syncs while a note is open.** A pull replaces files underneath the
  editor, so ticks are held back until you close the note.
- **The interval is per-device.** It lives in `settings.json`, which isn't
  synced, so a desktop can poll every 5 minutes while a phone stays on manual
  to save battery.

Sync only ever runs while the app is open — there's no background service. The
sync itself runs off the UI thread, so a slow remote doesn't freeze the app.

### Note filenames

Notes are stored as `<id>.md` by default, where the id is what sync, version
history, pins and reminders all identify a note by.

Settings → Vault → **Put note titles in filenames** switches that to
`Project Kickoff--a1b2c3.md`: readable in a file manager, in Obsidian, or in
your sync server's web UI, with the id kept as a suffix so identity survives.
Turning it on renames the notes you already have, and filenames follow the
title as you edit it.

The setting describes the **vault**, not the device, so it lives in
`.notemanager/vault-settings.json` and syncs: a device joining a vault adopts
its convention on the next sync and renames its own files to match. Were it
per-device instead, two devices that disagreed would rename each other's files
back and forth on every edit.

Conflicting changes to the setting resolve newest-write-wins, using a timestamp
stamped whenever a device changes it. That's deliberately not the union merge
used for pins and reminders — a union of booleans could never be turned back
off, because whichever device still had it on would re-enable it forever.

> Upgrading from a build that predates this: update **every** device before
> syncing again. An older build reads `Title--id.md` as an unfamiliar name and
> would import it as a second copy of the note.

### Option 1 — a shared folder

Anything both machines can see works: an rclone or Syncthing folder, an
NFS/SMB mount, a docker bind mount, or a second disk.

```bash
# On a bare Linux server, over SSH — no server software needed
sshfs user@server:/srv/notes ~/mnt/notes
# then point Settings → Sync → Sync folder at ~/mnt/notes
```

For **Google Drive**, mount it with rclone and point the app at the mount:

```bash
rclone config              # add a "gdrive" remote, follow the OAuth prompts
rclone mount gdrive:notes ~/mnt/gdrive-notes --vfs-cache-mode writes
```

### Option 2 — WebDAV

Works on Android too (a folder mount generally doesn't), and needs no client
software on the server. The examples below use
[dufs](https://github.com/sigoden/dufs), a single static binary that serves
a directory over WebDAV — it isn't bundled with this app, so install it
first from the [releases page](https://github.com/sigoden/dufs/releases) or
your package manager. (`rclone serve webdav` is a fine substitute if you'd
rather reuse an rclone setup you already have — see below.)

```bash
# Bare Linux server: one binary, no config
dufs /srv/notes --allow-all --auth admin:secret@/:rw
```

What each part does:

| Flag | Meaning |
| --- | --- |
| `/srv/notes` | The directory to serve — point this at wherever you want the vault to live on the server. |
| `--allow-all` | Enables upload/delete/search, which sync needs to actually write files, not just read them. |
| `--auth admin:secret@/:rw` | **`admin` and `secret` are not fixed values — they're a username and password you make up.** Everything after the `@` (`/:rw`) grants that user read-write access to the whole served directory; leave it as-is unless you're restricting access to a subdirectory. |

dufs defaults to port `5000` and has no HTTPS of its own — put it behind a
reverse proxy (Caddy, nginx, Tailscale Serve) if you need `https://`, since
Basic Auth over plain `http://` sends the password unencrypted.

Or, with rclone, which can serve any of its 70+ backends over WebDAV —
including Google Drive, which is the simplest way to reach Drive from Android:

```bash
rclone serve webdav gdrive:notes --addr :8080 --user admin --pass secret
```

**Docker / docker-compose**, e.g. for Portainer, CasaOS, or any compose-based
setup — use the list form for `command:` rather than a single string; some
compose front-ends pass a plain string through as one literal argument
instead of splitting it, which makes dufs try to open a path that includes
your whole `--auth` flag and fail:

```yaml
name: webdav-notes
services:
  webdav-notes:
    image: sigoden/dufs:latest
    container_name: webdav-notes
    restart: unless-stopped
    command:
      - "/data"
      - "--allow-all"
      - "--auth"
      - "admin:secret@/:rw"   # replace admin/secret with your own username/password
    ports:
      - "5000:5000"
    volumes:
      - /srv/notes:/data
```

**In the app**, go to Settings → Sync, choose **WebDAV server**, and enter:

- **URL** — the server's base address, including the port:
  `http://<host>:5000` (or `https://…` behind a proxy). Don't add a path
  after the port unless you've served the vault behind one (e.g. dufs's
  `--path-prefix`) — dufs serves the whole directory from `/`.
- **Username** / **Password** — exactly whatever you chose for `--auth`
  above (`admin` / `secret` in these examples, but pick your own).

The password is verified before anything is saved and is stored in your OS
keychain (Keychain, Credential Manager, or Secret Service) — never in the
app's config files. Android has no keychain backend, so it falls back to the
app-private data directory.

> Use `https://` for anything crossing a network you don't control. Basic auth
> over `http://` sends the password in the clear; it's allowed for LAN servers,
> but that's your call to make.

## Import and export

Settings → Sync → Import & export writes the whole vault to a zip file. Notes
are named by title (`Work/Project Kickoff.md`) rather than by internal id, so
the archive opens cleanly in Obsidian or any markdown editor — and an exported
vault's `folder/title` note-links match its real file paths. A
`.notemanager/manifest.json` carries ids, pins, and reminders so a re-import is
lossless; version history is included behind a toggle.

Import rebuilds every path from re-sanitized segments rather than trusting the
archive's own strings, so a hostile zip can't write outside the vault. Merge
adds to what you have; replace clears the vault first (history is kept) and
asks for confirmation.

## Project structure

```
index.html            Vite entry HTML
vite.config.js        Vite config (incl. Tauri mobile dev host handling)
src/
  main.js             Vue app entry
  App.vue             Layout + top-level state
  components/         AppHeader, AppSidebar, NoteCard, NoteModal,
                      HistoryModal, SettingsModal, ReminderModal, FolderModal
  composables/        Backend bridges: useNotes, useHistory, useSync,
                      useVault, useArchive, useReminders, useSettings
  lib/                Pure helpers
  styles.css          Global styles
src-tauri/src/        Rust backend (see the module table above)
src-tauri/gen/        Generated mobile (Android) project files
```

## Tests

```bash
cd src-tauri && cargo test
```

Covers the three-way merge, the line diff, zip-slip defenses on import, remote
path handling, and the pin/reminder merges.

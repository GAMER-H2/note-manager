# Note Manager

A simple markdown note manager meant to replace the notification/reminder
side of Google Keep. Built with [Tauri](https://tauri.app) (Rust backend) and
a [Vue 3](https://vuejs.org) + [Vite](https://vitejs.dev) frontend. Runs on
desktop (Linux/macOS/Windows) and Android.

## Features

- Create, edit, and delete notes as plain markdown files
- Notes auto-save while you type (debounced) and on blur/close
- Notes are stored as individual `.md` files on disk, one per note
- Per-note reminders: schedule a notification (one-time or repeating
  hourly/daily/weekly/monthly/yearly) that shows the note's title as the main
  text and the rest of the note as the body (via `tauri-plugin-notification`)
- Collapsible sidebar and a settings modal (general/notifications/appearance)

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

## Where notes live

Notes are saved as markdown files in the app's data directory
(`app_data_dir/notes/<id>.md`), managed via the Tauri commands `create_note`,
`update_note`, `delete_note`, and `list_notes`. These live in
`src-tauri/src/lib.rs` (the shared library crate) so desktop and Android run
the exact same backend — `src-tauri/src/main.rs` just calls into it.

## Project structure

```
index.html            Vite entry HTML
vite.config.js        Vite config (incl. Tauri mobile dev host handling)
src/
  main.js             Vue app entry
  App.vue             Layout + top-level state
  components/         AppHeader, AppSidebar, NoteCard, NoteModal, SettingsModal
  composables/        useNotes (backend bridge), useNotifications
  lib/                Pure helpers
  styles.css          Global styles
src-tauri/            Rust backend, Tauri config, and app icons
src-tauri/gen/        Generated mobile (Android) project files
```

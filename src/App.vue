<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onAction } from "@tauri-apps/plugin-notification";
import AppHeader from "./components/AppHeader.vue";
import AppSidebar from "./components/AppSidebar.vue";
import NoteCard from "./components/NoteCard.vue";
import NoteModal from "./components/NoteModal.vue";
import SettingsModal from "./components/SettingsModal.vue";
import FolderModal from "./components/FolderModal.vue";
import FolderDeleteModal from "./components/FolderDeleteModal.vue";
import DeleteNoteModal from "./components/DeleteNoteModal.vue";
import ContextMenu from "./components/ContextMenu.vue";
import { useNotes } from "./composables/useNotes.js";
import { useReminders } from "./composables/useReminders.js";
import { useSettings } from "./composables/useSettings.js";
import { useSync } from "./composables/useSync.js";
import { useAutoSync } from "./composables/useAutoSync.js";
import {
  useFolders,
  PINNED_FOLDER,
  GENERAL_FOLDER,
  ARCHIVE_FOLDER,
} from "./composables/useFolders.js";
import { usePinned } from "./composables/usePinned.js";
import { useNoteArchive } from "./composables/useNoteArchive.js";
import { useContextMenu } from "./composables/useContextMenu.js";
import { useContextMenuTrigger } from "./composables/useContextMenuTrigger.js";
import { useNoteClipboard } from "./composables/useNoteClipboard.js";
import { isAndroid } from "./lib/platform.js";
import refreshIcon from "./assets/refresh.png";
import { initNotifications } from "./composables/useNotifications.js";
import { useOverlayHistory } from "./composables/useOverlayHistory.js";
import {
  firstLineTitle,
  sortNotes,
  NOTE_SORT_OPTIONS,
  DEFAULT_NOTE_SORT,
} from "./lib/notes.js";

const { notes, loadNotes, createNote, updateNote, deleteNote, moveNote } = useNotes();
const { loadReminders, reloadReminders, rescheduleAllReminders, removeReminder } =
  useReminders();
const { settings, loadSettings, saveSettings } = useSettings();
const {
  realFolders,
  selectedFolder,
  loadFolders,
  selectFolder,
  deleteFolder,
  defaultNoteFolder,
} = useFolders();
const { loadPinned, reloadPinned, isPinned, togglePin, unpin } = usePinned();
const {
  loadArchiveOrigins,
  reloadArchiveOrigins,
  rememberOrigin,
  originOf,
  forgetOrigin,
  forgetOrigins,
} = useNoteArchive();
const {
  config: syncConfig,
  syncing,
  loadSyncConfig,
  syncNow,
} = useSync();
const { startAutoSync, stopAutoSync, syncSoon } = useAutoSync();
const { openMenu } = useContextMenu();
const { clipboard, copyNote } = useNoteClipboard();

const isArchived = (note) => note?.folder === ARCHIVE_FOLDER;
const archivedCount = computed(
  () => notes.value.filter((n) => n.folder === ARCHIVE_FOLDER).length,
);
const isArchiveView = computed(() => selectedFolder.value === ARCHIVE_FOLDER);

const visibleNotes = computed(() => {
  if (selectedFolder.value === PINNED_FOLDER) {
    // Archived notes are unpinned on archive, but guard anyway.
    return notes.value.filter((n) => isPinned(n.id) && n.folder !== ARCHIVE_FOLDER);
  }
  if (selectedFolder.value === ARCHIVE_FOLDER) {
    return notes.value.filter((n) => n.folder === ARCHIVE_FOLDER);
  }
  return notes.value.filter((n) => n.folder === selectedFolder.value);
});

// Sort mode + grid/list view are persisted per-device in settings.
const sortMode = computed(() => settings.noteSort ?? DEFAULT_NOTE_SORT);
const viewMode = computed(() => (settings.noteView === "list" ? "list" : "grid"));
const sortedNotes = computed(() => sortNotes(visibleNotes.value, sortMode.value));
const currentSortLabel = computed(
  () => NOTE_SORT_OPTIONS.find((o) => o.value === sortMode.value)?.label ?? "Sort",
);

const sortMenuOpen = ref(false);
const { requestClose: requestCloseSortMenu } = useOverlayHistory(
  () => sortMenuOpen.value,
  () => {
    sortMenuOpen.value = false;
  },
);

const toggleSortMenu = () => {
  if (sortMenuOpen.value) requestCloseSortMenu();
  else sortMenuOpen.value = true;
};

const selectSort = async (value) => {
  saveSettings({ noteSort: value });
  await requestCloseSortMenu();
};

const toggleView = () => {
  saveSettings({ noteView: viewMode.value === "grid" ? "list" : "grid" });
};

// Deleting a note also cancels/forgets any reminder attached to it and unpins it.
const deleteNoteAndReminder = async (id) => {
  await removeReminder(id);
  await unpin(id);
  await deleteNote(id);
};

// Archiving moves a note into the Archive folder, remembering where it came
// from so it can be restored. Archived notes can't hold reminders or pins, so
// those are stripped on the way in.
const archiveNote = async (note) => {
  try {
    await rememberOrigin(note.id, note.folder);
    await removeReminder(note.id);
    await unpin(note.id);
    await moveNote(note.id, ARCHIVE_FOLDER);
  } catch (err) {
    console.error("Failed to archive note:", err);
  }
};

// Restore returns a note to its original folder, or General if that folder is
// gone. Clears the remembered origin.
const restoreNote = async (note) => {
  if (!note) return;
  if (activeNote.value?.id === note.id) activeNote.value = null;
  try {
    const origin = originOf(note.id);
    const target =
      origin && realFolders.value.includes(origin) ? origin : GENERAL_FOLDER;
    await moveNote(note.id, target);
    await forgetOrigin(note.id);
  } catch (err) {
    console.error("Failed to restore note:", err);
  }
};

const permanentlyDeleteNote = async (note) => {
  if (!note) return;
  if (activeNote.value?.id === note.id) activeNote.value = null;
  try {
    await deleteNoteAndReminder(note.id);
    await forgetOrigin(note.id);
  } catch (err) {
    console.error("Failed to delete note:", err);
  }
};

// Delete confirmation: Archive / Delete / Cancel (Archive hidden for a note
// that's already archived).
const deleteModalOpen = ref(false);
const deleteTarget = ref(null);

const requestDeleteNote = (note) => {
  if (!note) return;
  deleteTarget.value = note;
  deleteModalOpen.value = true;
};

const onDeleteArchive = async () => {
  const note = deleteTarget.value;
  deleteModalOpen.value = false;
  if (!note) return;
  if (activeNote.value?.id === note.id) activeNote.value = null;
  await archiveNote(note);
};

const onDeleteConfirm = async () => {
  const note = deleteTarget.value;
  deleteModalOpen.value = false;
  if (note) await permanentlyDeleteNote(note);
};

// Empty the Archive (permanently deletes every archived note), with a confirm.
const emptyArchiveOpen = ref(false);

const confirmEmptyArchive = async () => {
  emptyArchiveOpen.value = false;
  try {
    const ids = await invoke("delete_folder", {
      req: { folder: ARCHIVE_FOLDER, mode: "delete" },
    });
    await forgetOrigins(Array.isArray(ids) ? ids : []);
    await loadFolders();
    await loadNotes();
    // The Archive view is empty (and greyed) now — step back to General.
    if (selectedFolder.value === ARCHIVE_FOLDER) selectFolder(GENERAL_FOLDER);
  } catch (err) {
    console.error("Failed to empty archive:", err);
  }
};

const settingsOpen = ref(false);
const folderModalOpen = ref(false);
// Full path of the folder a new subfolder should nest under, or null when the
// "+ Add Folder" (top-level) button opened the modal instead.
const folderModalParent = ref(null);
const activeNote = ref(null);

// Sidebar: on desktop it's part of the layout (visible by default); on mobile
// it's an overlay drawer that starts hidden and is toggled by the hamburger.
const mql = window.matchMedia("(max-width: 768px)");
const isMobile = ref(mql.matches);
const sidebarOpen = ref(!isMobile.value);

let requestCloseSidebar = () => {
  sidebarOpen.value = false;
};

const toggleSidebar = () => {
  if (isMobile.value && sidebarOpen.value) {
    requestCloseSidebar();
    return;
  }

  sidebarOpen.value = !sidebarOpen.value;
};

const onBreakpointChange = (e) => {
  isMobile.value = e.matches;
  // Desktop → show sidebar in the layout; mobile → collapse the drawer.
  sidebarOpen.value = !e.matches;
};

const openNote = (note) => {
  activeNote.value = note;
  if (isMobile.value) sidebarOpen.value = false;
};

const onNoteClose = () => {
  activeNote.value = null;
};

const addNote = async () => {
  try {
    const note = await createNote(defaultNoteFolder());
    openNote(note);
  } catch (err) {
    console.error("Failed to create note:", err);
  }
};

// The vault changed wholesale (an import, or a future sync pull), so every
// store that mirrors it has to be re-read. The open note is closed because the
// file it points at may no longer exist.
const onVaultChanged = async () => {
  activeNote.value = null;
  await Promise.all([
    loadNotes(),
    loadFolders(),
    reloadPinned(),
    reloadReminders(),
    reloadArchiveOrigins(),
  ]);
  // Arriving reminders exist only on disk — nothing is scheduled for them on
  // this device until we ask for it.
  await rescheduleAllReminders();
};

// Closing a note is the point an edit has settled: the content is written and
// nothing is holding the editor open, so it's safe to push it out.
watch(activeNote, (now, before) => {
  if (before && !now) syncSoon();
});

// An emptied Archive is a greyed-out dead end, so step back to General if the
// last archived note is restored or deleted while it's being viewed.
watch(archivedCount, (count) => {
  if (count === 0 && selectedFolder.value === ARCHIVE_FOLDER) {
    selectFolder(GENERAL_FOLDER);
  }
});

const syncConfigured = computed(() => !!syncConfig.value);

// Manual sync from the header refresh icon. Mirrors the settings panel's "Sync
// now": on a successful pull, reload the vault so the new state shows.
const onHeaderSync = async () => {
  if (syncing.value || !syncConfigured.value) return;
  const report = await syncNow();
  if (report) await onVaultChanged();
};

// "Let syncing finish before closing" (opt-in). When the window is asked to
// close mid-sync, hold it open and show a prompt until the sync settles.
const closingForSync = ref(false);
let stopSyncWatch = null;
let unlistenClose = null;

const finishClose = () => {
  stopSyncWatch?.();
  stopSyncWatch = null;
  closingForSync.value = false;
  getCurrentWindow().destroy();
};

// Stops waiting and leaves the app open (the sync keeps running in the
// background) — used when the prompt is dismissed.
const cancelCloseWait = () => {
  stopSyncWatch?.();
  stopSyncWatch = null;
  closingForSync.value = false;
};

const onWindowCloseRequested = (event) => {
  // Opt-in, and only worth intervening while a sync is actually in flight.
  if (settings.syncBeforeClose !== true || !syncing.value) return;
  event.preventDefault();
  closingForSync.value = true;
  stopSyncWatch?.();
  stopSyncWatch = watch(syncing, (running) => {
    if (!running && closingForSync.value) finishClose();
  });
};

const onSelectFolder = (name) => {
  selectFolder(name);
  if (isMobile.value) sidebarOpen.value = false;
};

const onAddFolder = () => {
  folderModalParent.value = null;
  folderModalOpen.value = true;
};

const onAddSubfolder = () => {
  folderModalParent.value = selectedFolder.value;
  folderModalOpen.value = true;
};

const onAddSubfolderFor = (path) => {
  folderModalParent.value = path;
  folderModalOpen.value = true;
};

// Duplicates the copied note into the folder currently being viewed. createNote
// mints a fresh id/file, so this is a real copy rather than a move.
const pasteInCurrentFolder = async () => {
  if (!clipboard.value) return;
  try {
    const note = await createNote(defaultNoteFolder());
    await updateNote(note.id, clipboard.value.content);
  } catch (err) {
    console.error("Failed to paste note:", err);
  }
};

// The reveal-in-file-browser entries are desktop-only: mobile has no file
// manager to open into.
const desktop = !isAndroid();

// Menu for a note card. Archived notes only offer Restore (they can't be
// pinned/pasted); otherwise the usual Pin/Copy/Paste. Both end in Delete, which
// opens the Archive/Delete confirmation.
const openNoteMenu = (note, x, y) => {
  const items = [];
  if (isArchived(note)) {
    items.push({ label: "Restore", action: () => restoreNote(note) });
  } else {
    items.push({
      label: isPinned(note.id) ? "Unpin" : "Pin",
      action: () => togglePin(note.id),
    });
    items.push({ label: "Copy", action: () => copyNote(note) });
    items.push({
      label: "Paste",
      disabled: !clipboard.value,
      action: pasteInCurrentFolder,
    });
  }
  if (desktop) {
    items.push({
      label: "Show in file browser",
      action: () => invoke("reveal_path", { path: note.path }),
    });
  }
  items.push({
    label: "Delete",
    danger: true,
    action: () => requestDeleteNote(note),
  });
  openMenu(x, y, items);
};

// Menu for empty space in the main panel — the natural place to paste. There's
// nothing to copy here, so only Paste and (on desktop) "Open in file browser"
// (which opens the folder currently being viewed) appear.
const openEmptyMenu = (x, y) => {
  const items = [
    {
      label: "Paste",
      disabled: !clipboard.value,
      action: pasteInCurrentFolder,
    },
  ];
  if (desktop) {
    items.push({
      label: "Open in file browser",
      action: () => invoke("open_folder", { folder: defaultNoteFolder() }),
    });
  }
  openMenu(x, y, items);
};

// One delegated trigger for the whole main panel: a gesture on a note card
// (found via its data-note-id) opens the note menu; anywhere else opens the
// empty-space menu.
const onMainContext = ({ x, y, target }) => {
  const cardEl = target?.closest?.(".note-card");
  if (cardEl) {
    const note = notes.value.find((n) => n.id === cardEl.dataset.noteId);
    if (note) openNoteMenu(note, x, y);
    return;
  }
  openEmptyMenu(x, y);
};
const mainContextTrigger = useContextMenuTrigger(onMainContext);

// Right-click / long-press on a folder. General can't be deleted or nested
// under (matching the sidebar).
const onFolderContext = ({ path, x, y }) => {
  const items = [];
  if (path !== GENERAL_FOLDER) {
    items.push({ label: "Add subfolder", action: () => onAddSubfolderFor(path) });
  }
  if (desktop) {
    items.push({
      label: "Open in file browser",
      action: () => invoke("open_folder", { folder: path }),
    });
  }
  if (path !== GENERAL_FOLDER) {
    items.push({
      label: "Delete",
      danger: true,
      action: () => requestDeleteFolder(path),
    });
  }
  openMenu(x, y, items);
};

// How many notes live under a folder (including its subfolders).
const notesUnder = (path) =>
  notes.value.filter((n) => n.folder === path || n.folder.startsWith(`${path}/`))
    .length;

const folderDeleteOpen = ref(false);
const folderDeleteTarget = ref("");
const folderDeleteNoteCount = computed(() => notesUnder(folderDeleteTarget.value));
const folderDeleteHasSubfolders = computed(() =>
  realFolders.value.some((f) => f.startsWith(`${folderDeleteTarget.value}/`)),
);

const requestDeleteFolder = async (path) => {
  // An empty folder (no notes anywhere in its subtree) has nothing to lose, so
  // skip the prompt and just remove it.
  if (notesUnder(path) === 0) {
    await runDeleteFolder(path, "delete");
    return;
  }
  folderDeleteTarget.value = path;
  folderDeleteOpen.value = true;
};

const confirmDeleteFolder = async (mode) => {
  const path = folderDeleteTarget.value;
  folderDeleteOpen.value = false;
  await runDeleteFolder(path, mode);
};

const runDeleteFolder = async (path, mode) => {
  try {
    const affectedIds = await deleteFolder(path, mode);
    // Deleting the notes means their pins/reminders are now dangling; drop them
    // (and cancel any scheduled notification). A "move" keeps the notes, so
    // their pins/reminders stay valid by id.
    if (mode === "delete") {
      for (const id of affectedIds) {
        await removeReminder(id);
        await unpin(id);
      }
    }
    await loadNotes();
  } catch (err) {
    console.error("Failed to delete folder:", err);
  }
};

// Resolves a markdown note-link's `folderPath/noteTitle` (or a bare title with
// no folder, searched across every folder) to a note and opens it, mirroring
// the same `openNote` used for notification taps. Returns whether a match
// was found so the caller can surface "not found" feedback.
const openNoteByLink = (folderPath, title) => {
  const normalizedTitle = title.trim().toLowerCase();
  const match = notes.value.find((n) => {
    if (folderPath != null && n.folder !== folderPath) return false;
    return firstLineTitle(n.content).trim().toLowerCase() === normalizedTitle;
  });
  if (!match) return false;
  openNote(match);
  return true;
};

({ requestClose: requestCloseSidebar } = useOverlayHistory(
  () => isMobile.value && sidebarOpen.value,
  () => {
    sidebarOpen.value = false;
  },
));

let actionListener = null;

// Tapping a reminder notification should jump straight to the note it was
// set for, using the `noteId` we stash in the notification's `extra` payload.
const onNotificationAction = (event) => {
  const noteId = event?.notification?.extra?.noteId;
  if (!noteId) return;
  const note = notes.value.find((n) => n.id === noteId);
  if (note) openNote(note);
};

onMounted(async () => {
  loadNotes();
  loadReminders();
  loadFolders();
  loadPinned();
  loadArchiveOrigins();
  // Load settings first so the theme applies and notifications use the chosen
  // urgency, then fire-and-forget notification setup (never blocks the UI).
  await loadSettings();
  initNotifications(settings.urgency);

  try {
    actionListener = await onAction(onNotificationAction);
  } catch (e) {
    console.warn("Failed to register notification action listener:", e);
  }

  if (typeof mql.addEventListener === "function") {
    mql.addEventListener("change", onBreakpointChange);
  } else if (typeof mql.addListener === "function") {
    mql.addListener(onBreakpointChange); // Safari fallback
  }

  // The scheduler needs to know whether a remote is configured at all, and
  // that lives in the sync config rather than in settings.
  await loadSyncConfig();
  startAutoSync({
    // Never pull the vault out from under an open editor — the refresh closes
    // the active note, which would discard whatever is being typed.
    shouldDefer: () => activeNote.value !== null,
    onChanged: onVaultChanged,
  });

  try {
    unlistenClose = await getCurrentWindow().onCloseRequested(
      onWindowCloseRequested,
    );
  } catch (e) {
    // No window to guard (e.g. running outside a Tauri desktop context).
    console.warn("Failed to register window close handler:", e);
  }
});

onUnmounted(() => {
  actionListener?.unregister();
  stopAutoSync();
  unlistenClose?.();
  stopSyncWatch?.();

  if (typeof mql.removeEventListener === "function") {
    mql.removeEventListener("change", onBreakpointChange);
  } else if (typeof mql.removeListener === "function") {
    mql.removeListener(onBreakpointChange);
  }
});
</script>

<template>
  <div class="app-shell" :class="{ 'sidebar-collapsed': !sidebarOpen }">
    <AppHeader
      :sidebar-open="sidebarOpen"
      :syncing="syncing"
      :sync-enabled="syncConfigured"
      :current-folder="selectedFolder"
      @toggle-sidebar="toggleSidebar"
      @open-settings="settingsOpen = true"
      @sync="onHeaderSync"
    />

    <AppSidebar
      :open="sidebarOpen"
      :selected-folder="selectedFolder"
      :archived-count="archivedCount"
      @select-folder="onSelectFolder"
      @add-folder="onAddFolder"
      @add-subfolder="onAddSubfolder"
      @folder-context="onFolderContext"
    />

    <main
      class="app-main"
      aria-label="Notes"
      @contextmenu="mainContextTrigger.onContextMenu"
      @touchstart="mainContextTrigger.onTouchStart"
      @touchmove="mainContextTrigger.onTouchMove"
      @touchend="mainContextTrigger.onTouchEnd"
      @touchcancel="mainContextTrigger.onTouchCancel"
    >
      <div class="notes-toolbar">
        <div class="notes-toolbar__sort">
          <button
            type="button"
            class="notes-toolbar__button"
            aria-haspopup="menu"
            :aria-expanded="String(sortMenuOpen)"
            @click="toggleSortMenu"
          >
            <span class="notes-toolbar__icon" aria-hidden="true">⇅</span>
            {{ currentSortLabel }}
            <span class="notes-toolbar__caret" aria-hidden="true">▾</span>
          </button>
          <div
            v-if="sortMenuOpen"
            class="notes-sort-menu__backdrop"
            @click="requestCloseSortMenu"
          ></div>
          <section
            v-if="sortMenuOpen"
            class="note-actions-menu notes-sort-menu"
            role="menu"
            aria-label="Sort notes"
          >
            <button
              v-for="opt in NOTE_SORT_OPTIONS"
              :key="opt.value"
              type="button"
              role="menuitemradio"
              :aria-checked="String(opt.value === sortMode)"
              class="note-actions-menu__item"
              :class="{ 'is-active': opt.value === sortMode }"
              @click="selectSort(opt.value)"
            >
              {{ opt.label }}
            </button>
          </section>
        </div>
        <button
          type="button"
          class="notes-toolbar__button"
          :aria-pressed="String(viewMode === 'list')"
          @click="toggleView"
        >
          <span class="notes-toolbar__icon" aria-hidden="true">{{
            viewMode === "grid" ? "☰" : "▦"
          }}</span>
          {{ viewMode === "grid" ? "List view" : "Grid view" }}
        </button>

        <button
          v-if="isArchiveView"
          type="button"
          class="notes-toolbar__button notes-toolbar__button--danger notes-toolbar__button--end"
          :disabled="archivedCount === 0"
          @click="emptyArchiveOpen = true"
        >
          <span class="notes-toolbar__icon" aria-hidden="true">🗑</span>
          Clear archive
        </button>
      </div>

      <section
        :class="viewMode === 'list' ? 'notes-list' : 'notes-grid'"
        aria-live="polite"
      >
        <NoteCard
          v-for="note in sortedNotes"
          :key="note.id"
          :note="note"
          @open="openNote"
        />
      </section>
      <button
        v-if="!isArchiveView"
        class="add-note-button"
        type="button"
        aria-label="Add note"
        @click="addNote"
      >
        +
      </button>
    </main>

    <!-- Mobile drawer backdrop -->
    <div
      v-if="isMobile && sidebarOpen"
      class="sidebar-backdrop"
      @click="requestCloseSidebar"
    ></div>

    <SettingsModal
      :open="settingsOpen"
      @close="settingsOpen = false"
      @imported="onVaultChanged"
    />
    <FolderModal
      :open="folderModalOpen"
      :parent-path="folderModalParent"
      @close="folderModalOpen = false"
    />
    <FolderDeleteModal
      :open="folderDeleteOpen"
      :folder="folderDeleteTarget"
      :note-count="folderDeleteNoteCount"
      :has-subfolders="folderDeleteHasSubfolders"
      @close="folderDeleteOpen = false"
      @confirm="confirmDeleteFolder"
    />

    <NoteModal
      :note="activeNote"
      :save="updateNote"
      :move-note="moveNote"
      :open-link="openNoteByLink"
      :archived="isArchived(activeNote)"
      @close="onNoteClose"
      @request-delete="requestDeleteNote(activeNote)"
      @request-restore="restoreNote(activeNote)"
    />

    <DeleteNoteModal
      :open="deleteModalOpen"
      :can-archive="!isArchived(deleteTarget)"
      @close="deleteModalOpen = false"
      @archive="onDeleteArchive"
      @delete="onDeleteConfirm"
    />

    <!-- Empty-archive confirmation (permanent). -->
    <div
      v-if="emptyArchiveOpen"
      class="folder-overlay"
      @click="emptyArchiveOpen = false"
    ></div>
    <section
      v-if="emptyArchiveOpen"
      class="folder-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Clear archive"
      aria-hidden="false"
      @click.self="emptyArchiveOpen = false"
    >
      <div class="folder-modal__content" role="document" tabindex="-1">
        <header class="folder-modal__header">
          <h2>Clear the archive?</h2>
          <button
            type="button"
            class="settings-close-button"
            aria-label="Close"
            @click="emptyArchiveOpen = false"
          >
            ×
          </button>
        </header>
        <div class="folder-modal__body">
          <p class="folder-delete__lead">
            This permanently deletes all
            {{ archivedCount }} archived note{{ archivedCount === 1 ? "" : "s" }}.
            This can't be undone.
          </p>
        </div>
        <footer class="reminder-modal__footer">
          <span class="reminder-footer-spacer"></span>
          <button
            type="button"
            class="settings-secondary"
            @click="emptyArchiveOpen = false"
          >
            Cancel
          </button>
          <button
            type="button"
            class="settings-primary settings-primary--danger"
            @click="confirmEmptyArchive"
          >
            Delete all
          </button>
        </footer>
      </div>
    </section>

    <ContextMenu />

    <!-- Shown only while holding the window open for an in-flight sync. -->
    <div v-if="closingForSync" class="folder-overlay" @click="cancelCloseWait"></div>
    <section
      v-if="closingForSync"
      class="folder-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Finishing sync"
      aria-hidden="false"
      @click.self="cancelCloseWait"
    >
      <div class="folder-modal__content close-sync" role="document" tabindex="-1">
        <img
          :src="refreshIcon"
          class="close-sync__icon sync-icon is-syncing"
          alt=""
          aria-hidden="true"
        />
        <div>
          <h2 class="close-sync__title">Finishing sync…</h2>
          <p class="close-sync__text">
            Waiting for the current sync to finish before closing. It'll close on
            its own when done.
          </p>
        </div>
        <footer class="close-sync__footer">
          <button type="button" class="settings-secondary" @click="cancelCloseWait">
            Keep app open
          </button>
          <button type="button" class="settings-primary" @click="finishClose">
            Close now
          </button>
        </footer>
      </div>
    </section>
  </div>
</template>

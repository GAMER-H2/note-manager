<script setup>
import { computed, onMounted, onUnmounted, ref } from "vue";
import { onAction } from "@tauri-apps/plugin-notification";
import AppHeader from "./components/AppHeader.vue";
import AppSidebar from "./components/AppSidebar.vue";
import NoteCard from "./components/NoteCard.vue";
import NoteModal from "./components/NoteModal.vue";
import SettingsModal from "./components/SettingsModal.vue";
import FolderModal from "./components/FolderModal.vue";
import { useNotes } from "./composables/useNotes.js";
import { useReminders } from "./composables/useReminders.js";
import { useSettings } from "./composables/useSettings.js";
import { useFolders, PINNED_FOLDER } from "./composables/useFolders.js";
import { usePinned } from "./composables/usePinned.js";
import { initNotifications } from "./composables/useNotifications.js";
import { useOverlayHistory } from "./composables/useOverlayHistory.js";
import { firstLineTitle } from "./lib/notes.js";

const { notes, loadNotes, createNote, updateNote, deleteNote, moveNote } = useNotes();
const { loadReminders, removeReminder } = useReminders();
const { settings, loadSettings } = useSettings();
const { selectedFolder, loadFolders, selectFolder, defaultNoteFolder } = useFolders();
const { loadPinned, isPinned, unpin } = usePinned();

const visibleNotes = computed(() =>
  selectedFolder.value === PINNED_FOLDER
    ? notes.value.filter((n) => isPinned(n.id))
    : notes.value.filter((n) => n.folder === selectedFolder.value),
);

// Deleting a note also cancels/forgets any reminder attached to it and unpins it.
const deleteNoteAndReminder = async (id) => {
  await removeReminder(id);
  await unpin(id);
  await deleteNote(id);
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
});

onUnmounted(() => {
  actionListener?.unregister();

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
      @toggle-sidebar="toggleSidebar"
      @open-settings="settingsOpen = true"
    />

    <AppSidebar
      :open="sidebarOpen"
      :selected-folder="selectedFolder"
      @select-folder="onSelectFolder"
      @add-folder="onAddFolder"
      @add-subfolder="onAddSubfolder"
    />

    <main class="app-main" aria-label="Notes">
      <section class="notes-grid" aria-live="polite">
        <NoteCard
          v-for="note in visibleNotes"
          :key="note.id"
          :note="note"
          @open="openNote"
        />
      </section>
      <button
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

    <SettingsModal :open="settingsOpen" @close="settingsOpen = false" />
    <FolderModal
      :open="folderModalOpen"
      :parent-path="folderModalParent"
      @close="folderModalOpen = false"
    />

    <NoteModal
      :note="activeNote"
      :save="updateNote"
      :remove="deleteNoteAndReminder"
      :move-note="moveNote"
      :open-link="openNoteByLink"
      @close="onNoteClose"
    />
  </div>
</template>

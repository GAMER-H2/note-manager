<script setup>
import { onMounted, onUnmounted, ref } from "vue";
import AppHeader from "./components/AppHeader.vue";
import AppSidebar from "./components/AppSidebar.vue";
import NoteCard from "./components/NoteCard.vue";
import NoteModal from "./components/NoteModal.vue";
import SettingsModal from "./components/SettingsModal.vue";
import { useNotes } from "./composables/useNotes.js";
import { useReminders } from "./composables/useReminders.js";
import { useSettings } from "./composables/useSettings.js";
import { initNotifications } from "./composables/useNotifications.js";
import { useOverlayHistory } from "./composables/useOverlayHistory.js";

const { notes, loadNotes, createNote, updateNote, deleteNote } = useNotes();
const { loadReminders, removeReminder } = useReminders();
const { settings, loadSettings } = useSettings();

// Deleting a note also cancels and forgets any reminder attached to it.
const deleteNoteAndReminder = async (id) => {
  await removeReminder(id);
  await deleteNote(id);
};

const settingsOpen = ref(false);
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
    const note = await createNote();
    openNote(note);
  } catch (err) {
    console.error("Failed to create note:", err);
  }
};

const onAddFolder = () => {
  // Folder management is a planned feature; no-op for now.
};

({ requestClose: requestCloseSidebar } = useOverlayHistory(
  () => isMobile.value && sidebarOpen.value,
  () => {
    sidebarOpen.value = false;
  },
));

onMounted(async () => {
  loadNotes();
  loadReminders();
  // Load settings first so the theme applies and notifications use the chosen
  // urgency, then fire-and-forget notification setup (never blocks the UI).
  await loadSettings();
  initNotifications(settings.urgency);

  if (typeof mql.addEventListener === "function") {
    mql.addEventListener("change", onBreakpointChange);
  } else if (typeof mql.addListener === "function") {
    mql.addListener(onBreakpointChange); // Safari fallback
  }
});

onUnmounted(() => {
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

    <AppSidebar :open="sidebarOpen" @add-folder="onAddFolder" />

    <main class="app-main" aria-label="Notes">
      <section class="notes-grid" aria-live="polite">
        <NoteCard
          v-for="note in notes"
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

    <NoteModal
      :note="activeNote"
      :save="updateNote"
      :remove="deleteNoteAndReminder"
      @close="onNoteClose"
    />
  </div>
</template>

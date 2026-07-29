<script setup>
import { ref, watch, onBeforeUnmount } from "vue";
import { useSettings, THEMES } from "../composables/useSettings.js";
import {
  URGENCY_LEVELS,
  ensureReminderChannel,
} from "../composables/useNotifications.js";
import { useReminders } from "../composables/useReminders.js";
import { useArchive } from "../composables/useArchive.js";
import { isAndroid } from "../lib/platform.js";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";

const props = defineProps({
  open: { type: Boolean, default: false },
});

// `imported` tells the app to reload notes/folders/pins after an import,
// since the vault changed underneath it.
const emit = defineEmits(["close", "imported"]);

const { settings, saveSettings } = useSettings();
const { rescheduleAllReminders } = useReminders();
const {
  busy: archiveBusy,
  status: archiveStatus,
  previewExport,
  exportVault,
  importVault,
} = useArchive();
const androidOnly = isAndroid();

const categories = [
  { id: "general", title: "General" },
  { id: "notifications", title: "Notifications" },
  { id: "appearance", title: "Appearance" },
  { id: "sync", title: "Sync" },
];

const descriptions = {
  general: "Configure behavior and accessibility preferences.",
  notifications: "Control how reminder notifications behave.",
  appearance: "Choose how the app looks.",
  sync: "Back up, restore, and move your notes.",
};

const activeCategory = ref("general");
const exportPreview = ref(null);
const includeHistory = ref(true);
const importMode = ref("merge");

const onExport = () => exportVault({ includeHistory: includeHistory.value });

const onImport = async () => {
  const summary = await importVault({ replace: importMode.value === "replace" });
  if (summary) {
    emit("imported");
    exportPreview.value = await previewExport();
  }
};

// Draft state — only committed to the store on "Apply Changes".
const draftAutosave = ref(true);
const draftUrgency = ref("default");
const draftTheme = ref("dark");

const syncDraftFromSaved = () => {
  draftAutosave.value = settings.autosave;
  draftUrgency.value = settings.urgency;
  draftTheme.value = settings.theme;
};

const closeModal = () => emit("close");
const { requestClose } = useOverlayHistory(
  () => props.open,
  closeModal,
);

const applyChanges = async () => {
  const previousUrgency = settings.urgency;

  await saveSettings({
    autosave: draftAutosave.value,
    urgency: draftUrgency.value,
    theme: draftTheme.value,
  });

  if (androidOnly && previousUrgency !== draftUrgency.value) {
    await ensureReminderChannel(draftUrgency.value);
    await rescheduleAllReminders();
  }

  requestClose();
};

const onEsc = (e) => {
  if (e.key === "Escape" && props.open) requestClose();
};

watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      syncDraftFromSaved();
      previewExport().then((p) => {
        exportPreview.value = p;
      });
      document.documentElement.classList.add("settings-open");
      document.body.classList.add("settings-open");
      window.addEventListener("keydown", onEsc);
    } else {
      document.documentElement.classList.remove("settings-open");
      document.body.classList.remove("settings-open");
      window.removeEventListener("keydown", onEsc);
    }
  },
);

onBeforeUnmount(() => {
  document.documentElement.classList.remove("settings-open");
  document.body.classList.remove("settings-open");
  window.removeEventListener("keydown", onEsc);
});
</script>

<template>
  <div class="settings-overlay" :hidden="!open" @click="requestClose"></div>
  <section
    class="settings-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-title"
    :aria-hidden="String(!open)"
  >
    <div class="settings-modal__content" role="document" tabindex="-1">
      <aside class="settings-categories" aria-label="Settings categories">
        <h2 id="settings-title">Settings</h2>
        <nav role="tablist" aria-label="Settings categories list">
          <button
            v-for="cat in categories"
            :key="cat.id"
            type="button"
            class="settings-category"
            role="tab"
            :aria-selected="String(activeCategory === cat.id)"
            @click="activeCategory = cat.id"
          >
            {{ cat.title }}
          </button>
        </nav>
      </aside>

      <div class="settings-details" role="tabpanel">
        <header class="settings-details__header">
          <div>
            <h3>{{ categories.find((c) => c.id === activeCategory).title }}</h3>
            <p class="settings-description">{{ descriptions[activeCategory] }}</p>
          </div>
          <button
            type="button"
            class="settings-close-button"
            aria-label="Close settings"
            @click="requestClose"
          >
            ×
          </button>
        </header>

        <div class="settings-scroll">
          <!-- General -->
          <ul v-show="activeCategory === 'general'" class="settings-toggle-list">
            <li class="settings-toggle">
              <span>
                <strong>Auto-save notes</strong>
                <small>Save changes automatically while you type.</small>
              </span>
              <label class="switch">
                <input type="checkbox" v-model="draftAutosave" />
                <span class="slider"></span>
              </label>
            </li>
          </ul>

          <!-- Notifications -->
          <ul
            v-show="activeCategory === 'notifications'"
            class="settings-toggle-list"
          >
            <li
              class="settings-toggle settings-toggle--stack"
              :class="{ 'is-disabled': !androidOnly }"
            >
              <span>
                <strong>Notification urgency</strong>
                <small v-if="androidOnly">
                  How prominently reminders appear (sound, heads-up, etc.).
                </small>
                <small v-else>Android only — not configurable on desktop.</small>
              </span>
              <select
                class="settings-select"
                v-model="draftUrgency"
                :disabled="!androidOnly"
              >
                <option
                  v-for="level in URGENCY_LEVELS"
                  :key="level.value"
                  :value="level.value"
                >
                  {{ level.label }}
                </option>
              </select>
            </li>
          </ul>

          <!-- Appearance -->
          <ul
            v-show="activeCategory === 'appearance'"
            class="settings-toggle-list"
          >
            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Theme</strong>
                <small>Choose the app's color theme.</small>
              </span>
              <select class="settings-select" v-model="draftTheme">
                <option
                  v-for="theme in THEMES"
                  :key="theme.value"
                  :value="theme.value"
                >
                  {{ theme.label }}
                </option>
              </select>
            </li>
          </ul>

          <!-- Sync -->
          <ul v-show="activeCategory === 'sync'" class="settings-toggle-list">
            <li class="settings-section-heading">Import &amp; export</li>

            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Export notes to a zip file</strong>
                <small v-if="exportPreview">
                  {{ exportPreview.notes }} note{{
                    exportPreview.notes === 1 ? "" : "s"
                  }}. Files are named by title, so the archive opens cleanly in
                  any markdown editor.
                </small>
                <small v-else>
                  Files are named by title, so the archive opens cleanly in any
                  markdown editor.
                </small>
              </span>
              <button
                type="button"
                class="settings-primary"
                :disabled="archiveBusy"
                @click="onExport"
              >
                Export…
              </button>
            </li>

            <li class="settings-toggle">
              <span>
                <strong>Include version history</strong>
                <small>
                  Adds every stored revision. Makes the archive bigger, but the
                  export becomes a complete backup.
                </small>
              </span>
              <label class="switch">
                <input type="checkbox" v-model="includeHistory" />
                <span class="slider"></span>
              </label>
            </li>

            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Import notes from a zip file</strong>
                <small>
                  Folders in the archive become folders in your vault.
                </small>
              </span>
              <select class="settings-select" v-model="importMode">
                <option value="merge">Merge with existing notes</option>
                <option value="replace">Replace all notes</option>
              </select>
            </li>

            <li class="settings-toggle settings-toggle--stack">
              <span>
                <small v-if="importMode === 'replace'" class="settings-warning">
                  Replace deletes every note in your vault first. Version
                  history is kept.
                </small>
                <small v-else>
                  Merge keeps your existing notes and adds the archive's on top.
                </small>
              </span>
              <button
                type="button"
                class="settings-secondary"
                :disabled="archiveBusy"
                @click="onImport"
              >
                Import…
              </button>
            </li>

            <li v-if="archiveStatus" class="settings-status">
              {{ archiveStatus }}
            </li>
          </ul>
        </div>

        <footer class="settings-modal__footer">
          <button type="button" class="settings-secondary" @click="requestClose">
            Cancel
          </button>
          <button type="button" class="settings-primary" @click="applyChanges">
            Apply Changes
          </button>
        </footer>
      </div>
    </div>
  </section>
</template>

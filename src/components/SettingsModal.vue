<script setup>
import { ref, watch, onBeforeUnmount } from "vue";
import { useSettings, THEMES } from "../composables/useSettings.js";
import {
  URGENCY_LEVELS,
  ensureReminderChannel,
} from "../composables/useNotifications.js";
import { useReminders } from "../composables/useReminders.js";
import { isAndroid } from "../lib/platform.js";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";

const props = defineProps({
  open: { type: Boolean, default: false },
});

const emit = defineEmits(["close"]);

const { settings, saveSettings } = useSettings();
const { rescheduleAllReminders } = useReminders();
const androidOnly = isAndroid();

const categories = [
  { id: "general", title: "General" },
  { id: "notifications", title: "Notifications" },
  { id: "appearance", title: "Appearance" },
];

const descriptions = {
  general: "Configure behavior and accessibility preferences.",
  notifications: "Control how reminder notifications behave.",
  appearance: "Choose how the app looks.",
};

const activeCategory = ref("general");

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

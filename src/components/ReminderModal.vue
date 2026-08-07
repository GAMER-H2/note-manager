<script setup>
import { computed, ref, watch } from "vue";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";
import { useReminders } from "../composables/useReminders.js";
import { URGENCY_LEVELS } from "../composables/useNotifications.js";
import { useSettings } from "../composables/useSettings.js";
import { isAndroid } from "../lib/platform.js";
import { firstLineTitle } from "../lib/notes.js";

const props = defineProps({
  open: { type: Boolean, default: false },
  note: { type: Object, default: null },
});

const emit = defineEmits(["close"]);
const closeModal = () => emit("close");
const { requestClose } = useOverlayHistory(
  () => props.open,
  closeModal,
);

const { getReminder, saveReminder, removeReminder, REPEAT_OPTIONS, BODY_MODES } =
  useReminders();
const { settings } = useSettings();
const androidOnly = isAndroid();

const localAt = ref("");
const localRepeat = ref("none");
const localUrgency = ref("default");
const localBodyMode = ref("full");
const busy = ref(false);
const error = ref("");

// Format a Date as the value a <input type="datetime-local"> expects.
const toInputValue = (date) => {
  const pad = (n) => String(n).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
};

const defaultAt = () => {
  const d = new Date();
  d.setHours(d.getHours() + 1, 0, 0, 0); // next hour, on the hour
  return toInputValue(d);
};

const existing = computed(() =>
  props.note ? getReminder(props.note.id) : null,
);

const noteTitle = computed(() =>
  props.note ? firstLineTitle(props.note.content) : "",
);

// Human-readable description of the current selection.
const summary = computed(() => {
  if (!localAt.value) return "";
  const d = new Date(localAt.value);
  if (Number.isNaN(d.getTime())) return "";
  const time = d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  switch (localRepeat.value) {
    case "hourly":
      return `Every hour at :${String(d.getMinutes()).padStart(2, "0")}`;
    case "daily":
      return `Every day at ${time}`;
    case "weekly":
      return `Every ${d.toLocaleDateString([], { weekday: "long" })} at ${time}`;
    case "monthly":
      return `Monthly on day ${d.getDate()} at ${time}`;
    case "yearly":
      return `Every year on ${d.toLocaleDateString([], { month: "long", day: "numeric" })} at ${time}`;
    default:
      return `Once on ${d.toLocaleDateString([], { dateStyle: "medium" })} at ${time}`;
  }
});

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen) return;
    error.value = "";
    const cfg = existing.value;
    localAt.value = cfg?.at || defaultAt();
    localRepeat.value = cfg?.repeat || "none";
    localUrgency.value = cfg?.urgency || settings.urgency;
    localBodyMode.value = cfg?.bodyMode || "full";
  },
);

const onSave = async () => {
  if (!props.note || !localAt.value) return;
  busy.value = true;
  error.value = "";
  try {
    await saveReminder(props.note, {
      at: localAt.value,
      repeat: localRepeat.value,
      urgency: localUrgency.value,
      bodyMode: localBodyMode.value,
    });
    requestClose();
  } catch (e) {
    error.value =
      "Couldn't schedule the reminder. Make sure notifications are allowed.";
    console.error(e);
  } finally {
    busy.value = false;
  }
};

const onRemove = async () => {
  if (!props.note) return;
  busy.value = true;
  try {
    await removeReminder(props.note.id);
    requestClose();
  } catch (e) {
    console.error(e);
  } finally {
    busy.value = false;
  }
};
</script>

<template>
  <div class="reminder-overlay" :hidden="!open" @click="requestClose"></div>
  <section
    class="reminder-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="reminder-title"
    :aria-hidden="String(!open)"
    @click.self="requestClose"
  >
    <div class="reminder-modal__content" role="document" tabindex="-1">
      <header class="reminder-modal__header">
        <div>
          <h2 id="reminder-title">Reminder</h2>
          <p class="reminder-modal__subtitle">{{ noteTitle }}</p>
        </div>
        <button
          type="button"
          class="settings-close-button"
          aria-label="Close reminder"
          @click="requestClose"
        >
          ×
        </button>
      </header>

      <div class="reminder-modal__body">
        <label class="reminder-field">
          <span class="reminder-field__label">When</span>
          <input
            type="datetime-local"
            class="reminder-input"
            v-model="localAt"
          />
        </label>

        <label class="reminder-field">
          <span class="reminder-field__label">Repeat</span>
          <select class="reminder-input" v-model="localRepeat">
            <option
              v-for="opt in REPEAT_OPTIONS"
              :key="opt.value"
              :value="opt.value"
            >
              {{ opt.label }}
            </option>
          </select>
        </label>

        <label class="reminder-field" :class="{ 'is-disabled': !androidOnly }">
          <span class="reminder-field__label">Priority</span>
          <select
            class="reminder-input"
            v-model="localUrgency"
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
        </label>
        <p v-if="!androidOnly" class="reminder-hint">
          Android only — not configurable on desktop.
        </p>

        <label class="reminder-field">
          <span class="reminder-field__label">Show in notification</span>
          <select class="reminder-input" v-model="localBodyMode">
            <option
              v-for="mode in BODY_MODES"
              :key="mode.value"
              :value="mode.value"
            >
              {{ mode.label }}
            </option>
          </select>
        </label>

        <p class="reminder-summary">{{ summary }}</p>
        <p v-if="error" class="reminder-error">{{ error }}</p>
      </div>

      <footer class="reminder-modal__footer">
        <button
          v-if="existing"
          type="button"
          class="note-delete-button"
          :disabled="busy"
          @click="onRemove"
        >
          Remove
        </button>
        <span class="reminder-footer-spacer"></span>
        <button
          type="button"
          class="settings-secondary"
          :disabled="busy"
          @click="requestClose"
        >
          Cancel
        </button>
        <button
          type="button"
          class="settings-primary"
          :disabled="busy || !localAt"
          @click="onSave"
        >
          {{ existing ? "Update" : "Set reminder" }}
        </button>
      </footer>
    </div>
  </section>
</template>

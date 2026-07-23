<script setup>
import { computed, ref, watch, nextTick, onBeforeUnmount } from "vue";
import {
  debounce,
  firstLineTitle,
  renderMarkdownPreviewLines,
} from "../lib/notes.js";
import { useReminders } from "../composables/useReminders.js";
import { useSettings } from "../composables/useSettings.js";
import { initVisualViewport } from "../composables/useVisualViewport.js";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";
import ReminderModal from "./ReminderModal.vue";

const props = defineProps({
  // The note currently being edited, or null when the editor is closed.
  note: { type: Object, default: null },
  // async (id, content) => void  — persists the note in the backend.
  save: { type: Function, required: true },
  // async (id) => void — deletes the note in the backend.
  remove: { type: Function, required: true },
});

const emit = defineEmits(["close"]);

const { getReminder, refreshReminder } = useReminders();
const { settings } = useSettings();

const reminderOpen = ref(false);
const mobileActionsOpen = ref(false);
const hasReminder = computed(() =>
  props.note ? !!getReminder(props.note.id) : false,
);

const editor = ref(null);
const preview = ref(null);
const draft = ref("");
const status = ref("Saved");
const lastSaved = ref("");
let cleanupViewport = () => {};

const mobileMql = window.matchMedia("(max-width: 720px)");
const isMobile = ref(mobileMql.matches);

const open = computed(() => !!props.note);
const autosaveEnabled = computed(() => settings.autosave !== false);
const dirty = computed(() => draft.value !== lastSaved.value);
const title = computed(() => firstLineTitle(draft.value));
const subtitle = computed(() => {
  if (!props.note) return "";
  const mode = autosaveEnabled.value ? "auto-saves" : "manual save";
  return `${props.note.id}.md • Markdown editor (${mode})`;
});
const previewLines = computed(() => renderMarkdownPreviewLines(draft.value));

// Rendered as one continuous flow (not one block box per line) so line
// breaks are laid out by the exact same algorithm the textarea itself uses.
// Per-line block elements each get their own independently-rounded line box,
// and small sub-pixel rounding differences between the two compound with
// every additional line — which is what caused the cursor to drift further
// out of sync the longer a note got.
const previewHtml = computed(() =>
  previewLines.value
    .map(
      (line) =>
        `<span class="note-modal__preview-line ${line.className}">${line.html}</span>`,
    )
    .join("\n"),
);

const closeModal = () => emit("close");
const { requestClose: requestHistoryClose } = useOverlayHistory(
  () => open.value,
  closeModal,
);
const { requestClose: requestCloseMobileActions } = useOverlayHistory(
  () => open.value && isMobile.value && mobileActionsOpen.value,
  () => {
    mobileActionsOpen.value = false;
  },
);

const syncPreviewScroll = () => {
  if (!editor.value || !preview.value) return;
  preview.value.scrollTop = editor.value.scrollTop;
  preview.value.scrollLeft = editor.value.scrollLeft;
};

const refreshCurrentReminder = async () => {
  if (!props.note || !hasReminder.value) return;
  await refreshReminder({ ...props.note, content: draft.value });
};

const persistNow = async ({ syncReminder = false } = {}) => {
  if (!props.note) return false;
  if (!dirty.value) {
    if (syncReminder) {
      await refreshCurrentReminder();
    }
    status.value = "Saved";
    return false;
  }

  try {
    status.value = "Saving…";
    await props.save(props.note.id, draft.value);
    lastSaved.value = draft.value;

    if (syncReminder) {
      await refreshCurrentReminder();
    }

    status.value = "Saved";
    return true;
  } catch (err) {
    console.error("Failed to persist note:", err);
    status.value = "Save failed";
    return false;
  }
};

const debouncedSave = debounce(() => {
  if (!autosaveEnabled.value || !dirty.value) return;
  persistNow();
}, 400);

const onInput = async () => {
  if (autosaveEnabled.value) {
    status.value = "Editing…";
    debouncedSave();
  } else {
    debouncedSave.cancel?.();
    status.value = dirty.value ? "Unsaved changes" : "Saved";
  }

  await nextTick();
  syncPreviewScroll();
};

const requestClose = async () => {
  debouncedSave.cancel?.();
  if (mobileActionsOpen.value) {
    await requestCloseMobileActions();
    await nextTick();
  }
  await persistNow({ syncReminder: true });
  requestHistoryClose();
};

// Save the latest text first so the reminder configuration uses current content.
const openReminder = async () => {
  if (mobileActionsOpen.value) {
    await requestCloseMobileActions();
    await nextTick();
  }
  debouncedSave.cancel?.();
  await persistNow({ syncReminder: true });
  reminderOpen.value = true;
};

const openMobileActions = () => {
  if (!isMobile.value) return;
  if (mobileActionsOpen.value) {
    requestCloseMobileActions();
    return;
  }
  mobileActionsOpen.value = true;
};

const handleDelete = async () => {
  if (mobileActionsOpen.value) {
    await requestCloseMobileActions();
    await nextTick();
  }
  if (!props.note) return;
  try {
    await props.remove(props.note.id);
    requestHistoryClose();
  } catch (err) {
    console.error("Failed to delete note:", err);
    status.value = "Delete failed";
  }
};

const onKeydown = (e) => {
  if (!open.value) return;

  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
    e.preventDefault();
    debouncedSave.cancel?.();
    persistNow({ syncReminder: true });
    return;
  }

  if (e.key === "Escape") {
    if (mobileActionsOpen.value) {
      requestCloseMobileActions();
      return;
    }
    requestClose();
  }
};

const onViewportChange = (e) => {
  isMobile.value = e.matches;
  if (!e.matches && mobileActionsOpen.value) {
    requestCloseMobileActions();
  }
};

watch(autosaveEnabled, (enabled) => {
  if (!open.value) return;

  if (enabled) {
    if (dirty.value) {
      status.value = "Editing…";
      debouncedSave();
    }
    return;
  }

  debouncedSave.cancel?.();
  status.value = dirty.value ? "Unsaved changes" : "Saved";
});

watch(
  draft,
  async () => {
    await nextTick();
    syncPreviewScroll();
  },
  { flush: "post" },
);

// Lock background scroll + wire keyboard handling while the editor is open, and
// initialise the draft whenever a (different) note is opened.
watch(
  () => props.note,
  async (note, prev) => {
    if (note) {
      draft.value = note.content ?? "";
      lastSaved.value = draft.value;
      status.value = "Saved";

      cleanupViewport();
      cleanupViewport = initVisualViewport();

      document.documentElement.classList.add("note-open");
      document.body.classList.add("note-open");
      window.removeEventListener("keydown", onKeydown);
      window.addEventListener("keydown", onKeydown);

      await nextTick();
      syncPreviewScroll();
      const el = editor.value;
      if (el) {
        el.focus();
        el.selectionStart = el.selectionEnd = el.value.length;
      }
    } else if (prev) {
      reminderOpen.value = false;
      mobileActionsOpen.value = false;
      debouncedSave.cancel?.();
      cleanupViewport();
      cleanupViewport = () => {};
      document.documentElement.classList.remove("note-open");
      document.body.classList.remove("note-open");
      window.removeEventListener("keydown", onKeydown);
    }
  },
);

if (typeof mobileMql.addEventListener === "function") {
  mobileMql.addEventListener("change", onViewportChange);
} else if (typeof mobileMql.addListener === "function") {
  mobileMql.addListener(onViewportChange);
}

onBeforeUnmount(() => {
  debouncedSave.cancel?.();
  cleanupViewport();
  document.documentElement.classList.remove("note-open");
  document.body.classList.remove("note-open");
  window.removeEventListener("keydown", onKeydown);

  if (typeof mobileMql.removeEventListener === "function") {
    mobileMql.removeEventListener("change", onViewportChange);
  } else if (typeof mobileMql.removeListener === "function") {
    mobileMql.removeListener(onViewportChange);
  }
});
</script>

<template>
  <div class="note-overlay" :hidden="!open" @click="requestClose"></div>
  <section
    class="note-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="note-editor-title"
    :aria-hidden="String(!open)"
  >
    <div class="note-modal__content" role="document" tabindex="-1">
      <header class="note-modal__header">
        <div>
          <h2 id="note-editor-title">{{ title }}</h2>
          <p class="note-modal__subtitle">{{ subtitle }}</p>
        </div>
        <div class="note-modal__actions">
          <template v-if="isMobile">
            <button
              type="button"
              class="note-actions-button"
              aria-label="Open note actions"
              :aria-expanded="String(mobileActionsOpen)"
              @click="openMobileActions"
            >
              ⋮
            </button>
          </template>
          <template v-else>
            <button
              type="button"
              class="note-reminder-button"
              :class="{ 'is-active': hasReminder }"
              :aria-label="hasReminder ? 'Edit reminder' : 'Add reminder'"
              @click="openReminder"
            >
              {{ hasReminder ? "🔔 Reminder" : "Reminder" }}
            </button>
            <button
              type="button"
              class="note-delete-button"
              aria-label="Delete note"
              @click="handleDelete"
            >
              Delete
            </button>
            <button
              type="button"
              class="note-close-button"
              aria-label="Close note"
              @click="requestClose"
            >
              ×
            </button>
          </template>
        </div>
      </header>

      <div class="note-modal__editor-shell">
        <div
          ref="preview"
          class="note-modal__preview"
          aria-hidden="true"
          v-html="previewHtml"
        ></div>

        <textarea
          ref="editor"
          v-model="draft"
          class="note-modal__editor"
          aria-label="Edit note (markdown)"
          placeholder="Start typing markdown…"
          spellcheck="true"
          @input="onInput"
          @scroll="syncPreviewScroll"
          @blur="autosaveEnabled ? persistNow() : undefined"
        ></textarea>
      </div>

      <footer class="note-modal__footer">
        <span class="note-modal__status">{{ status }}</span>
        <div class="note-modal__footer-actions">
          <button
            v-if="!autosaveEnabled"
            type="button"
            class="note-modal__done"
            :disabled="!dirty || status === 'Saving…'"
            @click="persistNow({ syncReminder: true })"
          >
            Save
          </button>
          <button
            type="button"
            class="note-modal__done note-modal__done--primary"
            aria-label="Done"
            @click="requestClose"
          >
            Done
          </button>
        </div>
      </footer>

      <div
        v-if="isMobile && mobileActionsOpen"
        class="note-actions-menu__backdrop"
        @click="requestCloseMobileActions"
      ></div>
      <section
        v-if="isMobile && mobileActionsOpen"
        class="note-actions-menu"
        role="dialog"
        aria-modal="true"
        aria-label="Note actions"
      >
        <button
          type="button"
          class="note-actions-menu__item"
          :class="{ 'is-active': hasReminder }"
          @click="openReminder"
        >
          {{ hasReminder ? "Edit reminder" : "Reminder" }}
        </button>
        <button
          type="button"
          class="note-actions-menu__item note-actions-menu__item--danger"
          @click="handleDelete"
        >
          Delete
        </button>
      </section>
    </div>
  </section>

  <ReminderModal
    :open="reminderOpen"
    :note="note"
    @close="reminderOpen = false"
  />
</template>

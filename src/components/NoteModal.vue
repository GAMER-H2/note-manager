<script setup>
import { computed, ref, watch, nextTick, onBeforeUnmount } from "vue";
import {
  debounce,
  firstLineTitle,
  renderMarkdownPreviewLines,
} from "../lib/notes.js";
import { useReminders } from "../composables/useReminders.js";
import { useSettings } from "../composables/useSettings.js";
import { useFolders } from "../composables/useFolders.js";
import { usePinned } from "../composables/usePinned.js";
import { initVisualViewport } from "../composables/useVisualViewport.js";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";
import { isAndroid } from "../lib/platform.js";
import ReminderModal from "./ReminderModal.vue";
import HistoryModal from "./HistoryModal.vue";

const props = defineProps({
  // The note currently being edited, or null when the editor is closed.
  note: { type: Object, default: null },
  // async (id, content) => void  — persists the note in the backend.
  save: { type: Function, required: true },
  // async (id) => void — deletes the note in the backend.
  remove: { type: Function, required: true },
  // async (id, folder) => void — moves the note to another folder.
  moveNote: { type: Function, required: true },
  // (folderPath, title) => boolean — opens a note referenced by a markdown
  // link, returning whether a match was found.
  openLink: { type: Function, required: true },
});

const emit = defineEmits(["close"]);

const { getReminder, refreshReminder } = useReminders();
const { settings } = useSettings();
const { realFolders } = useFolders();
const { isPinned, togglePin } = usePinned();

const reminderOpen = ref(false);
const historyOpen = ref(false);
const mobileActionsOpen = ref(false);
const hasReminder = computed(() =>
  props.note ? !!getReminder(props.note.id) : false,
);
const noteFolder = computed(() => props.note?.folder || "General");
const pinned = computed(() => (props.note ? isPinned(props.note.id) : false));

// Note-link clicks navigate on a plain tap on Android; on desktop they require
// Ctrl/Cmd+click, since the preview is a syntax-highlighted overlay (not real
// rendered HTML) and a plain click needs to keep placing the cursor normally.
const androidPlatform = isAndroid();
let notFoundTimeout = null;

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

// Persist before opening so the version you're looking at in the diff is the
// version on disk — otherwise unsaved edits read as "no changes since".
const openHistory = async () => {
  if (mobileActionsOpen.value) {
    await requestCloseMobileActions();
    await nextTick();
  }
  debouncedSave.cancel?.();
  await persistNow({ syncReminder: true });
  historyOpen.value = true;
};

// The backend already wrote the restored text to disk, so adopt it as the
// saved baseline too — treating it as a pending edit would immediately mark
// the note dirty and re-save identical content.
const onRestored = (content) => {
  draft.value = content ?? "";
  lastSaved.value = draft.value;
  status.value = "Restored";
};

const openMobileActions = () => {
  if (!isMobile.value) return;
  if (mobileActionsOpen.value) {
    requestCloseMobileActions();
    return;
  }
  mobileActionsOpen.value = true;
};

const onFolderChange = async (folder) => {
  if (mobileActionsOpen.value) {
    await requestCloseMobileActions();
    await nextTick();
  }
  if (!props.note || folder === noteFolder.value) return;
  try {
    await props.moveNote(props.note.id, folder);
  } catch (err) {
    console.error("Failed to move note:", err);
  }
};

const onTogglePin = async () => {
  if (mobileActionsOpen.value) {
    await requestCloseMobileActions();
    await nextTick();
  }
  if (!props.note) return;
  try {
    await togglePin(props.note.id);
  } catch (err) {
    console.error("Failed to toggle pin:", err);
  }
};

// Delegated click handler for the (pointer-events: none) preview pane — only
// `.md-note-link` spans opt back into receiving clicks (see styles.css).
const onPreviewClick = async (event) => {
  const linkEl = event.target.closest?.(".md-note-link");
  if (!linkEl) return;
  if (!androidPlatform && !(event.ctrlKey || event.metaKey)) return;

  const raw = linkEl.dataset.noteLink;
  if (!raw) return;
  let linkPath;
  try {
    linkPath = decodeURIComponent(raw);
  } catch {
    return;
  }

  const slash = linkPath.lastIndexOf("/");
  const folderPath = slash === -1 ? null : linkPath.slice(0, slash);
  const noteTitle = (slash === -1 ? linkPath : linkPath.slice(slash + 1)).trim();
  if (!noteTitle) return;

  if (mobileActionsOpen.value) {
    await requestCloseMobileActions();
    await nextTick();
  }
  debouncedSave.cancel?.();
  await persistNow({ syncReminder: true });

  const found = await props.openLink(folderPath, noteTitle);
  if (!found) {
    clearTimeout(notFoundTimeout);
    status.value = "Note not found";
    notFoundTimeout = setTimeout(() => {
      if (status.value === "Note not found") status.value = "Saved";
    }, 1500);
  }
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
      historyOpen.value = false;
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
            <select
              class="note-folder-select"
              aria-label="Folder"
              :value="noteFolder"
              @change="onFolderChange($event.target.value)"
            >
              <option v-for="f in realFolders" :key="f" :value="f">{{ f }}</option>
            </select>
            <button
              type="button"
              class="note-pin-button"
              :class="{ 'is-active': pinned }"
              :aria-label="pinned ? 'Unpin note' : 'Pin note'"
              @click="onTogglePin"
            >
              <span v-if="pinned" class="folder-icon folder-icon--pinned" aria-hidden="true"></span>
              {{ pinned ? "Pinned" : "Pin" }}
            </button>
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
              class="note-history-button"
              aria-label="Version history"
              @click="openHistory"
            >
              History
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
          @click="onPreviewClick"
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
        <label class="note-actions-menu__item note-actions-menu__item--select">
          Folder
          <select
            aria-label="Folder"
            :value="noteFolder"
            @change="onFolderChange($event.target.value)"
          >
            <option v-for="f in realFolders" :key="f" :value="f">{{ f }}</option>
          </select>
        </label>
        <button
          type="button"
          class="note-actions-menu__item"
          :class="{ 'is-active': pinned }"
          @click="onTogglePin"
        >
          {{ pinned ? "Unpin" : "Pin" }}
        </button>
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
          class="note-actions-menu__item"
          @click="openHistory"
        >
          Version history
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

  <HistoryModal
    :open="historyOpen"
    :note="note"
    @close="historyOpen = false"
    @restored="onRestored"
  />
</template>

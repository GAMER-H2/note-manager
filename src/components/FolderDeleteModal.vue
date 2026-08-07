<script setup>
import { computed } from "vue";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";

const props = defineProps({
  open: { type: Boolean, default: false },
  // Full path of the folder being deleted, e.g. "Work/Projects".
  folder: { type: String, default: "" },
  // Notes directly and indirectly under the folder.
  noteCount: { type: Number, default: 0 },
  // Whether the folder has any subfolders (they're always removed).
  hasSubfolders: { type: Boolean, default: false },
});

const emit = defineEmits(["close", "confirm"]);
const closeModal = () => emit("close");
const { requestClose } = useOverlayHistory(() => props.open, closeModal);

const folderName = computed(() => props.folder.split("/").pop() || props.folder);
const noteLabel = computed(() =>
  props.noteCount === 1 ? "1 note" : `${props.noteCount} notes`,
);

const leadText = computed(() => {
  const where = props.hasSubfolders ? " (across it and its subfolders)" : "";
  const tail = props.hasSubfolders
    ? " Either way, the subfolders are removed too."
    : "";
  return `This folder holds ${noteLabel.value}${where}. Choose what happens to them.${tail}`;
});

const moveNote = computed(() =>
  props.hasSubfolders
    ? "Every note is unpacked into General; the subfolder structure is not kept."
    : "Every note is moved into General.",
);

const deleteNote = computed(() =>
  props.hasSubfolders
    ? `Permanently removes ${noteLabel.value} and all subfolders. This can't be undone.`
    : `Permanently removes ${noteLabel.value}. This can't be undone.`,
);

const choose = (mode) => emit("confirm", mode);
</script>

<template>
  <div class="folder-overlay" :hidden="!open" @click="requestClose"></div>
  <section
    class="folder-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="folder-delete-title"
    :aria-hidden="String(!open)"
    @click.self="requestClose"
  >
    <div class="folder-modal__content" role="document" tabindex="-1">
      <header class="folder-modal__header">
        <h2 id="folder-delete-title">Delete “{{ folderName }}”?</h2>
        <button
          type="button"
          class="settings-close-button"
          aria-label="Close"
          @click="requestClose"
        >
          ×
        </button>
      </header>

      <div class="folder-modal__body">
        <p class="folder-delete__lead">{{ leadText }}</p>

        <div class="folder-delete__options">
          <button
            type="button"
            class="folder-delete__option"
            @click="choose('move')"
          >
            <span class="folder-delete__option-title">Move notes to General</span>
            <span class="folder-delete__option-note">{{ moveNote }}</span>
          </button>
          <button
            type="button"
            class="folder-delete__option folder-delete__option--danger"
            @click="choose('delete')"
          >
            <span class="folder-delete__option-title">Delete all notes</span>
            <span class="folder-delete__option-note">{{ deleteNote }}</span>
          </button>
        </div>
      </div>

      <footer class="reminder-modal__footer">
        <span class="reminder-footer-spacer"></span>
        <button type="button" class="settings-secondary" @click="requestClose">
          Cancel
        </button>
      </footer>
    </div>
  </section>
</template>

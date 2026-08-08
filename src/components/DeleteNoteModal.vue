<script setup>
import { useOverlayHistory } from "../composables/useOverlayHistory.js";

const props = defineProps({
  open: { type: Boolean, default: false },
  // Hidden when the note is already archived (nothing to archive it into).
  canArchive: { type: Boolean, default: true },
});

const emit = defineEmits(["close", "archive", "delete"]);
const closeModal = () => emit("close");
const { requestClose } = useOverlayHistory(() => props.open, closeModal);
</script>

<template>
  <div class="folder-overlay" :hidden="!open" @click="requestClose"></div>
  <section
    class="folder-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="delete-note-title"
    :aria-hidden="String(!open)"
    @click.self="requestClose"
  >
    <div class="folder-modal__content" role="document" tabindex="-1">
      <header class="folder-modal__header">
        <h2 id="delete-note-title">Delete this note?</h2>
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
        <p class="folder-delete__lead">
          <template v-if="canArchive">
            Archive it to keep it out of the way (you can restore it later), or
            delete it permanently.
          </template>
          <template v-else>
            This permanently deletes the note. This can't be undone.
          </template>
        </p>

        <div class="folder-delete__options">
          <button
            v-if="canArchive"
            type="button"
            class="folder-delete__option"
            @click="emit('archive')"
          >
            <span class="folder-delete__option-title">Archive</span>
            <span class="folder-delete__option-note">
              Moves it to the Archive folder. Restorable anytime.
            </span>
          </button>
          <button
            type="button"
            class="folder-delete__option folder-delete__option--danger"
            @click="emit('delete')"
          >
            <span class="folder-delete__option-title">Delete</span>
            <span class="folder-delete__option-note">
              Permanently removes the note. This can't be undone.
            </span>
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

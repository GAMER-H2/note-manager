<script setup>
import { computed, ref, watch } from "vue";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";
import { useFolders } from "../composables/useFolders.js";

const props = defineProps({
  open: { type: Boolean, default: false },
  // Full path of the folder to nest the new one under, or null for a top-level folder.
  parentPath: { type: String, default: null },
});

const heading = computed(() =>
  props.parentPath ? `New subfolder in ${props.parentPath}` : "New folder",
);

const emit = defineEmits(["close"]);
const closeModal = () => emit("close");
const { requestClose } = useOverlayHistory(
  () => props.open,
  closeModal,
);

const { createFolder } = useFolders();

const name = ref("");
const busy = ref(false);
const error = ref("");

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen) return;
    name.value = "";
    error.value = "";
  },
);

const onCreate = async () => {
  const trimmed = name.value.trim();
  if (!trimmed) return;
  busy.value = true;
  error.value = "";
  try {
    await createFolder(trimmed, props.parentPath);
    requestClose();
  } catch (e) {
    error.value = "Couldn't create that folder.";
    console.error(e);
  } finally {
    busy.value = false;
  }
};
</script>

<template>
  <div class="folder-overlay" :hidden="!open" @click="requestClose"></div>
  <section
    class="folder-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="folder-title"
    :aria-hidden="String(!open)"
  >
    <div class="folder-modal__content" role="document" tabindex="-1">
      <header class="folder-modal__header">
        <h2 id="folder-title">{{ heading }}</h2>
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
        <label class="reminder-field">
          <span class="reminder-field__label">Name</span>
          <input
            type="text"
            class="reminder-input"
            v-model="name"
            placeholder="e.g. Work"
            @keydown.enter.prevent="onCreate"
          />
        </label>
        <p v-if="error" class="reminder-error">{{ error }}</p>
      </div>

      <footer class="reminder-modal__footer">
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
          :disabled="busy || !name.trim()"
          @click="onCreate"
        >
          Create
        </button>
      </footer>
    </div>
  </section>
</template>

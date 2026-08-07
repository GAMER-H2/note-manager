<script setup>
import { computed, ref, watch } from "vue";
import { useHistory, formatRevisionTime } from "../composables/useHistory.js";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";

const props = defineProps({
  open: { type: Boolean, default: false },
  // The note whose history is being browsed, or null.
  note: { type: Object, default: null },
});

// `restored` carries the new content back up so the open editor can adopt it
// without a full reload (the modal writes through the backend, but the
// editor's draft is what the user is actually looking at).
const emit = defineEmits(["close", "restored"]);

const {
  revisions,
  loading,
  error,
  loadRevisions,
  diffRevisions,
  restoreRevision,
} = useHistory();

const selected = ref(null);
const diff = ref([]);
const diffLoading = ref(false);
const restoring = ref(false);
const deviceId = ref("");

const closeModal = () => emit("close");
const { requestClose } = useOverlayHistory(() => props.open, closeModal);

const selectedMeta = computed(
  () => revisions.value.find((r) => r.rev === selected.value) ?? null,
);

// Counts drive the "+3 −1" summary above the diff.
const addedCount = computed(() => diff.value.filter((l) => l.op === "add").length);
const removedCount = computed(
  () => diff.value.filter((l) => l.op === "remove").length,
);
// Derived from the diff rather than by comparing hashes: the note object the
// editor holds keeps its hash from load time, so it goes stale the moment you
// type, and this needs to stay accurate for the Restore button's disabled state.
const isCurrent = computed(
  () => diff.value.length > 0 && diff.value.every((l) => l.op === "same"),
);

const loadDiff = async (rev) => {
  if (!props.note?.id || rev == null) return;
  diffLoading.value = true;
  try {
    // No `to` — always compared against the note as it stands right now,
    // which is the question you actually have when browsing history.
    diff.value = await diffRevisions(props.note.id, rev);
  } catch (e) {
    console.error("Failed to diff revision:", e);
    diff.value = [];
  } finally {
    diffLoading.value = false;
  }
};

const selectRevision = async (rev) => {
  selected.value = rev;
  await loadDiff(rev);
};

const onRestore = async () => {
  if (!props.note?.id || selected.value == null || restoring.value) return;
  restoring.value = true;
  try {
    const content = await restoreRevision(props.note.id, selected.value);
    emit("restored", content);
    // Restoring records both the pre-restore and restored states, so the list
    // is stale the moment it succeeds.
    await loadRevisions(props.note.id);
    if (revisions.value.length) await selectRevision(revisions.value[0].rev);
  } catch (e) {
    console.error("Failed to restore revision:", e);
  } finally {
    restoring.value = false;
  }
};

watch(
  () => [props.open, props.note?.id],
  async ([isOpen]) => {
    if (!isOpen || !props.note?.id) return;
    selected.value = null;
    diff.value = [];
    await loadRevisions(props.note.id);
    if (revisions.value.length) await selectRevision(revisions.value[0].rev);
  },
  { immediate: true },
);
</script>

<template>
  <div class="history-overlay" :hidden="!open" @click="requestClose"></div>
  <section
    class="history-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="history-title"
    :aria-hidden="String(!open)"
    @click.self="requestClose"
  >
    <div class="history-modal__content" role="document" tabindex="-1">
      <header class="history-modal__header">
        <div>
          <h2 id="history-title">Version history</h2>
          <p class="history-modal__subtitle">
            Earlier versions of this note, newest first.
          </p>
        </div>
        <button
          type="button"
          class="history-close-button"
          aria-label="Close history"
          @click="requestClose"
        >
          ×
        </button>
      </header>

      <div class="history-modal__body">
        <aside class="history-list" aria-label="Revisions">
          <p v-if="loading" class="history-empty">Loading…</p>
          <p v-else-if="error" class="history-empty">{{ error }}</p>
          <p v-else-if="!revisions.length" class="history-empty">
            No earlier versions yet. They're recorded as you edit.
          </p>
          <button
            v-for="rev in revisions"
            :key="rev.rev"
            type="button"
            class="history-entry"
            :class="{ 'is-active': rev.rev === selected }"
            @click="selectRevision(rev.rev)"
          >
            <strong>{{ formatRevisionTime(rev.ts) }}</strong>
            <small>{{ rev.title }}</small>
            <small class="history-entry__meta">
              {{ rev.bytes }} bytes · {{ rev.device }}
            </small>
          </button>
        </aside>

        <div class="history-diff">
          <div v-if="selectedMeta" class="history-diff__summary">
            <span v-if="isCurrent" class="history-diff__badge">
              Matches the current note
            </span>
            <span v-else>
              <span class="history-diff__added">+{{ addedCount }}</span>
              <span class="history-diff__removed">−{{ removedCount }}</span>
              <span class="history-diff__hint">compared with the note now</span>
            </span>
          </div>

          <p v-if="diffLoading" class="history-empty">Comparing…</p>
          <p v-else-if="!selectedMeta" class="history-empty">
            Select a version to compare it with the current note.
          </p>
          <pre v-else class="history-diff__lines"><code
            v-for="(line, i) in diff"
            :key="i"
            class="history-diff__line"
            :class="`history-diff__line--${line.op}`"
          >{{ line.op === "add" ? "+" : line.op === "remove" ? "−" : " " }} {{ line.text }}
</code></pre>
        </div>
      </div>

      <footer class="history-modal__footer">
        <span class="history-modal__status">
          {{ revisions.length }} version{{ revisions.length === 1 ? "" : "s" }} kept
        </span>
        <div class="history-modal__footer-actions">
          <button type="button" class="settings-secondary" @click="requestClose">
            Close
          </button>
          <button
            type="button"
            class="settings-primary"
            :disabled="selected == null || restoring || isCurrent"
            @click="onRestore"
          >
            {{ restoring ? "Restoring…" : "Restore this version" }}
          </button>
        </div>
      </footer>
    </div>
  </section>
</template>

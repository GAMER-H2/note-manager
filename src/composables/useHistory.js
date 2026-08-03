import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

// Bridge to the Rust-side revision store (.notemanager/history/<id>.jsonl).
//
// Revisions are recorded by the backend on every save, coalescing edits made
// within ten minutes, so nothing here needs to decide *when* to snapshot —
// only how to read back what was stored.
export function useHistory() {
  const revisions = ref([]);
  const loading = ref(false);
  const error = ref("");

  const loadRevisions = async (noteId) => {
    if (!noteId) {
      revisions.value = [];
      return;
    }
    loading.value = true;
    error.value = "";
    try {
      const list = await invoke("list_revisions", { id: noteId });
      revisions.value = Array.isArray(list) ? list : [];
    } catch (e) {
      console.error("Failed to load revisions:", e);
      error.value = "Couldn't load history.";
      revisions.value = [];
    } finally {
      loading.value = false;
    }
  };

  const getRevision = (noteId, rev) => invoke("get_revision", { id: noteId, rev });

  // `to` omitted compares the revision against the note's current content.
  const diffRevisions = (noteId, from, to) =>
    invoke("diff_revisions", { id: noteId, from, to: to ?? null });

  const restoreRevision = (noteId, rev) =>
    invoke("restore_revision", { id: noteId, rev });

  const clearHistory = (noteId) => invoke("clear_history", { id: noteId });

  return {
    revisions,
    loading,
    error,
    loadRevisions,
    getRevision,
    diffRevisions,
    restoreRevision,
    clearHistory,
  };
}

// "just now" / "14 minutes ago" / "3 Feb, 09:12" — recent revisions are the
// ones you're most likely to be reaching for, so they get relative labels.
export const formatRevisionTime = (ms) => {
  if (!ms) return "unknown";
  const then = new Date(ms);
  const diff = Date.now() - ms;

  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) {
    const mins = Math.round(diff / 60_000);
    return `${mins} minute${mins === 1 ? "" : "s"} ago`;
  }
  if (diff < 86_400_000) {
    const hours = Math.round(diff / 3_600_000);
    return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  }
  return then.toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
};

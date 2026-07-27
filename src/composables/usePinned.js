import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

// Shared, module-level store of pinned note ids. "Pinned" is a virtual folder:
// it's just a filtered view over these ids, not a real directory.
const pinnedIds = reactive(new Set());
let loaded = false;

const persist = async () => {
  try {
    await invoke("set_pinned", { data: JSON.stringify([...pinnedIds]) });
  } catch (e) {
    console.error("Failed to persist pinned notes:", e);
  }
};

export function usePinned() {
  const loadPinned = async () => {
    if (loaded) return;
    try {
      const raw = await invoke("get_pinned");
      const ids = JSON.parse(raw || "[]");
      if (Array.isArray(ids)) {
        ids.forEach((id) => pinnedIds.add(id));
      }
    } catch (e) {
      console.warn("Failed to load pinned notes:", e);
    } finally {
      loaded = true;
    }
  };

  const isPinned = (noteId) => pinnedIds.has(noteId);

  const togglePin = async (noteId) => {
    if (pinnedIds.has(noteId)) {
      pinnedIds.delete(noteId);
    } else {
      pinnedIds.add(noteId);
    }
    await persist();
  };

  const unpin = async (noteId) => {
    if (!pinnedIds.has(noteId)) return;
    pinnedIds.delete(noteId);
    await persist();
  };

  return { pinnedIds, loadPinned, isPinned, togglePin, unpin };
}

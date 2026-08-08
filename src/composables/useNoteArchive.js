import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

// Shared, module-level store mapping a note id to the folder it lived in before
// it was archived. "Archive" itself is a real folder (notes physically move
// there); this only remembers where to send them back on restore.
const origins = reactive({});
let loaded = false;

const persist = async () => {
  try {
    await invoke("set_archive_origins", { data: JSON.stringify(origins) });
  } catch (e) {
    console.error("Failed to persist archive origins:", e);
  }
};

export function useNoteArchive() {
  const loadArchiveOrigins = async () => {
    if (loaded) return;
    try {
      const raw = await invoke("get_archive_origins");
      Object.assign(origins, JSON.parse(raw || "{}"));
    } catch (e) {
      console.warn("Failed to load archive origins:", e);
    } finally {
      loaded = true;
    }
  };

  // Re-reads from disk, bypassing the load-once guard, after an import or sync
  // pull rewrote the vault.
  const reloadArchiveOrigins = async () => {
    try {
      const raw = await invoke("get_archive_origins");
      Object.keys(origins).forEach((key) => delete origins[key]);
      Object.assign(origins, JSON.parse(raw || "{}"));
    } catch (e) {
      console.warn("Failed to reload archive origins:", e);
    } finally {
      loaded = true;
    }
  };

  const rememberOrigin = async (noteId, folder) => {
    origins[noteId] = folder;
    await persist();
  };

  const originOf = (noteId) => origins[noteId] ?? null;

  const forgetOrigin = async (noteId) => {
    if (!(noteId in origins)) return;
    delete origins[noteId];
    await persist();
  };

  const forgetOrigins = async (noteIds) => {
    let changed = false;
    for (const id of noteIds) {
      if (id in origins) {
        delete origins[id];
        changed = true;
      }
    }
    if (changed) await persist();
  };

  return {
    loadArchiveOrigins,
    reloadArchiveOrigins,
    rememberOrigin,
    originOf,
    forgetOrigin,
    forgetOrigins,
  };
}

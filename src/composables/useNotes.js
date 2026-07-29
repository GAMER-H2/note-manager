import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

// Central store + backend bridge for notes. Each note is
// { id, path, content, folder, mtime, hash }. The Rust side persists one
// markdown file per note under the configured vault root.
export function useNotes() {
  const notes = ref([]);
  const ready = ref(false);

  const loadNotes = async () => {
    try {
      const list = await invoke("list_notes");
      notes.value = Array.isArray(list) ? list : [];
    } catch (err) {
      // Happens when running outside a Tauri context (e.g. plain browser preview).
      console.warn("Failed to load notes (is the Tauri backend available?):", err);
      notes.value = [];
    } finally {
      ready.value = true;
    }
  };

  const createNote = async (folder) => {
    const res = await invoke("create_note", { folder });
    if (!res?.id) throw new Error("create_note returned no id");
    const note = {
      id: res.id,
      path: res.path ?? "",
      content: res.content ?? "",
      folder: res.folder ?? "General",
      mtime: res.mtime ?? 0,
      hash: res.hash ?? "",
    };
    // Backend lists newest-first; keep the same order in the UI.
    notes.value.unshift(note);
    return note;
  };

  const updateNote = async (id, content) => {
    await invoke("update_note", { req: { id, content } });
    const n = notes.value.find((x) => x.id === id);
    if (n) n.content = content;
  };

  const deleteNote = async (id) => {
    await invoke("delete_note", { req: { id } });
    notes.value = notes.value.filter((x) => x.id !== id);
  };

  // Mutates the existing note object in place (rather than replacing it) so
  // object identity is preserved for whoever else holds a reference to it
  // (e.g. the currently open NoteModal's `activeNote`).
  const moveNote = async (id, folder) => {
    const res = await invoke("move_note", { req: { id, folder } });
    const n = notes.value.find((x) => x.id === id);
    if (n) {
      n.folder = res?.folder ?? folder;
      n.path = res?.path ?? n.path;
    }
    return res;
  };

  return { notes, ready, loadNotes, createNote, updateNote, deleteNote, moveNote };
}

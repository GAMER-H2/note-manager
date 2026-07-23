import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

// Central store + backend bridge for notes. Each note is { id, path, content }.
// The Rust side persists one markdown file per note in the app data dir.
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

  const createNote = async () => {
    const res = await invoke("create_note");
    if (!res?.id) throw new Error("create_note returned no id");
    const note = { id: res.id, path: res.path ?? "", content: res.content ?? "" };
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

  return { notes, ready, loadNotes, createNote, updateNote, deleteNote };
}

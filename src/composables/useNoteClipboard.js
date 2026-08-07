import { ref } from "vue";

// Module-level "clipboard" for the copy/paste context-menu actions. Holds a
// copied note's markdown; Paste creates a fresh note (new id, new file) from it
// in whatever folder is being viewed, so it's a duplicate rather than a move.
const clipboard = ref(null);

export function useNoteClipboard() {
  const copyNote = (note) => {
    clipboard.value = { content: note?.content ?? "" };
  };

  const clearClipboard = () => {
    clipboard.value = null;
  };

  return { clipboard, copyNote, clearClipboard };
}

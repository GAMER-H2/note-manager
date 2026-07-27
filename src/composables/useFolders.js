import { computed, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export const PINNED_FOLDER = "Pinned";
export const GENERAL_FOLDER = "General";

// Shared, module-level store. `realFolders` mirrors the actual directories on
// disk (always includes "General"), each entry a full relative path
// ("Work", "Work/Projects", ...) since folders can be nested. "Pinned" is a
// virtual view, never a real folder, so it's prepended here rather than
// coming from the backend.
const realFolders = ref([GENERAL_FOLDER]);
const selectedFolder = ref(PINNED_FOLDER);
// Which folder-tree nodes are expanded in the sidebar, keyed by full path.
const expandedPaths = reactive(new Set());

export function useFolders() {
  const folders = computed(() => [PINNED_FOLDER, ...realFolders.value]);

  // Build a tree from the flat path list. Sorting alphabetically first also
  // guarantees a parent is always processed before its children, since a
  // path is always a string-prefix of its descendants.
  const folderTree = computed(() => {
    const sorted = [...realFolders.value].sort((a, b) => a.localeCompare(b));
    const nodesByPath = new Map();
    const roots = [];

    for (const path of sorted) {
      const segments = path.split("/");
      const name = segments[segments.length - 1];
      const node = { path, name, children: [] };
      nodesByPath.set(path, node);

      const parentPath = segments.slice(0, -1).join("/");
      const parent = parentPath ? nodesByPath.get(parentPath) : null;
      if (parent) {
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }

    // "General" first; stable sort keeps the rest in the alphabetical order
    // they were already in.
    roots.sort((a, b) => {
      if (a.path === GENERAL_FOLDER) return -1;
      if (b.path === GENERAL_FOLDER) return 1;
      return 0;
    });

    return roots;
  });

  // Always re-fetches (rather than caching like useReminders/useSettings)
  // since it's also called after creating a folder to pick up the new entry.
  const loadFolders = async () => {
    try {
      const list = await invoke("list_folders");
      realFolders.value = Array.isArray(list) && list.length ? list : [GENERAL_FOLDER];
    } catch (e) {
      console.warn("Failed to load folders:", e);
    }
  };

  const createFolder = async (name, parentPath) => {
    const created = await invoke("create_folder", {
      name,
      parent: parentPath || undefined,
    });
    await loadFolders();
    selectedFolder.value = created;
    if (parentPath) expandedPaths.add(parentPath);
    return created;
  };

  const selectFolder = (name) => {
    selectedFolder.value = name;
  };

  const toggleExpanded = (path) => {
    if (expandedPaths.has(path)) expandedPaths.delete(path);
    else expandedPaths.add(path);
  };

  // Pinned isn't a real storage location, so new notes created while viewing
  // it fall back to the default folder instead.
  const defaultNoteFolder = () =>
    selectedFolder.value === PINNED_FOLDER ? GENERAL_FOLDER : selectedFolder.value;

  return {
    folders,
    realFolders,
    folderTree,
    expandedPaths,
    selectedFolder,
    loadFolders,
    createFolder,
    selectFolder,
    toggleExpanded,
    defaultNoteFolder,
  };
}

<script setup>
import { computed } from "vue";
import { useFolders, PINNED_FOLDER, GENERAL_FOLDER } from "../composables/useFolders.js";
import FolderTreeItem from "./FolderTreeItem.vue";

const props = defineProps({
  open: { type: Boolean, default: false },
  selectedFolder: { type: String, default: "" },
});

defineEmits(["add-folder", "add-subfolder", "select-folder", "folder-context"]);

// useFolders() is a safe module-singleton (unlike useNotes()), so the tree
// structure/expand-state can be pulled directly rather than threaded through
// props from App.vue. `selectedFolder` stays a prop (see App.vue) since
// selecting a folder also needs to close the mobile drawer there.
const { folderTree, expandedPaths, toggleExpanded } = useFolders();

// Subfolders are only offered under real, user-created folders — not the
// virtual Pinned view, and not the default General folder.
const canAddSubfolder = computed(
  () => props.selectedFolder !== PINNED_FOLDER && props.selectedFolder !== GENERAL_FOLDER,
);
</script>

<template>
  <aside
    class="app-sidebar"
    :class="{ open }"
    aria-label="Folders"
    :aria-hidden="String(!open)"
  >
    <nav class="folder-list" aria-label="Folder list">
      <button
        class="folder-button"
        type="button"
        data-folder-id="Pinned"
        :aria-current="String(selectedFolder === PINNED_FOLDER)"
        @click="$emit('select-folder', PINNED_FOLDER)"
      >
        <span class="folder-icon folder-icon--pinned" aria-hidden="true"></span>
        Pinned
      </button>

      <FolderTreeItem
        v-for="node in folderTree"
        :key="node.path"
        :node="node"
        :selected-folder="selectedFolder"
        :expanded-paths="expandedPaths"
        @select-folder="$emit('select-folder', $event)"
        @toggle-expand="toggleExpanded"
        @context="$emit('folder-context', $event)"
      />
    </nav>
    <div class="sidebar-footer">
      <button
        class="add-folder-button"
        type="button"
        aria-label="Add folder"
        @click="$emit('add-folder')"
      >
        + Add Folder
      </button>
      <button
        v-if="canAddSubfolder"
        class="add-folder-button"
        type="button"
        aria-label="Add subfolder"
        @click="$emit('add-subfolder')"
      >
        + Add Subfolder
      </button>
    </div>
  </aside>
</template>

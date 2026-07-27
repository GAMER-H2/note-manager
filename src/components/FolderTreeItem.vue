<script setup>
import { computed } from "vue";

const props = defineProps({
  node: { type: Object, required: true },
  selectedFolder: { type: String, default: "" },
  expandedPaths: { type: Object, required: true },
});

defineEmits(["select-folder", "toggle-expand"]);

const isExpanded = computed(() => props.expandedPaths.has(props.node.path));
</script>

<template>
  <div class="folder-tree-item">
    <div class="folder-tree-row">
      <button
        v-if="node.children.length"
        type="button"
        class="folder-tree-toggle"
        :aria-expanded="String(isExpanded)"
        aria-label="Toggle subfolders"
        @click.stop="$emit('toggle-expand', node.path)"
      >
        <span class="folder-tree-chevron" :class="{ 'is-expanded': isExpanded }"></span>
      </button>
      <span v-else class="folder-tree-toggle-spacer" aria-hidden="true"></span>

      <button
        class="folder-button"
        type="button"
        :data-folder-id="node.path"
        :aria-current="String(node.path === selectedFolder)"
        @click="$emit('select-folder', node.path)"
      >
        <span class="folder-icon folder-icon--folder" aria-hidden="true"></span>
        {{ node.name }}
      </button>
    </div>

    <div v-if="isExpanded && node.children.length" class="folder-tree__children">
      <FolderTreeItem
        v-for="child in node.children"
        :key="child.path"
        :node="child"
        :selected-folder="selectedFolder"
        :expanded-paths="expandedPaths"
        @select-folder="$emit('select-folder', $event)"
        @toggle-expand="$emit('toggle-expand', $event)"
      />
    </div>
  </div>
</template>

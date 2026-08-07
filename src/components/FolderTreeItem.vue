<script setup>
import { computed } from "vue";
import { useContextMenuTrigger } from "../composables/useContextMenuTrigger.js";

const props = defineProps({
  node: { type: Object, required: true },
  selectedFolder: { type: String, default: "" },
  expandedPaths: { type: Object, required: true },
});

const emit = defineEmits(["select-folder", "toggle-expand", "context"]);

const isExpanded = computed(() => props.expandedPaths.has(props.node.path));

// App.vue builds the folder menu items; this just reports where and for which
// folder the menu was summoned (right-click on desktop, long-press on mobile).
const trigger = useContextMenuTrigger(({ x, y }) =>
  emit("context", { path: props.node.path, x, y }),
);
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
        @contextmenu="trigger.onContextMenu"
        @touchstart="trigger.onTouchStart"
        @touchmove="trigger.onTouchMove"
        @touchend="trigger.onTouchEnd"
        @touchcancel="trigger.onTouchCancel"
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
        @context="$emit('context', $event)"
      />
    </div>
  </div>
</template>

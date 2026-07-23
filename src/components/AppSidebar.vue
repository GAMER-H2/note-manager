<script setup>
defineProps({
  open: { type: Boolean, default: false },
});

defineEmits(["add-folder"]);
</script>

<template>
  <aside
    class="app-sidebar"
    :class="{ open }"
    aria-label="Folders"
    :aria-hidden="String(!open)"
  >
    <nav class="folder-list" aria-label="Folder list">
      <!-- Folder buttons will be populated dynamically in a future iteration. -->
      <button class="folder-button" type="button" data-folder-id="pinned">
        Pinned
      </button>
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
    </div>
  </aside>
</template>

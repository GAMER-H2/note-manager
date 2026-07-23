<script setup>
defineProps({
  sidebarOpen: { type: Boolean, default: false },
});

defineEmits(["toggle-sidebar", "open-settings"]);
</script>

<template>
  <header class="app-header" role="banner">
    <div class="header-left">
      <button
        class="hamburger-button"
        type="button"
        aria-label="Toggle sidebar"
        :aria-expanded="String(sidebarOpen)"
        @click="$emit('toggle-sidebar')"
      >
        <span class="bar"></span>
        <span class="bar"></span>
        <span class="bar"></span>
      </button>
      <h1 class="app-title">Note Manager</h1>
    </div>
    <button
      class="settings-button"
      type="button"
      aria-label="Open settings"
      @click="$emit('open-settings')"
    >
      Settings
    </button>
  </header>
</template>

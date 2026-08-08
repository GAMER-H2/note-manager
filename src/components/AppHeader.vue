<script setup>
import refreshIcon from "../assets/refresh.png";

defineProps({
  sidebarOpen: { type: Boolean, default: false },
  // Whether a sync is currently running (spins the icon) and whether a remote
  // is configured at all (otherwise there's nothing to sync).
  syncing: { type: Boolean, default: false },
  syncEnabled: { type: Boolean, default: false },
  // Full path of the folder being viewed (e.g. "Work/Projects", "Pinned"),
  // shown as a subheading under the app title.
  currentFolder: { type: String, default: "" },
});

defineEmits(["toggle-sidebar", "open-settings", "sync"]);
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
      <div class="app-title-block">
        <h1 class="app-title">Note Manager</h1>
        <span v-if="currentFolder" class="app-subtitle" :title="currentFolder">
          {{ currentFolder }}
        </span>
      </div>
    </div>
    <div class="header-right">
      <button
        class="sync-button"
        type="button"
        :class="{ 'is-syncing': syncing }"
        :disabled="!syncEnabled || syncing"
        :aria-busy="String(syncing)"
        :aria-label="syncing ? 'Syncing…' : 'Sync now'"
        :title="
          syncEnabled ? (syncing ? 'Syncing…' : 'Sync now') : 'Sync not set up'
        "
        @click="$emit('sync')"
      >
        <img :src="refreshIcon" class="sync-icon" alt="" aria-hidden="true" />
      </button>
      <button
        class="settings-button"
        type="button"
        aria-label="Open settings"
        @click="$emit('open-settings')"
      >
        Settings
      </button>
    </div>
  </header>
</template>

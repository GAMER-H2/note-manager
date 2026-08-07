<script setup>
import { computed, ref, watch, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useSettings, THEMES } from "../composables/useSettings.js";
import {
  URGENCY_LEVELS,
  ensureReminderChannel,
} from "../composables/useNotifications.js";
import { useReminders } from "../composables/useReminders.js";
import { useArchive } from "../composables/useArchive.js";
import { useSync } from "../composables/useSync.js";
import { AUTO_SYNC_INTERVALS } from "../composables/useAutoSync.js";
import { useVault } from "../composables/useVault.js";
import { isAndroid } from "../lib/platform.js";
import { useOverlayHistory } from "../composables/useOverlayHistory.js";

const props = defineProps({
  open: { type: Boolean, default: false },
});

// `imported` tells the app to reload notes/folders/pins after an import,
// since the vault changed underneath it.
const emit = defineEmits(["close", "imported"]);

const { settings, saveSettings } = useSettings();
const { reminders, rescheduleAllReminders, cancelAllReminders } = useReminders();

const reminderCount = computed(() => Object.keys(reminders).length);
const {
  busy: archiveBusy,
  status: archiveStatus,
  previewExport,
  exportVault,
  importVault,
} = useArchive();
const androidOnly = isAndroid();

const categories = [
  { id: "general", title: "General" },
  { id: "notifications", title: "Notifications" },
  { id: "appearance", title: "Appearance" },
  { id: "vault", title: "Vault" },
  { id: "sync", title: "Sync" },
  { id: "backup", title: "Import & export" },
];

const descriptions = {
  general: "Configure behavior and accessibility preferences.",
  notifications: "Control how reminder notifications behave.",
  appearance: "Choose how the app looks.",
  vault: "Where your notes live on this device.",
  sync: "Keep this device's notes in step with your others.",
  backup: "Move notes in and out as a zip archive.",
};

// Mobile shows one pane at a time — the category list, then the chosen page —
// rather than desktop's side-by-side layout. Matches the CSS breakpoint.
const settingsMql = window.matchMedia("(max-width: 720px)");
const isMobileSettings = ref(settingsMql.matches);
const mobileView = ref("list"); // "list" | "detail"

const onSettingsBreakpoint = (e) => {
  isMobileSettings.value = e.matches;
};

// On desktop both panes are always up; on mobile exactly one is.
const showCategories = computed(
  () => !isMobileSettings.value || mobileView.value === "list",
);
const showDetails = computed(
  () => !isMobileSettings.value || mobileView.value === "detail",
);

const openCategory = (id) => {
  activeCategory.value = id;
  mobileView.value = "detail";
};

// Drilling into a category gets its own history entry, so Android's back
// button returns to the category list instead of closing settings outright.
// Pushed on top of the modal's own entry, so back unwinds detail → list → closed.
const { requestClose: requestCloseDetail } = useOverlayHistory(
  () => isMobileSettings.value && mobileView.value === "detail",
  () => {
    mobileView.value = "list";
  },
);

const {
  config: syncConfig,
  lastSyncAt,
  syncing,
  status: syncStatus,
  loadSyncConfig,
  chooseFolder,
  testRemote,
  hasStoredPassword,
  saveSyncConfig,
  syncNow,
} = useSync();
const {
  vaultRoot,
  loadVaultRoot,
  chooseVaultRoot,
  resetVaultRoot,
  status: vaultStatus,
} = useVault();

const activeCategory = ref("general");
const exportPreview = ref(null);
const includeHistory = ref(true);
const importMode = ref("merge");
const syncKind = ref("folder");
const syncFolder = ref("");
const syncUrl = ref("");
const syncUsername = ref("");
const syncPassword = ref("");
const passwordOnFile = ref(false);

const lastSyncLabel = computed(() =>
  lastSyncAt.value
    ? new Date(lastSyncAt.value).toLocaleString(undefined, {
        day: "numeric",
        month: "short",
        hour: "2-digit",
        minute: "2-digit",
      })
    : "never",
);

const onChooseSyncFolder = async () => {
  const picked = await chooseFolder();
  if (picked) syncFolder.value = picked;
};

const candidateSync = () => ({
  kind: syncKind.value,
  path: syncFolder.value.trim(),
  url: syncUrl.value.trim(),
  username: syncUsername.value.trim(),
});

const syncReady = computed(() =>
  syncKind.value === "folder"
    ? !!syncFolder.value.trim()
    : !!syncUrl.value.trim(),
);

const onConnectSync = async () => {
  if (!syncReady.value) return;
  const candidate = candidateSync();
  // Only persist a remote we've proved we can reach and write to.
  if (await testRemote(candidate, syncPassword.value)) {
    await saveSyncConfig(candidate);
    // The password now lives in the keychain, so drop the copy in memory.
    syncPassword.value = "";
    passwordOnFile.value = await hasStoredPassword(candidate);
  }
};

const onDisconnectSync = async () => {
  await saveSyncConfig(null);
  syncFolder.value = "";
  syncUrl.value = "";
  syncUsername.value = "";
  syncPassword.value = "";
  passwordOnFile.value = false;
};

const onSyncNow = async () => {
  const report = await syncNow();
  if (report) emit("imported");
};

const onChooseVault = async () => {
  if (await chooseVaultRoot()) emit("imported");
};

const onResetVault = async () => {
  if (await resetVaultRoot()) emit("imported");
};

const onExport = () => exportVault({ includeHistory: includeHistory.value });

const onImport = async () => {
  const summary = await importVault({ replace: importMode.value === "replace" });
  if (summary) {
    emit("imported");
    exportPreview.value = await previewExport();
  }
};

// Draft state — only committed to the store on "Apply Changes".
const draftAutosave = ref(true);
const draftSyntaxHighlight = ref(true);
const draftUrgency = ref("default");
const draftNotificationsEnabled = ref(true);
const draftTheme = ref("dark");
const draftAutoSyncMinutes = ref(0);
const draftSyncBeforeClose = ref(false);
const draftBackgroundSyncAndroid = ref(false);
const draftTitledFilenames = ref(false);
// Vault-scoped, so it's read from the vault rather than the settings store —
// and can change under us when a sync brings another device's newer choice.
const savedTitledFilenames = ref(false);

const syncDraftFromSaved = () => {
  draftAutosave.value = settings.autosave;
  draftSyntaxHighlight.value = settings.syntaxHighlight !== false;
  draftUrgency.value = settings.urgency;
  draftNotificationsEnabled.value = settings.notificationsEnabled !== false;
  draftTheme.value = settings.theme;
  draftAutoSyncMinutes.value = settings.autoSyncMinutes;
  draftSyncBeforeClose.value = settings.syncBeforeClose === true;
  draftBackgroundSyncAndroid.value = settings.backgroundSyncAndroid === true;
};

const loadVaultSettings = async () => {
  try {
    savedTitledFilenames.value = await invoke("get_titled_filenames");
  } catch (e) {
    console.warn("Failed to read vault settings:", e);
    savedTitledFilenames.value = false;
  }
  draftTitledFilenames.value = savedTitledFilenames.value;
};

const closeModal = () => emit("close");
const { requestClose } = useOverlayHistory(
  () => props.open,
  closeModal,
);

// Unwinds the category page first when one is open, so the modal's own history
// entry is the one on top by the time we pop it. Closing straight from a detail
// page would otherwise strand its entry and eat the next back press.
const closeSettings = async () => {
  if (isMobileSettings.value && mobileView.value === "detail") {
    await requestCloseDetail();
  }
  await requestClose();
};

const applyChanges = async () => {
  const previousUrgency = settings.urgency;
  const notificationsToggled =
    settings.notificationsEnabled !== draftNotificationsEnabled.value;
  const filenameStyleChanged =
    savedTitledFilenames.value !== draftTitledFilenames.value;

  await saveSettings({
    autosave: draftAutosave.value,
    syntaxHighlight: draftSyntaxHighlight.value,
    urgency: draftUrgency.value,
    notificationsEnabled: draftNotificationsEnabled.value,
    theme: draftTheme.value,
    autoSyncMinutes: draftAutoSyncMinutes.value,
    syncBeforeClose: draftSyncBeforeClose.value,
    backgroundSyncAndroid: draftBackgroundSyncAndroid.value,
  });

  // Flipping the kill switch either cancels every scheduled notification or
  // reschedules them all (scheduling itself now respects the setting).
  if (notificationsToggled) {
    if (draftNotificationsEnabled.value) {
      await rescheduleAllReminders();
    } else {
      await cancelAllReminders();
    }
  } else if (androidOnly && previousUrgency !== draftUrgency.value) {
    await ensureReminderChannel(draftUrgency.value);
    await rescheduleAllReminders();
  }

  // Existing notes keep their old names until they're rewritten, so re-style
  // the whole vault once, after the setting is persisted for the backend to read.
  if (filenameStyleChanged) {
    try {
      // Persist to the vault first — the rename pass reads the setting back
      // from there — then bring existing notes into line with it.
      await invoke("set_titled_filenames", {
        enabled: draftTitledFilenames.value,
      });
      savedTitledFilenames.value = draftTitledFilenames.value;
      const renamed = await invoke("restyle_note_filenames");
      // Paths in the loaded note records are now stale.
      if (renamed) emit("imported");
    } catch (e) {
      console.error("Failed to change note filename style:", e);
    }
  }

  await closeSettings();
};

const onEsc = (e) => {
  if (e.key === "Escape" && props.open) closeSettings();
};

watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      // Always land on the category list when reopening on mobile, rather
      // than dropping the user back into whichever page they last visited.
      mobileView.value = "list";
      syncDraftFromSaved();
      loadVaultSettings();
      previewExport().then((p) => {
        exportPreview.value = p;
      });
      loadVaultRoot();
      loadSyncConfig().then(async () => {
        const cfg = syncConfig.value;
        syncKind.value = cfg?.kind ?? "folder";
        syncFolder.value = cfg?.path ?? "";
        syncUrl.value = cfg?.url ?? "";
        syncUsername.value = cfg?.username ?? "";
        syncPassword.value = "";
        passwordOnFile.value = cfg ? await hasStoredPassword(cfg) : false;
      });
      document.documentElement.classList.add("settings-open");
      document.body.classList.add("settings-open");
      window.addEventListener("keydown", onEsc);
    } else {
      document.documentElement.classList.remove("settings-open");
      document.body.classList.remove("settings-open");
      window.removeEventListener("keydown", onEsc);
    }
  },
);

onMounted(() => {
  if (typeof settingsMql.addEventListener === "function") {
    settingsMql.addEventListener("change", onSettingsBreakpoint);
  } else if (typeof settingsMql.addListener === "function") {
    settingsMql.addListener(onSettingsBreakpoint); // Safari fallback
  }
});

onBeforeUnmount(() => {
  document.documentElement.classList.remove("settings-open");
  document.body.classList.remove("settings-open");
  window.removeEventListener("keydown", onEsc);

  if (typeof settingsMql.removeEventListener === "function") {
    settingsMql.removeEventListener("change", onSettingsBreakpoint);
  } else if (typeof settingsMql.removeListener === "function") {
    settingsMql.removeListener(onSettingsBreakpoint);
  }
});
</script>

<template>
  <div class="settings-overlay" :hidden="!open" @click="closeSettings"></div>
  <section
    class="settings-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-title"
    :aria-hidden="String(!open)"
    @click.self="closeSettings"
  >
    <div class="settings-modal__content" role="document" tabindex="-1">
      <aside
        v-show="showCategories"
        class="settings-categories"
        aria-label="Settings categories"
      >
        <div class="settings-categories__header">
          <h2 id="settings-title">Settings</h2>
          <button
            v-if="isMobileSettings"
            type="button"
            class="settings-close-button"
            aria-label="Close settings"
            @click="closeSettings"
          >
            ×
          </button>
        </div>
        <nav role="tablist" aria-label="Settings categories list">
          <button
            v-for="cat in categories"
            :key="cat.id"
            type="button"
            class="settings-category"
            role="tab"
            :aria-selected="String(activeCategory === cat.id)"
            @click="openCategory(cat.id)"
          >
            {{ cat.title }}
          </button>
        </nav>
      </aside>

      <div v-show="showDetails" class="settings-details" role="tabpanel">
        <header class="settings-details__header">
          <button
            v-if="isMobileSettings"
            type="button"
            class="settings-back-button"
            aria-label="Back to settings categories"
            @click="requestCloseDetail"
          >
            ‹
          </button>
          <div>
            <h3>{{ categories.find((c) => c.id === activeCategory).title }}</h3>
            <p class="settings-description">{{ descriptions[activeCategory] }}</p>
          </div>
          <button
            v-if="!isMobileSettings"
            type="button"
            class="settings-close-button"
            aria-label="Close settings"
            @click="closeSettings"
          >
            ×
          </button>
        </header>

        <div class="settings-scroll">
          <!-- General -->
          <ul v-show="activeCategory === 'general'" class="settings-toggle-list">
            <li class="settings-toggle">
              <span>
                <strong>Auto-save notes</strong>
                <small>Save changes automatically while you type.</small>
              </span>
              <label class="switch">
                <input type="checkbox" v-model="draftAutosave" />
                <span class="slider"></span>
              </label>
            </li>
            <li class="settings-toggle">
              <span>
                <strong>Markdown syntax highlighting</strong>
                <small>
                  Style headings, lists, links and emphasis as you type. Turn
                  off to edit plain, unstyled text.
                </small>
              </span>
              <label class="switch">
                <input type="checkbox" v-model="draftSyntaxHighlight" />
                <span class="slider"></span>
              </label>
            </li>
          </ul>

          <!-- Notifications -->
          <ul
            v-show="activeCategory === 'notifications'"
            class="settings-toggle-list"
          >
            <li class="settings-status">
              {{ reminderCount }} reminder{{ reminderCount === 1 ? "" : "s" }}
              currently set up.
            </li>

            <li class="settings-toggle">
              <span>
                <strong>Reminder notifications</strong>
                <small>
                  Turn off to silence all reminders on this device. Your
                  reminders are kept and fire again when you turn this back on.
                </small>
              </span>
              <label class="switch">
                <input type="checkbox" v-model="draftNotificationsEnabled" />
                <span class="slider"></span>
              </label>
            </li>

            <li
              class="settings-toggle settings-toggle--stack"
              :class="{ 'is-disabled': !androidOnly || !draftNotificationsEnabled }"
            >
              <span>
                <strong>Notification urgency</strong>
                <small v-if="androidOnly">
                  How prominently reminders appear (sound, heads-up, etc.).
                </small>
                <small v-else>Android only — not configurable on desktop.</small>
              </span>
              <select
                class="settings-select"
                v-model="draftUrgency"
                :disabled="!androidOnly || !draftNotificationsEnabled"
              >
                <option
                  v-for="level in URGENCY_LEVELS"
                  :key="level.value"
                  :value="level.value"
                >
                  {{ level.label }}
                </option>
              </select>
            </li>
          </ul>

          <!-- Appearance -->
          <ul
            v-show="activeCategory === 'appearance'"
            class="settings-toggle-list"
          >
            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Theme</strong>
                <small>Choose the app's color theme.</small>
              </span>
              <select class="settings-select" v-model="draftTheme">
                <option
                  v-for="theme in THEMES"
                  :key="theme.value"
                  :value="theme.value"
                >
                  {{ theme.label }}
                </option>
              </select>
            </li>
          </ul>

          <!-- Vault -->
          <ul v-show="activeCategory === 'vault'" class="settings-toggle-list">
            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Vault folder</strong>
                <small class="settings-path">{{ vaultRoot || "Loading…" }}</small>
                <small>
                  Point this at a folder you can reach from other tools — an
                  rclone or Syncthing folder, a mounted share, a Dropbox or
                  Drive directory — and your notes are readable markdown files
                  in it.
                </small>
              </span>
              <div class="settings-button-row">
                <button type="button" class="settings-secondary" @click="onChooseVault">
                  Change…
                </button>
                <button type="button" class="settings-secondary" @click="onResetVault">
                  Reset
                </button>
              </div>
            </li>

            <li
              v-if="vaultStatus.message"
              class="settings-status"
              :class="{ 'is-error': vaultStatus.kind === 'error' }"
            >
              {{ vaultStatus.message }}
            </li>

            <li class="settings-toggle">
              <span>
                <strong>Put note titles in filenames</strong>
                <small>
                  Names files <code>Project Kickoff--a1b2c3.md</code> instead of
                  <code>a1b2c3.md</code>, on this device and on your sync
                  server. The trailing id is what sync and version history
                  identify a note by, so it stays. Applies to notes you already
                  have, and filenames follow the title as you edit it.
                  <br />
                  This one belongs to the vault, not the device — your other
                  devices adopt it on their next sync.
                </small>
              </span>
              <label class="switch">
                <input type="checkbox" v-model="draftTitledFilenames" />
                <span class="slider"></span>
              </label>
            </li>
          </ul>

          <!-- Sync -->
          <ul v-show="activeCategory === 'sync'" class="settings-toggle-list">
            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Sync method</strong>
                <small v-if="syncKind === 'folder'">
                  A directory both this device and your others can see. On a
                  Linux server or in a container, bind-mount it; for Google
                  Drive, use an rclone mount.
                </small>
                <small v-else>
                  Any WebDAV server — dufs or <code>rclone serve webdav</code> on
                  a bare Linux box, a one-line docker container, or Nextcloud.
                  rclone can also bridge Google Drive to WebDAV.
                </small>
              </span>
              <select class="settings-select" v-model="syncKind">
                <option value="folder">Shared folder</option>
                <option value="webdav">WebDAV server</option>
              </select>
            </li>

            <li v-if="syncKind === 'folder'" class="settings-toggle settings-toggle--stack">
              <span><strong>Sync folder</strong></span>
              <div class="settings-field-row">
                <input
                  v-model="syncFolder"
                  class="settings-input"
                  type="text"
                  placeholder="/mnt/notes-sync"
                  aria-label="Sync folder path"
                />
                <button
                  type="button"
                  class="settings-secondary"
                  @click="onChooseSyncFolder"
                >
                  Browse…
                </button>
              </div>
            </li>

            <template v-else>
              <li class="settings-toggle settings-toggle--stack">
                <span><strong>Server URL</strong></span>
                <input
                  v-model="syncUrl"
                  class="settings-input"
                  type="url"
                  placeholder="https://example.com/dav/notes"
                  aria-label="WebDAV URL"
                />
              </li>
              <li class="settings-toggle settings-toggle--stack">
                <span><strong>Username</strong></span>
                <input
                  v-model="syncUsername"
                  class="settings-input"
                  type="text"
                  autocomplete="username"
                  aria-label="WebDAV username"
                />
              </li>
              <li class="settings-toggle settings-toggle--stack">
                <span>
                  <strong>Password</strong>
                  <small v-if="passwordOnFile">
                    A password is saved in your system keychain. Leave blank to
                    keep it.
                  </small>
                  <small v-else>
                    Stored in your system keychain, never in the app's config
                    files.
                  </small>
                </span>
                <input
                  v-model="syncPassword"
                  class="settings-input"
                  type="password"
                  autocomplete="current-password"
                  :placeholder="passwordOnFile ? '••••••••' : ''"
                  aria-label="WebDAV password"
                />
              </li>
            </template>

            <li class="settings-toggle">
              <span>
                <strong>Automatic sync</strong>
                <small>
                  Syncs on a timer while the app is open, and when you switch
                  back to it. Paused while a note is open, so nothing is pulled
                  out from under an edit.
                </small>
              </span>
              <select class="settings-select" v-model="draftAutoSyncMinutes">
                <option
                  v-for="option in AUTO_SYNC_INTERVALS"
                  :key="option.value"
                  :value="option.value"
                >
                  {{ option.label }}
                </option>
              </select>
            </li>

            <li class="settings-toggle" :class="{ 'is-disabled': androidOnly }">
              <span>
                <strong>Finish syncing before closing</strong>
                <small v-if="androidOnly">
                  Desktop only — Android can dismiss the app instantly, so it
                  can't hold the window open to finish a sync.
                </small>
                <small v-else>
                  If you close the app mid-sync, keep it open and show a prompt
                  until the sync finishes, instead of quitting right away.
                </small>
              </span>
              <label class="switch">
                <input
                  type="checkbox"
                  v-model="draftSyncBeforeClose"
                  :disabled="androidOnly"
                />
                <span class="slider"></span>
              </label>
            </li>

            <li class="settings-toggle" :class="{ 'is-disabled': !androidOnly }">
              <span>
                <strong>Background sync (Android)</strong>
                <small v-if="androidOnly">
                  Runs each sync in a short-lived foreground service with a
                  silent notification, so a sync can finish even if you swipe
                  the app away mid-sync. Nothing runs while idle.
                </small>
                <small v-else>
                  Android only — desktop keeps running syncs in the app itself.
                </small>
              </span>
              <label class="switch">
                <input
                  type="checkbox"
                  v-model="draftBackgroundSyncAndroid"
                  :disabled="!androidOnly"
                />
                <span class="slider"></span>
              </label>
            </li>

            <li class="settings-toggle settings-toggle--stack">
              <span>
                <small v-if="syncConfig">
                  Connected to
                  <code>{{ syncConfig.url || syncConfig.path }}</code>. Last
                  synced {{ lastSyncLabel }}.
                </small>
                <small v-else>
                  Not connected. Conflicting edits are merged automatically
                  where possible, and kept as separate copies when not.
                </small>
              </span>
              <div class="settings-button-row">
                <button
                  type="button"
                  class="settings-primary"
                  :disabled="syncing || !syncReady"
                  @click="onConnectSync"
                >
                  {{ syncConfig ? "Update" : "Connect" }}
                </button>
                <button
                  v-if="syncConfig"
                  type="button"
                  class="settings-secondary"
                  :disabled="syncing"
                  @click="onSyncNow"
                >
                  {{ syncing ? "Syncing…" : "Sync now" }}
                </button>
                <button
                  v-if="syncConfig"
                  type="button"
                  class="settings-secondary"
                  :disabled="syncing"
                  @click="onDisconnectSync"
                >
                  Disconnect
                </button>
              </div>
            </li>

            <li
              v-if="syncStatus.message"
              class="settings-status"
              :class="{ 'is-error': syncStatus.kind === 'error' }"
            >
              {{ syncStatus.message }}
            </li>
          </ul>

          <!-- Import & export -->
          <ul v-show="activeCategory === 'backup'" class="settings-toggle-list">
            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Export notes to a zip file</strong>
                <small v-if="exportPreview">
                  {{ exportPreview.notes }} note{{
                    exportPreview.notes === 1 ? "" : "s"
                  }}. Files are named by title, so the archive opens cleanly in
                  any markdown editor.
                </small>
                <small v-else>
                  Files are named by title, so the archive opens cleanly in any
                  markdown editor.
                </small>
              </span>
              <button
                type="button"
                class="settings-primary"
                :disabled="archiveBusy"
                @click="onExport"
              >
                Export…
              </button>
            </li>

            <li class="settings-toggle">
              <span>
                <strong>Include version history</strong>
                <small>
                  Adds every stored revision. Makes the archive bigger, but the
                  export becomes a complete backup.
                </small>
              </span>
              <label class="switch">
                <input type="checkbox" v-model="includeHistory" />
                <span class="slider"></span>
              </label>
            </li>

            <li class="settings-toggle settings-toggle--stack">
              <span>
                <strong>Import notes from a zip file</strong>
                <small>
                  Folders in the archive become folders in your vault.
                </small>
              </span>
              <select class="settings-select" v-model="importMode">
                <option value="merge">Merge with existing notes</option>
                <option value="replace">Replace all notes</option>
              </select>
            </li>

            <li class="settings-toggle settings-toggle--stack">
              <span>
                <small v-if="importMode === 'replace'" class="settings-warning">
                  Replace deletes every note in your vault first. Version
                  history is kept.
                </small>
                <small v-else>
                  Merge keeps your existing notes and adds the archive's on top.
                </small>
              </span>
              <button
                type="button"
                class="settings-secondary"
                :disabled="archiveBusy"
                @click="onImport"
              >
                Import…
              </button>
            </li>

            <li v-if="archiveStatus" class="settings-status">
              {{ archiveStatus }}
            </li>
          </ul>
        </div>
      </div>

      <!-- Outside the details pane so Apply/Cancel stay reachable on mobile,
           where the details pane is hidden behind the category list. -->
      <footer class="settings-modal__footer">
        <button type="button" class="settings-secondary" @click="closeSettings">
          Cancel
        </button>
        <button type="button" class="settings-primary" @click="applyChanges">
          Apply Changes
        </button>
      </footer>
    </div>
  </section>
</template>

import { reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// Shared, module-level sync state so the settings panel and any future status
// indicator agree on whether a sync is in flight.
const config = ref(null);
const lastSyncAt = ref(0);
const syncing = ref(false);
const status = reactive({ message: "", kind: "" }); // kind: "" | "ok" | "error"

const describeReport = (r) => {
  const parts = [];
  if (r.pushed) parts.push(`${r.pushed} sent`);
  if (r.pulled) parts.push(`${r.pulled} received`);
  if (r.merged) parts.push(`${r.merged} merged`);
  if (r.deletedLocal) parts.push(`${r.deletedLocal} deleted here`);
  if (r.deletedRemote) parts.push(`${r.deletedRemote} deleted there`);
  if (r.conflicts) {
    parts.push(
      `${r.conflicts} conflict${r.conflicts === 1 ? "" : "s"} kept as separate copies`,
    );
  }
  if (!parts.length) return "Already up to date.";
  return `${parts.join(" · ")}.`;
};

export function useSync() {
  const loadSyncConfig = async () => {
    try {
      config.value = await invoke("get_sync_config");
      lastSyncAt.value = await invoke("get_last_sync");
    } catch (e) {
      console.warn("Failed to load sync config:", e);
      config.value = null;
    }
  };

  const chooseFolder = async () => {
    const path = await open({
      title: "Choose a sync folder",
      directory: true,
      multiple: false,
    });
    return typeof path === "string" ? path : null;
  };

  // Probes the remote before saving, so a typo'd path fails here rather than
  // silently doing nothing on every later sync.
  const testRemote = async (candidate) => {
    status.message = "Testing…";
    status.kind = "";
    try {
      const msg = await invoke("test_sync_remote", { cfg: candidate });
      status.message = msg;
      status.kind = "ok";
      return true;
    } catch (e) {
      status.message = `${e}`;
      status.kind = "error";
      return false;
    }
  };

  const saveSyncConfig = async (candidate) => {
    try {
      await invoke("set_sync_config", { sync: candidate });
      config.value = candidate;
      status.message = candidate ? "Sync folder saved." : "Sync disabled.";
      status.kind = "ok";
      return true;
    } catch (e) {
      status.message = `${e}`;
      status.kind = "error";
      return false;
    }
  };

  const syncNow = async () => {
    if (syncing.value) return null;
    syncing.value = true;
    status.message = "Syncing…";
    status.kind = "";

    try {
      const report = await invoke("sync_now");
      lastSyncAt.value = await invoke("get_last_sync");
      status.message = describeReport(report);
      status.kind = report.errors?.length ? "error" : "ok";
      if (report.errors?.length) {
        status.message += ` ${report.errors.length} problem(s): ${report.errors[0]}`;
      }
      return report;
    } catch (e) {
      status.message = `Sync failed: ${e}`;
      status.kind = "error";
      return null;
    } finally {
      syncing.value = false;
    }
  };

  return {
    config,
    lastSyncAt,
    syncing,
    status,
    loadSyncConfig,
    chooseFolder,
    testRemote,
    saveSyncConfig,
    syncNow,
  };
}

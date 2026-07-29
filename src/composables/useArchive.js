import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save, open, confirm } from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";

// Zip import/export of the whole vault.
//
// The Rust side builds and parses the archive in memory and hands the bytes
// across; the file itself is written/read here through the dialog + fs plugins.
// That split is deliberate: on Android the picker returns a content:// URI that
// Rust's std::fs can't open, but the fs plugin can.
const defaultFileName = () => {
  const now = new Date();
  const stamp = [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, "0"),
    String(now.getDate()).padStart(2, "0"),
  ].join("-");
  return `notes-${stamp}.zip`;
};

export function useArchive() {
  const busy = ref(false);
  const status = ref("");

  const previewExport = async () => {
    try {
      return await invoke("export_preview");
    } catch (e) {
      console.warn("Failed to read export preview:", e);
      return null;
    }
  };

  const exportVault = async ({ includeHistory = true } = {}) => {
    if (busy.value) return null;
    busy.value = true;
    status.value = "Preparing export…";

    try {
      const path = await save({
        title: "Export notes",
        defaultPath: defaultFileName(),
        filters: [{ name: "Zip archive", extensions: ["zip"] }],
      });
      if (!path) {
        status.value = "";
        return null;
      }

      status.value = "Writing archive…";
      const bytes = await invoke("export_vault", { includeHistory });
      await writeFile(path, new Uint8Array(bytes));

      status.value = "Export complete.";
      return path;
    } catch (e) {
      console.error("Export failed:", e);
      status.value = `Export failed: ${e}`;
      return null;
    } finally {
      busy.value = false;
    }
  };

  const importVault = async ({ replace = false } = {}) => {
    if (busy.value) return null;

    // Replace deletes every note in the vault, so make the user say so out
    // loud rather than relying on them having read the radio button.
    if (replace) {
      const ok = await confirm(
        "Replace will delete all notes currently in your vault before importing. Version history is kept. Continue?",
        { title: "Replace all notes?", kind: "warning" },
      );
      if (!ok) return null;
    }

    busy.value = true;
    status.value = "Choosing file…";

    try {
      const path = await open({
        title: "Import notes",
        multiple: false,
        directory: false,
        filters: [{ name: "Zip archive", extensions: ["zip"] }],
      });
      if (!path) {
        status.value = "";
        return null;
      }

      status.value = "Reading archive…";
      const data = await readFile(path);

      status.value = "Importing…";
      const summary = await invoke("import_vault", {
        data: Array.from(data),
        replace,
      });

      const parts = [`Imported ${summary.imported} note${summary.imported === 1 ? "" : "s"}`];
      if (summary.skipped) parts.push(`${summary.skipped} skipped`);
      if (summary.restoredHistory) parts.push(`${summary.restoredHistory} with history`);
      if (summary.withoutManifest) {
        parts.push("no manifest — pins and reminders weren't included");
      }
      status.value = `${parts.join(" · ")}.`;

      return summary;
    } catch (e) {
      console.error("Import failed:", e);
      status.value = `Import failed: ${e}`;
      return null;
    } finally {
      busy.value = false;
    }
  };

  return { busy, status, previewExport, exportVault, importVault };
}

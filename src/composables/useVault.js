import { reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// Where the notes actually live on disk. Moving the vault to a user-visible
// directory is what makes every "just point a sync tool at the folder"
// workflow possible, so this is the foundation the sync options sit on.
const vaultRoot = ref("");
const status = reactive({ message: "", kind: "" });

export function useVault() {
  const loadVaultRoot = async () => {
    try {
      vaultRoot.value = await invoke("get_vault_root");
    } catch (e) {
      console.warn("Failed to read vault root:", e);
    }
  };

  const applyChange = (change) => {
    vaultRoot.value = change.path;
    if (change.migrated) {
      status.message = `Notes moved to ${change.path}.`;
    } else if (change.adopted) {
      // Two vaults were never merged, because doing that blindly duplicates or
      // clobbers notes. The import flow is the supported way to combine them.
      status.message = `That folder already holds notes, so it was opened as-is. Your previous notes are still where they were.`;
    } else {
      status.message = `Vault is ${change.path}.`;
    }
    status.kind = "ok";
  };

  const chooseVaultRoot = async () => {
    const picked = await open({
      title: "Choose where notes are stored",
      directory: true,
      multiple: false,
    });
    if (typeof picked !== "string") return false;

    try {
      applyChange(await invoke("set_vault_root", { path: picked }));
      return true;
    } catch (e) {
      status.message = `${e}`;
      status.kind = "error";
      return false;
    }
  };

  const resetVaultRoot = async () => {
    try {
      applyChange(await invoke("reset_vault_root"));
      return true;
    } catch (e) {
      status.message = `${e}`;
      status.kind = "error";
      return false;
    }
  };

  return { vaultRoot, status, loadVaultRoot, chooseVaultRoot, resetVaultRoot };
}

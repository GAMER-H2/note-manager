import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

// Theme options exposed to the Appearance settings panel.
export const THEMES = [
  { value: "dark", label: "Default Dark" },
  { value: "light", label: "Blinding White" },
];

const DEFAULTS = {
  autosave: true, // General
  urgency: "default", // Notifications (Android)
  theme: "dark", // Appearance
};

// Shared, module-level settings store.
const settings = reactive({ ...DEFAULTS });
let loaded = false;

const applyTheme = () => {
  const theme = settings.theme === "light" ? "light" : "dark";
  document.documentElement.setAttribute("data-theme", theme);
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", theme === "light" ? "#f6f7fb" : "#0f1115");
};

export function useSettings() {
  const loadSettings = async () => {
    if (loaded) {
      applyTheme();
      return;
    }
    try {
      const raw = await invoke("get_settings");
      const parsed = JSON.parse(raw || "{}");
      Object.assign(settings, { ...DEFAULTS, ...parsed });
    } catch (e) {
      console.warn("Failed to load settings:", e);
    } finally {
      loaded = true;
      applyTheme();
    }
  };

  // Commit a full/partial set of settings, apply side effects, and persist.
  const saveSettings = async (next) => {
    Object.assign(settings, next);
    applyTheme();
    try {
      await invoke("set_settings", { data: JSON.stringify(settings) });
    } catch (e) {
      console.error("Failed to persist settings:", e);
    }
  };

  return { settings, loadSettings, saveSettings, applyTheme };
}

/**
 * Tauri 2 notification handling without ESM imports.
 * Uses the global window.__TAURI__.notification API if available.
 * Adds defensive logging to the on-page "Notification log" list.
 */

(function () {
  function log(message) {
    const ul = document.getElementById("log");
    const line = `[${new Date().toLocaleTimeString()}] ${message}`;
    if (!ul) {
      // Fallback to console if the UI log is missing
      console.log(line);
      return;
    }
    const li = document.createElement("li");
    li.textContent = line;
    ul.prepend(li);
  }

  function getNotificationApi() {
    const api = window.__TAURI__?.notification;
    if (!api) {
      log(
        "Tauri notification API not found. Ensure the notification plugin is initialized and global APIs are enabled.",
      );
    }
    return api;
  }

  async function isPermissionGrantedSafe(api) {
    try {
      if (!api?.isPermissionGranted) return false;
      return await api.isPermissionGranted();
    } catch (e) {
      log(`Error checking permission: ${String(e)}`);
      return false;
    }
  }

  async function requestPermissionSafe(api) {
    try {
      if (!api?.requestPermission) return "denied";
      const res = await api.requestPermission();
      return res;
    } catch (e) {
      log(`Error requesting permission: ${String(e)}`);
      return "denied";
    }
  }

  async function ensurePermission(api) {
    let granted = await isPermissionGrantedSafe(api);
    if (!granted) {
      const permission = await requestPermissionSafe(api);
      granted = permission === "granted";
      if (!granted && permission === "denied") {
        // This may indicate the system is not showing a prompt anymore
        // Suggest opening app settings for notifications
        log("Permission denied. Open app settings to enable notifications.");
      }
    }
    return granted;
  }

  async function sendNotificationSafe(api, options) {
    if (!api?.sendNotification) {
      throw new Error(
        "sendNotification is not available on the notification API.",
      );
    }
    return api.sendNotification(options);
  }

  async function handleNotifyClick() {
    const api = getNotificationApi();
    if (!api) {
      log("Notification API unavailable. Cannot send notifications.");
      return;
    }

    const titleEl = document.getElementById("title");
    const titleText = titleEl?.textContent?.trim() || "Notification";

    const granted = await ensurePermission(api);
    if (!granted) {
      log("Notification permission not granted.");
      return;
    }

    try {
      await sendNotificationSafe(api, {
        id: `note-${Date.now()}`,
        title: `${titleText} #${Math.floor(Date.now() / 1000)}`,
        body: "Test notification from Tauri.",
        // Use the high-importance channel created at startup
        category: "note-manager-high",
        sound: true,
      });
      log("Notification sent successfully.");
    } catch (err) {
      log(`Failed to send notification: ${String(err)}`);
    }
  }

  async function init() {
    const btn = document.getElementById("notify");
    if (!btn) {
      log("Notify button not found in DOM.");
      return;
    }

    btn.addEventListener("click", handleNotifyClick);
    log("Ready. Click the button to send a notification.");

    const api = getNotificationApi();
    if (!api) return;

    // Create a high-importance Android notification channel if supported by the plugin.
    // Some implementations expose `createChannel`. If not available, instruct the user via logs.
    try {
      if (api.createChannel) {
        await api.createChannel({
          id: "note-manager-high",
          name: "High Priority",
          description: "High importance notifications for Note Manager",
          // importance can be: "min" | "low" | "default" | "high" | "max"
          importance: "high",
          sound: true,
          vibration: true,
        });
        log("High-importance notification channel ensured: note-manager-high");
      } else {
        log(
          "Notification channel API not available. Set channel importance via system settings after the first notification.",
        );
      }
    } catch (e) {
      log(`Failed to create notification channel: ${String(e)}`);
    }

    // Pre-check permission to inform the user; some platforms may require a user gesture.
    const granted = await isPermissionGrantedSafe(api);
    log(
      granted
        ? "Notification permission granted."
        : "Notification permission pending or denied.",
    );
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    // DOM already loaded
    init();
  }
})();

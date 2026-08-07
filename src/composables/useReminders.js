import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  sendNotification,
  cancel,
  isPermissionGranted,
  requestPermission,
  Schedule,
} from "@tauri-apps/plugin-notification";
import { notificationTitle, notificationBody } from "../lib/notes.js";
import { ensureReminderChannel } from "./useNotifications.js";
import { useSettings } from "./useSettings.js";

// Repeat options exposed to the UI. `none` = one-time; the rest map to a
// calendar-based schedule derived from the chosen date/time.
export const REPEAT_OPTIONS = [
  { value: "none", label: "Does not repeat" },
  { value: "hourly", label: "Hourly" },
  { value: "daily", label: "Daily" },
  { value: "weekly", label: "Weekly" },
  { value: "monthly", label: "Monthly" },
  { value: "yearly", label: "Yearly" },
];

// How much of the note is revealed in the notification itself.
export const BODY_MODES = [
  { value: "full", label: "Title and body" },
  { value: "titleOnly", label: "Title only" },
  { value: "generic", label: 'Generic ("1 new reminder")' },
];

// Shared, module-level store:
// noteId -> { enabled, notificationId, at, repeat, urgency, bodyMode }.
// `at` is a datetime-local string ("YYYY-MM-DDTHH:mm") in the user's local time.
const reminders = reactive({});
let loaded = false;

// Stable positive 31-bit id derived from the (string) note id. The plugin
// requires a 32-bit integer id, and keeping it stable lets us cancel/replace.
const hashId = (str) => {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h & 0x7fffffff;
};

// Convert a reminder config into a plugin Schedule.
const buildSchedule = ({ at, repeat }) => {
  const date = new Date(at);
  if (repeat === "none") {
    return Schedule.at(date, false, true);
  }

  const hour = date.getHours();
  const minute = date.getMinutes();
  const interval = {};

  switch (repeat) {
    case "hourly":
      interval.minute = minute;
      break;
    case "daily":
      interval.hour = hour;
      interval.minute = minute;
      break;
    case "weekly":
      interval.weekday = date.getDay() + 1; // plugin: 1=Sunday … 7=Saturday
      interval.hour = hour;
      interval.minute = minute;
      break;
    case "monthly":
      interval.day = date.getDate();
      interval.hour = hour;
      interval.minute = minute;
      break;
    case "yearly":
      interval.month = date.getMonth() + 1;
      interval.day = date.getDate();
      interval.hour = hour;
      interval.minute = minute;
      break;
    default:
      break;
  }

  return Schedule.interval(interval, true);
};

const persist = async () => {
  try {
    await invoke("set_reminders", { data: JSON.stringify(reminders) });
  } catch (e) {
    console.error("Failed to persist reminders:", e);
  }
};

const getPermission = async ({ prompt } = { prompt: false }) => {
  let granted = false;
  try {
    granted = await isPermissionGranted();
  } catch (e) {
    console.warn("isPermissionGranted unavailable:", e);
  }

  if (!granted && prompt) {
    try {
      granted = (await requestPermission()) === "granted";
    } catch (e) {
      console.warn("requestPermission unavailable:", e);
    }
  }

  return granted;
};

const scheduleReminderNotification = async (
  note,
  cfg,
  { promptForPermission = false, persistResult = true } = {},
) => {
  if (!note?.id) return null;

  // Falls back to the global default urgency for reminders saved before
  // per-reminder urgency existed.
  const { settings } = useSettings();
  const existing = reminders[note.id];
  const urgency = cfg.urgency ?? existing?.urgency ?? settings.urgency;
  const bodyMode = cfg.bodyMode ?? existing?.bodyMode ?? "full";
  const id = cfg.notificationId ?? existing?.notificationId ?? hashId(note.id);

  const nextCfg = {
    enabled: true,
    notificationId: id,
    at: cfg.at,
    repeat: cfg.repeat,
    urgency,
    bodyMode,
  };

  // Notifications turned off on this device: remember the reminder config (so it
  // fires again once re-enabled) but schedule nothing, and clear anything that
  // was already scheduled. This runs before the permission prompt on purpose —
  // a disabled kill switch shouldn't ask for notification permission.
  if (settings.notificationsEnabled === false) {
    try {
      await cancel([id]);
    } catch (e) {
      console.warn("cancel while notifications off failed (may be none):", e);
    }
    reminders[note.id] = nextCfg;
    if (persistResult) {
      await persist();
    }
    return nextCfg;
  }

  const granted = await getPermission({ prompt: promptForPermission });
  if (!granted) {
    if (promptForPermission) {
      throw new Error("Notification permission was not granted");
    }
    return null;
  }

  // Make sure the channel for the chosen urgency exists so Android doesn't
  // silently drop the notification.
  const channelId = await ensureReminderChannel(urgency);

  // Clear any previously scheduled notification for this note.
  try {
    await cancel([id]);
  } catch (e) {
    console.warn("cancel before reschedule failed (may be none):", e);
  }

  let title;
  let body;
  switch (bodyMode) {
    case "generic":
      title = "1 new reminder";
      body = undefined;
      break;
    case "titleOnly":
      title = notificationTitle(note.content);
      body = undefined;
      break;
    case "full":
    default:
      title = notificationTitle(note.content);
      body = notificationBody(note.content) || undefined;
      break;
  }

  await sendNotification({
    id,
    title,
    body,
    largeBody: body,
    channelId,
    schedule: buildSchedule(nextCfg),
    extra: { noteId: note.id },
  });

  reminders[note.id] = nextCfg;
  if (persistResult) {
    await persist();
  }
  return nextCfg;
};

export function useReminders() {
  const loadReminders = async () => {
    if (loaded) return;
    try {
      const raw = await invoke("get_reminders");
      const parsed = JSON.parse(raw || "{}");
      Object.assign(reminders, parsed);
    } catch (e) {
      console.warn("Failed to load reminders:", e);
    } finally {
      loaded = true;
    }
  };

  // Re-reads the store, bypassing the load-once guard, after an import or sync
  // pull has rewritten it. Callers should follow with `rescheduleAllReminders`,
  // since anything newly arrived exists only on disk — it has no notification
  // scheduled on this device yet.
  const reloadReminders = async () => {
    try {
      const raw = await invoke("get_reminders");
      const parsed = JSON.parse(raw || "{}");
      Object.keys(reminders).forEach((key) => delete reminders[key]);
      Object.assign(reminders, parsed);
    } catch (e) {
      console.warn("Failed to reload reminders:", e);
    } finally {
      loaded = true;
    }
  };

  const getReminder = (noteId) => (noteId ? reminders[noteId] ?? null : null);

  // Schedule (or reschedule) a reminder for a note. `note` must carry the
  // latest content so the notification text is current.
  const saveReminder = async (note, { at, repeat, urgency, bodyMode }) =>
    scheduleReminderNotification(
      note,
      {
        notificationId: reminders[note.id]?.notificationId,
        at,
        repeat,
        urgency,
        bodyMode,
      },
      { promptForPermission: true },
    );

  // Rebuild an existing reminder using the note's latest content. Used after
  // edits are saved and when Android notification urgency changes.
  const refreshReminder = async (note, { persistResult = false } = {}) => {
    const existing = note?.id ? reminders[note.id] : null;
    if (!existing) return null;
    try {
      return await scheduleReminderNotification(note, existing, {
        promptForPermission: false,
        persistResult,
      });
    } catch (e) {
      console.warn("Failed to refresh scheduled reminder:", e);
      return null;
    }
  };

  const rescheduleAllReminders = async () => {
    const noteIds = Object.keys(reminders);
    if (noteIds.length === 0) return 0;

    const granted = await getPermission({ prompt: false });
    if (!granted) return 0;

    let rescheduled = 0;

    try {
      const notes = await invoke("list_notes");
      const notesById = new Map(
        Array.isArray(notes) ? notes.map((note) => [note.id, note]) : [],
      );

      for (const noteId of noteIds) {
        const note = notesById.get(noteId);
        if (!note) continue;

        const cfg = reminders[noteId];
        if (!cfg) continue;

        try {
          await scheduleReminderNotification(note, cfg, {
            promptForPermission: false,
            persistResult: false,
          });
          rescheduled += 1;
        } catch (e) {
          console.warn(`Failed to reschedule reminder for ${noteId}:`, e);
        }
      }

      if (rescheduled > 0) {
        await persist();
      }
    } catch (e) {
      console.error("Failed to reload notes while rescheduling reminders:", e);
    }

    return rescheduled;
  };

  const removeReminder = async (noteId) => {
    const existing = reminders[noteId];
    if (existing?.notificationId != null) {
      try {
        await cancel([existing.notificationId]);
      } catch (e) {
        console.warn("cancel on remove failed:", e);
      }
    }
    delete reminders[noteId];
    await persist();
  };

  // Cancels every scheduled notification without touching the stored configs,
  // so re-enabling notifications can reschedule them all. Used by the
  // "turn off all notifications" settings toggle.
  const cancelAllReminders = async () => {
    const ids = Object.values(reminders)
      .map((cfg) => cfg?.notificationId)
      .filter((id) => id != null);
    if (ids.length === 0) return;
    try {
      await cancel(ids);
    } catch (e) {
      console.warn("cancel all reminders failed:", e);
    }
  };

  return {
    reminders,
    loadReminders,
    reloadReminders,
    getReminder,
    saveReminder,
    refreshReminder,
    rescheduleAllReminders,
    removeReminder,
    cancelAllReminders,
    REPEAT_OPTIONS,
    BODY_MODES,
  };
}

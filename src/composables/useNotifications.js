import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
  registerActionTypes,
  createChannel,
  Importance,
  Visibility,
} from "@tauri-apps/plugin-notification";

// Some of these APIs only exist on Android (action types / channels). On
// desktop they are no-ops or missing, so we call them defensively and never
// let a failure bubble up and block app startup.
const callSafely = async (fn, ...args) => {
  try {
    if (typeof fn === "function") return await fn(...args);
  } catch (err) {
    console.warn("Notification optional call failed:", err);
  }
  return undefined;
};

// Notification urgency levels shown in Settings → Notifications. Android maps
// urgency to a channel Importance, and a channel's importance can't be changed
// after creation — so each level gets its own channel id.
export const URGENCY_LEVELS = [
  { value: "min", label: "Minimal", importance: Importance.Min },
  { value: "low", label: "Low", importance: Importance.Low },
  { value: "default", label: "Default", importance: Importance.Default },
  { value: "high", label: "High", importance: Importance.High },
];

export const channelIdFor = (urgency) => `reminders_${urgency}`;

const levelFor = (urgency) =>
  URGENCY_LEVELS.find((l) => l.value === urgency) ?? URGENCY_LEVELS[2];

// Ensure the channel for a given urgency exists. Idempotent on Android; a no-op
// on desktop. Returns the channel id to schedule against.
export async function ensureReminderChannel(urgency = "default") {
  const level = levelFor(urgency);
  const id = channelIdFor(level.value);
  await callSafely(createChannel, {
    id,
    name: `Reminders (${level.label})`,
    description: "Notifications for note reminders",
    importance: level.importance,
    visibility: Visibility?.Public ?? undefined,
    lights: true,
    lightColor: "#ff0000",
    vibration: true,
  });
  return id;
}

// Set up notification channels/permissions. Intentionally decoupled from UI
// wiring: this runs in the background so a pending permission dialog can never
// stall the rest of the app (which was a real bug in the old vanilla build).
export async function initNotifications(urgency = "default") {
  // Android-only: register the action buttons shown on a notification.
  await callSafely(registerActionTypes, [
    {
      id: "options",
      actions: [
        {
          id: "mark-complete",
          title: "Mark as Complete",
          foreground: false,
        },
      ],
    },
  ]);

  // Android-only: create the notification channel reminders are posted to.
  await ensureReminderChannel(urgency);

  let granted = false;
  try {
    granted = await isPermissionGranted();
  } catch (e) {
    console.warn("isPermissionGranted not available:", e);
  }

  if (!granted) {
    try {
      granted = (await requestPermission()) === "granted";
    } catch (e) {
      console.warn("requestPermission not available:", e);
    }
  }

  return granted;
}

// Kept available for reminder features. Safe no-op if permission is missing.
export async function notify({ title, body, largeBody, urgency = "default" } = {}) {
  const granted = await callSafely(isPermissionGranted);
  if (!granted) return;
  const channelId = await ensureReminderChannel(urgency);
  await callSafely(sendNotification, {
    title: title ?? "Reminder",
    body,
    largeBody,
    channelId,
  });
}

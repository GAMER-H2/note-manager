import { watch } from "vue";
import { useSync } from "./useSync.js";
import { useSettings } from "./useSettings.js";

// Intervals offered in Settings → Sync. 0 keeps sync manual, which stays the
// default: an interval is only worth paying for once a remote is set up.
export const AUTO_SYNC_INTERVALS = [
  { value: 0, label: "Manual only" },
  { value: 5, label: "Every 5 minutes" },
  { value: 15, label: "Every 15 minutes" },
  { value: 30, label: "Every 30 minutes" },
  { value: 60, label: "Every hour" },
];

const MINUTE = 60_000;

// How long to wait after a note closes before syncing it out. Long enough that
// closing several notes in a row collapses into one sync.
const EDIT_SETTLE_MS = 3_000;

// Module-level so the scheduler is a singleton however many components call
// the composable — two live timers would sync twice per tick.
let timer = null;
let editTimer = null;
let ticking = false;
let listeners = null;

export function useAutoSync() {
  const { config, lastSyncAt, syncNow } = useSync();
  const { settings } = useSettings();

  const minutes = () => Number(settings.autoSyncMinutes) || 0;

  // `shouldDefer` lets the caller hold a tick back while the user is mid-edit;
  // `onChanged` fires only when a sync actually brought something in.
  let shouldDefer = () => false;
  let onChanged = () => {};

  const tick = async () => {
    // A remote that isn't configured, a sync already in flight, or an open
    // editor all mean "not now" — the next tick will pick it up.
    if (ticking || !config.value || shouldDefer()) return;

    ticking = true;
    try {
      const report = await syncNow();
      // Only report a change when the vault actually moved. Refreshing on
      // every quiet tick would close the open note for nothing.
      //
      // `conflicts` belongs here even though it sounds like a failure: a
      // conflict writes a "conflicted copy" note to disk, and it's the one
      // counter that increments without `pulled` or `merged` alongside it.
      const changed =
        report &&
        (report.pulled ||
          report.merged ||
          report.deletedLocal ||
          report.conflicts);
      if (changed) await onChanged();
    } finally {
      ticking = false;
    }
  };

  const restart = () => {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    const every = minutes();
    if (every > 0) timer = setInterval(tick, every * MINUTE);
  };

  // Coming back to the app is the moment stale notes are most obvious, so
  // catch up then too — but only if a whole interval has already elapsed, or
  // every alt-tab would trigger a round trip.
  const catchUp = () => {
    const every = minutes();
    if (every <= 0) return;
    if (Date.now() - (lastSyncAt.value || 0) < every * MINUTE) return;
    tick();
  };

  const onVisibility = () => {
    if (document.visibilityState === "visible") catchUp();
  };

  // Called when an edit finishes (a note closes). Deliberately not called on
  // every keystroke: `shouldDefer` holds ticks back while a note is open, so a
  // mid-edit sync would be dropped anyway.
  const syncSoon = () => {
    if (minutes() <= 0) return;
    if (editTimer) clearTimeout(editTimer);
    editTimer = setTimeout(() => {
      editTimer = null;
      tick();
    }, EDIT_SETTLE_MS);
  };

  const startAutoSync = (options = {}) => {
    stopAutoSync();
    shouldDefer = options.shouldDefer ?? (() => false);
    onChanged = options.onChanged ?? (() => {});

    restart();
    listeners = { onVisibility, onFocus: catchUp };
    document.addEventListener("visibilitychange", listeners.onVisibility);
    window.addEventListener("focus", listeners.onFocus);

    // Pull anything that landed while the app was closed, so the first thing
    // you see is current rather than a whole interval stale.
    if (minutes() > 0) tick();
  };

  const stopAutoSync = () => {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    if (editTimer) {
      clearTimeout(editTimer);
      editTimer = null;
    }
    if (listeners) {
      document.removeEventListener("visibilitychange", listeners.onVisibility);
      window.removeEventListener("focus", listeners.onFocus);
      listeners = null;
    }
  };

  // Rebuild the timer when the user picks a different interval, so the change
  // takes effect without reopening the app.
  watch(() => settings.autoSyncMinutes, restart);

  return { startAutoSync, stopAutoSync, syncSoon };
}

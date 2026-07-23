import { onBeforeUnmount, watch } from "vue";

let overlaySequence = 0;

// Map overlay open/close state onto browser history so Android hardware back
// closes drawers/modals first instead of immediately leaving the app.
export function useOverlayHistory(isOpen, close) {
  const overlayId = `note-manager-overlay-${++overlaySequence}`;
  let listening = false;
  let pendingCloseResolvers = [];

  const currentOverlayId = () =>
    typeof window !== "undefined"
      ? window.history.state?.__noteManagerOverlay ?? null
      : null;

  const resolvePendingCloses = () => {
    for (const resolve of pendingCloseResolvers) resolve();
    pendingCloseResolvers = [];
  };

  const detach = () => {
    if (typeof window === "undefined" || !listening) return;
    window.removeEventListener("popstate", onPopState);
    listening = false;
  };

  const onPopState = () => {
    if (!isOpen()) {
      detach();
      resolvePendingCloses();
      return;
    }

    // If our id is still the current history entry, a child overlay was popped
    // and revealed us again. Only close when our own history entry is gone.
    if (currentOverlayId() === overlayId) return;

    detach();
    close();
    resolvePendingCloses();
  };

  watch(
    isOpen,
    (open) => {
      if (typeof window === "undefined") return;

      if (open) {
        if (!listening) {
          window.addEventListener("popstate", onPopState);
          listening = true;
          window.history.pushState({ __noteManagerOverlay: overlayId }, "");
        }
      } else {
        detach();
        resolvePendingCloses();
      }
    },
    { immediate: true },
  );

  onBeforeUnmount(() => {
    detach();
    resolvePendingCloses();
  });

  const requestClose = () => {
    if (!isOpen()) return Promise.resolve();

    if (typeof window === "undefined") {
      close();
      resolvePendingCloses();
      return Promise.resolve();
    }

    if (currentOverlayId() === overlayId) {
      return new Promise((resolve) => {
        pendingCloseResolvers.push(resolve);
        window.history.back();
      });
    }

    detach();
    close();
    resolvePendingCloses();
    return Promise.resolve();
  };

  return { requestClose };
}

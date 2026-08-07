import { onBeforeUnmount } from "vue";
import { isAndroid } from "../lib/platform.js";

// How long a touch has to be held before it counts as a long-press. Deliberately
// on the long side per the note-manager UX, but easily tuned here.
const LONG_PRESS_MS = 600;
// If the finger drifts more than this (a scroll, not a press), abandon it.
const MOVE_TOLERANCE = 10;

// Normalises the two ways the context menu is summoned into a single callback:
// desktop right-click, and a mobile long-press. `onOpen({ x, y, target })` is
// invoked with viewport coordinates and the element the gesture began on (so a
// delegated handler can tell a note card from empty space).
export function useContextMenuTrigger(onOpen) {
  const android = isAndroid();
  let timer = null;
  let startX = 0;
  let startY = 0;
  let startTarget = null;

  const cancelTimer = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  };

  // A long-press is followed by a synthetic click when the finger lifts, which
  // would otherwise land on the menu's backdrop (closing it instantly) or open
  // the note. Swallow that one click.
  const suppressNextClick = () => {
    const onClick = (e) => {
      e.stopPropagation();
      e.preventDefault();
      window.removeEventListener("click", onClick, true);
      clearTimeout(cleanup);
    };
    window.addEventListener("click", onClick, true);
    const cleanup = setTimeout(() => {
      window.removeEventListener("click", onClick, true);
    }, 800);
  };

  const onContextMenu = (e) => {
    if (android) return; // desktop right-click only
    e.preventDefault();
    onOpen({ x: e.clientX, y: e.clientY, target: e.target });
  };

  const onTouchStart = (e) => {
    if (!android) return;
    if (e.touches.length !== 1) {
      cancelTimer();
      return;
    }
    const touch = e.touches[0];
    startX = touch.clientX;
    startY = touch.clientY;
    startTarget = e.target;
    cancelTimer();
    timer = setTimeout(() => {
      timer = null;
      suppressNextClick();
      onOpen({ x: startX, y: startY, target: startTarget });
    }, LONG_PRESS_MS);
  };

  const onTouchMove = (e) => {
    if (!timer) return;
    const touch = e.touches[0];
    if (
      Math.abs(touch.clientX - startX) > MOVE_TOLERANCE ||
      Math.abs(touch.clientY - startY) > MOVE_TOLERANCE
    ) {
      cancelTimer();
    }
  };

  onBeforeUnmount(cancelTimer);

  return {
    onContextMenu,
    onTouchStart,
    onTouchMove,
    onTouchEnd: cancelTimer,
    onTouchCancel: cancelTimer,
  };
}

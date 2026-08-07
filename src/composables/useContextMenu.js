import { reactive } from "vue";

// A single, app-wide right-click menu. Any component can pop it open with a
// list of `{ label, action, danger? }` items; a lone <ContextMenu> in App.vue
// renders whatever is in this module-level state. Kept a singleton (like
// useFolders) so callers don't have to thread events up to App.vue.
const menu = reactive({ visible: false, x: 0, y: 0, items: [] });

// Rough per-row height + chrome, used only to keep the menu from opening off
// the bottom/right edge of the window.
const ROW_HEIGHT = 44;
const MENU_WIDTH = 220;
const MARGIN = 8;

let listenersAttached = false;

const closeMenu = () => {
  menu.visible = false;
  menu.items = [];
  detachDismissListeners();
};

const onKeydown = (e) => {
  if (e.key === "Escape") closeMenu();
};

const attachDismissListeners = () => {
  if (listenersAttached) return;
  window.addEventListener("keydown", onKeydown);
  // Any scroll (capture, so it catches the notes list scrolling) or resize
  // moves the anchor out from under the menu, so just dismiss it.
  window.addEventListener("scroll", closeMenu, true);
  window.addEventListener("resize", closeMenu);
  listenersAttached = true;
};

function detachDismissListeners() {
  if (!listenersAttached) return;
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("scroll", closeMenu, true);
  window.removeEventListener("resize", closeMenu);
  listenersAttached = false;
}

export function useContextMenu() {
  // `x`/`y` are viewport coordinates (from a right-click or a long-press),
  // clamped so the menu never opens off the bottom/right edge.
  const openMenu = (x, y, items) => {
    if (!items?.length) return;
    menu.items = items;
    menu.x = Math.max(MARGIN, Math.min(x, window.innerWidth - MENU_WIDTH - MARGIN));
    menu.y = Math.max(
      MARGIN,
      Math.min(y, window.innerHeight - (items.length * ROW_HEIGHT + MARGIN)),
    );
    menu.visible = true;
    attachDismissListeners();
  };

  return { menu, openMenu, closeMenu };
}

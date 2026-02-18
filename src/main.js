const notification = window.__TAURI__?.notification ?? {};
const {
  isPermissionGranted,
  requestPermission,
  sendNotification,
  registerActionTypes,
  createChannel,
  Importance,
  Visibility,
} = notification;

// Tauri invoke:
// - v2 (recommended): window.__TAURI__.core.invoke
// - some builds / older docs: window.__TAURI__.tauri.invoke
const invoke =
  window.__TAURI__?.core?.invoke ??
  window.__TAURI__?.tauri?.invoke ??
  window.__TAURI_INVOKE__;

// Ensure a function runs after DOM is ready
const onDomReady = (fn) => {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", fn, { once: true });
  } else {
    fn();
  }
};

// Note editor modal controller (popup editor like settings modal)
const initNoteModal = () => {
  const modal = document.querySelector("[data-note-modal]");
  const overlay = document.querySelector("[data-note-overlay]");
  const editor = document.querySelector("[data-note-editor]");
  const titleEl = document.querySelector("[data-note-editor-title]");
  const subtitleEl = document.querySelector("[data-note-editor-subtitle]");
  const statusEl = document.querySelector("[data-note-status]");
  const closeBtn = document.querySelector("[data-note-close]");
  const doneBtn = document.querySelector("[data-note-done]");
  const deleteBtn = document.querySelector("[data-note-delete]");

  const setStatus = (text) => {
    if (statusEl) statusEl.textContent = text;
  };

  const state = {
    open: false,
    noteId: null,
    activeCard: null,
    escHandler: null,
    debouncedSave: null,
    lastSavedValue: "",
  };

  const persistNow = async () => {
    if (!state.noteId) return;
    if (typeof invoke !== "function") return;

    const content = editor?.value ?? "";
    try {
      setStatus("Saving…");
      await invoke("update_note", { req: { id: state.noteId, content } });
      state.lastSavedValue = content;
      setStatus("Saved");
    } catch (err) {
      console.error("Failed to persist note:", err);
      setStatus("Save failed");
    }
  };

  const requestClose = async () => {
    await persistNow();

    // Keep the card's cached content in sync so reopening doesn't revert
    // to the original content passed into openModal.
    if (state.activeCard && editor) {
      state.activeCard.dataset.noteContent = editor.value ?? "";
    }

    closeModal();
  };

  const onEsc = (e) => {
    if (e.key === "Escape" && state.open) {
      requestClose();
    }
  };

  const openModal = ({ id, content, card }) => {
    if (!modal || !overlay || !editor) return;

    state.open = true;
    state.noteId = id;
    state.activeCard = card ?? null;

    if (titleEl) titleEl.textContent = firstLineTitle(content);
    if (subtitleEl)
      subtitleEl.textContent = `${id}.md • Markdown editor (auto-saves)`;

    editor.value = content ?? "";
    state.lastSavedValue = editor.value;

    overlay.removeAttribute("hidden");
    modal.setAttribute("aria-hidden", "false");
    document.documentElement.classList.add("note-open");
    document.body.classList.add("note-open");

    setStatus("Saved");
    editor.focus();
    editor.selectionStart = editor.value.length;
    editor.selectionEnd = editor.value.length;

    if (!state.debouncedSave) {
      state.debouncedSave = debounce(() => {
        // Avoid pointless writes if nothing changed
        if ((editor?.value ?? "") === state.lastSavedValue) return;
        persistNow();
      }, 400);
    }

    if (!state.escHandler) {
      state.escHandler = onEsc;
      window.addEventListener("keydown", state.escHandler);
    }
  };

  const closeModal = () => {
    if (!modal || !overlay || !editor) return;

    state.open = false;

    overlay.setAttribute("hidden", "");
    modal.setAttribute("aria-hidden", "true");
    document.documentElement.classList.remove("note-open");
    document.body.classList.remove("note-open");

    if (state.escHandler) {
      window.removeEventListener("keydown", state.escHandler);
      state.escHandler = null;
    }

    // Sync card UI + cached content from editor content
    const card = state.activeCard;
    if (card) {
      const title = card.querySelector(".note-title");
      const preview = card.querySelector(".note-preview");
      const v = editor.value ?? "";
      card.dataset.noteContent = v;
      if (title) title.textContent = firstLineTitle(v);
      if (preview) preview.textContent = previewText(v);
      card.focus?.();
    }

    state.noteId = null;
    state.activeCard = null;
  };

  overlay?.addEventListener("click", () => {
    requestClose();
  });

  closeBtn?.addEventListener("click", () => {
    requestClose();
  });

  doneBtn?.addEventListener("click", () => {
    requestClose();
  });

  editor?.addEventListener("input", () => {
    const v = editor.value ?? "";
    if (titleEl) titleEl.textContent = firstLineTitle(v);

    // Update cached content immediately so reopening mid-session doesn't revert
    if (state.activeCard) {
      state.activeCard.dataset.noteContent = v;
    }

    if (typeof state.debouncedSave === "function") state.debouncedSave();
    setStatus("Editing…");
  });

  editor?.addEventListener("blur", () => {
    // Persist when leaving the editor, but keep modal open
    persistNow();
  });

  deleteBtn?.addEventListener("click", async () => {
    if (!state.noteId) return;

    if (typeof invoke !== "function") {
      console.warn("Tauri invoke is not available; cannot delete note.");
      return;
    }

    try {
      // Delete the persisted markdown file in the backend
      await invoke("delete_note", { req: { id: state.noteId } });

      // Close the modal immediately after delete
      // (Optionally also remove the card so UI matches filesystem state)
      state.activeCard?.remove?.();
      closeModal();
    } catch (err) {
      console.error("Failed to delete note:", err);
      setStatus("Delete failed");
    }
  });

  return {
    openModal,
    closeModal,
    persistNow,
  };
};

// Helper: call a function if it exists, otherwise no-op
const callIfAvailable = async (fn, ...args) => {
  try {
    if (typeof fn === "function") {
      return await fn(...args);
    }
  } catch (err) {
    // Log for debugging, but don't crash on non-Android platforms
    console.warn("Notification optional call failed:", err);
  }
  return undefined;
};

const debounce = (fn, waitMs = 350) => {
  let t = null;
  return (...args) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => fn(...args), waitMs);
  };
};

const escapeHtml = (s) =>
  String(s).replace(/[&<>"']/g, (ch) => {
    switch (ch) {
      case "&":
        return "&amp;";
      case "<":
        return "&lt;";
      case ">":
        return "&gt;";
      case '"':
        return "&quot;";
      case "'":
        return "&#39;";
      default:
        return ch;
    }
  });

const firstLineTitle = (markdown) => {
  const text = String(markdown ?? "").trim();
  if (!text) return "Untitled";
  const firstLine = text.split("\n")[0].trim();
  if (!firstLine) return "Untitled";
  return firstLine.replace(/^#{1,6}\s+/, "").slice(0, 80) || "Untitled";
};

const previewText = (markdown) => {
  const text = String(markdown ?? "")
    .replace(/\r\n/g, "\n")
    .split("\n")
    .slice(0, 6)
    .join("\n")
    .trim();
  return text ? text.slice(0, 240) : "Click to edit…";
};

// Android-only setup (action types + channels). Will no-op on macOS/Windows/Linux.
await callIfAvailable(registerActionTypes, [
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

await callIfAvailable(createChannel, {
  id: "reminders",
  name: "Reminders",
  description: "Notifications for reminders",
  importance: Importance?.High ?? undefined,
  visibility: Visibility?.Public ?? undefined,
  lights: true,
  lightColor: "#ff0000",
  vibration: true,
});

// Do you have permission to send a notification?
let permissionGranted = false;
try {
  permissionGranted = await isPermissionGranted();
} catch (e) {
  console.warn("isPermissionGranted not available:", e);
}

// If not we need to request it
if (!permissionGranted) {
  try {
    const permission = await requestPermission();
    permissionGranted = permission === "granted";
  } catch (e) {
    console.warn("requestPermission not available:", e);
    // On some desktop setups, permission may be implicitly granted or handled by OS
  }
}

let sendIt = false;

// Once permission has been granted we can send the notification
if (permissionGranted && sendIt) {
  await sendNotification({
    title: "Morning Reminders",
    largeBody: "- Take out the trash\n- Walk the dog\n- Check emails",
    // actionTypeId only applies when action types are registered (Android)
    // actionTypeId: "options",
    // channelId only applies if createChannel was available (Android)
    channelId: "reminders",
  });
}

// Sidebar toggle (desktop collapse, mobile overlay)
(() => {
  const body = document.body;
  const hamburger = document.querySelector(".hamburger-button");
  const sidebar = document.querySelector(".app-sidebar");
  const main = document.querySelector(".app-main");
  const mql = window.matchMedia("(max-width: 768px)");

  if (!hamburger || !sidebar || !main) return;

  let overlayActive = false;
  let escHandler = null;

  // Create a backdrop element for overlay mode
  const backdrop = document.createElement("div");
  backdrop.className = "sidebar-backdrop";
  Object.assign(backdrop.style, {
    position: "fixed",
    inset: "0",
    background: "rgba(0,0,0,0.45)",
    backdropFilter: "blur(2px)",
    zIndex: "55",
  });

  const lockScroll = () => {
    document.documentElement.style.overflow = "hidden";
    body.style.overflow = "hidden";
  };
  const unlockScroll = () => {
    document.documentElement.style.overflow = "";
    body.style.overflow = "";
  };

  const onEsc = (e) => {
    if (e.key === "Escape" && overlayActive) {
      closeOverlaySidebar();
    }
  };

  const openOverlaySidebar = () => {
    if (overlayActive) return;
    overlayActive = true;

    // Ensure visible over mobile rule that hides sidebar
    sidebar.style.display = "flex";
    sidebar.style.position = "fixed";
    sidebar.style.top = "var(--header-height)";
    sidebar.style.left = "0";
    sidebar.style.bottom = "0";
    sidebar.style.width = "min(80vw, var(--sidebar-width))";
    sidebar.style.maxWidth = "90vw";
    sidebar.style.transform = "translateX(-100%)";
    sidebar.style.transition = "transform 200ms ease";
    sidebar.style.boxShadow = "0 10px 30px rgba(0,0,0,0.4)";
    sidebar.style.zIndex = "60";
    sidebar.setAttribute("aria-hidden", "false");
    hamburger.setAttribute("aria-expanded", "true");

    body.appendChild(backdrop);
    lockScroll();

    // Animate in
    requestAnimationFrame(() => {
      sidebar.style.transform = "translateX(0)";
    });

    // Close handlers
    backdrop.addEventListener("click", closeOverlaySidebar, { once: true });
    escHandler = onEsc;
    window.addEventListener("keydown", escHandler);
  };

  const closeOverlaySidebar = () => {
    if (!overlayActive) return;
    overlayActive = false;

    // Animate out then clean up
    sidebar.style.transform = "translateX(-100%)";
    const handleEnd = () => {
      sidebar.removeEventListener("transitionend", handleEnd);
      // Clear inline styles so CSS takes over again
      sidebar.style.display = "";
      sidebar.style.position = "";
      sidebar.style.top = "";
      sidebar.style.left = "";
      sidebar.style.bottom = "";
      sidebar.style.width = "";
      sidebar.style.maxWidth = "";
      sidebar.style.transform = "";
      sidebar.style.transition = "";
      sidebar.style.boxShadow = "";
      sidebar.style.zIndex = "";
    };
    sidebar.addEventListener("transitionend", handleEnd);

    hamburger.setAttribute("aria-expanded", "false");
    sidebar.setAttribute("aria-hidden", "true");

    if (backdrop.parentNode) backdrop.parentNode.removeChild(backdrop);
    unlockScroll();

    if (escHandler) {
      window.removeEventListener("keydown", escHandler);
      escHandler = null;
    }
  };

  const toggleDesktopCollapsed = () => {
    const collapsed = body.classList.toggle("sidebar-collapsed");
    hamburger.setAttribute("aria-expanded", String(!collapsed));
  };

  const onHamburgerClick = () => {
    if (mql.matches) {
      // Mobile: overlay mode
      if (overlayActive) {
        closeOverlaySidebar();
      } else {
        openOverlaySidebar();
      }
    } else {
      // Desktop: collapse/expand layout
      toggleDesktopCollapsed();
    }
  };

  const handleBreakpointChange = (e) => {
    if (!e.matches) {
      // Switched to desktop
      if (overlayActive) {
        // Ensure overlay is fully closed and show sidebar in layout
        closeOverlaySidebar();
        body.classList.remove("sidebar-collapsed");
        hamburger.setAttribute("aria-expanded", "true");
      }
    } else {
      // Switched to mobile: remove any desktop-collapsed state
      body.classList.remove("sidebar-collapsed");
      hamburger.setAttribute("aria-expanded", "false");
    }
  };

  hamburger.addEventListener("click", onHamburgerClick);
  // Initialize ARIA expanded to reflect current desktop state
  hamburger.setAttribute(
    "aria-expanded",
    String(!body.classList.contains("sidebar-collapsed")),
  );

  // Watch for viewport changes
  if (typeof mql.addEventListener === "function") {
    mql.addEventListener("change", handleBreakpointChange);
  } else if (typeof mql.addListener === "function") {
    // Safari fallback
    mql.addListener(handleBreakpointChange);
  }
  // Initialize state based on current breakpoint
  handleBreakpointChange(mql);
})();

const settingsSchema = {
  general: {
    title: "General",
    description: "Configure behavior and accessibility preferences.",
    toggles: [
      {
        id: "autosave",
        label: "Auto-save notes",
        description: "Save changes automatically every few seconds.",
        default: true,
      },
      {
        id: "quickActions",
        label: "Show quick actions",
        description: "Display inline controls when hovering notes.",
        default: true,
      },
    ],
  },
  notifications: {
    title: "Notifications",
    description: "Control reminders and push alerts.",
    toggles: [
      {
        id: "desktopNotifications",
        label: "Desktop notifications",
        description: "Receive alerts when reminders are due.",
        default: false,
      },
      {
        id: "playSound",
        label: "Play reminder sound",
        description: "Play an alert tone for reminders.",
        default: true,
      },
    ],
  },
  appearance: {
    title: "Appearance",
    description: "Set display density and UI preferences.",
    toggles: [
      {
        id: "compact",
        label: "Compact note cards",
        description: "Reduce spacing to show more notes per row.",
        default: false,
      },
      {
        id: "highContrast",
        label: "High contrast mode",
        description: "Boost contrast for improved readability.",
        default: false,
      },
    ],
  },
};

const collectDefaultSettings = () => {
  const defaults = {};
  Object.values(settingsSchema).forEach((section) => {
    section.toggles?.forEach((toggle) => {
      if (!(toggle.id in defaults)) {
        defaults[toggle.id] = Boolean(toggle.default);
      }
    });
  });
  return defaults;
};

const settingsState = {
  activeCategory: "general",
  savedValues: collectDefaultSettings(),
  draftValues: {},
};

const syncDraftFromSaved = () => {
  settingsState.draftValues = { ...settingsState.savedValues };
};

syncDraftFromSaved();

const initSettingsModal = () => {
  const modal = document.querySelector("[data-settings-modal]");
  const overlay = document.querySelector("[data-settings-overlay]");
  const openButton = document.querySelector(".settings-button");

  if (!modal || !overlay || !openButton) return;

  const modalContent = modal.querySelector(".settings-modal__content");
  const cancelButton = modal.querySelector("[data-settings-cancel]");
  const applyButton = modal.querySelector("[data-settings-apply]");
  const closeButton = modal.querySelector("[data-settings-close]");
  const toggleList = modal.querySelector("[data-settings-toggle-list]");
  const emptyState = modal.querySelector("[data-settings-empty]");
  const toggleTemplate = document.getElementById("settings-toggle-template");
  const detailsTitle = modal.querySelector("[data-settings-section-title]");
  const detailsDescription = modal.querySelector(".settings-description");
  const categoryButtons = Array.from(
    modal.querySelectorAll(".settings-category"),
  );

  let previouslyFocused = null;

  function handleEscape(event) {
    if (event.key === "Escape") {
      closeModal();
    }
  }

  function setOverlayVisibility(visible) {
    if (visible) {
      overlay.removeAttribute("hidden");
      modal.setAttribute("aria-hidden", "false");
      document.documentElement.classList.add("settings-open");
      document.body.classList.add("settings-open");
      previouslyFocused = document.activeElement;
      modalContent?.focus();
      window.addEventListener("keydown", handleEscape);
    } else {
      overlay.setAttribute("hidden", "");
      modal.setAttribute("aria-hidden", "true");
      document.documentElement.classList.remove("settings-open");
      document.body.classList.remove("settings-open");
      window.removeEventListener("keydown", handleEscape);
      previouslyFocused?.focus?.();
      previouslyFocused = null;
    }
  }

  function updateCategorySelection(activeCategory) {
    categoryButtons.forEach((button) => {
      const isActive = button.dataset.category === activeCategory;
      button.setAttribute("aria-selected", String(isActive));
    });
  }

  function renderCategory(category) {
    const config = settingsSchema[category];
    if (!config || !toggleList) return;

    settingsState.activeCategory = category;
    updateCategorySelection(category);

    if (detailsTitle) {
      detailsTitle.textContent = config.title;
    }
    if (detailsDescription) {
      detailsDescription.textContent = config.description;
    }

    toggleList
      .querySelectorAll("[data-setting-row]")
      .forEach((row) => row.remove());

    const toggles = config.toggles ?? [];
    if (!toggles.length || !toggleTemplate) {
      emptyState?.removeAttribute("hidden");
      return;
    }

    emptyState?.setAttribute("hidden", "");

    toggles.forEach((toggle) => {
      const fragment = toggleTemplate.content.cloneNode(true);
      const row = fragment.querySelector("[data-setting-row]");
      const nameEl = fragment.querySelector("[data-setting-name]");
      const descEl = fragment.querySelector("[data-setting-description]");
      const input = fragment.querySelector("[data-setting-input]");

      if (!row || !nameEl || !descEl || !input) return;

      row.dataset.settingId = toggle.id;
      nameEl.textContent = toggle.label;
      descEl.textContent = toggle.description;

      const fallback =
        settingsState.savedValues[toggle.id] ?? Boolean(toggle.default);
      const currentValue = settingsState.draftValues[toggle.id] ?? fallback;
      input.checked = currentValue;

      input.addEventListener("change", () => {
        settingsState.draftValues[toggle.id] = input.checked;
      });

      toggleList.appendChild(fragment);
    });
  }

  function openModal() {
    syncDraftFromSaved();
    renderCategory(settingsState.activeCategory);
    setOverlayVisibility(true);
  }

  function closeModal() {
    setOverlayVisibility(false);
  }

  openButton.addEventListener("click", openModal);
  overlay.addEventListener("click", closeModal);
  cancelButton?.addEventListener("click", closeModal);
  closeButton?.addEventListener("click", closeModal);

  applyButton?.addEventListener("click", () => {
    settingsState.savedValues = { ...settingsState.draftValues };
    console.table(settingsState.savedValues);
    closeModal();
  });

  categoryButtons.forEach((button) => {
    button.addEventListener("click", () => {
      const category = button.dataset.category;
      if (!category || category === settingsState.activeCategory) return;
      renderCategory(category);
    });
  });
};

const bootstrapSettingsModal = () => {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", initSettingsModal, {
      once: true,
    });
  } else {
    initSettingsModal();
  }
};

bootstrapSettingsModal();

// Notes: add-note button creates an editable card and persists its markdown via backend commands.
(() => {
  const grid = document.querySelector(".notes-grid");
  const addButton = document.querySelector(".add-note-button");
  if (!grid || !addButton) return;

  const persistNote = async (id, content) => {
    if (typeof invoke !== "function") {
      console.warn(
        "Tauri invoke is not available; note will not be persisted.",
      );
      return;
    }
    try {
      await invoke("update_note", { req: { id, content } });
    } catch (err) {
      console.error("Failed to persist note:", err);
    }
  };

  const noteModal = initNoteModal();

  const makeCard = ({ id, content = "" }) => {
    const card = document.createElement("article");
    card.className = "note-card";
    card.tabIndex = 0;
    card.dataset.noteId = id;

    const titleEl = document.createElement("h2");
    titleEl.className = "note-title";
    titleEl.textContent = firstLineTitle(content);

    const previewEl = document.createElement("p");
    previewEl.className = "note-preview";
    previewEl.textContent = previewText(content);

    const openInModal = () => {
      if (!noteModal) return;
      noteModal.openModal({ id, content: currentContent(), card });
    };

    const currentContent = () => {
      // Prefer the freshest content from the card's dataset if available
      return card.dataset.noteContent ?? content ?? "";
    };

    // Keep a copy so the modal can open without extra backend reads
    card.dataset.noteContent = content ?? "";

    card.addEventListener("click", openInModal);
    card.addEventListener("dblclick", openInModal);
    card.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        openInModal();
      }
    });

    card.appendChild(titleEl);
    card.appendChild(previewEl);
    return card;
  };

  const renderExistingNotes = async () => {
    if (typeof invoke !== "function") {
      console.warn(
        "Tauri invoke is not available; cannot load existing notes.",
      );
      return;
    }

    try {
      const notes = (await invoke("list_notes")) ?? [];
      if (!Array.isArray(notes) || notes.length === 0) return;

      // Clear grid before rendering to avoid duplicates if this runs more than once
      grid.innerHTML = "";

      notes.forEach((n) => {
        const id = n?.id;
        const content = n?.content ?? "";
        if (!id) return;
        grid.appendChild(makeCard({ id, content }));
      });
    } catch (err) {
      console.error("Failed to load existing notes:", err);
    }
  };

  // Load notes on startup (after DOM is ready)
  onDomReady(renderExistingNotes);

  addButton.addEventListener("click", async () => {
    if (typeof invoke !== "function") {
      console.warn(
        "Tauri invoke is not available; cannot create backend markdown file.",
      );
      return;
    }

    try {
      const res = await invoke("create_note");
      const id = res?.id;
      const content = res?.content ?? "";

      if (!id) {
        console.error("create_note returned no id:", res);
        return;
      }

      const card = makeCard({ id, content });
      grid.prepend(card);

      // Immediately open in modal so you can start typing
      if (noteModal) {
        noteModal.openModal({ id, content, card });
      }
    } catch (err) {
      console.error("Failed to create note:", err);
    }
  });
})();

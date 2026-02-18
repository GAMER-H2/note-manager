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

// Tauri invoke (v2 exposes it on window.__TAURI__.core.invoke)
const invoke = window.__TAURI__?.core?.invoke;

// Ensure a function runs after DOM is ready
const onDomReady = (fn) => {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", fn, { once: true });
  } else {
    fn();
  }
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

  const makeCard = ({ id, content = "" }) => {
    const card = document.createElement("article");
    card.className = "note-card";
    card.tabIndex = 0;
    card.dataset.noteId = id;

    // View mode
    const titleEl = document.createElement("h2");
    titleEl.className = "note-title";
    titleEl.textContent = firstLineTitle(content);

    const previewEl = document.createElement("p");
    previewEl.className = "note-preview";
    previewEl.textContent = previewText(content);

    // Edit mode
    const editor = document.createElement("textarea");
    editor.className = "note-editor";
    editor.value = content;
    editor.setAttribute("aria-label", "Edit note (markdown)");
    editor.hidden = true;

    const saveHint = document.createElement("p");
    saveHint.className = "note-hint";
    saveHint.innerHTML = `${escapeHtml("Auto-saves after typing.  Esc to close.")}`;
    saveHint.hidden = true;

    const openEditor = () => {
      editor.hidden = false;
      saveHint.hidden = false;
      titleEl.hidden = true;
      previewEl.hidden = true;

      editor.focus();
      // Put cursor at end
      editor.selectionStart = editor.value.length;
      editor.selectionEnd = editor.value.length;
    };

    const closeEditor = () => {
      editor.hidden = true;
      saveHint.hidden = true;
      titleEl.hidden = false;
      previewEl.hidden = false;

      const newContent = editor.value ?? "";
      titleEl.textContent = firstLineTitle(newContent);
      previewEl.textContent = previewText(newContent);
      card.focus();
    };

    const debouncedSave = debounce(() => {
      persistNote(id, editor.value ?? "");
    }, 400);

    card.addEventListener("dblclick", openEditor);
    card.addEventListener("keydown", (e) => {
      // Enter opens editor when card is focused (view mode)
      if (e.key === "Enter" && editor.hidden) {
        e.preventDefault();
        openEditor();
      }
    });

    editor.addEventListener("input", () => {
      debouncedSave();
      // Live update title/preview while editing
      const v = editor.value ?? "";
      titleEl.textContent = firstLineTitle(v);
      previewEl.textContent = previewText(v);
    });

    editor.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        closeEditor();
      }
      // Cmd/Ctrl+Enter closes editor
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        closeEditor();
      }
    });

    editor.addEventListener("blur", () => {
      // Persist on blur and return to view mode
      persistNote(id, editor.value ?? "");
      closeEditor();
    });

    card.appendChild(titleEl);
    card.appendChild(previewEl);
    card.appendChild(editor);
    card.appendChild(saveHint);
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

      // list_notes already sorts, but keep it deterministic anyway
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

      // Immediately open editor so you can start typing
      const editor = card.querySelector(".note-editor");
      if (editor) {
        editor.hidden = false;
        const hint = card.querySelector(".note-hint");
        if (hint) hint.hidden = false;
        const title = card.querySelector(".note-title");
        const preview = card.querySelector(".note-preview");
        if (title) title.hidden = true;
        if (preview) preview.hidden = true;
        editor.focus();
      }
    } catch (err) {
      console.error("Failed to create note:", err);
    }
  });
})();

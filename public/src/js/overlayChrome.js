// aion-copilot: what every copilot overlay window shares — transparency at
// hand, a lock that keeps the window where it is, dragging by the header, and
// closing. The overlay's name is its window's label, else its page
// ("/group.html" -> "group").
//
// Transparency and lock live in the app's settings (copilotBus.js), as
// copilot.ov.<label>.opacity / .locked, so they outlast an update. Until the
// settings arrive the window shows what it showed last (a copy in
// localStorage), so it never flickers. Up to 1.0.0 they were only in
// localStorage, as copilot.<label>.opacity / .locked: taken over once, and
// left there.
(() => {
  const label =
    window.__TAURI__?.window?.getCurrentWindow?.()?.label ||
    location.pathname.replace(/^\//, "").replace(/\.html$/, "") ||
    "overlay";
  const bus = window.copilotBus;
  const invoke = (cmd, args) => {
    const fn = window.__TAURI__?.core?.invoke;
    return fn ? fn(cmd, args).catch(() => {}) : Promise.resolve();
  };
  const get = (key, fallback) => {
    try {
      return localStorage.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  };
  const set = (key, value) => {
    try {
      localStorage.setItem(key, value);
    } catch {
      // not kept, still applied
    }
  };
  const svg = (path) =>
    `<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${path}</svg>`;

  window.copilotOverlay = { label, invoke, svg };

  const KEYS = {
    opacity: { key: `copilot.ov.${label}.opacity`, old: `copilot.${label}.opacity` },
    locked: { key: `copilot.ov.${label}.locked`, old: `copilot.${label}.locked` },
  };
  // A setting: kept in the app, and its copy here for the next start.
  const save = (name, value) => {
    set(KEYS[name].key, value);
    bus?.settings.set(KEYS[name].key, value);
  };
  // Appliers by name, and the ones the player has touched before the
  // settings arrived (theirs wins over the stored value).
  const appliers = {};
  const touched = new Set();
  const initial = (name, fallback) => get(KEYS[name].key, null) ?? get(KEYS[name].old, fallback);

  const panel = document.querySelector(".panel");
  const opacityInput = document.querySelector(".opacityInput");
  if (panel && opacityInput) {
    const show = (value) => {
      let opacity = Number(value);
      if (!Number.isFinite(opacity)) opacity = 55;
      opacityInput.value = String(opacity);
      panel.style.setProperty("--panel-alpha", String(opacity / 100));
    };
    appliers.opacity = show;
    show(initial("opacity", 55));
    opacityInput.addEventListener("input", () => {
      touched.add("opacity");
      panel.style.setProperty("--panel-alpha", String(Number(opacityInput.value) / 100));
      set(KEYS.opacity.key, opacityInput.value);
    });
    // Kept when let go, not on every step of the drag.
    opacityInput.addEventListener("change", () => save("opacity", opacityInput.value));
  }

  const lockBtn = document.querySelector(".lockBtn");
  if (lockBtn) {
    const open = svg('<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 7.5-2"/>');
    const shut = svg('<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>');
    const show = (value) => {
      const locked = String(value) === "true";
      document.body.classList.toggle("locked", locked);
      lockBtn.innerHTML = locked ? shut : open;
      const text = locked ? "Destravar posição" : "Travar posição";
      lockBtn.setAttribute("aria-label", text);
      lockBtn.title = text;
    };
    appliers.locked = show;
    show(initial("locked", "false"));
    lockBtn.addEventListener("click", () => {
      touched.add("locked");
      const locked = !document.body.classList.contains("locked");
      show(locked);
      save("locked", String(locked));
    });
  }

  if (bus) {
    const migrated = `copilot.ov.migrated.${label}`;
    bus.settings.ready().then(() => {
      // Once: what 1.0.0 kept in localStorage becomes the setting, unless
      // the setting already exists.
      if (bus.settings.peek(migrated) !== "1") {
        for (const { key, old } of Object.values(KEYS)) {
          const value = get(old, null);
          if (value !== null && bus.settings.peek(key) === undefined) bus.settings.set(key, value);
        }
        bus.settings.set(migrated, "1");
      }
      for (const [name, show] of Object.entries(appliers)) {
        const value = bus.settings.peek(KEYS[name].key);
        if (value !== undefined && !touched.has(name)) {
          show(value);
          set(KEYS[name].key, value);
        }
      }
    });
    // Changed elsewhere (another window, or the Painel later on).
    for (const [name, show] of Object.entries(appliers)) {
      bus.settings.on(KEYS[name].key, (value) => {
        if (value === undefined) return;
        show(value);
        set(KEYS[name].key, value);
      });
    }
  }

  // Drag by the header (native, like the meter), unless locked.
  document.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || document.body.classList.contains("locked")) return;
    const target = event.target;
    if (!target.closest("[data-drag]") || target.closest("button, input, select, label")) return;
    event.preventDefault();
    invoke("start_tool_drag");
  });

  document.querySelector(".closeBtn")?.addEventListener("click", () => invoke("toggle_copilot_overlay", { label }));
})();

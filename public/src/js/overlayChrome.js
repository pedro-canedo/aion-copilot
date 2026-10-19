// aion-copilot: what every copilot overlay window shares — transparency at
// hand, a lock that keeps the window where it is, dragging by the header, and
// closing. The overlay's name comes from its page ("/group.html" -> "group").
(() => {
  const label = location.pathname.replace(/^\//, "").replace(/\.html$/, "") || "overlay";
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

  const panel = document.querySelector(".panel");
  const opacityInput = document.querySelector(".opacityInput");
  if (panel && opacityInput) {
    const key = `copilot.${label}.opacity`;
    const apply = (value) => panel.style.setProperty("--panel-alpha", String(value / 100));
    let opacity = Number(get(key, 55));
    if (!Number.isFinite(opacity)) opacity = 55;
    opacityInput.value = String(opacity);
    apply(opacity);
    opacityInput.addEventListener("input", () => {
      apply(Number(opacityInput.value));
      set(key, opacityInput.value);
    });
  }

  const lockBtn = document.querySelector(".lockBtn");
  if (lockBtn) {
    const key = `copilot.${label}.locked`;
    const open = svg('<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 7.5-2"/>');
    const shut = svg('<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>');
    const setLocked = (locked) => {
      document.body.classList.toggle("locked", locked);
      lockBtn.innerHTML = locked ? shut : open;
      const text = locked ? "Destravar posição" : "Travar posição";
      lockBtn.setAttribute("aria-label", text);
      lockBtn.title = text;
      set(key, String(locked));
    };
    setLocked(get(key, "false") === "true");
    lockBtn.addEventListener("click", () => setLocked(!document.body.classList.contains("locked")));
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

// aion-copilot: the bar — always on screen. One icon per overlay turns it on
// or off; the mark opens the Painel; the arrow folds it down to the mark. A
// pill says when this version's Novidades are unread or a newer version is
// out, and opens the Painel there.
(() => {
  const tauri = window.__TAURI__;
  const invoke = (cmd, args) => (tauri?.core?.invoke ? tauri.core.invoke(cmd, args) : Promise.resolve(null));
  const ICON = {
    meter: '<path d="M3 3v18h18"/><path d="M7 15v3M11 10v8M15 6v12M19 12v6"/>',
    target: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3"/>',
    group: '<circle cx="9" cy="8" r="3.2"/><path d="M3 20c0-3.3 2.7-5.5 6-5.5s6 2.2 6 5.5"/><circle cx="17" cy="9" r="2.6"/><path d="M16 14.6c2.7.2 5 2 5 5.4"/>',
    bosses: '<path d="M12 3c-4.4 0-7.5 3-7.5 7 0 2.4 1.1 4.2 2.8 5.3V19a1 1 0 0 0 1 1h7.4a1 1 0 0 0 1-1v-3.7c1.7-1.1 2.8-2.9 2.8-5.3 0-4-3.1-7-7.5-7z"/><circle cx="9.2" cy="11" r="1.6"/><circle cx="14.8" cy="11" r="1.6"/><path d="M10.5 20v-2.5M13.5 20v-2.5"/>',
    timers: '<circle cx="12" cy="13" r="8"/><path d="M12 9v4l2.5 2"/><path d="M9.5 2.5h5M12 2.5V5"/>',
    routine: '<path d="M9 6h11M9 12h11M9 18h11"/><path d="M3.5 6l1.5 1.5L7.5 5M3.5 12l1.5 1.5 2.5-2.5M3.5 18l1.5 1.5 2.5-2.5"/>',
    map: '<path d="M3 6.5l6-3 6 3 6-3v14l-6 3-6-3-6 3z"/><path d="M9 3.5v14M15 6.5v14"/>',
    pets: '<path d="M12 13c3 0 5.5 3 5.5 5.5 0 1.6-1.4 2.5-3 2.5-1 0-1.6-.5-2.5-.5s-1.5.5-2.5.5c-1.6 0-3-.9-3-2.5C6.5 16 9 13 12 13z"/><circle cx="5" cy="10" r="1.8"/><circle cx="9" cy="5.5" r="1.8"/><circle cx="15" cy="5.5" r="1.8"/><circle cx="19" cy="10" r="1.8"/>',
  };
  const OVERLAYS = [
    { label: "meter", name: "Medidor de DPS", hint: "dano, cura e mortes por luta" },
    { label: "target", name: "Alvo", hint: "vida, tempo para matar; PvP: CP e placar" },
    { label: "group", name: "Grupo", hint: "classe, nível e CP do grupo" },
    { label: "bosses", name: "Chefes de campo", hint: "quando cada chefe volta" },
    { label: "timers", name: "Timers", hint: "eventos, chefes de campo e resets, do mais próximo" },
    { label: "map", name: "Mapa", hint: "onde você está, com camadas de lugares e recursos" },
    { label: "pets", name: "Mascotes", hint: "mascotes por perto e sua coleção" },
    { label: "routine", name: "Rotina", hint: "diárias, semanais e resets" },
  ];

  const hub = document.querySelector(".hub");
  const group = document.querySelector(".group.overlays");
  const tip = document.querySelector(".tip");
  const menu = document.querySelector(".menu");
  const moreBtn = document.querySelector(".moreBtn");
  const collapseBtn = document.querySelector(".collapse");
  const pillBox = document.querySelector(".pills");
  const bus = window.copilotBus;
  let states = {};

  const store = {
    get: (k) => {
      try {
        return localStorage.getItem(k);
      } catch {
        return null;
      }
    },
    set: (k, v) => {
      try {
        localStorage.setItem(k, v);
      } catch {
        // kept for this session
      }
    },
  };

  // The window follows what is on screen: the bar, and the tip or the menu.
  let sizing = 0;
  const fit = () => {
    cancelAnimationFrame(sizing);
    sizing = requestAnimationFrame(async () => {
      const win = tauri?.window?.getCurrentWindow?.();
      const LogicalSize = tauri?.dpi?.LogicalSize;
      if (!win || !LogicalSize) return;
      const r = hub.getBoundingClientRect();
      try {
        await win.setSize(new LogicalSize(Math.ceil(r.width) + 4, Math.ceil(r.height) + 4));
      } catch (e) {
        console.warn("[copilot] resize", e);
      }
    });
  };

  const button = (o) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tool";
    b.dataset.label = o.label;
    b.dataset.tip = `<b>${o.name}</b> · ${o.hint}`;
    b.setAttribute("aria-label", o.name);
    b.setAttribute("aria-pressed", "false");
    b.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[o.label]}</svg>`;
    b.addEventListener("click", () => toggle(o.label));
    return b;
  };
  group.replaceChildren(...OVERLAYS.map(button));

  const render = () => {
    for (const b of group.querySelectorAll(".tool")) {
      b.setAttribute("aria-pressed", String(!!states[b.dataset.label]));
    }
  };
  const refresh = async () => {
    const s = await invoke("copilot_overlay_states").catch(() => null);
    if (s) {
      states = s;
      render();
    }
  };
  const toggle = async (label) => {
    if (label === "meter") {
      await invoke("set_meter_shown", { shown: !states.meter }).catch(() => {});
    } else {
      await invoke("toggle_copilot_overlay", { label }).catch(() => {});
    }
    await refresh();
  };

  // Tips: what each icon is, under the bar.
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest("[data-tip]");
    if (!el || !menu.hidden) return;
    tip.innerHTML = el.dataset.tip;
    tip.style.marginLeft = `${Math.max(0, el.offsetLeft - 8)}px`;
    if (tip.hidden) {
      tip.hidden = false;
      fit();
    }
  });
  document.addEventListener("mouseout", (e) => {
    if (!e.relatedTarget?.closest?.("[data-tip]") && !tip.hidden) {
      tip.hidden = true;
      fit();
    }
  });

  // Folded down to the mark and back; it starts open and remembers.
  const setCollapsed = (on) => {
    hub.classList.toggle("isCollapsed", on);
    collapseBtn.setAttribute("aria-expanded", String(!on));
    collapseBtn.setAttribute("aria-label", on ? "Abrir a barra" : "Recolher a barra");
    store.set("copilot.hubCollapsed", String(on));
    if (on) setMenu(false);
    tip.hidden = true;
    fit();
  };
  collapseBtn.addEventListener("click", () => setCollapsed(!hub.classList.contains("isCollapsed")));

  // The settings menu.
  const setMenu = (open) => {
    menu.hidden = !open;
    moreBtn.setAttribute("aria-expanded", String(open));
    if (open) tip.hidden = true;
    fit();
  };
  moreBtn.addEventListener("click", () => setMenu(menu.hidden));
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !menu.hidden) setMenu(false);
  });

  const openPainel = () => invoke("toggle_copilot_overlay", { label: "painel" }).then(refresh);
  // The Painel on one of its views: it reads the view from this setting at
  // start, or hears it while open (a toggle would close it then).
  const openPainelAt = async (view) => {
    await bus?.settings.set("copilot.painel.view", view);
    const s = await invoke("copilot_overlay_states").catch(() => null);
    if (!s?.painel) await invoke("toggle_copilot_overlay", { label: "painel" }).catch(() => {});
    await refresh();
  };
  document.querySelector(".painelBtn").addEventListener("click", openPainel);
  document.querySelector(".brand").addEventListener("click", openPainel);
  document.querySelector(".settingsOpt").addEventListener("click", () => {
    setMenu(false);
    invoke("open_settings_window").catch(() => {});
  });

  const autostart = document.querySelector(".autostartOpt");
  const showAutostart = (on) => autostart.setAttribute("aria-checked", String(!!on));
  invoke("copilot_autostart", {}).then(showAutostart).catch(() => {});
  autostart.addEventListener("click", async () => {
    const want = autostart.getAttribute("aria-checked") !== "true";
    showAutostart(await invoke("copilot_autostart", { enable: want }).catch(() => !want));
  });

  // Drag the bar (native), not from its buttons; a click on the mark is not a drag.
  document.addEventListener("mousedown", (event) => {
    if (event.button !== 0) return;
    if (!event.target.closest("[data-drag]") || event.target.closest("button")) return;
    event.preventDefault();
    invoke("start_tool_drag").catch(() => {});
  });

  // The pills (whatsNewCore.js decides which).
  let whatsNew = null;
  let update = null;
  const renderPills = () => {
    const pills = window.copilotWhatsNew?.hubPills(whatsNew, update) || [];
    pillBox.replaceChildren(...pills.map((p) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `pill ${p.kind}`;
      b.textContent = p.text;
      b.dataset.tip = p.kind === "news" ? "<b>Novidades</b> · o que mudou nesta versão" : "<b>Atualização</b> · veja em Ajustes";
      b.addEventListener("click", () => openPainelAt(p.view));
      return b;
    }));
    pillBox.hidden = !pills.length;
    hub.classList.toggle("hasPills", pills.length > 0);
    renderVersion();
    fit();
  };
  // The installed version on the bar; in the menu, how updates are going.
  const verTag = document.querySelector(".ver");
  const brand = document.querySelector(".brand");
  const renderVersion = () => {
    const W = window.copilotWhatsNew;
    const current = update?.current || whatsNew?.current || "";
    verTag.textContent = current ? `v${current}` : "";
    verTag.hidden = !current;
    brand.title = current ? `Abrir o Painel · aion-copilot v${current}` : "Abrir o Painel";
    document.querySelector(".updateOptTitle").textContent = current ? `Versão ${current}` : "Atualizações";
    document.querySelector(".updateOptLine").textContent = update && W ? W.updateLine(update).text : "—";
  };
  document.querySelector(".updateOpt").addEventListener("click", () => {
    setMenu(false);
    openPainelAt("ajustes");
  });
  invoke("copilot_whats_new").then((w) => {
    if (w) whatsNew = { current: w.current, lastSeen: w.lastSeen };
    renderPills();
  }).catch(() => {});
  invoke("copilot_update_status").then((s) => {
    if (s) update = s;
    renderPills();
  }).catch(() => {});
  bus?.listen("copilot-update-status", (s) => {
    update = s;
    renderPills();
  });
  // Read in the Painel.
  bus?.settings.on("copilot.lastSeenVersion", (v) => {
    if (whatsNew && v) whatsNew.lastSeen = v;
    renderPills();
  });

  setCollapsed(store.get("copilot.hubCollapsed") === "true");
  refresh();
  setInterval(refresh, 3000);
})();

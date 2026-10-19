// aion-copilot: wiring for what the fork adds to the meter window.

// Combat power as the game's HUD writes it: "105,8K" (cut, not rounded).
window.copilotCp = (n) => {
  const v = Number(n) || 0;
  const lang = document.documentElement.lang || undefined;
  const cut = (x) => (Math.floor(x * 10) / 10).toLocaleString(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  if (v >= 1e6) return `${cut(v / 1e6)}M`;
  if (v >= 1e3) return `${cut(v / 1e3)}K`;
  return String(Math.floor(v));
};

(() => {
  // Content tabs (Chefes / Monstros / Treino / Todos) over the meter's own
  // target modes, kept in step with them however the mode changes.
  const tabs = [...document.querySelectorAll(".copilotTab")];
  // Treino and Monstros both show the one target you hit last (each training
  // dummy its own count); which of the two is lit is remembered here.
  let lastHitTab = "mob";
  const syncTabs = (mode) => {
    for (const tab of tabs) {
      const active = tab.dataset.mode === mode && (mode !== "lastHitByMe" || tab.dataset.tab === lastHitTab);
      tab.classList.toggle("isActive", active);
      tab.setAttribute("aria-selected", String(active));
    }
  };
  const hook = () => {
    // The meter window only: Settings, History and Details share this page.
    if ((document.documentElement.dataset.a2view || "main") !== "main") return true;
    const app = typeof DpsApp !== "undefined" ? DpsApp.instance : null;
    if (!app || typeof app.setTargetSelection !== "function") return false;
    const original = app.updateTargetModeButton.bind(app);
    app.updateTargetModeButton = () => {
      original();
      syncTabs(app.targetSelection);
    };
    syncTabs(app.targetSelection);

    // A target you killed leaves the meter 30 s later, so it is clean for the
    // next one. Only what is shown: the fight stays in History.
    const KILLED_CLEAR_MS = 30_000;
    const deadSince = new Map();
    const build = app.buildRowsFromPayload.bind(app);
    app.buildRowsFromPayload = (raw) => {
      const r = build(raw);
      const tracksOne = r.targetMode === "bossTargets" || r.targetMode === "lastHitByMe";
      if (!tracksOne || !(r.targetId > 0) || r.targetCurrentHp !== 0 || !(r.targetMaxHp > 0)) {
        if (r.targetId > 0) deadSince.delete(r.targetId);
        return r;
      }
      const now = window.copilotNowOverride?.() ?? Date.now();
      if (!deadSince.has(r.targetId)) deadSince.set(r.targetId, now);
      if (deadSince.size > 64) deadSince.delete(deadSince.keys().next().value);
      if (now - deadSince.get(r.targetId) < KILLED_CLEAR_MS) return r;
      return { ...r, rows: [], targetId: 0, targetName: "", targetMaxHp: 0, targetTotalDamage: 0, targetCurrentHp: -1, targetIsBoss: false };
    };

    const select = (mode, reason, tab) => {
      if (tab) lastHitTab = tab;
      if (app.targetSelection === mode) {
        syncTabs(mode);
        return;
      }
      app.setTargetSelection(mode, { persist: true, syncBackend: true, reason });
      if (!app.isCollapse) app.fetchDps?.();
    };
    // Auto (on unless turned off): the tab follows the kind of target you hit
    // last, boss / training dummy / monster. Picking a tab by hand pauses it.
    const autoBtn = document.querySelector(".copilotAuto");
    let auto = (() => {
      try {
        return localStorage.getItem("copilot.autoTab") !== "false";
      } catch {
        return true;
      }
    })();
    const setAuto = (on) => {
      auto = on;
      autoBtn?.classList.toggle("isActive", on);
      autoBtn?.setAttribute("aria-pressed", String(on));
      try {
        localStorage.setItem("copilot.autoTab", String(on));
      } catch {
        // kept for this session
      }
    };
    setAuto(auto);
    autoBtn?.addEventListener("click", () => setAuto(!auto));
    for (const tab of tabs) {
      tab.addEventListener("click", () => {
        setAuto(false);
        select(tab.dataset.mode, "copilot tabs", tab.dataset.tab);
      });
    }
    // Each kind of content, the view it gets: bosses their fight, a dungeon's
    // trash the whole party's damage, a dummy or a monster the one you hit.
    // PvP keeps the view (hits between players are not counted as damage)
    // and the Target overlay shows the player.
    const VIEW = {
      worldBoss: { mode: "bossTargets", label: "Chefe mundial" },
      crowdBoss: { mode: "lastHitByMe", tab: "mob", label: "Chefe mundial" },
      dungeonBoss: { mode: "bossTargets", label: "Chefe de expedição" },
      boss: { mode: "bossTargets", label: "Chefe" },
      dungeon: { mode: "allTargets", label: "Expedição" },
      train: { mode: "lastHitByMe", tab: "train", label: "Treino" },
      mob: { mode: "lastHitByMe", tab: "mob", label: "Monstros" },
      pvp: { label: "PvP" },
    };
    const contextEl = document.createElement("span");
    contextEl.className = "copilotContext";
    autoBtn?.appendChild(contextEl);
    const invoke = (cmd, args) => window.__TAURI__?.core?.invoke?.(cmd, args) ?? Promise.resolve(null);
    setInterval(async () => {
      const kind = await invoke("copilot_target_kind").catch(() => null);
      const view = VIEW[kind];
      // In the Abyss the zone leads (copilotPvp.js reads it).
      const abyss = window.copilotZoneKind === "abyss";
      contextEl.textContent = abyss ? (kind === "pvp" ? "Abismo · PvP" : "Abismo") : view ? view.label : "";
      autoBtn?.setAttribute("data-kind", kind || "none");
      if (!auto || !view?.mode) return;
      select(view.mode, "copilot auto", view.tab);
    }, 1000);
    return true;
  };
  if (tabs.length && !hook()) {
    const timer = setInterval(() => hook() && clearInterval(timer), 200);
  }
})();

// aion-copilot: the meter's own header — the metric switcher, your DPS, the
// fight clock, Top N, copy, reset, the in-overlay History by kind of fight and
// a word on what an empty view is waiting for. All of it drives the meter's
// existing machinery (display mode, player limit, screenshot, reset).
(() => {
  // The meter window only: Settings, History and Details share this page.
  const isMeter = () => (document.documentElement.dataset.a2view || "main") === "main";
  if (!document.querySelector(".copilotMetric") || !document.querySelector(".cvLabel")) return;
  const $ = (sel) => document.querySelector(sel);
  const lang = () => document.documentElement.lang || undefined;
  const invoke = (cmd, args) => window.__TAURI__?.core?.invoke?.(cmd, args) ?? Promise.resolve(null);
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
  const compact = (n) => {
    const v = Number(n) || 0;
    const fmt = (x, d) => x.toLocaleString(lang(), { minimumFractionDigits: d, maximumFractionDigits: d });
    if (v >= 1e9) return `${fmt(v / 1e9, 2)}B`;
    if (v >= 1e6) return `${fmt(v / 1e6, 2)}M`;
    if (v >= 1e3) return `${fmt(v / 1e3, 1)}k`;
    return String(Math.round(v));
  };
  const mmss = (ms) => {
    const s = Math.max(0, Math.round((Number(ms) || 0) / 1000));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  };
  const hhmm = (ms) => new Date(ms).toLocaleTimeString(lang(), { hour: "2-digit", minute: "2-digit" });

  const METRICS = [
    { mode: "both", label: "Dano causado", hint: "dano total e DPS" },
    { mode: "dps", label: "DPS", hint: "dano por segundo" },
    { mode: "totalHeal", label: "Cura", hint: "cura total" },
  ];
  const TOPS = [5, 10, 24];

  const hook = () => {
    if (!isMeter()) return true;
    const app = typeof DpsApp !== "undefined" ? DpsApp.instance : null;
    if (!app || typeof app.setDisplayMode !== "function" || typeof app.buildRowsFromPayload !== "function") return false;

    // ---------- metric ----------
    const metricBox = $(".copilotMetric");
    const menu = $(".cmMenu");
    const current = $(".cmCurrent");
    const indexOfMode = () => Math.max(0, METRICS.findIndex((m) => m.mode === app.displayMode));
    const showMetric = () => {
      const m = METRICS[indexOfMode()];
      $(".cmLabel").textContent = m.label;
      document.documentElement.dataset.copilotMetric = m.mode;
      const col = document.querySelector(".copilotCols .colDmg");
      if (col) col.textContent = m.mode === "dps" ? "DPS" : "Dano";
      for (const b of menu.querySelectorAll("button")) b.setAttribute("aria-checked", String(b.dataset.mode === m.mode));
    };
    const setMetric = (i) => {
      const m = METRICS[(i + METRICS.length) % METRICS.length];
      app.setDisplayMode(m.mode, { persist: true });
      app.renderCurrentRows?.();
      showMetric();
    };
    menu.replaceChildren(
      ...METRICS.map((m, i) => {
        const b = document.createElement("button");
        b.type = "button";
        b.setAttribute("role", "menuitemradio");
        b.dataset.mode = m.mode;
        b.innerHTML = "<span></span><small></small>";
        b.firstChild.textContent = m.label;
        b.lastChild.textContent = m.hint;
        b.addEventListener("click", () => {
          setMetric(i);
          openMenu(false);
        });
        return b;
      }),
    );
    const openMenu = (open) => {
      menu.hidden = !open;
      current.setAttribute("aria-expanded", String(open));
    };
    current.addEventListener("click", () => openMenu(menu.hidden));
    $(".cmPrev").addEventListener("click", () => setMetric(indexOfMode() - 1));
    $(".cmNext").addEventListener("click", () => setMetric(indexOfMode() + 1));
    document.addEventListener("click", (e) => {
      if (!menu.hidden && !metricBox.contains(e.target)) openMenu(false);
    });
    // Only these three here: an older choice (HPS) comes back to the closest.
    if (!METRICS.some((m) => m.mode === app.displayMode)) app.setDisplayMode(app.displayMode === "hps" ? "totalHeal" : "both", { persist: true });
    showMetric();

    // ---------- the view, one pick instead of four tabs ----------
    const tabs = [...document.querySelectorAll(".copilotTab")];
    const viewBtn = $(".copilotViewBtn");
    const viewMenu = $(".cvMenu");
    const showView = () => {
      const active = tabs.find((t) => t.classList.contains("isActive"));
      const label = $(".cvLabel");
      if (label) label.textContent = active ? active.textContent : "Visão";
    };
    const openViewMenu = (open) => {
      viewMenu.hidden = !open;
      viewBtn.setAttribute("aria-expanded", String(open));
    };
    viewMenu.replaceChildren(
      ...tabs.map((t) => {
        const b = document.createElement("button");
        b.type = "button";
        b.setAttribute("role", "menuitem");
        b.innerHTML = "<span></span><small></small>";
        b.firstChild.textContent = t.textContent;
        b.lastChild.textContent = t.title;
        b.addEventListener("click", () => {
          t.click();
          openViewMenu(false);
          showView();
        });
        return b;
      }),
    );
    viewBtn.addEventListener("click", () => openViewMenu(viewMenu.hidden));
    document.addEventListener("click", (e) => {
      if (!viewMenu.hidden && !$(".copilotViewPick").contains(e.target)) openViewMenu(false);
    });
    new MutationObserver(showView).observe($(".copilotTabs"), { subtree: true, attributes: true, attributeFilter: ["class"] });
    showView();

    // ---------- Top N ----------
    const topBtn = $(".copilotTopBtn");
    const showTop = () => {
      const n = Number(app.playerLimit) || 6;
      topBtn.textContent = n >= TOPS[TOPS.length - 1] ? "Todos" : `Top ${n}`;
    };
    const savedTop = Number(store.get("copilot.top"));
    if (TOPS.includes(savedTop)) app.setPlayerLimit(savedTop);
    topBtn.addEventListener("click", () => {
      const at = TOPS.indexOf(Number(app.playerLimit));
      const next = TOPS[(at + 1) % TOPS.length];
      app.setPlayerLimit(next, { persist: true });
      store.set("copilot.top", String(next));
      showTop();
    });
    showTop();

    // ---------- copy, reset ----------
    $(".copilotCopyBtn").addEventListener("click", () => document.querySelector(".bossIcon")?.click());
    $(".copilotResetBtn").addEventListener("click", () => document.querySelector(".resetBtn")?.click());

    // ---------- your DPS, the clock, the empty view ----------
    const myDps = $(".copilotMyDps b");
    const clock = $(".copilotClock");
    const empty = $(".copilotEmpty");
    let emptySince = Date.now();
    const EMPTY_TEXT = {
      bossTargets: (t) => `Nenhum chefe desde ${t}: entra aqui quando você ou seu grupo baterem num chefe.`,
      mob: (t) => `Nada desde ${t}: o último monstro que você atacar aparece aqui.`,
      train: () => "Bata num boneco de treino: cada boneco tem o seu próprio contador.",
      allTargets: (t) => `Nada desde ${t}: cada alvo que o grupo enfrenta soma aqui, monstros comuns incluídos.`,
    };
    const build = app.buildRowsFromPayload.bind(app);
    app.buildRowsFromPayload = (raw) => {
      const r = build(raw);
      const me = r.rows.find((row) => row.isUser);
      myDps.textContent = me ? `${compact(me.dps)}/s` : "—";
      clock.textContent = mmss(r.rows.length ? r.battleTimeMs : 0);
      if (r.rows.length) {
        emptySince = Date.now();
        empty.hidden = true;
      } else {
        const tab = r.targetMode === "lastHitByMe" ? (document.querySelector(".copilotTab.isActive")?.dataset.tab || "mob") : r.targetMode;
        const text = EMPTY_TEXT[tab] || EMPTY_TEXT.allTargets;
        empty.textContent = text(hhmm(emptySince));
        empty.hidden = false;
      }
      return r;
    };
    $(".copilotMyDps").addEventListener("click", () => {
      const me = app.lastSnapshot?.find?.((row) => row.isUser);
      if (me && typeof app.openDetails === "function") app.openDetails(me);
    });

    // ---------- History, in the overlay ----------
    const hist = $(".copilotHist");
    const histBtn = $(".copilotHistBtn");
    const list = $(".chList");
    let kind = store.get("copilot.histKind") || "world";
    let worldBossCodes = null;
    const HIST_EMPTY = {
      dungeon: "Nenhuma tentativa em expedição ainda: cada chefe de expedição aparece aqui quando a luta termina.",
      world: "Nenhuma luta no mundo ainda: chefes mundiais e outros chefes aparecem aqui quando a luta termina.",
      train: "Nenhum treino ainda: as sessões no boneco aparecem aqui.",
      pvp: "Nenhum confronto ainda: quem você matou e para quem morreu aparece aqui.",
    };
    const kindOf = (f) => (f.isPvp ? "pvp" : f.isTrain ? "train" : f.dungeonId ? "dungeon" : "world");
    const item = (main, sub, right, onClick) => {
      const li = document.createElement("li");
      li.innerHTML = '<span class="chMain"><b></b><small></small></span><span class="chRight"></span>';
      li.querySelector("b").textContent = main;
      li.querySelector("small").textContent = sub;
      li.querySelector(".chRight").textContent = right;
      if (onClick) {
        li.tabIndex = 0;
        li.classList.add("isLink");
        li.addEventListener("click", onClick);
      }
      return li;
    };
    const when = (ms) => {
      const d = new Date(ms);
      const today = new Date().toDateString() === d.toDateString();
      return today ? `hoje ${hhmm(ms)}` : d.toLocaleDateString(lang(), { day: "2-digit", month: "2-digit" }) + ` ${hhmm(ms)}`;
    };
    const renderHist = async () => {
      for (const t of hist.querySelectorAll(".chTab")) {
        const on = t.dataset.kind === kind;
        t.classList.toggle("isActive", on);
        t.setAttribute("aria-selected", String(on));
      }
      let rows = [];
      if (kind === "pvp") {
        const pvp = (await invoke("copilot_pvp").catch(() => [])) || [];
        const record = new Map(pvp.map((p) => [p.name, p]));
        const fights = ((await invoke("get_fight_history").catch(() => [])) || [])
          .filter((f) => !f.isLive && f.isPvp)
          .sort((a, b) => b.startTimeMs - a.startTimeMs)
          .slice(0, 20);
        rows = fights.map((f) => {
          const p = record.get(f.bossName);
          const score = p ? ` · ⚔ ${p.kills || 0} ☠ ${p.deaths || 0}` : "";
          return item(f.bossName || "—", `${when(f.startTimeMs)} · ${mmss(f.durationMs)}${score}`, compact(f.totalDamage), () =>
            invoke("request_details_view", { payload: { kind: "fight", fightId: f.id } }),
          );
        });
        const fought = new Set(fights.map((f) => f.bossName));
        rows.push(...pvp.filter((p) => !fought.has(p.name)).slice(0, 20).map((p) => item(p.name, when(p.lastMs), `⚔ ${p.kills || 0} · ☠ ${p.deaths || 0}`)));
      } else {
        if (!worldBossCodes) worldBossCodes = new Set((await invoke("copilot_field_boss_codes").catch(() => [])) || []);
        const fights = ((await invoke("get_fight_history").catch(() => [])) || [])
          .filter((f) => !f.isLive && kindOf(f) === kind)
          .sort((a, b) => b.startTimeMs - a.startTimeMs)
          .slice(0, 20);
        rows = fights.map((f) => {
          const dps = f.durationMs > 0 ? (f.totalDamage / f.durationMs) * 1000 : 0;
          const tag = kind === "world" && worldBossCodes.has(f.mobCode) ? " · chefe mundial" : "";
          return item(f.bossName || "—", `${when(f.startTimeMs)} · ${mmss(f.durationMs)}${tag}`, `${compact(dps)}/s`, () =>
            invoke("request_details_view", { payload: { kind: "fight", fightId: f.id } }),
          );
        });
      }
      if (!rows.length) {
        const li = document.createElement("li");
        li.className = "chEmpty";
        li.textContent = HIST_EMPTY[kind];
        rows = [li];
      }
      list.replaceChildren(...rows);
    };
    const openHist = (open) => {
      hist.hidden = !open;
      histBtn.setAttribute("aria-expanded", String(open));
      if (open) renderHist();
      window.javaBridge?.updateOverlaySize?.();
    };
    histBtn.addEventListener("click", () => openHist(hist.hidden));
    for (const t of hist.querySelectorAll(".chTab")) {
      t.addEventListener("click", () => {
        kind = t.dataset.kind;
        store.set("copilot.histKind", kind);
        renderHist();
      });
    }
    $(".chAll").addEventListener("click", () => {
      openHist(false);
      document.querySelector(".historyBtn")?.click();
    });
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return;
      if (!hist.hidden) openHist(false);
      if (!menu.hidden) openMenu(false);
    });
    // ---------- hover card: every skill with its share ----------
    const esc = (t) => String(t ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
    const pct = (x) => `${(Number(x) || 0).toLocaleString(lang(), { maximumFractionDigits: 1, minimumFractionDigits: 1 })}%`;
    const PLACEHOLDER = "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    const baseRender = app.renderHoverTooltip.bind(app);
    app.renderHoverTooltip = (details, row, rowEl) => {
      const el = app.hoverTooltipEl;
      if (!el || !rowEl) return;
      const skills = Array.isArray(details?.skills) ? details.skills : [];
      const state = details?.state;
      if (!skills.length && state) return baseRender(details, row, rowEl);
      const total = skills.reduce((n, s) => n + (Number(s?.dmg) || 0), 0) || Number(row?.totalDamage) || 0;
      const body = skills
        .map((s, i) => {
          const dmg = Number(s?.dmg) || 0;
          const hits = Number(s?.time) || 0;
          const crit = hits > 0 ? (Number(s?.crit) || 0) / hits : 0;
          const share = total > 0 ? (dmg / total) * 100 : 0;
          return `<tr style="--share:${share.toFixed(1)}%"><td class="i">${i + 1}</td>`
            + `<td class="n"><img class="skillIcon isPlaceholder" alt="" src="${PLACEHOLDER}" onerror="window.skillIcons&&window.skillIcons.handleImgError&&window.skillIcons.handleImgError(this)"><span>${esc(s?.name || "-")}</span></td>`
            + `<td class="v">${compact(dmg)}</td><td class="v c">${s?.countForTotals === false ? "—" : pct(crit * 100)}</td>`
            + `<td class="v">${hits}</td><td class="v">${pct(share)}</td></tr>`;
        })
        .join("");
      const icon = row?.job ? `<img class="ctIcon" src="./assets/${esc(row.job)}.png" alt="" onerror="this.style.display='none'">` : "";
      el.classList.add("copilotCard");
      el.innerHTML = `
        <div class="ctHead">${icon}<span class="ctName">${esc(row?.name || "-")}</span></div>
        <div class="ctNums">
          <div class="ctDps"><b>${compact(row?.dps)}/s</b><small>DPS</small></div>
          <div><small>Dano</small><b>${compact(row?.totalDamage)}</b></div>
          <div><small>Parte</small><b>${pct(row?.damageContribution)}</b></div>
        </div>
        <div class="ctTable"><table>
          <thead><tr><th></th><th>Habilidade</th><th>Dano</th><th>% crít.</th><th>Golpes</th><th>Parte</th></tr></thead>
          <tbody>${body || `<tr><td colspan="6" class="ctEmpty">Sem habilidades nesta luta ainda.</td></tr>`}</tbody>
        </table></div>
        <div class="ctFoot">${skills.length} habilidade${skills.length === 1 ? "" : "s"}${skills.length > 8 ? " · a roda do mouse rola a lista" : ""}</div>`;
      if (window.skillIcons?.applyIconToImage) {
        el.querySelectorAll(".ctTable .skillIcon").forEach((img, i) => skills[i] && window.skillIcons.applyIconToImage(img, skills[i]));
      }
      el.classList.add("isVisible");
      el.setAttribute("aria-hidden", "false");
      app.scheduleHoverTooltipPosition();
    };

    window.copilotView = { setMetric, openHist, renderHist };
    return true;
  };
  if (!hook()) {
    const timer = setInterval(() => hook() && clearInterval(timer), 200);
  }
})();

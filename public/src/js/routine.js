// aion-copilot: Routine overlay — the character playing, countdowns to the
// game's resets, "Agora" (Odyle energy, time in the Abyss, Abyss points, kinah
// earned today, the daily attendance, the missions and the arena, read from
// the game: routineAuto.js) and a daily and weekly checklist that clears
// itself at the resets (the attendance, missions and arena items fill
// themselves from what the game says). The checklist is kept in this window's
// localStorage; nothing leaves the PC.
(() => {
  const KEY = "copilot.routine.v1";
  const DAY = 86_400_000;
  // Global servers: 07:00 UTC daily, Wednesdays weekly (timersCore.js).
  const { lastDailyReset, lastWeeklyReset } = window.copilotTimers;

  const invoke = (cmd, args) => {
    const fn = window.__TAURI__?.core?.invoke;
    return fn ? fn(cmd, args).catch(() => {}) : Promise.resolve();
  };

  const DEFAULTS = [
    ["Missões de desafio", "daily", 5],
    ["Limite de Kinah", "daily", 0],
    ["Suprimento: emergência", "daily", 10],
    ["Presença diária", "daily", 0],
    ["Fabricar Energia de Od", "weekly", 13],
    ["Comprar Pedras de Fenda", "weekly", 7],
    ["Suprimento: semanal", "weekly", 10],
    ["Arena", "weekly", 80],
    ["Campo de batalha", "weekly", 6],
  ];

  const newId = () => Math.random().toString(36).slice(2, 10);
  const makeItem = (label, scope, max) => ({ id: newId(), label, scope, max: max || 0, count: 0, done: false });

  const load = () => {
    try {
      const raw = JSON.parse(localStorage.getItem(KEY) || "null");
      if (raw && Array.isArray(raw.items)) return raw;
    } catch {
      // fall through to defaults
    }
    const now = Date.now();
    return {
      items: DEFAULTS.map(([label, scope, max]) => makeItem(label, scope, max)),
      lastDaily: lastDailyReset(now),
      lastWeekly: lastWeeklyReset(now),
    };
  };

  let state = load();
  const save = () => {
    try {
      localStorage.setItem(KEY, JSON.stringify(state));
    } catch {
      // storage blocked: the list still works for this session
    }
  };
  // The starting list is kept at once, so the Painel shows it too.
  try {
    if (!localStorage.getItem(KEY)) save();
  } catch {
    // storage blocked
  }

  // Clear what a reset passed since the last look.
  const applyResets = (now = Date.now()) => {
    const daily = lastDailyReset(now);
    const weekly = lastWeeklyReset(now);
    let changed = false;
    const clear = (scope) => {
      for (const item of state.items) {
        if (item.scope === scope && (item.done || item.count)) {
          item.done = false;
          item.count = 0;
        }
      }
      changed = true;
    };
    if ((state.lastDaily || 0) < daily) {
      clear("daily");
      state.lastDaily = daily;
    }
    if ((state.lastWeekly || 0) < weekly) {
      clear("weekly");
      state.lastWeekly = weekly;
    }
    if (changed) save();
    return changed;
  };

  const countdown = (ms) => {
    const totalMin = Math.max(0, Math.floor(ms / 60_000));
    const d = Math.floor(totalMin / 1440);
    const h = Math.floor((totalMin % 1440) / 60);
    const m = totalMin % 60;
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
    return `${m}m`;
  };

  const dailyInEl = document.querySelector(".dailyIn");
  const weeklyInEl = document.querySelector(".weeklyIn");
  // "Não perca": tasks still open close to their reset.
  const DAILY_WARN_MS = 3 * 3600_000;
  const WEEKLY_WARN_MS = 24 * 3600_000;
  const warnEl = document.querySelector(".naoPerca");
  const pendingOf = (scope) => state.items.filter((item) => item.scope === scope && !item.done);
  const updateWarnings = (now) => {
    const lines = [];
    const warned = [];
    const dailyLeft = lastDailyReset(now) + DAY - now;
    const weeklyLeft = lastWeeklyReset(now) + 7 * DAY - now;
    const daily = pendingOf("daily");
    const weekly = pendingOf("weekly");
    if (daily.length && dailyLeft <= DAILY_WARN_MS) {
      warned.push(...daily);
      lines.push(`${daily.length} diária${daily.length > 1 ? "s" : ""} pendente${daily.length > 1 ? "s" : ""} · reset em ${countdown(dailyLeft)}`);
    }
    if (weekly.length && weeklyLeft <= WEEKLY_WARN_MS) {
      warned.push(...weekly);
      lines.push(`${weekly.length} semana${weekly.length > 1 ? "is" : "l"} pendente${weekly.length > 1 ? "s" : ""} · reset em ${countdown(weeklyLeft)}`);
    }
    warnEl.hidden = lines.length === 0;
    warnEl.querySelector(".naoPercaText").textContent = lines.join("\n");
    warnEl.title = warned.map((item) => item.label).join(", ");
  };

  // The clock, replaceable from DevTools to check the warnings.
  const clockNow = () => window.copilotNowOverride ?? Date.now();
  const tickCountdowns = () => {
    const now = clockNow();
    if (applyResets(now)) render();
    dailyInEl.textContent = countdown(lastDailyReset(now) + DAY - now);
    weeklyInEl.textContent = countdown(lastWeeklyReset(now) + 7 * DAY - now);
    updateWarnings(now);
  };
  window.copilotRoutineTick = tickCountdowns;

  const svg = (path) =>
    `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${path}</svg>`;

  const render = () => {
    // Only the checklists: the "Agora" group has no .items and its own badge.
    for (const group of document.querySelectorAll(".group[data-scope]")) {
      const scope = group.dataset.scope;
      const list = group.querySelector(".items");
      const items = state.items.filter((item) => item.scope === scope);
      list.replaceChildren(
        ...items.map((item) => {
          const li = document.createElement("li");
          li.className = "item";
          li.classList.toggle("isDone", item.done);
          if (item.max > 0) {
            li.innerHTML =
              `<button type="button" class="step minus" aria-label="Menos">${svg('<path d="M6 12h12"/>')}</button>` +
              `<span class="label"></span>` +
              `<span class="count"></span>` +
              `<button type="button" class="step plus" aria-label="Mais">${svg('<path d="M12 6v12M6 12h12"/>')}</button>`;
            li.querySelector(".count").textContent = `${item.count}/${item.max}`;
            li.querySelector(".minus").addEventListener("click", () => step(item, -1));
            li.querySelector(".plus").addEventListener("click", () => step(item, 1));
          } else {
            li.innerHTML =
              `<button type="button" class="check" role="checkbox" aria-checked="${item.done}">${item.done ? svg('<path d="M5 12l5 5 9-10"/>') : ""}</button>` +
              `<span class="label"></span>`;
            li.querySelector(".check").addEventListener("click", () => {
              item.done = !item.done;
              save();
              render();
            });
          }
          li.querySelector(".label").textContent = item.label;
          const del = document.createElement("button");
          del.type = "button";
          del.className = "del";
          del.setAttribute("aria-label", `Remover ${item.label}`);
          del.innerHTML = svg('<path d="M6 6l12 12M18 6L6 18"/>');
          del.addEventListener("click", () => {
            state.items = state.items.filter((other) => other !== item);
            save();
            render();
          });
          li.appendChild(del);
          return li;
        }),
      );
      const done = items.filter((item) => item.done).length;
      if (typeof updateWarnings === "function") updateWarnings(clockNow());
      group.querySelector(".groupCount").textContent = items.length ? `${done}/${items.length}` : "";
    }
  };

  const step = (item, delta) => {
    item.count = Math.max(0, Math.min(item.max, item.count + delta));
    item.done = item.count >= item.max;
    save();
    render();
  };

  // "Arena 0/80" or "Arena 80" makes a counter; anything else a checkbox.
  document.querySelector(".add").addEventListener("submit", (event) => {
    event.preventDefault();
    const input = document.querySelector(".newTask");
    const text = input.value.trim();
    if (!text) return;
    const match = text.match(/^(.*?)\s+(?:\d+\s*\/\s*)?(\d+)$/);
    const label = (match ? match[1] : text).trim() || text;
    const max = match ? Number(match[2]) : 0;
    state.items.push(makeItem(label, document.querySelector(".newScope").value, max));
    input.value = "";
    save();
    render();
  });

  // "Agora": what the game says (copilot_routine_state), refreshed every 5 s.
  const auto = window.copilotRoutineAuto;
  const charEl = document.querySelector(".charName");
  const kickerEl = document.querySelector(".kicker");
  const raList = document.querySelector(".raList");
  const sourceEl = document.querySelector(".raSource");
  const liveEl = document.querySelector(".autoLive");
  const devBox = document.querySelector(".devCounters");
  let autoState = null;
  const renderAuto = () => {
    const now = clockNow();
    // The character, under a small "Rotina"; just "Rotina" before any is known.
    charEl.textContent = autoState?.name || "Rotina";
    kickerEl.hidden = !autoState?.name;
    const rows = auto.render(raList, autoState, now);
    const source = auto.sourceLine(autoState, now);
    sourceEl.hidden = !source;
    sourceEl.textContent = source;
    liveEl.textContent = rows.length ? (autoState.live ? "ao vivo" : "salvo") : "";
    liveEl.classList.toggle("isLive", !!autoState?.live && rows.length > 0);
    // Developer mode only: every counter as the game sent it, and the supply
    // boards (which field says "delivered" isn't confirmed: both guesses).
    const dev = autoState?.devCounters;
    devBox.hidden = !Array.isArray(dev);
    if (Array.isArray(dev)) {
      const window_ = (b) => {
        const span = b.endMs - b.startMs;
        return span <= 26 * 3600_000 ? "diário" : span <= 8 * 86_400_000 ? "semanal" : "sem fim";
      };
      const supply = (autoState.devSupply || []).map((b) => {
        const li = document.createElement("li");
        li.textContent = `Suprimento ${b.slot} (${window_(b)}): ${b.requests} pedidos · bit ${b.flagged} · valor ${b.withValue} (a confirmar)`;
        return li;
      });
      devBox.querySelector(".devList").replaceChildren(...supply, ...dev.map((c) => {
        const li = document.createElement("li");
        const parts = [`${c.count}${c.charged ? ` (+${c.charged})` : ""}`];
        if (c.timeMs || c.chargedMs) parts.push(`${auto.hours(c.timeMs)}${c.chargedMs ? ` (+${auto.hours(c.chargedMs)})` : ""}`);
        li.textContent = `${c.id}: ${parts.join(" · ")}`;
        return li;
      }));
    }
  };

  // What the game says fills the checklist: the attendance ticks its item,
  // the missions done today and the arena matches this week raise their
  // counters (never lowered or unticked: the player's own count stays).
  const tickFromGame = () => {
    const now = clockNow();
    const attended = auto.attendanceDone(autoState, now);
    const raise = {
      daily: [[/^miss(õ|o)es de (dever|desafio)/i, auto.missionsToday(autoState, now)]],
      weekly: [[/^arena/i, auto.arenaThisWeek(autoState, now)]],
    };
    let changed = false;
    for (const item of state.items) {
      if (attended && item.scope === "daily" && !item.done && /^presen[çc]a/i.test(item.label)) {
        item.done = true;
        changed = true;
      }
      for (const [re, n] of raise[item.scope] || []) {
        if (!re.test(item.label) || !(n > item.count)) continue;
        if (item.max > 0) {
          item.count = Math.min(item.max, n);
          item.done = item.count >= item.max;
        } else {
          item.done = true;
        }
        changed = true;
      }
    }
    if (changed) {
      save();
      render();
    }
  };
  const loadAuto = async () => {
    const next = await invoke("copilot_routine_state");
    if (next && typeof next === "object") autoState = next;
    renderAuto();
    tickFromGame();
  };

  applyResets();
  render();
  tickCountdowns();
  renderAuto();
  loadAuto();
  setInterval(tickCountdowns, 15_000);
  setInterval(loadAuto, 5_000);
})();

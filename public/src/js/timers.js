// aion-copilot: Timers overlay — the game's scheduled events (from the
// schedule shipped with the app), the field bosses' next returns and the
// resets, in one list: what is on now first, then the soonest. Times are shown
// in the Windows clock; the rules themselves are worked out in timersCore.js.
(() => {
  const { invoke } = window.copilotOverlay;
  const T = window.copilotTimers;
  const BOSSES_KEY = "copilot.bosses.v1";
  const SOON_MS = 5 * 60_000;
  const lang = document.documentElement.lang || undefined;
  const list = document.querySelector(".timers");
  const empty = document.querySelector(".empty");
  const region = document.querySelector(".region");

  // 24px stroke drawings, one per kind of event.
  const ICON = {
    festa: '<path d="M12 3l1.8 4.7 4.7 1.8-4.7 1.8L12 16l-1.8-4.7-4.7-1.8 4.7-1.8z"/><path d="M19 15l.8 2.2 2.2.8-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z"/>',
    portal: '<ellipse cx="12" cy="12" rx="6.5" ry="9"/><ellipse cx="12" cy="12" rx="2.5" ry="5"/>',
    rift: '<path d="M13 2l-3 6 4 3-4 5 3 6"/><path d="M6 5c-2 4-2 10 0 14M18 5c2 4 2 10 0 14"/>',
    eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
    siege: '<path d="M4 21V8h3V5h3v3h4V5h3v3h3v13z"/><path d="M10 21v-5a2 2 0 0 1 4 0v5"/>',
    crown: '<path d="M3 7l4 4 5-7 5 7 4-4-2 12H5z"/><path d="M5 21h14"/>',
    guardian: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/><path d="M9 12l2 2 4-4"/>',
    boss: '<path d="M12 3c-4.4 0-7.5 3-7.5 7 0 2.4 1.1 4.2 2.8 5.3V19a1 1 0 0 0 1 1h7.4a1 1 0 0 0 1-1v-3.7c1.7-1.1 2.8-2.9 2.8-5.3 0-4-3.1-7-7.5-7z"/><circle cx="9.2" cy="11" r="1.6"/><circle cx="14.8" cy="11" r="1.6"/>',
    reset: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  };
  const icon = (kind) =>
    `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[kind] || ICON.clock}</svg>`;

  // The field bosses are shared with the Bosses overlay and the Painel through
  // localStorage, so a reading seen here outlives a restart and reaches them.
  const readBosses = () => {
    try {
      return Object.values(JSON.parse(localStorage.getItem(BOSSES_KEY) || "{}") || {});
    } catch {
      return [];
    }
  };
  const saveBosses = (known) => {
    try {
      localStorage.setItem(BOSSES_KEY, JSON.stringify(known));
    } catch {
      // kept for this session only
    }
  };
  const sameBoss = (a, b) => !!a && !!b && a.seenMs === b.seenMs && a.up === b.up && a.timeMs === b.timeMs && a.name === b.name;

  let schedule = null;
  let tried = false;
  let bosses = {};

  // "21:00" today, "seg 21:00" on another day, in the Windows clock.
  const at = (ms, now) => {
    const d = new Date(ms);
    const time = d.toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });
    if (d.toDateString() === new Date(now).toDateString()) return time;
    return `${d.toLocaleDateString(lang, { weekday: "short" }).replace(/\.$/, "")} ${time}`;
  };
  const full = (ms) => new Date(ms).toLocaleString(lang, { weekday: "long", day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });

  const row = (r, now) => {
    const li = document.createElement("li");
    li.className = "timer";
    li.classList.toggle("isNow", r.state === "active");
    li.classList.toggle("isReset", r.state === "reset");
    li.classList.toggle("isBoss", r.icon === "boss");
    const ico = document.createElement("span");
    ico.className = "ico";
    ico.innerHTML = icon(r.icon);
    // The name may be cut short; the "aprox." mark next to it never is, and
    // the tooltip always has the whole name.
    const name = document.createElement("span");
    name.className = "name";
    const label = document.createElement("span");
    label.className = "label";
    label.textContent = r.name;
    name.append(label);
    name.title = r.name;
    if (r.confidence === "low") {
      const approx = document.createElement("span");
      approx.className = "approx";
      approx.textContent = "aprox.";
      name.append(approx);
      name.title = `${r.name} · horário ainda sem confirmação`;
    }
    if (r.icon === "boss") name.title = `${r.name} · chefe de campo${r.seenMs ? `, lido às ${at(r.seenMs, now)}` : ""}`;
    const time = document.createElement("span");
    time.className = "at";
    const left = document.createElement("span");
    left.className = "left";
    li.append(ico, name, time, left);
    if (r.state === "active") {
      time.textContent = at(r.start, now);
      time.title = `Começou: ${full(r.start)} · termina às ${at(r.end, now)}`;
      left.textContent = "agora";
      // With an entry window it counts to the entry closing (after that the
      // row shows the next start).
      const sub = document.createElement("span");
      sub.className = "sub";
      const until = r.entryEnd ?? r.end;
      const mins = Math.max(1, Math.ceil((until - now) / 60_000));
      sub.textContent = r.entryEnd ? `Entrada aberta, fecha em ${mins} min` : `Acontecendo agora, faltam ${mins} min`;
      if (r.entryEnd) sub.title = `Entrada até ${at(r.entryEnd, now)} · termina às ${at(r.end, now)}`;
      li.append(sub);
    } else {
      time.textContent = at(r.start, now);
      time.title = full(r.start);
      left.textContent = T.countdown(r.start - now);
      li.classList.toggle("isSoon", r.start - now <= SOON_MS);
    }
    return li;
  };
  const head = (text) => Object.assign(document.createElement("li"), { className: "secHead", textContent: text });

  // The clock, replaceable from DevTools (like the Routine's) to check the list.
  const clockNow = () => window.copilotNowOverride ?? Date.now();
  const render = () => {
    const now = clockNow();
    const rules = schedule?.rules || [];
    const rows = T.buildRows(rules, Object.values(bosses), now);
    // The field bosses and the resets do not need the schedule, so the list
    // shows as soon as it was asked for, even if it did not come.
    list.hidden = !tried;
    empty.hidden = tried;
    if (!tried) return;
    const parts = [];
    if (!rules.length) {
      parts.push(Object.assign(document.createElement("li"), {
        className: "note",
        textContent: schedule ? "Nenhum evento cadastrado." : "Os horários dos eventos não carregaram; tentando de novo…",
      }));
    }
    const section = (title, state) => {
      const rs = rows.filter((r) => r.state === state);
      if (rs.length) parts.push(head(title), ...rs.map((r) => row(r, now)));
    };
    section("Agora", "active");
    section("Próximos", "next");
    section("Resets", "reset");
    list.replaceChildren(...parts);
  };

  const refresh = async () => {
    if (!schedule) {
      const s = await invoke("copilot_timer_rules");
      if (s && Array.isArray(s.rules)) {
        schedule = s;
        region.textContent = s.region ? `· ${s.region}` : "";
        let zone = "";
        try {
          zone = Intl.DateTimeFormat().resolvedOptions().timeZone || "";
        } catch {
          // no zone name: the times still follow the Windows clock
        }
        region.title = `Horários no relógio do Windows${zone ? ` (${zone})` : ""}`;
      }
    }
    // The saved readings, then whatever the game sent since; saved back when
    // that changed anything (merged with a fresh read, as the Bosses overlay
    // may have written meanwhile).
    bosses = T.mergeBosses(bosses, readBosses());
    const fresh = await invoke("copilot_field_bosses");
    if (Array.isArray(fresh) && fresh.length) {
      bosses = T.mergeBosses(bosses, fresh);
      const saved = T.mergeBosses({}, readBosses());
      if (Object.values(bosses).some((b) => !sameBoss(b, saved[b.id]))) saveBosses(T.mergeBosses(saved, Object.values(bosses)));
    }
    tried = true;
    render();
  };
  window.copilotTimersRender = render;
  refresh();
  setInterval(refresh, 5000);
})();

// aion-copilot: the Routine's automatic part — Odyle energy, time in the Abyss,
// the week's Abyss points, kinah earned today, the daily attendance, the
// missions done today and the arenas (rating, matches this week), read from
// the game (copilot_routine_state) —
// for the Routine overlay and the Painel. `describe` turns the state into
// rows (pure); `render` draws them. Needs timersCore.js for the weekly reset.
(() => {
  const MIN = 60_000;
  const HOUR = 3_600_000;
  // Older than this, a row says when it was read.
  const STALE_MS = 5 * MIN;
  const lang = "pt-BR";

  const num = (n, digits = 0) => Number(n || 0).toLocaleString(lang, { maximumFractionDigits: digits });
  // 35,31k · 1,5M, as the game writes big numbers.
  const compact = (n) => {
    const v = Number(n) || 0;
    if (v >= 1e6) return `${num(v / 1e6, 2)}M`;
    if (v >= 1e4) return `${num(v / 1e3, 2)}k`;
    return num(v);
  };
  // 3 h 28 min · 1 h · 12 min
  const hours = (ms) => {
    const total = Math.max(0, Math.floor(ms / MIN));
    const h = Math.floor(total / 60);
    const m = total % 60;
    if (h && m) return `${h} h ${m} min`;
    if (h) return `${h} h`;
    return `${m} min`;
  };
  const ago = (ms, now) => {
    const diff = Math.max(0, now - ms);
    if (diff < MIN) return "agora";
    if (diff < HOUR) return `há ${Math.floor(diff / MIN)} min`;
    if (diff < 48 * HOUR) return `há ${hours(diff)}`;
    return `há ${Math.floor(diff / (24 * HOUR))} dias`;
  };
  const ARENA_NAMES = { 1: "Solo", 2: "Cooperação", 3: "Tática" };
  const pct = (part, whole) => (whole > 0 ? Math.max(0, Math.min(1, part / whole)) : 0);

  // How much a running total moved since `reset`: 0 when it last moved
  // before it; else the total less its value at that reset, when known;
  // null when it isn't (first read after it moved).
  const since = (tally, reset) => {
    if (!tally) return null;
    if ((tally.lastMs || 0) < reset) return 0;
    if (tally.resetMs === reset && tally.start !== null && tally.start !== undefined) return Math.max(0, tally.total - tally.start);
    return null;
  };
  const missionsToday = (state, now = Date.now()) =>
    since(state?.missions, window.copilotTimers?.lastDailyReset?.(now) ?? 0);
  const arenaThisWeek = (state, now = Date.now()) =>
    since(state?.arenaPlayed, window.copilotTimers?.lastWeeklyReset?.(now) ?? 0);

  // One row per item the game has sent: name, big value, "/ max", bar (0..1),
  // tone ("" | "warn" | "done" | "out") and the line under it.
  const describe = (state, now = Date.now()) => {
    const rows = [];
    if (!state) return rows;
    const stale = (seen) => (seen && now - seen > STALE_MS ? ` · atualizado ${ago(seen, now)}` : "");

    const o = state.odyle;
    if (o) {
      const max = o.max || 840;
      const full = o.basic >= max;
      rows.push({
        key: "odyle",
        name: "Energia de Odyle",
        value: num(o.basic),
        extra: o.charged > 0 ? `(+${num(o.charged)})` : "",
        max: `/ ${num(max)}`,
        fill: pct(o.basic, max),
        tone: full ? "warn" : "",
        sub: (full ? "Cheia: gaste para não desperdiçar" : `faltam ${num(max - o.basic)} para encher`) +
          (o.charged > 0 ? ` · +${num(o.charged)} carregada` : "") + stale(o.seenMs),
        seenMs: o.seenMs,
      });
    }

    const t = state.abyssTime;
    if (t) {
      const cap = t.capMs || 7 * HOUR;
      const left = t.remainingMs + t.chargedMs;
      rows.push({
        key: "abyssTime",
        name: "Tempo no Abismo",
        value: hours(t.remainingMs),
        extra: t.chargedMs > 0 ? `(+${hours(t.chargedMs)})` : "",
        max: `/ ${hours(cap)}`,
        fill: pct(t.remainingMs, cap),
        tone: left <= 0 ? "out" : left < 30 * MIN ? "warn" : "",
        sub: (left <= 0 ? "Esgotado" : `pode ficar mais ${hours(left)} no Abismo`) + stale(t.seenMs),
        seenMs: t.seenMs,
      });
    }

    const p = state.abyssPoints;
    if (p) {
      // The game only says on a gain: read before this week's reset, it is 0 now.
      const lastWeekly = window.copilotTimers?.lastWeeklyReset?.(now) ?? 0;
      const reset = p.seenMs < lastWeekly;
      const gained = reset ? 0 : p.gained;
      const ratio = pct(gained, p.limit);
      rows.push({
        key: "abyssPoints",
        name: "Pontos do Abismo",
        value: compact(gained),
        extra: "",
        max: `/ ${compact(p.limit)}`,
        fill: ratio,
        tone: gained >= p.limit ? "done" : ratio >= 0.9 ? "warn" : "",
        sub: reset
          ? "Zerou no reset semanal"
          : (gained >= p.limit ? "Limite da semana atingido" : `faltam ${compact(p.limit - gained)} nesta semana`) + stale(p.seenMs),
        seenMs: p.seenMs,
      });
    }

    // Kinah earned today: the game says it at login only, and not its limit.
    const lastDaily = window.copilotTimers?.lastDailyReset?.(now) ?? 0;
    const k = state.kinah;
    if (k) {
      const reset = k.seenMs < lastDaily;
      rows.push({
        key: "kinah",
        name: "Kinah ganho hoje",
        value: reset ? "0" : num(k.earned),
        extra: "",
        max: "",
        fill: null,
        tone: "",
        sub: reset ? "Zerou no reset diário" : `lido ao entrar no jogo${stale(k.seenMs)}`,
        seenMs: k.seenMs,
      });
    }

    // Attendance: marked today when an event's last day is past the reset.
    const a = state.attendance;
    if (a && Array.isArray(a.events) && a.events.length) {
      const today = a.events.filter((e) => e.lastMs >= lastDaily);
      const toClaim = a.events.reduce((sum, e) => sum + Math.max(0, e.days - e.claimed), 0);
      const done = today.length > 0;
      const days = today.map((e) => `dia ${e.days}`).join(" · ");
      rows.push({
        key: "attendance",
        name: "Presença diária",
        value: done ? "Feita" : "Pendente",
        extra: "",
        max: "",
        fill: null,
        tone: done ? "done" : "warn",
        sub: (done ? `marcada hoje (${days})` : "ainda não marcada hoje") +
          (toClaim > 0 ? ` · ${toClaim} recompensa${toClaim > 1 ? "s" : ""} a resgatar` : "") + stale(a.seenMs),
        seenMs: a.seenMs,
      });
    }

    // Missions done today, out of the running total the game keeps.
    const m = state.missions;
    if (m) {
      const today = missionsToday(state, now);
      rows.push({
        key: "missions",
        name: "Missões de desafio",
        value: today === null ? "?" : num(today),
        extra: "",
        max: "hoje",
        fill: null,
        tone: "",
        sub: (today === null ? "a contagem de hoje volta no próximo login" : `concluídas hoje · ${num(m.total)} no total`) + stale(m.seenMs),
        seenMs: m.seenMs,
      });
    }

    // Arena: the rating of the one played most (the solo one before any),
    // wins and losses, and the others that were played; the matches played
    // this week first.
    const ar = state.arena;
    const records = Array.isArray(ar?.records) ? ar.records : [];
    const week = arenaThisWeek(state, now);
    const weekLine = week === null ? "" : `${num(week)} partida${week === 1 ? "" : "s"} nesta semana`;
    if (!records.length && state.arenaPlayed) {
      rows.push({
        key: "arena",
        name: "Arena",
        value: week === null ? "?" : num(week),
        extra: "",
        max: "na semana",
        fill: null,
        tone: "",
        sub: (weekLine || "a contagem da semana volta no próximo login") + stale(state.arenaPlayed.seenMs),
        seenMs: state.arenaPlayed.seenMs,
      });
    }
    if (records.length) {
      const games = (r) => (r.wins || 0) + (r.losses || 0);
      const main = [...records].sort((x, y) => games(y) - games(x) || x.arena - y.arena)[0];
      const label = (r) => ARENA_NAMES[r.arena] || `Arena ${r.arena}`;
      const line = (r) => {
        const n = games(r);
        if (!n) return `${label(r)} · nenhuma partida`;
        return `${label(r)} · ${num(r.wins)} V / ${num(r.losses)} D (${Math.round((100 * r.wins) / n)}%)`;
      };
      const others = records.filter((r) => r !== main && games(r) > 0).map((r) => `${label(r)} ${num(r.rating)}`);
      // A match ended since the login said the ratings: they are the login's.
      const played = state.arenaPlayed;
      const outdated = !!played && (played.lastMs || 0) > ar.seenMs;
      const seen = Math.max(ar.seenMs || 0, played?.seenMs || 0);
      rows.push({
        key: "arena",
        name: "Arena",
        value: num(main.rating),
        extra: "",
        max: "",
        fill: null,
        tone: "",
        sub: [weekLine, line(main) + (outdated ? " (até o login)" : ""), ...others].filter(Boolean).join(" · ") + stale(seen),
        seenMs: seen,
      });
    }
    return rows;
  };

  // Whether the game says today's attendance is marked (for the checklist).
  const attendanceDone = (state, now = Date.now()) => {
    const lastDaily = window.copilotTimers?.lastDailyReset?.(now) ?? Infinity;
    return !!state?.attendance?.events?.some((e) => e.lastMs >= lastDaily);
  };

  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  };

  // Draw the rows into `box` (a <ul>); an empty state says what to do.
  const render = (box, state, now = Date.now()) => {
    const rows = describe(state, now);
    if (!rows.length) {
      box.replaceChildren(el("li", "raEmpty", "Abra o jogo: a energia, o tempo e os pontos do Abismo, o kinah de hoje, a presença e a arena aparecem sozinhos ao entrar com o personagem."));
      return rows;
    }
    box.replaceChildren(...rows.map((r) => {
      const li = el("li", `raItem${r.tone ? ` is-${r.tone}` : ""}`);
      li.dataset.item = r.key;
      const top = el("div", "raTop");
      const val = el("span", "raVal");
      val.append(el("b", "raBig", r.value));
      if (r.extra) val.append(el("span", "raExtra", r.extra));
      val.append(el("span", "raMax", r.max));
      top.append(el("span", "raName", r.name), val);
      li.append(top);
      // No limit, no bar.
      if (r.fill !== null) {
        const bar = el("div", "raBar");
        const fill = el("div", "raFill");
        fill.style.width = `${Math.round(r.fill * 100)}%`;
        bar.append(fill);
        li.append(bar);
      }
      li.append(el("div", "raSub", r.sub));
      if (r.seenMs) li.title = `Lido do jogo em ${new Date(r.seenMs).toLocaleString(lang)}`;
      return li;
    }));
    return rows;
  };

  // A line about where the numbers come from: nothing when live and fresh.
  const sourceLine = (state, now = Date.now()) => {
    if (!state || !describe(state, now).length) return "";
    if (state.live) return "";
    const seen = Math.max(...[state.odyle, state.abyssTime, state.abyssPoints, state.kinah, state.attendance, state.arena, state.missions, state.arenaPlayed].map((x) => x?.seenMs || 0));
    return `Salvo do último acesso${state.name ? ` de ${state.name}` : ""} (atualizado ${ago(seen, now)}). Entre no jogo para atualizar.`;
  };

  window.copilotRoutineAuto = { describe, render, sourceLine, attendanceDone, missionsToday, arenaThisWeek, compact, hours, ago };
})();

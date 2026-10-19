// aion-copilot: the clock rules behind the Timers overlay, the Routine and the
// Painel — when a scheduled event starts next, whether it is on now, and the
// game's daily/weekly resets. Pure functions of a unix-ms instant (no DOM, no
// storage), so node can load this file too and check them.
(() => {
  const MIN = 60_000;
  const HOUR = 3_600_000;
  const DAY = 86_400_000;

  // Global servers: daily and weekly resets at 07:00 UTC (16:00 KST), weekly on Wednesday.
  const DAILY_RESET = { id: "reset-diario", name: "Reset diário", icon: "reset", recurrence: { kind: "daily", hour: 7, minute: 0 }, utcOffsetMin: 0 };
  const WEEKLY_RESET = { id: "reset-semanal", name: "Reset semanal", icon: "reset", recurrence: { kind: "weekly", days: [3], hour: 7, minute: 0 }, utcOffsetMin: 0 };

  const mod = (a, n) => ((a % n) + n) % n;

  // The minutes of a day (in the rule's own clock) at which it starts, ascending.
  // `weekday` is 0 = Sunday … 6 = Saturday, also in the rule's clock.
  // "everyHours" counts from `anchorHour` and assumes `every` divides 24.
  const slotsOf = (recurrence, weekday) => {
    const r = recurrence || {};
    const minute = Number(r.minute) || 0;
    switch (r.kind) {
      case "hourly":
        return Array.from({ length: 24 }, (_, h) => h * 60 + minute);
      case "everyHours": {
        const every = Number(r.every) || 0;
        if (every <= 0) return [];
        const hours = [];
        for (let h = 0; h < 24; h++) if (mod(h - (Number(r.anchorHour) || 0), every) === 0) hours.push(h * 60 + minute);
        return hours;
      }
      case "daily":
        return [(Number(r.hour) || 0) * 60 + minute];
      case "weekly":
        return Array.isArray(r.days) && r.days.includes(weekday) ? [(Number(r.hour) || 0) * 60 + minute] : [];
      default:
        return [];
    }
  };

  // Every start (unix ms) on the rule's days from `back` days before `now`'s
  // day to `ahead` days after it, ascending.
  const startsAround = (rule, now, back, ahead) => {
    const offset = (Number(rule.utcOffsetMin) || 0) * MIN;
    // Shifted so the UTC getters read the rule's wall clock.
    const today = Math.floor((now + offset) / DAY) * DAY;
    const out = [];
    for (let d = -back; d <= ahead; d++) {
      const day = today + d * DAY;
      const weekday = new Date(day).getUTCDay();
      for (const m of slotsOf(rule.recurrence, weekday)) out.push(day + m * MIN - offset);
    }
    return out;
  };

  // The latest start at or before `now`, or null.
  const lastStart = (rule, now) => {
    let last = null;
    for (const t of startsAround(rule, now, 8, 0)) if (t <= now) last = t;
    return last;
  };

  // The first start after `now`, or null.
  const nextStart = (rule, now) => startsAround(rule, now, 0, 8).find((t) => t > now) ?? null;

  // The occurrence on at `now` (start inclusive, end exclusive), or null.
  const activeAt = (rule, now) => {
    const duration = (Number(rule.durationMin) || 0) * MIN;
    const start = lastStart(rule, now);
    if (start === null || duration <= 0 || now >= start + duration) return null;
    const entry = (Number(rule.entryMin) || 0) * MIN;
    return { start, end: start + duration, entryEnd: entry > 0 ? start + entry : null };
  };

  const lastDailyReset = (now) => lastStart(DAILY_RESET, now);
  const lastWeeklyReset = (now) => lastStart(WEEKLY_RESET, now);

  // Newer readings of a map replace older ones, boss by boss (as the Bosses overlay keeps them).
  const mergeBosses = (known, fresh) => {
    const out = { ...(known || {}) };
    for (const boss of fresh || []) {
      const prev = out[boss.id];
      if (!prev || boss.seenMs >= prev.seenMs) out[boss.id] = boss;
    }
    return out;
  };

  // What the Timers overlay lists, in its order: events on now that can still
  // be joined (the soonest to close first), then the next start of every event
  // and of every dead field boss with a known return (soonest first), then the
  // resets. Once an event's entry closes it shows its next start instead.
  const buildRows = (rules, bosses, now) => {
    const active = [];
    const upcoming = [];
    for (const rule of rules || []) {
      const base = { id: rule.id, name: rule.name, icon: rule.icon, confidence: rule.confidence || "medium" };
      const on = activeAt(rule, now);
      if (on && !(on.entryEnd !== null && now >= on.entryEnd)) {
        active.push({ ...base, state: "active", ...on });
        continue;
      }
      const start = nextStart(rule, now);
      if (start !== null) upcoming.push({ ...base, state: "next", start });
    }
    for (const boss of bosses || []) {
      if (boss.up || !(boss.timeMs > now)) continue;
      upcoming.push({ id: `boss-${boss.id}`, name: boss.name, icon: "boss", confidence: "high", state: "next", start: boss.timeMs, seenMs: boss.seenMs });
    }
    active.sort((a, b) => (a.entryEnd ?? a.end) - (b.entryEnd ?? b.end) || a.name.localeCompare(b.name));
    upcoming.sort((a, b) => a.start - b.start || a.name.localeCompare(b.name));
    const resets = [DAILY_RESET, WEEKLY_RESET].map((rule) => ({
      id: rule.id, name: rule.name, icon: rule.icon, confidence: "high", state: "reset", start: nextStart(rule, now),
    }));
    return [...active, ...upcoming, ...resets];
  };

  // "45m", "1h 05m", "2d 3h"; a part of a minute counts as a minute.
  const countdown = (ms) => {
    const total = Math.max(0, Math.ceil(ms / MIN));
    const d = Math.floor(total / 1440);
    const h = Math.floor((total % 1440) / 60);
    const m = total % 60;
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
    return `${m}m`;
  };

  const api = { MIN, HOUR, DAY, DAILY_RESET, WEEKLY_RESET, slotsOf, lastStart, nextStart, activeAt, lastDailyReset, lastWeeklyReset, mergeBosses, buildRows, countdown };
  (typeof window !== "undefined" ? window : globalThis).copilotTimers = api;
})();

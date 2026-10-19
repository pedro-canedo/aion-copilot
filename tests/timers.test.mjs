import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

function timers() {
  const context = vm.createContext({});
  vm.runInContext(readFileSync(new URL("../public/src/js/timersCore.js", import.meta.url), "utf8"), context);
  return context.copilotTimers;
}

const T = timers();
const schedule = JSON.parse(readFileSync(new URL("../src/data/copilot_timers.json", import.meta.url), "utf8"));
const rule = (id) => schedule.rules.find((r) => r.id === id) ?? assert.fail(`no rule ${id}`);
const Z = (s) => Date.parse(s);
const iso = (t) => (t === null ? null : new Date(t).toISOString());
// Objects made inside the vm context have its own prototypes.
const plain = (v) => JSON.parse(JSON.stringify(v ?? null));
const DAY = 86_400_000;

// 2026-10-18 is a Sunday; the schedule's rules are in Am. do Sul time (UTC-3).
test("the reference day is a Sunday", () => {
  assert.equal(new Date(Z("2026-10-18T08:45:00Z")).getUTCDay(), 0);
});

test("hourly events start on their minute and stay on for their duration", () => {
  const festa = rule("shugo-festa");
  assert.equal(iso(T.nextStart(festa, Z("2026-10-18T08:45:00Z"))), "2026-10-18T09:00:00.000Z");
  const on = T.activeAt(festa, Z("2026-10-18T09:02:00Z"));
  assert.equal(iso(on.start), "2026-10-18T09:00:00.000Z");
  assert.equal(iso(on.end), "2026-10-18T09:10:00.000Z");
  assert.equal(iso(on.entryEnd), "2026-10-18T09:03:00.000Z");
  assert.equal(T.activeAt(festa, Z("2026-10-18T09:10:00Z")), null, "the end is exclusive");

  const invasao = rule("invasao-dimensional");
  assert.equal(iso(T.nextStart(invasao, Z("2026-10-18T08:45:00Z"))), "2026-10-18T09:30:00.000Z");
  assert.equal(iso(T.activeAt(invasao, Z("2026-10-18T08:44:00Z")).start), "2026-10-18T08:30:00.000Z");
});

test("every-3-hours events count from their anchor hour in their own clock", () => {
  // Fenda: 02, 05, 08 … BRT = 05, 08, 11 … UTC.
  const fenda = rule("fenda-espaco-temporal");
  assert.equal(iso(T.nextStart(fenda, Z("2026-10-18T05:10:00Z"))), "2026-10-18T08:00:00.000Z");
  assert.equal(iso(T.nextStart(fenda, Z("2026-10-18T05:00:00Z"))), "2026-10-18T08:00:00.000Z", "a start at now is not next");
  assert.equal(iso(T.activeAt(fenda, Z("2026-10-18T05:00:00Z")).end), "2026-10-18T05:10:00.000Z", "the start is inclusive");
  assert.equal(T.activeAt(fenda, Z("2026-10-18T05:00:00Z")).entryEnd, null, "no entry window");
  // 23:30 BRT -> 02:00 BRT the next day.
  assert.equal(iso(T.nextStart(fenda, Z("2026-10-19T02:30:00Z"))), "2026-10-19T05:00:00.000Z");

  const kairah = rule("vigilante-kairah");
  assert.equal(iso(T.nextStart(kairah, Z("2026-10-18T08:45:00Z"))), "2026-10-18T10:00:00.000Z");
  assert.equal(iso(T.activeAt(kairah, Z("2026-10-18T10:20:00Z")).start), "2026-10-18T10:00:00.000Z");
});

test("weekly events fall on their weekdays in their own clock, not in UTC", () => {
  // Mon 21:00 BRT = Tue 00:00 UTC.
  const cerco = rule("cerco-artefatos");
  assert.equal(iso(T.nextStart(cerco, Z("2026-10-18T08:45:00Z"))), "2026-10-20T00:00:00.000Z");
  assert.equal(iso(T.nextStart(cerco, Z("2026-10-19T00:00:00Z"))), "2026-10-20T00:00:00.000Z", "Mon 00:00 UTC is still Sunday in BRT");
  assert.equal(T.activeAt(cerco, Z("2026-10-19T00:05:00Z")), null, "not on Sunday 21:00 BRT");
  assert.equal(iso(T.nextStart(cerco, Z("2026-10-20T00:00:00Z"))), "2026-10-23T00:00:00.000Z", "Monday -> Thursday");
  assert.equal(iso(T.nextStart(cerco, Z("2026-10-23T00:00:00Z"))), "2026-10-25T00:00:00.000Z", "Thursday -> Saturday");
  assert.equal(iso(T.nextStart(rule("chefes-do-cerco"), Z("2026-10-18T08:45:00Z"))), "2026-10-20T00:30:00.000Z");

  const nahma = rule("senhor-guardiao-nahma");
  assert.equal(iso(T.activeAt(nahma, Z("2026-10-19T00:00:00Z")).end), "2026-10-19T00:30:00.000Z");
  assert.equal(iso(T.nextStart(nahma, Z("2026-10-19T00:00:00Z"))), "2026-10-24T00:00:00.000Z", "Sunday -> Friday");
  // A one-day-a-week rule is still found from the day after it.
  const once = { recurrence: { kind: "weekly", days: [2], hour: 10, minute: 0 }, utcOffsetMin: 0 };
  assert.equal(iso(T.nextStart(once, Z("2026-10-20T10:00:01Z"))), "2026-10-27T10:00:00.000Z");
});

test("a rule the overlay does not understand never starts", () => {
  assert.equal(T.nextStart({ recurrence: { kind: "lunar" } }, 0), null);
  assert.equal(T.lastStart({ recurrence: { kind: "everyHours", every: 0 } }, Z("2026-10-18T00:00:00Z")), null);
  assert.equal(T.activeAt({ recurrence: { kind: "hourly", minute: 0 } }, Z("2026-10-18T09:00:00Z")), null, "no duration");
  assert.deepEqual(plain(T.slotsOf({ kind: "weekly", days: [1], hour: 21 }, 0)), []);
});

test("the daily and weekly resets are at 07:00 UTC, the weekly on Wednesday", () => {
  assert.equal(iso(T.lastDailyReset(Z("2026-10-18T06:59:00Z"))), "2026-10-17T07:00:00.000Z");
  assert.equal(iso(T.lastDailyReset(Z("2026-10-18T07:00:00Z"))), "2026-10-18T07:00:00.000Z");
  assert.equal(iso(T.lastWeeklyReset(Z("2026-10-18T08:45:00Z"))), "2026-10-14T07:00:00.000Z");
  assert.equal(iso(T.lastWeeklyReset(Z("2026-10-21T06:59:00Z"))), "2026-10-14T07:00:00.000Z");
  assert.equal(iso(T.nextStart(T.WEEKLY_RESET, Z("2026-10-21T06:59:00Z"))), "2026-10-21T07:00:00.000Z");
});

test("the resets match the Routine's previous reset code all through a month", () => {
  const oldDaily = (now) => {
    const d = new Date(now);
    d.setUTCHours(7, 0, 0, 0);
    if (d.getTime() > now) d.setUTCDate(d.getUTCDate() - 1);
    return d.getTime();
  };
  const oldWeekly = (now) => {
    let t = oldDaily(now);
    while (new Date(t).getUTCDay() !== 3) t -= DAY;
    return t;
  };
  for (let t = Z("2026-10-01T00:00:00Z"); t < Z("2026-11-01T00:00:00Z"); t += 37 * 60_000) {
    assert.equal(T.lastDailyReset(t), oldDaily(t), `daily ${iso(t)}`);
    assert.equal(T.lastWeeklyReset(t), oldWeekly(t), `weekly ${iso(t)}`);
  }
});

const bosses = [
  { id: 7, name: "Chefe A", up: false, timeMs: Z("2026-10-18T09:05:00Z"), seenMs: 1, mapId: 1 },
  { id: 8, name: "Chefe vivo", up: true, timeMs: 0, seenMs: 1, mapId: 1 },
  { id: 9, name: "Chefe passou", up: false, timeMs: Z("2026-10-18T08:00:00Z"), seenMs: 1, mapId: 1 },
];

test("the list puts what is on first, then the soonest, then the resets", () => {
  const rows = T.buildRows(schedule.rules, bosses, Z("2026-10-18T09:02:00Z"));
  assert.deepEqual(plain(rows.map((r) => `${r.state}:${r.id}`)), [
    "active:shugo-festa",
    "next:boss-7",
    "next:invasao-dimensional",
    "next:vigilante-kairah",
    "next:fenda-espaco-temporal",
    "next:senhor-guardiao-nahma", // Sun 21:00 BRT tonight
    "next:cerco-artefatos",
    "next:chefes-do-cerco",
    "reset:reset-diario",
    "reset:reset-semanal",
  ]);
  assert.equal(rows.find((r) => r.id === "senhor-guardiao-nahma").confidence, "low");
  assert.equal(rows.find((r) => r.id === "boss-7").seenMs, 1);
  assert.equal(rows.find((r) => r.id === "boss-7").mapId, undefined, "map ids stay out of the rows");
});

test("an event whose entry closed shows its next start instead of being on", () => {
  // Shugo Festa: entry 3 min, runs 10.
  const rows = T.buildRows([rule("shugo-festa")], [], Z("2026-10-18T09:05:00Z"));
  assert.deepEqual(plain(rows[0]), {
    id: "shugo-festa", name: "Shugo Festa", icon: "festa", confidence: "high", state: "next", start: Z("2026-10-18T10:00:00Z"),
  });
  const open = T.buildRows([rule("shugo-festa")], [], Z("2026-10-18T09:02:59Z"))[0];
  assert.equal(open.state, "active");
  assert.equal(iso(open.entryEnd), "2026-10-18T09:03:00.000Z");
  assert.equal(T.buildRows([rule("shugo-festa")], [], Z("2026-10-18T09:03:00Z"))[0].state, "next", "the entry end is exclusive");
  // Without an entry window it stays on for its whole duration.
  assert.equal(T.buildRows([rule("fenda-espaco-temporal")], [], Z("2026-10-18T05:09:00Z"))[0].state, "active");
  // Events on now close in order of their entry, not of their end.
  const both = T.buildRows([rule("fenda-espaco-temporal"), rule("shugo-festa")], [], Z("2026-10-18T05:01:00Z"));
  assert.deepEqual(plain(both.filter((r) => r.state === "active").map((r) => r.id)), ["shugo-festa", "fenda-espaco-temporal"]);
});

test("without rules the list still has the field bosses and the resets", () => {
  const rows = T.buildRows([], bosses, Z("2026-10-18T09:02:00Z"));
  assert.deepEqual(plain(rows.map((r) => `${r.state}:${r.id}`)), ["next:boss-7", "reset:reset-diario", "reset:reset-semanal"]);
  assert.deepEqual(plain(T.buildRows(undefined, undefined, 0).map((r) => r.state)), ["reset", "reset"]);
});

test("a newer reading of a boss replaces an older one, never the other way", () => {
  const merged = T.mergeBosses({ 1: { id: 1, seenMs: 5, name: "new" } }, [{ id: 1, seenMs: 3, name: "old" }, { id: 2, seenMs: 1, name: "b" }]);
  assert.deepEqual(plain(merged), { 1: { id: 1, seenMs: 5, name: "new" }, 2: { id: 2, seenMs: 1, name: "b" } });
  assert.deepEqual(plain(T.mergeBosses({ 1: { id: 1, seenMs: 5, name: "a" } }, [{ id: 1, seenMs: 5, name: "b" }])), { 1: { id: 1, seenMs: 5, name: "b" } });
  assert.deepEqual(plain(T.mergeBosses(null, null)), {});
});

test("countdowns round a part of a minute up", () => {
  assert.deepEqual(
    [T.countdown(45 * 60_000), T.countdown(65 * 60_000), T.countdown(30_000), T.countdown(0), T.countdown(-5000), T.countdown(2 * DAY + 3 * 3_600_000)],
    ["45m", "1h 05m", "1m", "0m", "0m", "2d 3h"],
  );
});

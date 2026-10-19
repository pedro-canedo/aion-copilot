import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

function routineAuto() {
  const context = vm.createContext({});
  context.window = context;
  for (const file of ["timersCore.js", "routineAuto.js"]) {
    vm.runInContext(readFileSync(new URL(`../public/src/js/${file}`, import.meta.url), "utf8"), context);
  }
  return context.copilotRoutineAuto;
}

const R = routineAuto();
const NOW = Date.parse("2026-10-18T16:56:00Z");
const row = (state, key) => R.describe({ live: true, ...state }, NOW).find((r) => r.key === key);

// The login of 2026-10-18 (copilot.rs keeps the same values).
const ARENA = {
  records: [
    { arena: 1, rating: 1527, wins: 3, losses: 2 },
    { arena: 2, rating: 1500, wins: 0, losses: 0 },
    { arena: 3, rating: 1500, wins: 0, losses: 0 },
  ],
  seenMs: NOW - 60_000,
};

test("the arena row shows the played arena's rating, wins and losses", () => {
  const r = row({ arena: ARENA }, "arena");
  assert.equal(r.name, "Arena");
  assert.equal(r.value, "1.527");
  assert.equal(r.fill, null);
  assert.equal(r.sub, "Solo · 3 V / 2 D (60%)");
});

test("with no games yet the solo arena shows, and other played arenas follow", () => {
  const none = { records: ARENA.records.map((r) => ({ ...r, rating: 1500, wins: 0, losses: 0 })), seenMs: ARENA.seenMs };
  assert.equal(row({ arena: none }, "arena").sub, "Solo · nenhuma partida");
  const two = { records: [ARENA.records[0], { arena: 3, rating: 1612, wins: 9, losses: 1 }], seenMs: ARENA.seenMs };
  const r = row({ arena: two }, "arena");
  assert.equal(r.value, "1.612");
  assert.equal(r.sub, "Tática · 9 V / 1 D (90%) · Solo 1.527");
});

test("no arena records, no arena row; an old reading says when it was read", () => {
  assert.equal(row({ arena: { records: [], seenMs: 1 } }, "arena"), undefined);
  assert.equal(row({}, "arena"), undefined);
  const old = row({ arena: { ...ARENA, seenMs: NOW - 2 * 3_600_000 } }, "arena");
  assert.match(old.sub, /atualizado há 2 h$/);
});

// 2026-10-18 07:00 UTC, and Wednesday 2026-10-14 07:00 UTC.
const DAILY = Date.parse("2026-10-18T07:00:00Z");
const WEEKLY = Date.parse("2026-10-14T07:00:00Z");

test("the missions done today are the total less its value at the daily reset", () => {
  const missions = { total: 13, lastMs: Date.parse("2026-10-18T16:02:25Z"), seenMs: NOW - 1000, resetMs: DAILY, start: 11 };
  const r = row({ missions }, "missions");
  assert.equal(r.value, "2");
  assert.equal(r.sub, "concluídas hoje · 13 no total");
  assert.equal(R.missionsToday({ missions }, NOW), 2);
  // None since the reset: 0, whatever the start.
  assert.equal(R.missionsToday({ missions: { ...missions, lastMs: DAILY - 1, start: null } }, NOW), 0);
  // Read only after one was done, start unknown: not made up.
  const unknown = { missions: { ...missions, start: null } };
  assert.equal(R.missionsToday(unknown, NOW), null);
  assert.equal(row(unknown, "missions").value, "?");
  // A start from yesterday's reset doesn't count for today.
  assert.equal(R.missionsToday({ missions: { ...missions, resetMs: DAILY - 86_400_000 } }, NOW), null);
});

test("the arena matches this week lead the arena row, or make it alone", () => {
  const arenaPlayed = { total: 6, lastMs: Date.parse("2026-10-18T16:09:31Z"), seenMs: NOW - 1000, resetMs: WEEKLY, start: 5 };
  assert.equal(R.arenaThisWeek({ arenaPlayed }, NOW), 1);
  assert.equal(row({ arena: ARENA, arenaPlayed }, "arena").sub, "1 partida nesta semana · Solo · 3 V / 2 D (60%)");
  const alone = row({ arenaPlayed }, "arena");
  assert.equal(alone.value, "1");
  assert.equal(alone.sub, "1 partida nesta semana");
  assert.equal(R.arenaThisWeek({ arenaPlayed: { ...arenaPlayed, lastMs: WEEKLY - 1 } }, NOW), 0);
});

test("after a match the login's ratings say so, and the row is as fresh as its newest part", () => {
  const login = { ...ARENA, seenMs: NOW - 5 * 3_600_000 };
  const arenaPlayed = { total: 6, lastMs: NOW - 60_000, seenMs: NOW - 60_000, resetMs: WEEKLY, start: 5 };
  const r = row({ arena: login, arenaPlayed }, "arena");
  assert.equal(r.sub, "1 partida nesta semana · Solo · 3 V / 2 D (60%) (até o login)");
  assert.equal(r.seenMs, arenaPlayed.seenMs);
});

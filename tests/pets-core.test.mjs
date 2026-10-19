import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

function petsCore() {
  const context = vm.createContext({});
  context.window = context;
  vm.runInContext(readFileSync(new URL("../public/src/js/petsCore.js", import.meta.url), "utf8"), context);
  return context.copilotPetsCore;
}

const P = petsCore();

test("a pet is called by your name, else by a species known to give it, else by its id", () => {
  assert.equal(P.petLabel({ pet: 1048, name: "Fofo", speciesName: "Espírito da Floresta da Serra" }), "Fofo");
  assert.equal(P.petLabel({ pet: 1048, name: null, speciesName: "Espírito da Floresta da Serra" }), "Espírito da Floresta da Serra");
  assert.equal(P.petLabel({ pet: 1048, name: null, speciesName: null }), "Mascote #1048");
});

test("each status has its chip: missing, level up, complete, collection not read, unknown", () => {
  const chip = (s) => {
    const { cls, text } = P.speciesStatus(s);
    return `${cls}: ${text}`;
  };
  assert.equal(chip({ status: "missing", pet: 1048, level: 0 }), "missing: Falta na coleção");
  assert.equal(chip({ status: "upgrade", pet: 1048, level: 1 }), "upgrade: Nv.1 · upar");
  assert.equal(chip({ status: "upgrade", pet: 1048, level: 2 }), "upgrade: Nv.2 · upar");
  assert.equal(chip({ status: "complete", pet: 1048, level: 3 }), "complete: Completo");
  assert.equal(chip({ status: "unread", pet: 1048, level: null }), "unread: Nv.? · coleção não lida");
  assert.equal(chip({ status: "unknown", pet: null }), "unknown: Mascote ?");
  // Anything else reads as unknown, and says how a pet gets known without
  // claiming the creature gives one.
  assert.equal(P.speciesStatus({}).cls, "unknown");
  const unknown = P.speciesStatus({ status: "unknown" });
  assert.match(unknown.title, /alma cai logo depois de você derrotar/);
  assert.match(unknown.title, /se esta criatura dá mascote/);
  assert.doesNotMatch(`${unknown.text} ${unknown.title}`, /desconhecido|qual mascote/);
});

test("the line under a species names its pet and your souls of it", () => {
  assert.equal(P.speciesSub({ pet: 1048, petName: "Fofo", level: 1, souls: 3, estimated: true }), "Fofo · ~3 almas");
  assert.equal(P.speciesSub({ pet: 1048, petName: null, level: 0, souls: 1, estimated: false }), "Mascote #1048 · 1 alma");
  assert.equal(P.speciesSub({ pet: 1048, petName: null, level: null, souls: null }), "Mascote #1048");
  // Named after this very species: the id says more.
  assert.equal(P.speciesSub({ name: "Zaif da Colina", pet: 1100, petName: "Zaif da Colina", level: 2, souls: 0 }), "Mascote #1100 · 0 almas");
  // No pet known: not even whether it gives one.
  assert.equal(P.speciesSub({ pet: null }), "ainda não se sabe se dá mascote");
  assert.equal(P.soulsText(1234, false), "1.234 almas");
});

test("the redraw key leaves out the ages that tick every second", () => {
  const view = (age) => ({
    game: true,
    nearby: [{ pet: 1291, seenAgoMs: age }],
    species: [{ code: 2100555, count: 2, seenAgoMs: age, status: "unknown" }],
  });
  assert.equal(P.viewKey(view(100)), P.viewKey(view(5_000)));
  const more = view(100);
  more.species[0].count = 3;
  assert.notEqual(P.viewKey(view(100)), P.viewKey(more));
  // An older backend without species still keys.
  assert.equal(typeof P.viewKey({ game: true, nearby: [] }), "string");
  assert.equal(P.viewKey(null), "");
});

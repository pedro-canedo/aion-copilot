import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const SOURCE = readFileSync(new URL("../public/src/js/copilotBus.js", import.meta.url), "utf8");
// Objects made inside the page's context compare by value only once copied out.
const plain = (v) => JSON.parse(JSON.stringify(v));

// The bus over a fake app: a settings map, the commands it answers and the
// events it can send.
function inApp(settings = {}) {
  const calls = [];
  const listeners = new Map();
  const emit = (event, payload) => {
    for (const cb of listeners.get(event) || []) cb({ payload });
  };
  const context = vm.createContext({ console });
  context.window = context;
  context.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        calls.push([cmd, args]);
        if (cmd === "get_settings") return { ...settings };
        if (cmd === "update_settings") {
          if (settings[args.key] !== args.value) {
            settings[args.key] = args.value;
            emit("setting-changed", { key: args.key, value: args.value });
          }
          return null;
        }
        if (cmd === "copilot_store_get") return { rev: 3, doc: { a: 1 } };
        if (cmd === "copilot_store_put") return args.baseRev + 1;
        throw new Error(`unknown command ${cmd}`);
      },
    },
    event: {
      listen: async (event, cb) => {
        if (!listeners.has(event)) listeners.set(event, new Set());
        listeners.get(event).add(cb);
        return () => listeners.get(event).delete(cb);
      },
    },
  };
  vm.runInContext(SOURCE, context);
  return { bus: context.copilotBus, calls, emit };
}

test("settings are asked for once, read with a fallback, blank as unset", async () => {
  const { bus, calls } = inApp({ "copilot.sound.preset": "ping", "copilot.blank": "" });
  assert.equal(bus.settings.peek("copilot.sound.preset", "sino"), "sino");
  assert.equal(await bus.settings.get("copilot.sound.preset", "sino"), "ping");
  assert.equal(await bus.settings.get("copilot.blank", "x"), "x");
  assert.equal(await bus.settings.get("copilot.none", "y"), "y");
  assert.equal(calls.filter(([c]) => c === "get_settings").length, 1);
});

test("a set is told once here, sent as text, and its echo is not told again", async () => {
  const { bus, calls } = inApp();
  await bus.settings.ready();
  const heard = [];
  bus.settings.on("copilot.sound.volume", (v) => heard.push(v));
  await bus.settings.set("copilot.sound.volume", 0.5);
  assert.deepEqual(plain(calls.at(-1)), ["update_settings", { key: "copilot.sound.volume", value: "0.5" }]);
  assert.deepEqual(heard, ["0.5"]);
  assert.equal(bus.settings.peek("copilot.sound.volume"), "0.5");
});

test("a change from another window is heard; one removed reads as unset", async () => {
  const { bus, emit } = inApp({ "copilot.update.mode": "auto" });
  await bus.settings.ready();
  const heard = [];
  const stop = bus.settings.on("copilot.update.mode", (v) => heard.push(v));
  emit("setting-changed", { key: "copilot.update.mode", value: "off" });
  emit("setting-changed", { key: "copilot.update.mode", value: "off" });
  emit("setting-changed", { key: "copilot.update.mode", value: "" });
  stop();
  emit("setting-changed", { key: "copilot.update.mode", value: "notify" });
  assert.deepEqual(heard, ["off", undefined]);
  assert.equal(bus.settings.peek("copilot.update.mode", "auto"), "notify");
});

test("the store passes its arguments through and hears only its own document", async () => {
  const { bus, calls, emit } = inApp();
  assert.deepEqual(plain(await bus.store.get("checklist")), { rev: 3, doc: { a: 1 } });
  assert.equal(await bus.store.put("checklist", { a: 2 }, 3), 4);
  assert.deepEqual(plain(calls.at(-1)), ["copilot_store_put", { name: "checklist", doc: { a: 2 }, baseRev: 3 }]);
  const heard = [];
  const stop = bus.store.subscribe("checklist", (p) => heard.push(p.rev));
  await new Promise((r) => setTimeout(r, 0));
  emit("copilot-store-changed", { name: "auras", rev: 9 });
  emit("copilot-store-changed", { name: "checklist", rev: 4 });
  stop();
  emit("copilot-store-changed", { name: "checklist", rev: 5 });
  assert.deepEqual(heard, [4]);
});

test("outside the app: fallbacks, an empty store, a write that fails, and nothing throws", async () => {
  const context = vm.createContext({ console });
  context.window = context;
  vm.runInContext(SOURCE, context);
  const bus = context.copilotBus;
  assert.equal(bus.inApp, false);
  assert.equal(await bus.settings.get("copilot.sound.preset", "sino"), "sino");
  await bus.settings.set("copilot.sound.preset", "ping");
  assert.equal(bus.settings.peek("copilot.sound.preset", "sino"), "ping");
  assert.deepEqual(plain(await bus.store.get("checklist")), { rev: 0, doc: null });
  await assert.rejects(bus.store.put("checklist", {}, 0));
  const stop = bus.store.subscribe("checklist", () => {});
  stop();
});

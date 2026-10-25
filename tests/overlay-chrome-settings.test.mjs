import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const BUS = readFileSync(new URL("../public/src/js/copilotBus.js", import.meta.url), "utf8");
const CHROME = readFileSync(new URL("../public/src/js/overlayChrome.js", import.meta.url), "utf8");

const element = () => {
  const handlers = {};
  const classes = new Set();
  return {
    handlers,
    value: "",
    title: "",
    innerHTML: "",
    attrs: {},
    style: { props: {}, setProperty(k, v) { this.props[k] = v; } },
    classList: {
      toggle: (c, on) => (on ? classes.add(c) : classes.delete(c)),
      contains: (c) => classes.has(c),
    },
    setAttribute(k, v) { this.attrs[k] = v; },
    addEventListener(type, cb) { handlers[type] = cb; },
  };
};

// An overlay page (panel, transparency slider, lock) over a fake app holding
// `settings`, with `local` as its localStorage. Resolves once the settings
// have arrived.
async function overlay({ settings = {}, local = {}, label = "group" } = {}) {
  const panel = element();
  const opacityInput = element();
  const lockBtn = element();
  const body = element();
  const sent = [];
  const listeners = new Map();
  const context = vm.createContext({ console });
  context.window = context;
  context.location = { pathname: "/whatever.html" };
  context.localStorage = {
    getItem: (k) => (k in local ? local[k] : null),
    setItem: (k, v) => (local[k] = String(v)),
  };
  context.document = {
    body,
    querySelector: (sel) => ({ ".panel": panel, ".opacityInput": opacityInput, ".lockBtn": lockBtn })[sel] || null,
    addEventListener() {},
  };
  context.__TAURI__ = {
    window: { getCurrentWindow: () => ({ label }) },
    core: {
      invoke: async (cmd, args) => {
        if (cmd === "get_settings") return { ...settings };
        if (cmd === "update_settings") {
          sent.push([args.key, args.value]);
          if (settings[args.key] !== args.value) {
            settings[args.key] = args.value;
            for (const cb of listeners.get("setting-changed") || []) cb({ payload: { key: args.key, value: args.value } });
          }
        }
        return null;
      },
    },
    event: {
      listen: async (event, cb) => {
        if (!listeners.has(event)) listeners.set(event, new Set());
        listeners.get(event).add(cb);
        return () => {};
      },
    },
  };
  vm.runInContext(BUS, context);
  vm.runInContext(CHROME, context);
  const shown = { opacity: () => opacityInput.value, locked: () => body.classList.contains("locked") };
  const firstPaint = { opacity: shown.opacity(), locked: shown.locked() };
  await context.copilotBus.settings.ready();
  await new Promise((r) => setTimeout(r, 0));
  const emit = (key, value) => {
    for (const cb of listeners.get("setting-changed") || []) cb({ payload: { key, value } });
  };
  return { firstPaint, shown, settings, local, sent, opacityInput, lockBtn, emit, label: context.copilotOverlay.label };
}

test("the label is the window's, not the page's", async () => {
  const o = await overlay({ label: "copilot-banner" });
  assert.equal(o.label, "copilot-banner");
});

test("1.0.0's localStorage values are taken over once, shown from the first paint, and left in place", async () => {
  const o = await overlay({ local: { "copilot.group.opacity": "30", "copilot.group.locked": "true" } });
  assert.deepEqual(o.firstPaint, { opacity: "30", locked: true });
  assert.equal(o.settings["copilot.ov.group.opacity"], "30");
  assert.equal(o.settings["copilot.ov.group.locked"], "true");
  assert.equal(o.settings["copilot.ov.migrated.group"], "1");
  assert.equal(o.local["copilot.group.opacity"], "30");
  assert.equal(o.local["copilot.group.locked"], "true");
});

test("once migrated, the settings win over the old localStorage values", async () => {
  const o = await overlay({
    settings: { "copilot.ov.migrated.group": "1", "copilot.ov.group.opacity": "80", "copilot.ov.group.locked": "false" },
    local: { "copilot.group.opacity": "30", "copilot.group.locked": "true" },
  });
  assert.equal(o.shown.opacity(), "80");
  assert.equal(o.shown.locked(), false);
  assert.equal(o.sent.length, 0, "nothing written back");
  // Kept here too, so the next start paints 80 straight away.
  assert.equal(o.local["copilot.ov.group.opacity"], "80");
});

test("a setting that exists is not overwritten by the migration", async () => {
  const o = await overlay({ settings: { "copilot.ov.group.opacity": "70" }, local: { "copilot.group.opacity": "30" } });
  assert.equal(o.settings["copilot.ov.group.opacity"], "70");
  assert.equal(o.shown.opacity(), "70");
  assert.equal(o.settings["copilot.ov.migrated.group"], "1");
});

test("the player's changes are saved; the slider when let go; a change from elsewhere is applied", async () => {
  const o = await overlay({ settings: { "copilot.ov.migrated.group": "1" } });
  assert.equal(o.shown.opacity(), "55");
  o.opacityInput.value = "40";
  o.opacityInput.handlers.input();
  assert.equal(o.settings["copilot.ov.group.opacity"], undefined, "not on every step");
  o.opacityInput.handlers.change();
  assert.equal(o.settings["copilot.ov.group.opacity"], "40");
  o.lockBtn.handlers.click();
  assert.equal(o.shown.locked(), true);
  assert.equal(o.settings["copilot.ov.group.locked"], "true");
  o.emit("copilot.ov.group.locked", "false");
  o.emit("copilot.ov.group.opacity", "90");
  assert.equal(o.shown.locked(), false);
  assert.equal(o.shown.opacity(), "90");
});

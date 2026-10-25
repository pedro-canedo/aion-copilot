// aion-copilot: what every copilot window shares with the others through the
// app. `settings` is the Rust settings map (settings.json; a change reaches
// every window as "setting-changed"), for what the backend reads or several
// windows show. `store` is the document store (copilot_store.rs) for what the
// player writes: a checklist, auras, alerts. Values in the map are text.
//
// Outside the app (a page opened in a browser) nothing is kept: settings read
// as their fallback, the store as empty, and a write to it fails.
(() => {
  const tauri = window.__TAURI__;
  const inApp = !!tauri?.core?.invoke;
  const invoke = (cmd, args) => (inApp ? tauri.core.invoke(cmd, args) : Promise.reject(new Error("fora do app")));
  // Resolves to a function that stops listening (a no-op outside the app).
  const listen = (event, cb) => {
    const fn = tauri?.event?.listen;
    if (!fn) return Promise.resolve(() => {});
    return fn(event, (e) => cb(e?.payload)).catch(() => () => {});
  };

  // ── settings ─────────────────────────────────────────────────────────────
  // Blank counts as unset, as in Rust (`copilot_update::setting`): a key the
  // backend removes is announced with value "".
  const unset = (v) => v === undefined || v === null || v === "";
  const cache = {};
  const watchers = new Map(); // key -> Set of callbacks
  let loaded = null;

  const tell = (key, value) => {
    for (const cb of watchers.get(key) || []) {
      try {
        cb(unset(value) ? undefined : value, key);
      } catch (e) {
        console.warn("[copilot] setting", key, e);
      }
    }
  };
  // A change from any window, this one's own echo included: told once, when
  // the value is new here.
  listen("setting-changed", (p) => {
    if (typeof p?.key !== "string") return;
    const value = p.value == null ? "" : String(p.value);
    if ((cache[p.key] ?? "") === value) return;
    cache[p.key] = value;
    tell(p.key, value);
  });

  // The whole map, asked once; what changed meanwhile (the events) wins.
  const ready = () =>
    (loaded ??= invoke("get_settings")
      .then((all) => {
        for (const [k, v] of Object.entries(all || {})) if (!(k in cache)) cache[k] = String(v);
      })
      .catch(() => {})
      .then(() => cache));

  const settings = {
    ready,
    // What is known now, without waiting (fallback before `ready`).
    peek: (key, fallback) => (unset(cache[key]) ? fallback : cache[key]),
    get: (key, fallback) => ready().then(() => settings.peek(key, fallback)),
    // Applied here at once (the callbacks of `on` run); the other windows
    // hear it from the backend.
    set: (key, value) => {
      const text = value == null ? "" : String(value);
      if ((cache[key] ?? "") !== text) {
        cache[key] = text;
        tell(key, text);
      }
      return invoke("update_settings", { key, value: text }).catch(() => {});
    },
    // cb(value, key) on every change of `key` (value undefined when unset).
    // Returns a function that stops it.
    on: (key, cb) => {
      if (!watchers.has(key)) watchers.set(key, new Set());
      watchers.get(key).add(cb);
      return () => watchers.get(key)?.delete(cb);
    },
  };

  // ── document store ───────────────────────────────────────────────────────
  const store = {
    // {rev, doc}: rev 0 and doc null when there is none yet.
    get: (name) => (inApp ? invoke("copilot_store_get", { name }) : Promise.resolve({ rev: 0, doc: null })),
    // The new rev. Fails with "stale rev: …" when another window wrote it
    // since `baseRev`: read it again and redo the change on top.
    put: (name, doc, baseRev) => invoke("copilot_store_put", { name, doc, baseRev }),
    // cb({name, rev}) whenever `name` is written, from any window. Returns a
    // function that stops it.
    subscribe: (name, cb) => {
      let stop = null;
      let stopped = false;
      listen("copilot-store-changed", (p) => {
        if (!stopped && p?.name === name) cb(p);
      }).then((fn) => {
        stop = fn;
        if (stopped) fn();
      });
      return () => {
        stopped = true;
        stop?.();
      };
    },
  };

  window.copilotBus = { inApp, invoke, listen, settings, store };
})();

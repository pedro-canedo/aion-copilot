// aion-copilot: Map overlay — where you are on the world's map (tc-imba's
// tiles, the art NCSOFT's; see mapTiles.js) with the layers you pick: the
// bundled markers, the places learned from your own sightings (drawn hollow,
// "aprendido") and what is in view right now. copilot_map_state is asked
// every second and only answers when something changed (`since`); the layers
// picked, the zoom and where you left the map are kept per world.
(() => {
  const { invoke } = window.copilotOverlay;
  const tiles = window.copilotMapTiles;
  const TILE = 1024;
  const MAX_ZOOM = 4;
  const DEFAULT_ZOOM = 0.35;
  const DEFAULT_LAYERS = ["fieldBoss", "named", "pet", "village", "battlefield", "occupation", "teleport", "seal", "dungeon"];
  // Full tiles kept decoded; the rest stay as 256 px thumbnails for far zooms.
  const MAX_FULL_TILES = 24;
  const THUMB = 256;
  const RETRY_TILE_MS = 30_000;

  // Lucide-style 24 px icons, one per layer glyph; paths only, so the canvas
  // can draw them too (Path2D).
  const circle = (cx, cy, r) => `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
  const GLYPHS = {
    crown: "M3 18h18M4 7l4.5 5L12 5l3.5 7L20 7l-1.5 11h-13z",
    star: "M12 3l2.8 5.7 6.2.9-4.5 4.4 1.1 6.2L12 17.3l-5.6 2.9 1.1-6.2L3 9.6l6.2-.9z",
    home: "M3 11l9-8 9 8M5 9.5V21h5v-6h4v6h5V9.5",
    swords: "M14.5 17.5L3 6V3h3l11.5 11.5M13 19l6-6M16 16l4 4M19 21l2-2M14.5 6.5L18 3h3v3l-3.5 3.5M5 14l4 4M7 17l-3 3M3 19l2 2",
    flag: "M5 21V4M5 4h12l-2.5 4.5L17 13H5",
    portal: `${circle(12, 12, 9)}M12 7c2 0 3.5 2.2 3.5 5S14 17 12 17s-3.5-2.2-3.5-5S10 7 12 7z`,
    seal: "M12 2l2.4 2.6 3.5-.4.4 3.5L21 10l-2 2 2 2-2.7 2.3-.4 3.5-3.5-.4L12 22l-2.4-2.6-3.5.4-.4-3.5L3 14l2-2-2-2 2.7-2.3.4-3.5 3.5.4zM9 12l2 2 4-4",
    door: "M5 21V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16M3 21h18M14.5 12h.01",
    monolith: "M9 21V6l3-3 3 3v15M6 21h12M12 8v8",
    cube: "M12 2l9 5v10l-9 5-9-5V7zM3 7l9 5 9-5M12 12v10",
    crystal: "M12 2l5 6-5 14-5-14zM7 8h10",
    ore: "M4 17l3-8 6-3 6 5 1 7-8 2zM7 9l5 4 7-2M12 13v7",
    wood: "M5 8h12a3 4 0 0 1 0 8H5a3 4 0 0 1 0-8zM17 10.5a1.2 1.5 0 1 0 0 3a1.2 1.5 0 1 0 0-3",
    gem: "M6 3h12l4 6-10 12L2 9zM2 9h20M9 3l3 6 3-6M12 9v12",
    herb: "M12 21V10M12 10C12 6 9 3 5 3c0 4 3 7 7 7zM12 14c0-3 3-6 7-6 0 4-3 6-7 6z",
    leaf: "M5 21c0-9 5-15 15-16-1 10-7 15-15 16zM5 21l8-8",
    food: "M12 7c-2-2-7-1.5-7 4.5S8 21 10 21c1 0 1.2-.6 2-.6s1 .6 2 .6c2 0 5-3.5 5-9.5S14 5 12 7zM12 7c0-2 1-3.5 3-4",
    bag: "M6 8h12l1 13H5zM9 8V6a3 3 0 0 1 6 0v2",
    box: "M3 8l9-5 9 5v8l-9 5-9-5zM3 8l9 5 9-5M12 13v8",
    person: `${circle(12, 7, 4)}M4 21c0-4.4 3.6-7 8-7s8 2.6 8 7`,
    paw: `M12 13c3 0 5.5 3 5.5 5.5 0 1.6-1.4 2.5-3 2.5-1 0-1.6-.5-2.5-.5s-1.5.5-2.5.5c-1.6 0-3-.9-3-2.5C6.5 16 9 13 12 13z${circle(5, 10, 1.8)}${circle(9, 5.5, 1.8)}${circle(15, 5.5, 1.8)}${circle(19, 10, 1.8)}`,
    scroll: "M7 3h12v14a4 4 0 0 1-4 4H5a2 2 0 0 1-2-2v-2h12M7 3a2 2 0 0 0-2 2v12M10 8h6M10 12h6",
  };
  const glyphSvg = (glyph, size = 14) =>
    `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${GLYPHS[glyph] || circle(12, 12, 5)}"/></svg>`;
  const paths = new Map();
  const glyphPath = (glyph) => {
    if (!paths.has(glyph)) paths.set(glyph, new Path2D(GLYPHS[glyph] || circle(12, 12, 5)));
    return paths.get(glyph);
  };

  const store = {
    get: (key, fallback) => {
      try {
        const v = localStorage.getItem(key);
        return v === null ? fallback : JSON.parse(v);
      } catch {
        return fallback;
      }
    },
    set: (key, value) => {
      try {
        localStorage.setItem(key, JSON.stringify(value));
      } catch {
        // kept for this session only
      }
    },
  };

  const $ = (sel) => document.querySelector(sel);
  const wrap = $(".mapWrap");
  const canvas = $(".mapCanvas");
  const ctx = canvas.getContext("2d");
  const worldName = $(".worldName");
  const worldBtn = $(".worldBtn");
  const worldMenu = $(".worldMenu");
  const layersBtn = $(".layersBtn");
  const layersCount = $(".layersCount");
  const layersPanel = $(".layersPanel");
  const groupsEl = $(".lpGroups");
  const followBtn = $(".followBtn");
  const note = $(".mapNote");
  const empty = $(".mapEmpty");
  const tip = $(".mapTip");
  const askEl = $(".mapAsk");
  const devLine = $(".devLine");
  const credit = $(".credit");
  credit.title = $(".accuracyNote").textContent;

  // ===== State =====
  let live = null; // the last copilot_map_state
  let seq = null;
  let world = null; // WorldView of what is shown
  let catalogue = null; // copilot_map_layers of that world
  let selected = new Set();
  let markers = {};
  let learnedKey = "";
  let pinned = null; // the world picked by hand (the app keeps it), null = told by the game
  let follow = store.get("copilot.map.follow", true) !== false;
  const view = { cx: 0, cy: 0, scale: DEFAULT_ZOOM };
  let size = { w: 0, h: 0, dpr: 1 };
  let hits = [];
  let asking = null; // the object being told: { code, variant, layer, px, py }
  let dev = false; // developer mode: the object's code in "O que é isto?"
  invoke("copilot_dev_mode").then((v) => {
    dev = v === true;
  });

  const layerOf = (id) => catalogue?.groups.flatMap((g) => g.layers).find((l) => l.id === id);
  // An object of these layers (or "object") can be told what it is.
  const ASK_GROUPS = ["recurso", "colecionaveis"];
  const askable = (id) => id === "object" || ASK_GROUPS.includes(catalogue?.groups.find((g) => g.layers.some((l) => l.id === id))?.id);
  // A gathering point is told by its code and variant (which resource it
  // is; the app sends the variant only for those), anything else by its code.
  const askKey = (ask) => (ask.variant != null ? `${ask.code}:${ask.variant}` : `${ask.code}`);

  // ===== Time and words =====
  const minutes = (ms) => Math.max(1, Math.ceil(ms / 60_000));
  const ago = (ms) => {
    const s = Math.max(0, Math.round(ms / 1000));
    if (s < 60) return s < 5 ? "agora" : `há ${s} s`;
    if (s < 3600) return `há ${Math.round(s / 60)} min`;
    if (s < 86_400) return `há ${Math.round(s / 3600)} h`;
    const d = Math.round(s / 86_400);
    return `há ${d} dia${d === 1 ? "" : "s"}`;
  };
  const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

  // ===== View =====
  const fitScale = () => (world ? Math.min(size.w / world.width, size.h / world.height) : 0.05);
  const minScale = () => Math.min(DEFAULT_ZOOM, fitScale() * 0.8);
  const clampView = () => {
    view.scale = Math.min(MAX_ZOOM, Math.max(minScale(), view.scale));
    if (!world) return;
    view.cx = Math.min(world.width, Math.max(0, view.cx));
    view.cy = Math.min(world.height, Math.max(0, view.cy));
  };
  const toScreen = (px, py) => [(px - view.cx) * view.scale + size.w / 2, (py - view.cy) * view.scale + size.h / 2];
  const toMap = (sx, sy) => [(sx - size.w / 2) / view.scale + view.cx, (sy - size.h / 2) / view.scale + view.cy];
  const me = () => (live?.me && live.me.px != null && world && live.world?.id === world.id ? live.me : null);

  let saving = 0;
  const saveView = () => {
    if (!world) return;
    clearTimeout(saving);
    const id = world.id;
    const kept = { zoom: view.scale, cx: Math.round(view.cx), cy: Math.round(view.cy) };
    saving = setTimeout(() => store.set(`copilot.map.view.${id}`, kept), 400);
  };
  const setFollow = (on) => {
    follow = on;
    followBtn.setAttribute("aria-pressed", String(on));
    followBtn.title = on ? "Centralizado em você (arraste o mapa para soltar)" : "Centralizar em mim";
    store.set("copilot.map.follow", on);
    if (on) centreOnMe();
  };
  const centreOnMe = () => {
    const m = me();
    if (!m) return;
    view.cx = m.px;
    view.cy = m.py;
    redraw();
  };

  // ===== Tiles =====
  // Per tile: "loading" | "ok" (full image) | "thumb" (only the small copy) | "fail".
  // The tile's data: URL is not kept: a thumbnail made full again asks for it
  // again (from the app's map-tiles/ folder).
  const tileCache = new Map();
  const fullOrder = [];
  const keepFull = (key, t) => {
    const at = fullOrder.indexOf(key);
    if (at >= 0) fullOrder.splice(at, 1);
    fullOrder.push(key);
    while (fullOrder.length > MAX_FULL_TILES) {
      const old = tileCache.get(fullOrder.shift());
      if (old?.state === "ok") {
        old.img = null;
        old.state = "thumb";
      }
    }
    t.state = "ok";
  };
  const decode = (key, t, url) => {
    const img = new Image();
    img.addEventListener("load", () => {
      t.img = img;
      if (!t.thumb) {
        const c = document.createElement("canvas");
        c.width = THUMB;
        c.height = THUMB;
        c.getContext("2d").drawImage(img, 0, 0, THUMB, THUMB);
        t.thumb = c;
      }
      keepFull(key, t);
      redraw();
    });
    img.addEventListener("error", () => {
      t.state = t.thumb ? "thumb" : "fail";
      t.at = Date.now();
      t.retryAt = t.at + RETRY_TILE_MS;
    });
    img.src = url;
  };
  const request = (key, t, worldId, x, y) => {
    t.state = "loading";
    tiles.get(worldId, x, y).then((url) => {
      // Forgotten meanwhile (another world shown): nothing to draw it on.
      if (tileCache.get(key) !== t) return;
      if (!url) {
        t.state = t.thumb ? "thumb" : "fail";
        t.at = Date.now();
        t.retryAt = t.at + RETRY_TILE_MS;
        redraw();
        return;
      }
      decode(key, t, url);
    });
  };
  // What to draw for a tile at this zoom, asking for it when missing.
  const tileImage = (worldId, x, y, wantFull) => {
    const key = `${worldId}/${x}/${y}`;
    let t = tileCache.get(key);
    if (!t || (t.state === "fail" && Date.now() - t.at > RETRY_TILE_MS)) {
      t = { state: "loading", img: null, thumb: null, at: Date.now(), retryAt: 0 };
      tileCache.set(key, t);
      request(key, t, worldId, x, y);
    } else if (t.state === "thumb" && wantFull && Date.now() >= t.retryAt) {
      request(key, t, worldId, x, y);
    } else if (t.state === "ok") {
      keepFull(key, t);
    }
    if (wantFull && t.img) return t.img;
    return t.thumb || t.img;
  };
  const forgetTiles = (keepWorld) => {
    for (const key of [...tileCache.keys()]) {
      if (!key.startsWith(`${keepWorld}/`)) tileCache.delete(key);
    }
    for (let i = fullOrder.length - 1; i >= 0; i--) {
      if (!tileCache.has(fullOrder[i])) fullOrder.splice(i, 1);
    }
  };

  // ===== Drawing =====
  let frame = 0;
  const redraw = () => {
    if (!frame) frame = requestAnimationFrame(draw);
  };

  const drawGrid = (step, alpha) => {
    const [x0, y0] = toMap(0, 0);
    const [x1, y1] = toMap(size.w, size.h);
    const maxX = world ? world.width : x1;
    const maxY = world ? world.height : y1;
    const minX = world ? 0 : x0;
    const minY = world ? 0 : y0;
    ctx.strokeStyle = `rgba(143, 211, 255, ${alpha})`;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let gx = Math.max(minX, Math.floor(x0 / step) * step); gx <= Math.min(maxX, x1); gx += step) {
      const [sx] = toScreen(gx, 0);
      const [, sy0] = toScreen(0, Math.max(minY, y0));
      const [, sy1] = toScreen(0, Math.min(maxY, y1));
      ctx.moveTo(Math.round(sx) + 0.5, sy0);
      ctx.lineTo(Math.round(sx) + 0.5, sy1);
    }
    for (let gy = Math.max(minY, Math.floor(y0 / step) * step); gy <= Math.min(maxY, y1); gy += step) {
      const [, sy] = toScreen(0, gy);
      const [sx0] = toScreen(Math.max(minX, x0), 0);
      const [sx1] = toScreen(Math.min(maxX, x1), 0);
      ctx.moveTo(sx0, Math.round(sy) + 0.5);
      ctx.lineTo(sx1, Math.round(sy) + 0.5);
    }
    ctx.stroke();
  };

  const drawTiles = () => {
    const set = world.tiles;
    if (!set) return 0;
    const [x0, y0] = toMap(0, 0);
    const [x1, y1] = toMap(size.w, size.h);
    const tx0 = Math.max(0, Math.floor(x0 / TILE));
    const ty0 = Math.max(0, Math.floor(y0 / TILE));
    const tx1 = Math.min(set.nx - 1, Math.floor(x1 / TILE));
    const ty1 = Math.min(set.ny - 1, Math.floor(y1 / TILE));
    // Full pictures only up close, and only as many as are kept decoded.
    const visible = (tx1 - tx0 + 1) * (ty1 - ty0 + 1);
    const wantFull = view.scale * size.dpr > THUMB / TILE && visible <= MAX_FULL_TILES - 4;
    const side = TILE * view.scale;
    let drawn = 0;
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    for (let ty = ty0; ty <= ty1; ty++) {
      for (let tx = tx0; tx <= tx1; tx++) {
        const img = tileImage(world.id, tx, ty, wantFull);
        if (!img) continue;
        const [sx, sy] = toScreen(tx * TILE, ty * TILE);
        // A hair larger, so no seam shows between tiles.
        ctx.drawImage(img, Math.floor(sx), Math.floor(sy), Math.ceil(side) + 1, Math.ceil(side) + 1);
        drawn++;
      }
    }
    return drawn;
  };

  const markerRadius = () => (view.scale >= 0.6 ? 9 : view.scale >= 0.22 ? 6 : 4);

  const drawMarker = (x, y, r, color, glyph, learnedOne) => {
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    if (learnedOne) {
      ctx.fillStyle = "rgba(10, 16, 21, 0.75)";
      ctx.fill();
      ctx.setLineDash([2.5, 2]);
      ctx.lineWidth = 1.6;
      ctx.strokeStyle = color;
      ctx.stroke();
      ctx.setLineDash([]);
    } else {
      ctx.fillStyle = color;
      ctx.fill();
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = "rgba(6, 10, 13, 0.85)";
      ctx.stroke();
    }
    if (r >= 9) {
      const k = (r * 1.25) / 24;
      ctx.save();
      ctx.translate(x - 12 * k, y - 12 * k);
      ctx.scale(k, k);
      ctx.lineWidth = 2.6;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.strokeStyle = learnedOne ? color : "#0b1216";
      ctx.stroke(glyphPath(glyph));
      ctx.restore();
    }
  };

  // A name under its marker, unless it would cover one already written.
  const label = (text, x, y, taken) => {
    ctx.font = "600 11px Sora, 'Segoe UI', sans-serif";
    const w = ctx.measureText(text).width + 6;
    const box = [x - w / 2, y, x + w / 2, y + 14];
    if (taken.some((b) => box[0] < b[2] && b[0] < box[2] && box[1] < b[3] && b[1] < box[3])) return;
    taken.push(box);
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    ctx.lineWidth = 3;
    ctx.strokeStyle = "rgba(6, 10, 13, 0.85)";
    ctx.strokeText(text, x, y);
    ctx.fillStyle = "#eaf3f8";
    ctx.fillText(text, x, y);
  };

  const drawMe = (m) => {
    const [x, y] = toScreen(m.px, m.py);
    const pxPerM = TILE / (world.sectors[4] / 100);
    const exact = m.accuracy === "exact";
    const color = m.accuracy === "unknown" ? "#9fb7c4" : getComputedStyle(document.documentElement).getPropertyValue("--me").trim() || "#ff9f43";
    if (!exact && m.radiusM) {
      const rr = Math.max(14, m.radiusM * pxPerM * view.scale);
      ctx.beginPath();
      ctx.arc(x, y, rr, 0, Math.PI * 2);
      ctx.fillStyle = "rgba(255, 159, 67, 0.12)";
      ctx.fill();
      ctx.setLineDash([4, 3]);
      ctx.lineWidth = 1.2;
      ctx.strokeStyle = "rgba(255, 159, 67, 0.7)";
      ctx.stroke();
      ctx.setLineDash([]);
    }
    if (m.heading != null && Number.isFinite(m.heading)) {
      // Degrees from +X towards +Y, which on this map is screen right/down.
      const a = (m.heading * Math.PI) / 180;
      ctx.beginPath();
      ctx.moveTo(x + Math.cos(a) * 17, y + Math.sin(a) * 17);
      ctx.lineTo(x + Math.cos(a + 2.5) * 8, y + Math.sin(a + 2.5) * 8);
      ctx.lineTo(x + Math.cos(a - 2.5) * 8, y + Math.sin(a - 2.5) * 8);
      ctx.closePath();
      ctx.fillStyle = "rgba(255, 159, 67, 0.85)";
      ctx.fill();
    }
    ctx.beginPath();
    ctx.arc(x, y, 9, 0, Math.PI * 2);
    ctx.fillStyle = "rgba(255, 159, 67, 0.22)";
    ctx.fill();
    ctx.beginPath();
    ctx.arc(x, y, 6, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = "#fff";
    ctx.stroke();
    hits.push({ x, y, r: 10, me: true, tip: () => meTip(m) });
  };

  function draw() {
    frame = 0;
    const { w, h, dpr } = size;
    if (!w || !h) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    hits = [];
    if (!world) {
      // No world: a quiet grid behind the empty state.
      const saved = { ...view };
      view.cx = 0;
      view.cy = 0;
      view.scale = 0.25;
      drawGrid(TILE / 4, 0.06);
      Object.assign(view, saved);
      return;
    }
    const [wx0, wy0] = toScreen(0, 0);
    ctx.fillStyle = "rgba(9, 15, 19, 0.85)";
    ctx.fillRect(wx0, wy0, world.width * view.scale, world.height * view.scale);
    const drawn = drawTiles();
    // The sectors; finer lines too where there is no picture.
    if (!drawn && view.scale >= 0.3) drawGrid(TILE / 4, 0.05);
    drawGrid(TILE, drawn ? 0.07 : 0.14);
    ctx.strokeStyle = "rgba(143, 211, 255, 0.35)";
    ctx.lineWidth = 1;
    ctx.strokeRect(Math.round(wx0) + 0.5, Math.round(wy0) + 0.5, Math.round(world.width * view.scale), Math.round(world.height * view.scale));

    const r = markerRadius();
    const pad = 20;
    const inView = (x, y) => x > -pad && y > -pad && x < w + pad && y < h + pad;
    const labels = [];
    // Bundled first, then learned, layer by layer in the panel's order.
    const order = catalogue ? catalogue.groups.flatMap((g) => g.layers) : [];
    for (const layer of order) {
      if (!selected.has(layer.id)) continue;
      for (const m of markers[layer.id] || []) {
        const [x, y] = toScreen(m.px, m.py);
        if (!inView(x, y)) continue;
        drawMarker(x, y, r, layer.color, layer.glyph, !!m.learned);
        // Bundled markers have no code: only what you saw can be told.
        const ask = m.learned && m.code != null && askable(layer.id) ? { code: m.code, variant: m.variant ?? null, layer: layer.id, px: m.px, py: m.py } : null;
        hits.push({ x, y, r: Math.max(r, 6), ask, tip: () => markerTip(m, layer, ask) });
        // Village names from afar, the other names up close.
        if (m.name && !m.learned && view.scale >= (layer.id === "village" ? 0.12 : 0.5)) labels.push([m.name, x, y + r + 3]);
      }
    }
    // What is in view now, of the layers picked (players have no layer).
    for (const e of live?.world?.id === world.id ? live.entities : []) {
      if (!e.layer || !selected.has(e.layer)) continue;
      const layer = layerOf(e.layer);
      const [x, y] = toScreen(e.px, e.py);
      if (!inView(x, y)) continue;
      // A pet creature: always big enough for its paw, ringed green (in view).
      if (e.pet != null) {
        drawMarker(x, y, 9, layer?.color || "#ff8fd0", layer?.glyph || "paw", false);
        ctx.beginPath();
        ctx.arc(x, y, 10.5, 0, Math.PI * 2);
        ctx.lineWidth = 2;
        ctx.strokeStyle = "#8fe0a8";
        ctx.stroke();
        hits.push({ x, y, r: 10, ask: null, tip: () => liveTip(e, layer, null) });
        continue;
      }
      ctx.beginPath();
      ctx.arc(x, y, Math.max(3.5, r - 1.5), 0, Math.PI * 2);
      ctx.fillStyle = layer?.color || "#8fe0a8";
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = "#8fe0a8";
      ctx.stroke();
      const ask = e.kind === "object" && askable(e.layer) ? { code: e.code, variant: e.variant ?? null, layer: e.layer, px: e.px, py: e.py } : null;
      hits.push({ x, y, r: 7, ask, tip: () => liveTip(e, layer, ask) });
    }
    const taken = [];
    for (const [text, x, y] of labels) label(text, x, y, taken);
    const m = me();
    if (m) drawMe(m);
    placeAsk();
  }

  // ===== Tooltips =====
  const line = (text, cls) => {
    const s = document.createElement("span");
    s.textContent = text;
    if (cls) s.className = cls;
    return s;
  };
  const meTip = (m) => {
    const b = document.createElement("b");
    b.textContent = "Você";
    const age = (live ? Date.now() - live.receivedAt : 0) + (m.ageMs || 0);
    const how =
      m.accuracy === "exact"
        ? "posição exata"
        : m.accuracy === "estimated"
          ? `posição estimada${m.radiusM ? ` (~${Math.round(m.radiusM)} m)` : ""}`
          : "onde você estava por último";
    return [b, line(`${how} · ${ago(age)}`)];
  };
  // An object not told yet says so; one told says it can be changed. A
  // gathering point not told yet (the catalogue's "Pontos de coleta") asks
  // which resource it is.
  const askTip = (ask) => {
    const b = document.createElement("b");
    b.textContent = `Objeto desconhecido${dev ? ` #${askKey(ask)}` : ""}`;
    return [b, line("clique para dizer o que é", "tipAsk")];
  };
  const gatherAsk = (ask) => ask?.layer === "gatherUnknown";
  const gatherTitle = (ask) => {
    const b = document.createElement("b");
    b.textContent = `Ponto de coleta${dev ? ` #${askKey(ask)}` : ""}`;
    return b;
  };
  const askLine = (ask) => line(gatherAsk(ask) ? "clique para dizer qual é" : "clique para mudar o que é", "tipAsk");
  const markerTip = (m, layer, ask) => {
    if (ask?.layer === "object") {
      const out = askTip(ask);
      if (m.learned) out.push(line(`aprendido${m.seen ? ` · visto ${m.seen}×` : ""}${m.lastMs ? ` · ${ago(Date.now() - m.lastMs)}` : ""}`, "tipLearned"));
      return out;
    }
    let b = document.createElement("b");
    b.textContent = m.name || layer.name;
    if (gatherAsk(ask)) b = gatherTitle(ask);
    const out = [b];
    if (m.name && m.name !== layer.name && !gatherAsk(ask)) out.push(line(layer.name));
    if (m.learned) {
      const seen = m.seen ? ` · visto ${m.seen}×` : "";
      out.push(line(`aprendido${seen}${m.lastMs ? ` · ${ago(Date.now() - m.lastMs)}` : ""}`, "tipLearned"));
    }
    if (m.backToMs) {
      const now = Date.now();
      if (now < (m.backFromMs ?? m.backToMs)) out.push(line(`esgotado · volta em ${minutes(m.backFromMs - now)}–${minutes(m.backToMs - now)} min`, "tipBack"));
      else if (now < m.backToMs) out.push(line(`esgotado · deve voltar em até ${minutes(m.backToMs - now)} min`, "tipBack"));
      else out.push(line("já deve ter voltado", "tipBack"));
    }
    if (ask) out.push(askLine(ask));
    return out;
  };
  const liveTip = (e, layer, ask) => {
    if (ask?.layer === "object") return [...askTip(ask), line("em vista agora", "tipLive")];
    // A creature carrying a pet's soul: which pet, and your level of it.
    if (e.pet != null) {
      const b = document.createElement("b");
      const level = e.petLevel == null ? "seu nível ?" : e.petLevel > 0 ? `seu nível ${e.petLevel}` : "seu nível 0 (falta)";
      b.textContent = `Mascote #${e.pet} · ${level}`;
      const out = [b];
      if (e.name) out.push(line(e.name));
      out.push(line("em vista agora", "tipLive"));
      return out;
    }
    if (gatherAsk(ask)) return [gatherTitle(ask), line("em vista agora", "tipLive"), askLine(ask)];
    const b = document.createElement("b");
    b.textContent = e.name || layer?.name || "—";
    const out = [b];
    if (layer && e.name && e.name !== layer.name) out.push(line(layer.name));
    out.push(line("em vista agora", "tipLive"));
    if (ask) out.push(askLine(ask));
    return out;
  };
  const hitAt = (sx, sy) => {
    let best = null;
    let bestD = Infinity;
    for (const h of hits) {
      const d = Math.hypot(h.x - sx, h.y - sy);
      if (d > h.r + 4) continue;
      // You win over whatever is under you.
      const score = h.me ? d - 6 : d;
      if (score < bestD) {
        best = h;
        bestD = score;
      }
    }
    return best;
  };
  const showTip = (hit, sx, sy) => {
    if (!hit) {
      tip.hidden = true;
      wrap.classList.remove("isOver");
      return;
    }
    tip.replaceChildren(...hit.tip());
    tip.hidden = false;
    wrap.classList.add("isOver");
    const tw = tip.offsetWidth;
    const th = tip.offsetHeight;
    let left = sx + 14;
    let top = sy + 12;
    if (left + tw > size.w - 6) left = sx - tw - 14;
    if (top + th > size.h - 6) top = sy - th - 12;
    tip.style.left = `${Math.max(6, left)}px`;
    tip.style.top = `${Math.max(6, top)}px`;
  };

  // ===== "O que é isto?": tell the map what an object is =====
  // Once per object code (a gathering point: per code and variant, the
  // resource): every object of it, learned before or not, moves to the
  // layer picked (the app keeps it in copilot-map-labels.json).
  const placeAsk = () => {
    if (!asking || askEl.hidden) return;
    const [x, y] = toScreen(asking.px, asking.py);
    const w = askEl.offsetWidth;
    const h = askEl.offsetHeight;
    let left = x + 14;
    let top = y - 16;
    if (left + w > size.w - 6) left = x - w - 14;
    if (top + h > size.h - 6) top = size.h - h - 6;
    askEl.style.left = `${Math.max(6, left)}px`;
    askEl.style.top = `${Math.max(6, top)}px`;
  };
  const closeAsk = () => {
    if (!asking) return;
    asking = null;
    askEl.hidden = true;
    askEl.replaceChildren();
  };
  const tell = async (ask, layerId) => {
    closeAsk();
    const ok = await invoke("copilot_map_label_object", { code: ask.code, variant: ask.variant ?? null, layer: layerId });
    // What you just named stays in sight.
    if (ok === true && layerId && !selected.has(layerId)) setSelected([...selected, layerId]);
    loadLayers();
    loadMarkers();
    seq = null;
    poll();
  };
  const askItem = (ask, l, name, value) => {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "menuitemradio");
    b.setAttribute("aria-checked", String(ask.layer === (value ?? "object")));
    const icon = document.createElement("span");
    icon.className = "lpIcon";
    icon.style.setProperty("--c", l?.color || "#90a4ae");
    icon.innerHTML = glyphSvg(l?.glyph || "box");
    const nm = document.createElement("span");
    nm.className = "askName";
    nm.textContent = name;
    b.append(icon, nm);
    b.addEventListener("click", () => tell(ask, value));
    return b;
  };
  const openAsk = (ask) => {
    if (!catalogue) return;
    asking = ask;
    tip.hidden = true;
    const head = document.createElement("div");
    head.className = "askHead";
    const title = document.createElement("b");
    title.textContent = "O que é isto?";
    head.append(title);
    if (dev) {
      const code = document.createElement("span");
      code.className = "askCode";
      code.textContent = `#${askKey(ask)}`;
      head.append(code);
    }
    const parts = [head];
    for (const g of catalogue.groups.filter((g) => ASK_GROUPS.includes(g.id))) {
      const name = document.createElement("div");
      name.className = "askGroup";
      name.textContent = g.name;
      const list = document.createElement("div");
      list.className = "askList";
      list.append(...g.layers.map((l) => askItem(ask, l, l.name, l.id)));
      parts.push(name, list);
    }
    const other = document.createElement("div");
    other.className = "askList askOther";
    other.append(askItem(ask, layerOf("object"), "Outro objeto", null));
    parts.push(other);
    askEl.replaceChildren(...parts);
    askEl.hidden = false;
    placeAsk();
    (askEl.querySelector('[aria-checked="true"]') || askEl.querySelector("button"))?.focus({ preventScroll: true });
  };
  document.addEventListener("mousedown", (e) => {
    if (asking && !e.target.closest(".mapAsk")) closeAsk();
  });

  // ===== Status: empty states, notes, header =====
  const setEmpty = (title, text, pick) => {
    empty.hidden = !title;
    if (!title) return;
    empty.querySelector(".emptyTitle").textContent = title;
    empty.querySelector(".emptyText").textContent = text;
    empty.querySelector(".emptyPick").hidden = !pick;
  };
  const renderStatus = () => {
    worldName.textContent = world?.name || "Mapa desconhecido";
    wrap.classList.toggle("noWorld", !world);
    layersBtn.disabled = !world;
    if (!live) {
      setEmpty("Carregando o mapa…", "", false);
    } else if (!world && !live.game) {
      setEmpty("O jogo está fechado", "Abra o AION 2: o mapa mostra onde você está e o que está por perto.", true);
    } else if (!world) {
      setEmpty("Em que mapa você está?", "Ande um pouco ou abra o mapa no jogo (M).", true);
    } else {
      setEmpty(null);
    }
    let text = "";
    if (world && live && !live.game) text = me() ? "Jogo fechado · última posição conhecida" : "Jogo fechado";
    else if (world && !world.tiles) text = "Sem imagem deste mapa ainda: grade e marcadores";
    else if (world && live && !me()) text = "Sua posição aparece quando você se mover ou lutar";
    note.textContent = text;
    note.hidden = !text;
    // Developer mode only: the backend sends these two only there.
    const dev = [];
    if (live?.world?.detectedBy) dev.push(`mundo: ${live.world.detectedBy}`);
    if (live?.me?.source) dev.push(`posição: ${live.me.source}`);
    devLine.textContent = dev.join(" · ");
    devLine.hidden = !dev.length;
  };
  const renderCount = () => {
    layersCount.textContent = plural(selected.size, "camada", "camadas");
  };

  // ===== Layers =====
  const saveSelected = () => {
    if (world) store.set(`copilot.map.layers.${world.id}`, [...selected]);
  };
  let markerReq = 0;
  let markerTimer = 0;
  const loadMarkers = () => {
    clearTimeout(markerTimer);
    markerTimer = setTimeout(async () => {
      const id = ++markerReq;
      if (!world || !selected.size) {
        markers = {};
        redraw();
        return;
      }
      const res = await invoke("copilot_map_markers", { world: world.id, layers: [...selected] });
      if (id !== markerReq) return;
      markers = res?.markers || {};
      redraw();
    }, 120);
  };
  const setSelected = (ids) => {
    selected = new Set(ids);
    saveSelected();
    renderCount();
    syncChecks();
    loadMarkers();
  };
  const syncChecks = () => {
    for (const input of groupsEl.querySelectorAll("input[data-layer]")) {
      input.checked = selected.has(input.dataset.layer);
    }
  };
  const renderLayers = () => {
    if (!catalogue) {
      groupsEl.replaceChildren();
      return;
    }
    if (catalogue.credit) credit.textContent = catalogue.credit;
    groupsEl.replaceChildren(
      ...catalogue.groups.map((g) => {
        const box = document.createElement("div");
        box.className = "lpGroup";
        const head = document.createElement("button");
        head.type = "button";
        head.className = "lpGroupHead";
        head.title = "Marcar ou desmarcar o grupo";
        const name = document.createElement("span");
        name.textContent = g.name;
        const count = document.createElement("span");
        count.className = "groupCount";
        count.textContent = String(g.count);
        head.append(name, count);
        head.addEventListener("click", () => {
          const ids = g.layers.map((l) => l.id);
          const all = ids.every((id) => selected.has(id));
          const next = new Set(selected);
          for (const id of ids) all ? next.delete(id) : next.add(id);
          setSelected(next);
        });
        box.append(head);
        // No database names the resources: they come from what you see.
        if (g.layers.some((l) => l.id.startsWith("gather") && l.count === 0)) {
          const hint = document.createElement("p");
          hint.className = "lpHint";
          hint.textContent = "Os recursos aparecem conforme você os vê no jogo; clique num objeto no mapa para dizer qual é.";
          box.append(hint);
        }
        for (const l of g.layers) {
          const row = document.createElement("label");
          row.className = "lpRow";
          row.classList.toggle("isEmpty", l.count === 0);
          row.title = l.learned ? `${l.bundled} do mapa · ${l.learned} aprendido${l.learned === 1 ? "" : "s"} por você` : `${l.bundled} no mapa`;
          const input = document.createElement("input");
          input.type = "checkbox";
          input.dataset.layer = l.id;
          input.checked = selected.has(l.id);
          input.addEventListener("change", () => {
            const next = new Set(selected);
            input.checked ? next.add(l.id) : next.delete(l.id);
            setSelected(next);
          });
          const icon = document.createElement("span");
          icon.className = "lpIcon";
          icon.style.setProperty("--c", l.color);
          icon.innerHTML = glyphSvg(l.glyph);
          const nm = document.createElement("span");
          nm.className = "lpName";
          nm.textContent = l.name;
          const ct = document.createElement("span");
          ct.className = "lpCount";
          ct.textContent = l.bundled || !l.learned ? String(l.bundled) : "";
          if (l.learned) {
            const more = document.createElement("small");
            more.textContent = `+${l.learned}`;
            ct.append(more);
          }
          row.append(input, icon, nm, ct);
          box.append(row);
        }
        return box;
      }),
    );
  };
  const loadLayers = async () => {
    if (!world) {
      catalogue = null;
      renderLayers();
      return;
    }
    const id = world.id;
    const res = await invoke("copilot_map_layers", { world: id });
    if (!world || world.id !== id || !res) return;
    catalogue = res;
    const known = new Set(res.groups.flatMap((g) => g.layers.map((l) => l.id)));
    const kept = [...selected].filter((l) => known.has(l));
    if (kept.length !== selected.size) setSelected(kept);
    renderLayers();
    redraw();
  };
  const setPanel = (open) => {
    layersPanel.hidden = !open;
    layersBtn.setAttribute("aria-expanded", String(open));
    if (open) {
      setWorldMenu(false);
      closeAsk();
    }
  };
  layersBtn.addEventListener("click", () => setPanel(layersPanel.hidden));
  $(".lpClose").addEventListener("click", () => setPanel(false));
  $(".lpAll").addEventListener("click", () => {
    if (catalogue) setSelected(catalogue.groups.flatMap((g) => g.layers.map((l) => l.id)));
  });
  $(".lpNone").addEventListener("click", () => setSelected([]));

  // ===== World =====
  const switchWorld = (next) => {
    world = next;
    markers = {};
    catalogue = null;
    tip.hidden = true;
    closeAsk();
    if (!world) {
      forgetTiles(null);
      renderLayers();
      redraw();
      return;
    }
    forgetTiles(world.id);
    selected = new Set(store.get(`copilot.map.layers.${world.id}`, DEFAULT_LAYERS));
    renderCount();
    const saved = store.get(`copilot.map.view.${world.id}`, null);
    view.scale = Number(saved?.zoom) || DEFAULT_ZOOM;
    view.cx = Number.isFinite(saved?.cx) ? saved.cx : world.width / 2;
    view.cy = Number.isFinite(saved?.cy) ? saved.cy : world.height / 2;
    clampView();
    if (follow) centreOnMe();
    loadLayers();
    loadMarkers();
    redraw();
  };

  const SIDE = { light: "Elyos", dark: "Asmodianos", abyss: "Abismo" };
  let worlds = null;
  const setWorldMenu = async (open) => {
    worldBtn.setAttribute("aria-expanded", String(open));
    if (!open) {
      worldMenu.hidden = true;
      return;
    }
    setPanel(false);
    worlds ??= await invoke("copilot_map_worlds");
    const item = (text, side, id) => {
      const b = document.createElement("button");
      b.type = "button";
      b.setAttribute("role", "menuitemradio");
      b.setAttribute("aria-checked", String(pinned === id));
      const t = document.createElement("span");
      t.textContent = text;
      b.append(t);
      if (side) {
        const s = document.createElement("small");
        s.textContent = side;
        b.append(s);
      }
      b.addEventListener("click", () => pick(id));
      return b;
    };
    const sep = document.createElement("div");
    sep.className = "wmSep";
    worldMenu.replaceChildren(
      item("Automático", "pelo que o jogo mostra", null),
      sep,
      ...(worlds || []).map((w) => item(w.name, SIDE[w.side] || "", w.id)),
    );
    worldMenu.hidden = false;
  };
  const pick = async (id) => {
    setWorldMenu(false);
    const ok = await invoke("copilot_map_pick_world", { world: id });
    if (ok === false) return;
    pinned = id;
    seq = null;
    poll();
  };
  worldBtn.addEventListener("click", () => setWorldMenu(worldMenu.hidden));
  $(".emptyPick").addEventListener("click", () => setWorldMenu(true));
  document.addEventListener("mousedown", (e) => {
    if (!worldMenu.hidden && !e.target.closest(".worldMenu, .worldBtn, .emptyPick")) setWorldMenu(false);
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (asking) {
      // The question first; the next Escape closes the rest.
      closeAsk();
      return;
    }
    setWorldMenu(false);
    setPanel(false);
  });

  // ===== Live state =====
  const apply = (next) => {
    live = { ...next, receivedAt: Date.now() };
    seq = next.seq;
    pinned = next.pinned ?? null;
    const nextWorld = next.world || null;
    if ((nextWorld?.id ?? null) !== (world?.id ?? null)) {
      learnedKey = JSON.stringify(next.learned || {});
      switchWorld(nextWorld);
    } else {
      const key = JSON.stringify(next.learned || {});
      if (key !== learnedKey) {
        learnedKey = key;
        loadLayers();
        loadMarkers();
      }
    }
    if (follow) centreOnMe();
    renderStatus();
    redraw();
  };
  let busy = false;
  const poll = async () => {
    if (busy) return;
    busy = true;
    try {
      const next = await invoke("copilot_map_state", { since: seq });
      if (next) apply(next);
      else if (!live) renderStatus();
    } finally {
      busy = false;
    }
  };
  // For checking the layout from DevTools with a sample state.
  window.copilotMapApply = apply;

  // ===== Input: drag to pan, wheel to zoom, hover for names =====
  let pan = null;
  canvas.addEventListener("mousedown", (e) => {
    if (e.button !== 0 || !world) return;
    e.preventDefault();
    pan = { x: e.clientX, y: e.clientY, cx: view.cx, cy: view.cy, moved: false };
  });
  window.addEventListener("mousemove", (e) => {
    const rect = canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    if (pan) {
      const dx = e.clientX - pan.x;
      const dy = e.clientY - pan.y;
      if (!pan.moved && Math.hypot(dx, dy) < 3) return;
      if (!pan.moved) {
        pan.moved = true;
        wrap.classList.add("isPanning");
        tip.hidden = true;
        if (follow) setFollow(false);
      }
      view.cx = pan.cx - dx / view.scale;
      view.cy = pan.cy - dy / view.scale;
      clampView();
      redraw();
      return;
    }
    if (e.target !== canvas) {
      if (!tip.hidden) showTip(null);
      return;
    }
    showTip(hitAt(sx, sy), sx, sy);
  });
  window.addEventListener("mouseup", (e) => {
    if (!pan) return;
    if (pan.moved) saveView();
    else {
      // A click, not a drag: an object asks what it is.
      const rect = canvas.getBoundingClientRect();
      const hit = hitAt(e.clientX - rect.left, e.clientY - rect.top);
      if (hit?.ask) openAsk(hit.ask);
    }
    pan = null;
    wrap.classList.remove("isPanning");
  });
  canvas.addEventListener("mouseleave", () => {
    if (!pan) showTip(null);
  });

  const zoomAt = (factor, sx, sy) => {
    if (!world) return;
    const m = me();
    // Following you, the zoom keeps you in the middle.
    if (follow && m) {
      sx = size.w / 2;
      sy = size.h / 2;
    }
    const [mx, my] = toMap(sx, sy);
    view.scale *= factor;
    clampView();
    view.cx = mx - (sx - size.w / 2) / view.scale;
    view.cy = my - (sy - size.h / 2) / view.scale;
    clampView();
    saveView();
    tip.hidden = true;
    closeAsk();
    redraw();
  };
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      zoomAt(e.deltaY < 0 ? 1.2 : 1 / 1.2, e.clientX - rect.left, e.clientY - rect.top);
    },
    { passive: false },
  );
  canvas.addEventListener("dblclick", (e) => {
    const rect = canvas.getBoundingClientRect();
    zoomAt(1.6, e.clientX - rect.left, e.clientY - rect.top);
  });
  $(".zoomIn").addEventListener("click", () => zoomAt(1.4, size.w / 2, size.h / 2));
  $(".zoomOut").addEventListener("click", () => zoomAt(1 / 1.4, size.w / 2, size.h / 2));
  followBtn.addEventListener("click", () => setFollow(!follow));

  // The window resizes from its corner (it has no frame).
  $(".resizeGrip").addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    window.__TAURI__?.window?.getCurrentWindow?.()?.startResizeDragging?.("SouthEast")?.catch?.(() => {});
  });

  // The canvas follows the map's box, sharp on any screen scale.
  new ResizeObserver(() => {
    const rect = wrap.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    size = { w: Math.round(rect.width), h: Math.round(rect.height), dpr };
    canvas.width = Math.max(1, Math.round(rect.width * dpr));
    canvas.height = Math.max(1, Math.round(rect.height * dpr));
    clampView();
    draw();
  }).observe(wrap);

  setFollow(follow);
  renderCount();
  renderStatus();
  poll();
  setInterval(poll, 1000);
})();

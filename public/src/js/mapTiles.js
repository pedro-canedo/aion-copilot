// aion-copilot: the map's tiles (tc-imba AION2 Interactive Map; the art is
// NCSOFT's). Downloaded the first time each is shown, for personal use, and
// kept in the app's data folder (map-tiles/); never part of the installer.
// The personal build has no HTTP client in Rust, so this page fetches a tile
// the app does not have yet (the CSP allows tc-imba's site) and hands it to
// Rust to keep: `copilot_map_tile` answers with the kept tile or its address,
// `copilot_map_tile_store` keeps it.
(() => {
  const CREDIT = "Mapa: tc-imba AION2 Interactive Map (CC BY-NC 4.0); arte © NCSOFT";
  const invoke = (cmd, args) => {
    const fn = window.__TAURI__?.core?.invoke;
    return fn ? fn(cmd, args) : Promise.reject(new Error("no Tauri"));
  };
  // One request per tile, even when several draws ask at once; only while it
  // is on its way (the map keeps the decoded image, not the data: URL).
  const tiles = new Map();

  const asDataUrl = (blob) =>
    new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.addEventListener("load", () => resolve(reader.result));
      reader.addEventListener("error", () => reject(reader.error));
      reader.readAsDataURL(blob);
    });

  const load = async (world, x, y) => {
    const tile = await invoke("copilot_map_tile", { world, z: 0, x, y });
    if (tile?.dataUrl) return tile.dataUrl;
    if (!tile?.url) return null;
    const response = await fetch(tile.url, { cache: "force-cache" });
    if (!response.ok) return null;
    const dataUrl = await asDataUrl(await response.blob());
    invoke("copilot_map_tile_store", { world, x, y, dataUrl }).catch(() => {});
    return dataUrl;
  };

  // A tile as a data: URL for an <img> or a canvas, or null when the world
  // has none there (or the site cannot be reached; asked again next time).
  const get = (world, x, y) => {
    const key = `${world}/${x}/${y}`;
    if (!tiles.has(key)) {
      const pending = load(world, x, y).catch(() => null);
      tiles.set(key, pending);
      pending.then(() => tiles.delete(key));
    }
    return tiles.get(key);
  };

  window.copilotMapTiles = { get, CREDIT };
})();

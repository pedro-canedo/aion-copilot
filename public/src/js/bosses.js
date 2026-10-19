// aion-copilot: Bosses overlay — the field bosses the game reported when you
// last opened each map (packet 01 91): who is up, and a countdown to each
// one's return. Kept in localStorage so the times survive a restart.
(() => {
  const { invoke } = window.copilotOverlay;
  const KEY = "copilot.bosses.v1";
  const SOON_MS = 5 * 60_000;
  const list = document.querySelector(".bosses");
  const empty = document.querySelector(".empty");
  const summary = document.querySelector(".summary");

  let known = {};
  try {
    known = JSON.parse(localStorage.getItem(KEY) || "{}") || {};
  } catch {
    known = {};
  }
  const save = () => {
    try {
      localStorage.setItem(KEY, JSON.stringify(known));
    } catch {
      // kept for this session only
    }
  };

  // Newer readings of a map replace older ones, boss by boss.
  const merge = (bosses) => {
    let changed = false;
    for (const boss of bosses) {
      const prev = known[boss.id];
      if (!prev || boss.seenMs >= prev.seenMs) {
        if (!prev || prev.seenMs !== boss.seenMs || prev.up !== boss.up || prev.timeMs !== boss.timeMs || prev.name !== boss.name) {
          known[boss.id] = boss;
          changed = true;
        }
      }
    }
    if (changed) save();
  };

  const clock = (ms) => {
    const totalMin = Math.max(0, Math.ceil(ms / 60_000));
    const h = Math.floor(totalMin / 60);
    const m = totalMin % 60;
    return h > 0 ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m`;
  };
  const at = (ms) => new Date(ms).toLocaleTimeString(document.documentElement.lang || undefined, { hour: "2-digit", minute: "2-digit" });

  const render = () => {
    const now = Date.now();
    const bosses = Object.values(known).map((b) => {
      // A dead boss whose time passed is presumably back.
      const back = !b.up && b.timeMs > 0 && b.timeMs <= now;
      const up = b.up || back;
      const left = !up && b.timeMs > 0 ? b.timeMs - now : null;
      return { ...b, up, back, left, soon: left !== null && left <= SOON_MS };
    });
    bosses.sort((a, b) => (a.up - b.up) || ((a.left ?? Infinity) - (b.left ?? Infinity)) || a.name.localeCompare(b.name));
    const has = bosses.length > 0;
    list.hidden = !has;
    empty.hidden = has;
    if (!has) {
      summary.textContent = "—";
      list.replaceChildren();
      return;
    }
    const up = bosses.filter((b) => b.up).length;
    const soon = bosses.filter((b) => b.soon).length;
    summary.textContent = `${up} vivo${up === 1 ? "" : "s"} · ${bosses.length - up} aguardando${soon ? ` · ${soon} em breve` : ""}`;
    list.replaceChildren(
      ...bosses.map((b) => {
        const li = document.createElement("li");
        li.className = "boss";
        li.classList.toggle("isUp", b.up);
        li.classList.toggle("isSoon", b.soon);
        const dot = document.createElement("span");
        dot.className = "dot";
        const name = document.createElement("span");
        name.className = "name";
        name.textContent = b.name;
        name.title = `${b.name} · lido às ${at(b.seenMs)}`;
        const when = document.createElement("span");
        when.className = "when";
        if (b.back) {
          when.textContent = "voltou?";
          when.title = `Deveria ter voltado às ${at(b.timeMs)}; abra o mapa para confirmar.`;
        }
        else if (b.up) when.textContent = "vivo";
        else if (b.left !== null) {
          when.textContent = `volta em ${clock(b.left)}`;
          when.title = `às ${at(b.timeMs)}`;
        } else when.textContent = "sem horário";
        li.append(dot, name, when);
        return li;
      }),
    );
  };

  const refresh = async () => {
    const bosses = await invoke("copilot_field_bosses");
    if (Array.isArray(bosses) && bosses.length) merge(bosses);
    render();
  };
  // For checking the layout from DevTools with sample bosses.
  window.copilotBossesMerge = (bosses) => {
    merge(bosses);
    render();
  };
  refresh();
  setInterval(refresh, 5000);
  setInterval(render, 15_000);
})();

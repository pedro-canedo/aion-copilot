// aion-copilot: the PvP board — in the Abyss, or when Auto sees PvP, your side
// against the players you fought replaces the meter's list.
(() => {
  const box = document.querySelector(".copilotPvp");
  if (!box || (document.documentElement.dataset.a2view || "main") !== "main") return;
  const invoke = (cmd, args) => window.__TAURI__?.core?.invoke?.(cmd, args) ?? Promise.resolve(null);
  const lang = () => document.documentElement.lang || undefined;
  const esc = (t) => String(t ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
  const compact = (n) => {
    const v = Number(n) || 0;
    const fmt = (x, d) => x.toLocaleString(lang(), { minimumFractionDigits: d, maximumFractionDigits: d });
    if (v >= 1e6) return `${fmt(v / 1e6, 2)}M`;
    if (v >= 1e3) return `${fmt(v / 1e3, 1)}k`;
    return String(Math.round(v));
  };
  const mmss = (ms) => {
    const s = Math.max(0, Math.round((Number(ms) || 0) / 1000));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  };
  const SKULL =
    '<svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><path d="M12 2C7 2 3.5 5.6 3.5 10c0 2.6 1.2 4.6 3 5.8V19a1 1 0 0 0 1 1h1.5v-2h1.5v2h3v-2h1.5v2H16.5a1 1 0 0 0 1-1v-3.2c1.8-1.2 3-3.2 3-5.8C20.5 5.6 17 2 12 2zm-3.5 11a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm7 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"/></svg>';

  const rowHtml = (r, i, total, side) => {
    const share = total > 0 ? (r.damage / total) * 100 : 0;
    const icon = r.job ? `<img src="./assets/${esc(r.job)}.png" alt="" onerror="this.style.display='none'">` : "<span></span>";
    const power =
      r.combatPower > 0
        ? `<span class="cpCp" title="Combat Power ${r.combatPower.toLocaleString(lang())}">${window.copilotCp?.(r.combatPower) ?? r.combatPower}</span>`
        : r.gearScore > 0
          ? `<span class="cpCp isGs" title="GS lido do spawn (a confirmar)">GS ${r.gearScore.toLocaleString(lang())}</span>`
          : "<span></span>";
    const deathsTitle = side === "ours" ? "Suas mortes nesta sessão" : "Vezes que você o matou nesta sessão";
    const deaths = r.deaths > 0 ? `<span class="cpDeaths" title="${deathsTitle}">${SKULL}${r.deaths}</span>` : "<span></span>";
    const pct = share.toLocaleString(lang(), { maximumFractionDigits: 1, minimumFractionDigits: 1 });
    return (
      `<div class="cpRow ${side}${r.isYou ? " isYou" : ""}" style="--share:${share.toFixed(1)}%">` +
      `<span class="cpRank">${i + 1}</span>${icon}<span class="cpName">${esc(r.name)}</span>${power}${deaths}` +
      `<span class="cpDmg">${compact(r.damage)}</span><span class="cpPct">${pct}%</span></div>`
    );
  };

  let lastJson = "";
  const render = (zone, board) => {
    box.querySelector(".cpZone").textContent = zone?.kind === "abyss" ? "⚔ Abismo" : "⚔ PvP";
    box.querySelector(".cpTime").textContent = board.lastMs > board.firstMs ? mmss(board.lastMs - board.firstMs) : "0:00";
    box.querySelector(".cpK").textContent = String(board.kills || 0);
    box.querySelector(".cpD").textContent = String(board.deaths || 0);
    const json = JSON.stringify(board);
    if (json === lastJson) return;
    lastJson = json;
    const body = box.querySelector(".cpBody");
    if (!board.ours.length && !board.opponents.length) {
      body.innerHTML = '<p class="cpEmpty">Nenhum PvP ainda: o combate aparece aqui desde o primeiro golpe.</p>';
      return;
    }
    const all = board.oursTotal + board.opponentsTotal;
    const oursPct = all > 0 ? (board.oursTotal / all) * 100 : 50;
    const theirs = board.opponents.length
      ? board.opponents.map((r, i) => rowHtml(r, i, board.opponentsTotal, "theirs")).join("")
      : '<p class="cpEmpty">Ninguém bateu em você ainda.</p>';
    body.innerHTML = `
      <div class="cpCols"><span>Jogador</span><span>Dano</span><span>Parte</span></div>
      <div class="cpSide ours">Seu lado</div>
      ${board.ours.map((r, i) => rowHtml(r, i, board.oursTotal, "ours")).join("")}
      <div class="cpSide theirs">Oponentes</div>
      ${theirs}
      <div class="cpBar" style="--ours:${oursPct.toFixed(1)}%"><span></span></div>
      <div class="cpTotals"><span class="ours">Seu lado</span><b>${compact(board.oursTotal)}</b><span class="theirs">Oponentes (em você)</span><b>${compact(board.opponentsTotal)}</b></div>`;
  };

  const tick = async () => {
    const [zone, kind] = await Promise.all([
      invoke("copilot_zone").catch(() => null),
      invoke("copilot_target_kind").catch(() => null),
    ]);
    const auto = document.querySelector(".copilotAuto")?.classList.contains("isActive");
    const forced = document.documentElement.dataset.copilotPvp === "on";
    const on = forced || (auto && (zone?.kind === "abyss" || kind === "pvp"));
    document.documentElement.classList.toggle("copilotPvpOn", !!on);
    box.hidden = !on;
    window.copilotZoneKind = zone?.kind || "";
    if (!on) return;
    const board = await invoke("copilot_pvp_board").catch(() => null);
    if (board) render(zone, board);
  };
  window.copilotPvpRender = render;
  setInterval(tick, 1000);
  tick();
})();

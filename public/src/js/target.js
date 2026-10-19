// aion-copilot: Target overlay — the meter's current target: its HP, how long
// the fight has run, the group's DPS on it and a time-to-kill estimate, plus
// your own place. Reads the same snapshot the meter shows (get_dps_snapshot).
(() => {
  const { invoke } = window.copilotOverlay;
  const lang = document.documentElement.lang || undefined;
  const $ = (sel) => document.querySelector(sel);
  const box = $(".target");
  const empty = $(".empty");

  const compact = (n) => {
    const v = Number(n) || 0;
    const fmt = (x, d) => x.toLocaleString(lang, { minimumFractionDigits: d, maximumFractionDigits: d });
    if (v >= 1e9) return `${fmt(v / 1e9, 2)}B`;
    if (v >= 1e6) return `${fmt(v / 1e6, 2)}M`;
    if (v >= 1e3) return `${fmt(v / 1e3, 1)}k`;
    return String(Math.round(v));
  };
  const mmss = (ms) => {
    const s = Math.max(0, Math.round(ms / 1000));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  };

  // Pure, so it can be checked from DevTools with a sample snapshot.
  const view = (snap) => {
    const rows = Object.entries(snap?.map || {}).map(([id, p]) => ({ id: Number(id), ...p }));
    const hasTarget = !!snap && (snap.targetId > 0 || rows.length > 0) && rows.some((r) => r.amount > 0);
    if (!hasTarget) return null;
    const groupDps = rows.reduce((sum, r) => sum + (Number(r.dps) || 0), 0);
    const total = rows.reduce((sum, r) => sum + (Number(r.amount) || 0), 0);
    const max = Number(snap.targetMaxHp) || 0;
    const current = Number(snap.targetCurrentHp) >= 0 ? Number(snap.targetCurrentHp) : max > 0 ? Math.max(0, max - (Number(snap.targetTotalDamage) || 0)) : -1;
    const pct = max > 0 && current >= 0 ? (current / max) * 100 : null;
    const ttkMs = current > 0 && groupDps > 0 ? (current / groupDps) * 1000 : null;
    const sorted = [...rows].sort((a, b) => b.amount - a.amount);
    const local = Number(snap.localPlayerId);
    const youIndex = sorted.findIndex((r) => r.id === local);
    const you = youIndex >= 0 ? sorted[youIndex] : null;
    return {
      name: ({ Train: "Treino", "All Targets": "Todos os alvos" })[snap.targetName] || snap.targetName || (snap.targetMode === "trainTargets" ? "Treino" : "Alvo"),
      boss: !!snap.targetIsBoss,
      pct,
      current,
      max,
      time: mmss(snap.battleTime),
      groupDps,
      ttk: ttkMs === null ? "—" : mmss(ttkMs),
      you: you && { rank: `${youIndex + 1}º de ${sorted.length}`, dps: `${compact(you.dps)}/s`, pct: `${total > 0 ? ((you.amount / total) * 100).toLocaleString(lang, { maximumFractionDigits: 1 }) : 0}%` },
    };
  };

  const render = (v) => {
    box.hidden = !v;
    empty.hidden = !!v;
    if (!v) return;
    $(".tLabel").textContent = v.name;
    $(".chip").hidden = !v.boss;
    $(".hpFill").style.width = v.pct === null ? "0%" : `${v.pct.toFixed(1)}%`;
    $(".hpPct").textContent = v.pct === null ? "vida desconhecida" : `${v.pct.toLocaleString(lang, { maximumFractionDigits: 1 })}%`;
    $(".hpAbs").textContent = v.max > 0 && v.current >= 0 ? `${compact(v.current)} / ${compact(v.max)}` : "";
    $(".sTime").textContent = v.time;
    $(".sDps").textContent = `${compact(v.groupDps)}/s`;
    $(".sTtk").textContent = v.ttk;
    $(".you").hidden = !v.you;
    if (v.you) {
      $(".youRank").textContent = v.you.rank;
      $(".youDps").textContent = v.you.dps;
      $(".youPct").textContent = v.you.pct;
    }
  };

  window.copilotTargetView = view;
  window.copilotTargetRender = (snap) => render(view(snap));

  // PvP: the player you hit last, their gear and your record against them.
  const pvpBox = $(".pvp");
  const renderPvp = (p) => {
    pvpBox.hidden = !p;
    if (!p) return;
    box.hidden = true;
    empty.hidden = true;
    $(".pName").textContent = p.name;
    const img = $(".pJob");
    img.hidden = !p.job;
    if (p.job) img.src = `/assets/${p.job}.png`;
    const cut = (x) => (Math.floor(x * 10) / 10).toLocaleString(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
    $(".pCp").textContent = p.combatPower > 0 ? `${cut(p.combatPower / 1000)}K` : "—";
    $(".pCp").title = p.combatPower > 0 ? p.combatPower.toLocaleString(lang) : "";
    $(".pHp").textContent = Number.isFinite(p.hp) && p.hp >= 0 ? compact(p.hp) : "—";
    $(".pKills").textContent = String(p.kills || 0);
    $(".pDeaths").textContent = String(p.deaths || 0);
  };
  window.copilotTargetRenderPvp = renderPvp;

  const refresh = async () => {
    const [snap, kind] = await Promise.all([invoke("get_dps_snapshot"), invoke("copilot_target_kind")]);
    const pvp = kind === "pvp" ? await invoke("copilot_pvp_target") : null;
    if (pvp) {
      renderPvp(pvp);
      return;
    }
    renderPvp(null);
    render(view(snap));
  };
  refresh();
  setInterval(refresh, 1000);
})();

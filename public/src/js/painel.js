// aion-copilot: the Painel — the app's home. A summary of everything (resets,
// "não perca", the last fight, bosses coming back, your characters) and the
// full views: fight history, characters, routine and field bosses; then what
// each version brought (Novidades) and the app's own settings (Ajustes).
(() => {
  const tauri = window.__TAURI__;
  const invoke = (cmd, args) => (tauri?.core?.invoke ? tauri.core.invoke(cmd, args) : Promise.resolve(null));
  const bus = window.copilotBus;
  const news = window.copilotWhatsNew;
  const $ = (sel, root = document) => root.querySelector(sel);
  const lang = document.documentElement.lang || undefined;
  const DAY = 86_400_000;

  // ---------- helpers ----------
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
  const span = (ms) => {
    const min = Math.max(0, Math.floor(ms / 60_000));
    const d = Math.floor(min / 1440), h = Math.floor((min % 1440) / 60), m = min % 60;
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
    return `${m}m`;
  };
  const when = (ms) => {
    const d = new Date(ms);
    const today = new Date();
    const sameDay = d.toDateString() === today.toDateString();
    const time = d.toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });
    return sameDay ? `hoje ${time}` : `${d.toLocaleDateString(lang, { day: "2-digit", month: "2-digit" })} ${time}`;
  };
  const ago = (ms) => {
    const diff = Date.now() - ms;
    if (diff < 60_000) return "agora";
    return `há ${span(diff)}`;
  };
  const icon = (job) => {
    const img = document.createElement("img");
    img.alt = "";
    if (job && job !== "Unknown") img.src = `/assets/${job}.png`;
    else img.style.visibility = "hidden";
    return img;
  };
  const read = (key, fallback) => {
    try {
      return JSON.parse(localStorage.getItem(key) || "null") ?? fallback;
    } catch {
      return fallback;
    }
  };
  // Global servers: daily and weekly resets at 07:00 UTC, weekly on Wednesday (timersCore.js).
  const timers = window.copilotTimers;
  const { lastDailyReset: lastDaily, lastWeeklyReset: lastWeekly } = timers;
  // The open-world (field) bosses, by NPC code: they are the world bosses.
  let worldBossCodes = new Set();
  const kindOf = (f) => (f.isPvp ? "pvp" : f.isTrain ? "train" : f.dungeonId ? "dungeon" : worldBossCodes.has(f.mobCode) ? "world" : "boss");
  const KIND_LABEL = { world: "Chefe mundial", boss: "Chefe", train: "Treino", dungeon: "Expedição", pvp: "PvP" };

  // ---------- navigation ----------
  const show = (view) => {
    if (!document.querySelector(`.view[data-view="${view}"]`)) return;
    for (const b of document.querySelectorAll(".navBtn")) b.classList.toggle("isActive", b.dataset.view === view);
    for (const v of document.querySelectorAll(".view")) v.hidden = v.dataset.view !== view;
    if (view === "novidades") markNewsSeen();
  };
  for (const b of document.querySelectorAll(".navBtn")) b.addEventListener("click", () => show(b.dataset.view));
  for (const b of document.querySelectorAll("[data-goto]")) b.addEventListener("click", () => show(b.dataset.goto));
  for (const b of document.querySelectorAll("[data-overlay]")) {
    b.addEventListener("click", () => invoke("toggle_copilot_overlay", { label: b.dataset.overlay }));
  }
  $(".openFullHistory").addEventListener("click", () => invoke("request_details_view", { payload: { kind: "history" } }));
  const openFight = (id) => invoke("request_details_view", { payload: { kind: "fight", fightId: id } });

  // ---------- state ----------
  let fights = [];
  let chars = [];
  let timerRules = [];
  // The Routine's automatic part (copilot_routine_state).
  let routineAuto = null;
  let filter = "all";
  let search = "";

  // ---------- Início ----------
  const renderHome = () => {
    const now = Date.now();
    const hour = new Date().getHours();
    $(".greetingTime").textContent = hour < 6 ? "Boa madrugada" : hour < 12 ? "Bom dia" : hour < 18 ? "Boa tarde" : "Boa noite";
    const main = chars[0];
    $(".greeting").innerHTML = main ? `Olá, <b></b>` : "Bem-vindo ao aion-<b>copilot</b>";
    if (main) $(".greeting b").textContent = main.name;
    $(".heroSub").textContent = fights.length
      ? `${fights.length} luta${fights.length > 1 ? "s" : ""} registrada${fights.length > 1 ? "s" : ""} · ${chars.length} personage${chars.length === 1 ? "m" : "ns"}`
      : "Abra o jogo e lute: o resumo se monta sozinho.";

    const dNext = lastDaily(now) + DAY;
    const wNext = lastWeekly(now) + 7 * DAY;
    $(".dailyIn").textContent = span(dNext - now);
    $(".weeklyIn").textContent = span(wNext - now);
    $(".dailyAt").textContent = `às ${new Date(dNext).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" })}`;
    $(".weeklyAt").textContent = new Date(wNext).toLocaleDateString(lang, { weekday: "short", hour: "2-digit", minute: "2-digit" });

    // Não perca: the Routine's pending tasks near their reset.
    const routine = read("copilot.routine.v1", { items: [] });
    const pendDaily = routine.items.filter((i) => i.scope === "daily" && !i.done);
    const pendWeekly = routine.items.filter((i) => i.scope === "weekly" && !i.done);
    const lines = [];
    if (pendDaily.length && dNext - now <= 3 * 3600_000) lines.push(`${pendDaily.length} diária${pendDaily.length > 1 ? "s" : ""} pendente${pendDaily.length > 1 ? "s" : ""} (reset em ${span(dNext - now)})`);
    if (pendWeekly.length && wNext - now <= 24 * 3600_000) lines.push(`${pendWeekly.length} semana${pendWeekly.length > 1 ? "is" : "l"} pendente${pendWeekly.length > 1 ? "s" : ""} (reset em ${span(wNext - now)})`);
    $(".naoPercaBig").hidden = !lines.length;
    $(".naoPercaBigText").textContent = lines.join(" · ");

    // Rotina de hoje: what the game says first (energy, Abyss), then the checklist.
    window.copilotRoutineAuto?.render($(".raHome"), routineAuto, now);
    // The checklist: progress and what is still open.
    const daily = routine.items.filter((i) => i.scope === "daily");
    const doneDaily = daily.filter((i) => i.done).length;
    $(".rtPct").textContent = `${doneDaily}/${daily.length}`;
    $(".rtFill").style.width = daily.length ? `${(doneDaily / daily.length) * 100}%` : "0%";
    const open = daily.filter((i) => !i.done).slice(0, 4);
    $(".rtMini").replaceChildren(...(open.length ? open.map((i) => {
      const li = document.createElement("li");
      li.innerHTML = '<span></span><span class="t"></span>';
      li.firstChild.textContent = i.label;
      li.lastChild.textContent = i.max ? `${i.count}/${i.max}` : "pendente";
      return li;
    }) : [Object.assign(document.createElement("li"), { className: "emptyText", textContent: daily.length ? "Tudo feito hoje." : "Monte sua rotina no overlay." })]));

    // Última luta
    const last = fights[0];
    const lf = $(".lastFight");
    if (last) {
      lf.innerHTML = `<div><div class="lfName"></div><div class="lfMeta"></div></div><div class="icons"></div><div class="lfNums"></div>`;
      $(".lfName", lf).textContent = last.bossName || "Luta";
      $(".lfMeta", lf).textContent = `${KIND_LABEL[kindOf(last)]} · ${when(last.startTimeMs)}`;
      $(".icons", lf).append(...(last.memberJobs?.length ? last.memberJobs : last.jobs || []).slice(0, 8).map(icon));
      const dps = last.durationMs > 0 ? (last.totalDamage / last.durationMs) * 1000 : 0;
      for (const [k, v] of [["Duração", mmss(last.durationMs)], ["Dano total", compact(last.totalDamage)], ["DPS do grupo", `${compact(dps)}/s`]]) {
        const d = document.createElement("div");
        d.innerHTML = '<span class="k"></span><span class="v"></span>';
        d.firstChild.textContent = k;
        d.lastChild.textContent = v;
        $(".lfNums", lf).append(d);
      }
      const open = document.createElement("button");
      open.type = "button";
      open.className = "pillBtn";
      open.textContent = "Abrir detalhes";
      open.addEventListener("click", () => openFight(last.id));
      $(".lfNums", lf).append(open);
    }

    // Números
    const real = fights.filter((f) => !f.isTrain);
    $(".statFights").textContent = fights.length.toLocaleString(lang);
    $(".statBosses").textContent = real.length.toLocaleString(lang);
    $(".statTime").textContent = span(fights.reduce((s, f) => s + (f.durationMs || 0), 0));

    // Chefes que voltam primeiro
    const bosses = Object.values(read("copilot.bosses.v1", {}))
      .filter((b) => !b.up && b.timeMs > now)
      .sort((a, b) => a.timeMs - b.timeMs)
      .slice(0, 5);
    const bl = $(".bossesSoon");
    if (bosses.length) {
      bl.replaceChildren(...bosses.map((b) => {
        const li = document.createElement("li");
        li.innerHTML = '<span></span><span class="t"></span>';
        li.firstChild.textContent = b.name;
        li.lastChild.textContent = `volta em ${span(b.timeMs - now)}`;
        li.lastChild.classList.toggle("soon", b.timeMs - now <= 5 * 60_000);
        return li;
      }));
    }

    // Timers: the next four events or field bosses, what is on now first.
    if (timerRules.length) {
      const next = timers.buildRows(timerRules, Object.values(read("copilot.bosses.v1", {})), now).filter((r) => r.state !== "reset").slice(0, 4);
      $(".timersMini").replaceChildren(...next.map((r) => {
        const li = document.createElement("li");
        li.innerHTML = '<span></span><span class="t"></span>';
        li.firstChild.textContent = r.confidence === "low" ? `${r.name} (aprox.)` : r.name;
        const time = new Date(r.start).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });
        const mins = r.state === "active" ? Math.max(1, Math.ceil(((r.entryEnd ?? r.end) - now) / 60_000)) : 0;
        li.lastChild.textContent = r.state !== "active"
          ? `${time} · em ${span(r.start - now)}`
          : r.entryEnd ? `agora · entrada fecha em ${mins} min` : `agora · faltam ${mins} min`;
        li.lastChild.classList.toggle("soon", r.state === "active" || r.start - now <= 5 * 60_000);
        return li;
      }));
    }

    // Personagens
    const cl = $(".charsMini");
    if (chars.length) {
      cl.replaceChildren(...chars.slice(0, 5).map((c) => {
        const li = document.createElement("li");
        const who = document.createElement("span");
        who.className = "who";
        const n = document.createElement("span");
        n.textContent = c.name;
        who.append(icon(c.job), n);
        const t = document.createElement("span");
        t.className = "t";
        t.textContent = c.combatPower ? c.combatPower.toLocaleString(lang) : ago(c.lastSeenMs);
        li.append(who, t);
        return li;
      }));
    }
  };

  // ---------- Histórico ----------
  const renderFights = () => {
    const q = search.trim().toLowerCase();
    const list = fights.filter((f) => (filter === "all" || kindOf(f) === filter) && (!q || (f.bossName || "").toLowerCase().includes(q)));
    const body = $(".fights tbody");
    body.replaceChildren(...list.slice(0, 300).map((f) => {
      const tr = document.createElement("tr");
      const k = kindOf(f);
      const dps = f.durationMs > 0 ? (f.totalDamage / f.durationMs) * 1000 : 0;
      const cells = [when(f.startTimeMs), f.bossName || "—", null, null, mmss(f.durationMs), compact(f.totalDamage), `${compact(dps)}/s`];
      cells.forEach((text, i) => {
        const td = document.createElement("td");
        if (i === 2) {
          const s = document.createElement("span");
          s.className = `kind ${k}`;
          s.textContent = KIND_LABEL[k];
          td.append(s);
        } else if (i === 3) {
          const box = document.createElement("span");
          box.className = "icons";
          box.append(...(f.memberJobs?.length ? f.memberJobs : f.jobs || []).slice(0, 8).map(icon));
          td.append(box);
        } else {
          td.textContent = text;
          if (i >= 4) td.className = "num";
        }
        tr.append(td);
      });
      tr.title = "Abrir os detalhes desta luta";
      tr.addEventListener("click", () => openFight(f.id));
      return tr;
    }));
    $(".fightsEmpty").hidden = list.length > 0;
  };
  for (const b of document.querySelectorAll(".chipBtn")) {
    b.addEventListener("click", () => {
      filter = b.dataset.filter;
      for (const o of document.querySelectorAll(".chipBtn")) o.classList.toggle("isActive", o === b);
      renderFights();
    });
  }
  $(".searchInput").addEventListener("input", (e) => {
    search = e.target.value;
    renderFights();
  });

  // ---------- Personagens ----------
  const renderChars = () => {
    const box = $(".chars");
    if (!chars.length) {
      box.innerHTML = '<p class="emptyText">Nenhum personagem ainda. Jogue com o aion-copilot aberto e ele aparece aqui.</p>';
      return;
    }
    box.replaceChildren(...chars.map((c) => {
      const card = document.createElement("div");
      card.className = "charCard";
      const n = document.createElement("span");
      n.className = "cn";
      n.textContent = c.name;
      const cp = document.createElement("span");
      cp.className = "cp";
      cp.textContent = c.combatPower ? `CP ${c.combatPower.toLocaleString(lang)}` : "CP aparece em grupo";
      const seen = document.createElement("span");
      seen.className = "seen";
      seen.textContent = `Visto ${ago(c.lastSeenMs)} · desde ${new Date(c.firstSeenMs).toLocaleDateString(lang)}`;
      card.append(icon(c.job), n, cp, seen);
      return card;
    }));
  };

  // ---------- Rotina ----------
  const renderRoutine = () => {
    const auto = window.copilotRoutineAuto;
    if (auto) {
      const now = Date.now();
      auto.render($(".raView"), routineAuto, now);
      $(".raWho").textContent = routineAuto?.name || "";
      const source = auto.sourceLine(routineAuto, now);
      $(".view[data-view='rotina'] .raSource").hidden = !source;
      $(".view[data-view='rotina'] .raSource").textContent = source;
    }
    const routine = read("copilot.routine.v1", { items: [] });
    const col = (scope, title) => {
      const card = document.createElement("article");
      card.className = "card";
      const items = routine.items.filter((i) => i.scope === scope);
      const done = items.filter((i) => i.done).length;
      card.innerHTML = `<div class="cardHead"><h2></h2><span class="emptyText"></span></div><ul class="rtList"></ul>`;
      $("h2", card).textContent = title;
      $(".emptyText", card).textContent = `${done}/${items.length}`;
      $(".rtList", card).append(...items.map((i) => {
        const li = document.createElement("li");
        li.classList.toggle("isDone", i.done);
        li.innerHTML = '<span></span><span class="c"></span>';
        li.firstChild.textContent = i.label;
        li.lastChild.textContent = i.max ? `${i.count}/${i.max}` : i.done ? "feito" : "—";
        return li;
      }));
      return card;
    };
    $(".routineCols").replaceChildren(col("daily", "Hoje"), col("weekly", "Esta semana"));
  };

  // ---------- Chefes ----------
  const renderBosses = () => {
    const now = Date.now();
    const list = Object.values(read("copilot.bosses.v1", {}));
    const ul = $(".bossList");
    if (!list.length) {
      ul.innerHTML = '<li class="emptyText">Abra o mapa no jogo (M) para ler os chefes de campo.</li>';
      return;
    }
    list.sort((a, b) => {
      const ka = a.up || a.timeMs <= now ? Infinity : a.timeMs;
      const kb = b.up || b.timeMs <= now ? Infinity : b.timeMs;
      return ka - kb || a.name.localeCompare(b.name);
    });
    ul.replaceChildren(...list.map((b) => {
      const li = document.createElement("li");
      const back = !b.up && b.timeMs > 0 && b.timeMs <= now;
      const left = !b.up && b.timeMs > now ? b.timeMs - now : null;
      li.classList.toggle("up", b.up || back);
      li.classList.toggle("soon", left !== null && left <= 5 * 60_000);
      li.innerHTML = '<span></span><span class="t"></span>';
      li.firstChild.textContent = b.name;
      li.lastChild.textContent = b.up ? "vivo" : back ? "voltou?" : left !== null ? `volta em ${span(left)}` : "sem horário";
      return li;
    }));
  };

  // ---------- PvP ----------
  let pvp = [];
  const renderPvp = () => {
    const box = $(".pvpList");
    const kills = pvp.reduce((n, p) => n + (p.kills || 0), 0);
    const deaths = pvp.reduce((n, p) => n + (p.deaths || 0), 0);
    $(".pvpKills").textContent = String(kills);
    $(".pvpDeaths").textContent = String(deaths);
    $(".pvpRatio").textContent = deaths > 0 ? (kills / deaths).toLocaleString(lang, { maximumFractionDigits: 2 }) : kills > 0 ? "∞" : "—";
    if (!pvp.length) {
      box.innerHTML = '<p class="emptyText">Nenhum confronto ainda. Quando você matar ou morrer para outro jogador, ele aparece aqui.</p>';
      return;
    }
    box.replaceChildren(...pvp.map((p) => {
      const card = document.createElement("div");
      card.className = "pvpCard";
      if (p.job) card.append(icon(p.job));
      else card.append(document.createElement("span"));
      const mid = document.createElement("div");
      mid.className = "pvpMid";
      const n = document.createElement("span");
      n.className = "cn";
      n.textContent = p.name;
      const seen = document.createElement("span");
      seen.className = "seen";
      seen.textContent = when(p.lastMs);
      seen.title = "Último confronto";
      mid.append(n, seen);
      const score = document.createElement("div");
      score.className = "pvpScore";
      score.innerHTML = '<span class="k" title="Vezes que você o matou"></span><span class="d" title="Vezes que ele matou você"></span>';
      score.querySelector(".k").textContent = `⚔ ${p.kills || 0}`;
      score.querySelector(".d").textContent = `☠ ${p.deaths || 0}`;
      card.append(mid, score);
      return card;
    }));
  };

  // ---------- Novidades ----------
  let whatsNew = null;
  let update = null;
  const unread = () => !!whatsNew?.lastSeen && news.isNewer(whatsNew.current, whatsNew.lastSeen);
  const renderNews = () => {
    const box = $(".versions");
    $(".navDot").hidden = !unread();
    if (!whatsNew?.versions?.length) {
      box.innerHTML = '<p class="emptyText">Nenhuma novidade registrada.</p>';
      return;
    }
    box.replaceChildren(...whatsNew.versions.map((v) => {
      const card = document.createElement("article");
      const current = v.version === whatsNew.current;
      card.className = "card version";
      card.classList.toggle("isCurrent", current);
      const head = document.createElement("div");
      head.className = "versionHead";
      head.innerHTML = '<span class="versionNum"></span><span class="versionTitle"></span><span class="versionDate"></span>';
      $(".versionNum", head).textContent = v.version;
      $(".versionTitle", head).textContent = v.title;
      $(".versionDate", head).textContent = news.formatDate(v.date);
      if (current) {
        const yours = document.createElement("span");
        yours.className = "yours";
        yours.textContent = "Sua versão";
        $(".versionTitle", head).after(yours);
      }
      const ul = document.createElement("ul");
      ul.className = "items";
      ul.append(...(v.items || []).map((item) => {
        const li = document.createElement("li");
        const chip = news.tagChip(item.tag);
        li.innerHTML = '<span class="kind"></span><span></span>';
        li.firstChild.classList.add(chip.cls);
        li.firstChild.textContent = chip.text;
        li.lastChild.textContent = item.text;
        return li;
      }));
      card.append(head, ul);
      return card;
    }));
  };
  // Read: the bar's pill and the dot here go away.
  const markNewsSeen = () => {
    if (!unread()) return;
    whatsNew.lastSeen = whatsNew.current;
    renderNews();
    invoke("copilot_whats_new_seen").catch(() => {});
  };
  const loadNews = async () => {
    const w = await invoke("copilot_whats_new").catch(() => null);
    if (w && Array.isArray(w.versions)) whatsNew = w;
    renderNews();
    if (!$(".view[data-view='novidades']").hidden) markNewsSeen();
  };

  // ---------- Ajustes: atualizações ----------
  const updateEls = {
    line: $(".updateLine"),
    bar: $(".updateBar"),
    fill: $(".updateFill"),
    check: $(".updateCheck"),
    skip: $(".updateSkip"),
    now: document.querySelectorAll(".updateNowBtn"),
  };
  // What the updater is doing, here and on the Novidades card.
  const renderUpdate = () => {
    const s = update || {};
    const { text, tone } = news.updateLine(s);
    updateEls.line.textContent = text;
    updateEls.line.className = `updateLine ${tone}`;
    $(".currentVersion").textContent = s.current ? `Versão instalada: ${s.current}` : "";
    const foot = $(".appVersion");
    foot.textContent = s.current ? `v${s.current} · ${news.versionFoot(s)}` : "";
    foot.classList.toggle("warn", news.updateOffered(s));
    foot.hidden = !s.current;
    updateEls.bar.hidden = s.state !== "baixando";
    updateEls.fill.style.width = `${Number(s.progress) || 0}%`;
    updateEls.check.disabled = ["indisponível", "verificando", "instalando"].includes(s.state);
    // "Atualizar agora" also takes back a skipped version, and retries after an error.
    const offer = !!s.latest && ["disponível", "pronta", "ignorada", "erro"].includes(s.state);
    for (const b of updateEls.now) {
      b.hidden = !offer;
      b.disabled = false;
    }
    updateEls.skip.hidden = !(s.latest && (s.state === "disponível" || s.state === "pronta"));
    $(".updateCard").hidden = !news.updateOffered(s);
    $(".updateCardText").textContent =
      s.state === "pronta" ? `Atualização ${s.latest} pronta para instalar`
        : s.state === "baixando" ? `Atualização ${s.latest} baixando… ${Number(s.progress) || 0}%`
          : `Atualização ${s.latest} disponível`;
    $(".updateCard .updateNowBtn").hidden = s.state === "baixando";
    for (const r of document.querySelectorAll('input[name="updateMode"]')) r.checked = r.value === (s.mode || "auto");
  };
  const setUpdate = (s) => {
    if (s && typeof s === "object") update = s;
    renderUpdate();
  };
  // A command that fails says why on the line.
  const updateFailed = (e) => setUpdate({ ...(update || {}), state: "erro", error: String(e?.message || e || "") || null });
  updateEls.check.addEventListener("click", () => {
    setUpdate({ ...(update || {}), state: "verificando" });
    invoke("copilot_update_check").then(setUpdate, updateFailed);
  });
  for (const b of updateEls.now) {
    b.addEventListener("click", () => {
      for (const o of updateEls.now) o.disabled = true;
      invoke("copilot_update_install").then(() => invoke("copilot_update_status").then(setUpdate), updateFailed);
    });
  }
  updateEls.skip.addEventListener("click", () => {
    if (update?.latest) invoke("copilot_update_skip", { version: update.latest }).then(setUpdate, updateFailed);
  });
  for (const r of document.querySelectorAll('input[name="updateMode"]')) {
    r.addEventListener("change", () => {
      if (!r.checked) return;
      bus?.settings.set("copilot.update.mode", r.value);
      if (update) update.mode = r.value;
      // Switched to automatic with a version waiting: it starts downloading.
      if (r.value === "auto" && update?.state === "disponível") invoke("copilot_update_check").then(setUpdate, updateFailed);
    });
  }
  bus?.listen("copilot-update-status", setUpdate);
  bus?.listen("copilot-update-progress", (p) => {
    if (update?.state !== "baixando") return;
    update.progress = Number(p?.pct) || 0;
    renderUpdate();
  });

  // ---------- Ajustes: sons ----------
  const preset = $(".soundPreset");
  const volume = $(".soundVolume");
  const soundNote = $(".soundNote");
  const PRESETS = ["sino", "ping", "alerta", "suave"];
  // The volume is kept as 0..1, the way copilot_play_sound takes it.
  const showSound = () => {
    const p = bus?.settings.peek("copilot.sound.preset", "sino");
    preset.value = PRESETS.includes(p) ? p : "sino";
    const v = Number(bus?.settings.peek("copilot.sound.volume", "0.7") ?? 0.7);
    volume.value = String(Math.round((Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0.7) * 100));
    $(".soundVolumeValue").textContent = `${volume.value}%`;
  };
  preset.addEventListener("change", () => bus?.settings.set("copilot.sound.preset", preset.value));
  volume.addEventListener("input", () => ($(".soundVolumeValue").textContent = `${volume.value}%`));
  // Kept when let go, not on every step of the drag.
  volume.addEventListener("change", () => bus?.settings.set("copilot.sound.volume", String(Number(volume.value) / 100)));
  $(".soundTest").addEventListener("click", () => {
    const vol = Number(volume.value) / 100;
    soundNote.classList.remove("isError");
    soundNote.textContent = vol === 0 ? "Volume no zero: nada toca." : "";
    if (vol === 0) return;
    invoke("copilot_play_sound", { preset: preset.value, volume: vol }).catch((e) => {
      soundNote.textContent = `Não tocou: ${e?.message || e}`;
      soundNote.classList.add("isError");
    });
  });

  // ---------- Ajustes: inicialização e atalhos ----------
  const autostart = $(".autostartOpt");
  const showAutostart = (on) => autostart.setAttribute("aria-checked", String(!!on));
  invoke("copilot_autostart", {}).then(showAutostart).catch(() => {});
  autostart.addEventListener("click", async () => {
    const want = autostart.getAttribute("aria-checked") !== "true";
    showAutostart(await invoke("copilot_autostart", { enable: want }).catch(() => !want));
  });

  // The meter's keys, with its defaults (app.rs, start_hotkeys); changed in
  // its own settings, which "Mudar" opens.
  const HOTKEYS = [
    ["dpsMeter.toggleWindowHotkey", "Mostrar/ocultar o medidor", "Ctrl+Alt+Up"],
    ["dpsMeter.lockHotkey", "Travar/destravar o medidor", "Ctrl+Alt+L"],
    ["dpsMeter.hotkey", "Recarregar dados", "Ctrl+Alt+R"],
  ];
  const showHotkeys = () => {
    $(".hotkeys").replaceChildren(...HOTKEYS.map(([key, name, fallback]) => {
      const li = document.createElement("li");
      li.innerHTML = '<span></span><span class="t"></span>';
      li.firstChild.textContent = name;
      li.lastChild.textContent = bus?.settings.peek(key, fallback) ?? fallback;
      return li;
    }));
    // Unset, the OS default (core.js globalHotkeysOn): off on Linux.
    const stored = bus?.settings.peek("dpsMeter.globalHotkeys");
    const on = stored === "true" || stored === "false" ? stored === "true" : !/Linux/.test(globalThis.navigator?.userAgent || "");
    $(".hotkeysOff").hidden = on;
  };
  $(".openMeterSettings").addEventListener("click", () => invoke("open_settings_window").catch(() => {}));

  // ---------- opened on a view ----------
  // The bar's pills open the Painel on a view through this setting, read at
  // start or heard while open, and cleared once shown.
  const GOTO_KEY = "copilot.painel.view";
  const goto = (view) => {
    if (!view) return;
    show(view);
    bus?.settings.set(GOTO_KEY, "");
    tauri?.window?.getCurrentWindow?.()?.setFocus?.()?.catch?.(() => {});
  };

  if (bus) {
    bus.settings.ready().then(() => {
      showSound();
      showHotkeys();
      goto(bus.settings.peek(GOTO_KEY));
    });
    bus.settings.on(GOTO_KEY, goto);
    bus.settings.on("copilot.sound.preset", showSound);
    bus.settings.on("copilot.sound.volume", showSound);
    bus.settings.on("copilot.update.mode", (mode) => {
      if (update) update.mode = mode || "auto";
      renderUpdate();
    });
    for (const [key] of HOTKEYS) bus.settings.on(key, showHotkeys);
    bus.settings.on("dpsMeter.globalHotkeys", showHotkeys);
    bus.settings.on("copilot.lastSeenVersion", (v) => {
      if (whatsNew && v) whatsNew.lastSeen = v;
      renderNews();
    });
  } else {
    showSound();
    showHotkeys();
  }
  loadNews();
  invoke("copilot_update_status").then(setUpdate, () => setUpdate(null));

  // ---------- status ----------
  const renderStatus = async () => {
    const s = await invoke("debug_status").catch(() => null);
    const el = $(".status");
    const live = !!(s && s.port);
    el.classList.toggle("isLive", live);
    $(".statusText").textContent = !s ? "—" : live ? "Capturando o AION 2" : s.aion2Window ? "AION 2 aberto, aguardando combate" : "Aguardando o AION 2";
  };

  const load = async () => {
    const [f, c, p, codes, schedule, ra] = await Promise.all([
      invoke("get_fight_history").catch(() => []),
      invoke("copilot_characters").catch(() => []),
      invoke("copilot_pvp").catch(() => []),
      worldBossCodes.size ? null : invoke("copilot_field_boss_codes").catch(() => []),
      timerRules.length ? null : invoke("copilot_timer_rules").catch(() => null),
      invoke("copilot_routine_state").catch(() => null),
    ]);
    if (ra && typeof ra === "object") routineAuto = ra;
    if (Array.isArray(codes)) worldBossCodes = new Set(codes);
    if (Array.isArray(schedule?.rules)) timerRules = schedule.rules;
    pvp = Array.isArray(p) ? p : [];
    fights = (Array.isArray(f) ? f : []).filter((x) => !x.isLive).sort((a, b) => b.startTimeMs - a.startTimeMs);
    chars = Array.isArray(c) ? c : [];
    renderHome();
    renderFights();
    renderChars();
    renderRoutine();
    renderBosses();
    renderPvp();
    renderStatus();
  };
  window.copilotPainelReload = load;
  load();
  setInterval(load, 15_000);
  setInterval(() => {
    renderHome();
    renderBosses();
  }, 30_000);
})();

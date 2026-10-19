// aion-copilot: Mascotes overlay — the monsters in view by species ("Perto":
// the soul creatures first, then the species whose pet you lack or have at a
// low level, with distance and direction, and whether its pet is known yet)
// and your collection ("Coleção"), from `copilot_pet_radar`, asked every
// second. Names are yours: the game data has none, so a pet takes the name
// of a species known to give it, else "Mascote #id", until you name it
// (copilot-pet-names.json). Needs petsCore.js.
(() => {
  const { invoke, svg } = window.copilotOverlay;
  const { MAX_LEVEL, petLabel, soulsText, speciesStatus, speciesSub, viewKey } = window.copilotPetsCore;
  const TAB_KEY = "copilot.pets.tab";
  const MISSING_KEY = "copilot.pets.missingOnly";
  const POINT_NAMES = { N: "norte", NE: "nordeste", L: "leste", SE: "sudeste", S: "sul", SO: "sudoeste", O: "oeste", NO: "noroeste" };
  const lang = document.documentElement.lang || undefined;

  const $ = (sel) => document.querySelector(sel);
  const who = $(".who");
  const tabs = [...document.querySelectorAll(".tab")];
  const panes = { near: $(".nearPane"), collection: $(".collectionPane") };
  const nearList = $(".nearList");
  const nearEmpty = $(".nearEmpty");
  const nearNote = $(".nearNote");
  const chips = $(".chips");
  const collectionList = $(".collectionList");
  const collectionEmpty = $(".collectionEmpty");
  const collectionCount = $(".collectionCount");
  const missingOnly = $(".missingOnly");

  const store = {
    get: (k, fallback) => {
      try {
        return localStorage.getItem(k) ?? fallback;
      } catch {
        return fallback;
      }
    },
    set: (k, v) => {
      try {
        localStorage.setItem(k, v);
      } catch {
        // kept for this session only
      }
    },
  };

  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  };
  const PENCIL = svg('<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M13.5 6.5l4 4"/>');
  const ARROW = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20V4M5 11l7-7 7 7"/></svg>';

  let data = null;
  let lastKey = "";
  // The pet being named: the list is not redrawn under the input.
  let editing = null;
  let tab = store.get(TAB_KEY, "near") === "collection" ? "collection" : "near";
  missingOnly.checked = store.get(MISSING_KEY, "false") === "true";

  const levelClass = (level) => (level == null ? "" : `lv${Math.min(level, MAX_LEVEL)}`);
  const ago = (ms) => {
    const s = Math.max(0, Math.round(ms / 1000));
    if (s < 60) return `há ${s} s`;
    const m = Math.round(s / 60);
    return m < 60 ? `há ${m} min` : `há ${Math.round(m / 60)} h`;
  };

  // The name, its pencil, and the inline input that replaces them (Enter
  // saves, Esc or leaving the field gives up).
  const nameLine = (p) => {
    const line = el("span", "nameLine");
    if (editing === p.pet) {
      const input = el("input", "nameInput");
      input.type = "text";
      input.maxLength = 40;
      input.value = p.name || "";
      input.placeholder = p.speciesName || `Mascote #${p.pet}`;
      input.setAttribute("aria-label", `Nome do mascote #${p.pet}`);
      let done = false;
      const finish = async (save) => {
        if (done) return;
        done = true;
        if (save) await invoke("copilot_pet_name", { pet: p.pet, name: input.value.trim() || null });
        editing = null;
        lastKey = "";
        await refresh();
      };
      input.addEventListener("keydown", (e) => {
        if (e.key === "Enter") finish(true);
        else if (e.key === "Escape") finish(false);
      });
      input.addEventListener("blur", () => finish(false));
      line.append(input);
      requestAnimationFrame(() => {
        input.focus();
        input.select();
      });
      return line;
    }
    const name = el("span", `petName${p.name || p.speciesName ? "" : " isAnon"}`, petLabel(p));
    name.title = p.name || p.speciesName ? `${petLabel(p)} (mascote #${p.pet})` : `Mascote #${p.pet}`;
    const pencil = el("button", "pencil");
    pencil.type = "button";
    pencil.innerHTML = PENCIL;
    pencil.title = p.name ? "Renomear" : "Dar um nome";
    pencil.setAttribute("aria-label", pencil.title);
    pencil.addEventListener("click", () => {
      editing = p.pet;
      render(true);
    });
    line.append(name, pencil);
    return line;
  };

  const levelLine = (p, withSouls) => {
    const sub = el("span", "petSub");
    if (p.level == null) {
      sub.textContent = "nível ? · faça login para ler a coleção";
      return sub;
    }
    const tag = el("span", "lvTag", p.level > 0 ? `Nv. ${p.level}` : "Nv. 0 (falta)");
    sub.append(tag);
    if (withSouls) sub.append(` · ${soulsText(p.souls, p.estimated)}`);
    if (p.estimated) sub.title = "Almas somadas desde o login (estimativa)";
    return sub;
  };

  // ===== Perto =====
  const POINTS_TITLE = (d) => POINT_NAMES[d] || d || "";
  // Distance, compass point and an arrow (north up, as on the map).
  const whereCell = (p) => {
    const where = el("span", "where");
    if (p.distanceM != null) {
      const arrow = el("span", "arrow");
      arrow.innerHTML = ARROW;
      arrow.querySelector("svg").style.transform = `rotate(${p.bearing || 0}deg)`;
      const dir = el("span", "dir", p.direction || "");
      where.append(`${Math.round(p.distanceM)} m`, dir, arrow);
      where.title = `${Math.round(p.distanceM)} m ao ${POINTS_TITLE(p.direction)} (norte para cima, como no mapa) · visto ${ago(p.seenAgoMs)}`;
    } else {
      where.append("—");
      where.title = `Distância: sua posição ainda não é conhecida · visto ${ago(p.seenAgoMs)}`;
    }
    return where;
  };
  // One species in view: its name and how many, the nearest one's place, and
  // its pet's standing in your collection as a chip.
  const speciesRow = (s) => {
    const st = speciesStatus(s);
    const li = el("li", `pet species st-${st.cls}${s.soul ? " isSoul" : ""}`);
    const line = el("span", "nameLine");
    const name = el("span", "petName", s.name);
    name.title = s.name;
    line.append(name);
    if (s.count > 1) line.append(el("span", "speciesCount", `×${s.count}`));
    const sub = el("span", "petSub");
    if (s.soul) {
      const soul = el("span", "chip soulChip", "Alma");
      soul.title = "Criatura que carrega a alma de um mascote";
      sub.append(soul);
    }
    const chip = el("span", `chip stChip st-${st.cls}`, st.text);
    chip.title = st.title;
    // The full line on hover: at the overlay's narrowest it may be cut.
    const text = el("span", "subText", speciesSub(s));
    text.title = s.estimated ? `${speciesSub(s)} (almas somadas desde o login: estimativa)` : speciesSub(s);
    sub.append(chip, text);
    if (s.entity != null) li.title = `código ${s.code} · entidade ${s.entity} (a mais perto)${s.pet != null ? ` · mascote #${s.pet}` : ""}`;
    li.append(el("span", "dot"), line, whereCell(s), sub);
    return li;
  };
  const renderNear = () => {
    const species = data?.species || [];
    const notes = [];
    if (data && !data.hasCollection) notes.push("Sua coleção ainda não foi lida: ela chega no próximo login (ou carregamento de zona).");
    if (data?.game && species.length && !data.position) notes.push("A distância aparece quando sua posição for conhecida (use uma habilidade ou buff).");
    if (data?.game && species.some((s) => s.status === "unknown")) {
      const n = data.learnedSpecies || 0;
      notes.push(`Mascote ?: ainda não aprendido (${n === 1 ? "1 já conhecida" : `${n} já conhecidas`})`);
    }
    nearNote.textContent = notes.join(" ");
    nearNote.hidden = !notes.length;
    if (!data) {
      nearEmpty.textContent = "Carregando…";
    } else if (!data.game) {
      nearEmpty.textContent = "O jogo está fechado. Abra o AION 2 para ver as criaturas por perto.";
    } else if (data.unidentified > 0) {
      // Spawned before the app started: the game says what they are only
      // when they show up again.
      nearEmpty.textContent = "As criaturas por perto já estavam aqui antes do app abrir: elas entram na lista quando reaparecerem ou quando você andar até outras.";
    } else {
      nearEmpty.textContent = "Nenhuma criatura por perto. Ande por áreas com monstros.";
    }
    const show = !!data?.game && species.length > 0;
    nearList.hidden = !show;
    nearEmpty.hidden = show;
    if (show) nearList.replaceChildren(...species.map(speciesRow));
  };

  // ===== Coleção =====
  const collectionRow = (p) => {
    const li = el("li", `pet ${levelClass(p.level)}`);
    const souls = el("span", "souls", soulsText(p.souls, p.estimated));
    if (p.estimated) souls.title = "Almas somadas desde o login (estimativa)";
    li.append(el("span", "dot"), nameLine(p), souls, levelLine(p, false));
    return li;
  };
  const GROUPS = [
    [0, "Sem o mascote (nível 0)"],
    [1, "Nível 1"],
    [2, "Nível 2"],
    [3, "Nível 3"],
  ];
  const renderCollection = () => {
    const s = data?.summary;
    const counts = s?.byLevel || [0, 0, 0, 0];
    chips.replaceChildren(
      ...[3, 2, 1, 0].map((lv) => {
        const chip = el("span", `chip lv${lv}`, lv ? `Nv.${lv} ` : "sem ");
        chip.append(el("b", null, String(counts[lv] || 0)));
        return chip;
      }),
    );
    collectionCount.textContent = s ? `${s.owned} de ${s.known} conhecidos` : "";
    const all = data?.collection || [];
    const list = missingOnly.checked ? all.filter((p) => p.level < MAX_LEVEL) : all;
    const show = list.length > 0;
    collectionList.hidden = !show;
    collectionEmpty.hidden = show;
    if (!data) collectionEmpty.textContent = "Carregando…";
    else if (!data.hasCollection) collectionEmpty.textContent = "Coleção ainda não lida: entre no jogo (login ou carregamento de zona) com o app aberto.";
    else collectionEmpty.textContent = "Todos os mascotes conhecidos já estão no nível máximo.";
    if (!show) return;
    const rows = [];
    for (const [lv, title] of GROUPS) {
      const of = list.filter((p) => Math.min(p.level, MAX_LEVEL) === lv);
      if (!of.length) continue;
      rows.push(el("li", "secHead", `${title} · ${of.length}`));
      of.sort((a, b) => b.souls - a.souls || a.pet - b.pet);
      rows.push(...of.map(collectionRow));
    }
    collectionList.replaceChildren(...rows);
  };

  const renderWho = () => {
    who.replaceChildren();
    if (!data) return;
    const name = data.character || "personagem desconhecido";
    who.append(name);
    if (data.live) {
      who.append(" · ", el("span", "isLive", "ao vivo"));
    } else if (data.seenMs) {
      const d = new Date(data.seenMs);
      who.append(` · coleção de ${d.toLocaleDateString(lang, { day: "2-digit", month: "2-digit" })} ${d.toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" })}`);
    }
    if (data.active) {
      const active = data.activeName || `#${data.active}`;
      who.title = `Mascote ativo: ${active}`;
    } else {
      who.removeAttribute("title");
    }
  };

  const render = (force) => {
    if (editing != null && !force) return;
    renderWho();
    if (tab === "near") renderNear();
    else renderCollection();
  };

  const setTab = (next) => {
    tab = next;
    store.set(TAB_KEY, next);
    for (const t of tabs) t.setAttribute("aria-selected", String(t.dataset.tab === next));
    panes.near.hidden = next !== "near";
    panes.collection.hidden = next !== "collection";
    editing = null;
    render(true);
  };
  for (const t of tabs) t.addEventListener("click", () => setTab(t.dataset.tab));
  missingOnly.addEventListener("change", () => {
    store.set(MISSING_KEY, String(missingOnly.checked));
    render(true);
  });

  // Redrawn only when something shown changed (the age of each sighting
  // ticks every second, so it is left out).
  const refresh = async () => {
    const next = await invoke("copilot_pet_radar");
    if (!next) return;
    data = next;
    const key = viewKey(next);
    if (key === lastKey) return;
    lastKey = key;
    render(false);
  };

  setTab(tab);
  refresh();
  setInterval(refresh, 1000);
})();

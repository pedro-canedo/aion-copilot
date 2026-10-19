// aion-copilot: Group overlay — your party from the roster packet: class,
// level, combat power, and who is down. Refreshed every second.
(() => {
  const { invoke, svg } = window.copilotOverlay;
  const list = document.querySelector(".members");
  const empty = document.querySelector(".empty");
  const summary = document.querySelector(".summary");
  const lang = document.documentElement.lang || undefined;
  const skull = svg('<path d="M12 3a7 7 0 0 0-7 7c0 2.4 1.2 4.1 3 5.2V19h8v-3.8c1.8-1.1 3-2.8 3-5.2a7 7 0 0 0-7-7z"/><circle cx="9.5" cy="10.5" r="1.3" fill="currentColor"/><circle cx="14.5" cy="10.5" r="1.3" fill="currentColor"/><path d="M10 19v2M14 19v2"/>');

  let lastKey = "";
  const render = (members) => {
    const key = JSON.stringify(members);
    if (key === lastKey) return;
    lastKey = key;
    const has = members.length > 0;
    list.hidden = !has;
    empty.hidden = has;
    if (!has) {
      summary.textContent = "—";
      list.replaceChildren();
      return;
    }
    const down = members.filter((m) => m.dead).length;
    const avgCp = Math.round(members.reduce((sum, m) => sum + (m.combatPower || 0), 0) / members.length);
    summary.textContent = `${members.length} membros · CP médio ${avgCp.toLocaleString(lang)}${down ? ` · ${down} caído${down > 1 ? "s" : ""}` : ""}`;
    list.replaceChildren(
      ...members.map((m) => {
        const li = document.createElement("li");
        li.className = "member";
        li.classList.toggle("isLocal", m.isLocal);
        li.classList.toggle("isDead", m.dead);
        const icon = document.createElement("img");
        icon.alt = "";
        if (m.job) icon.src = `/assets/${m.job}.png`;
        else icon.style.visibility = "hidden";
        const name = document.createElement("span");
        name.className = "name";
        name.textContent = m.name;
        if (m.dead) {
          const mark = document.createElement("span");
          mark.className = "down";
          mark.title = "Caído";
          mark.innerHTML = skull;
          name.appendChild(mark);
        }
        const lvl = document.createElement("span");
        lvl.className = "lvl";
        lvl.textContent = m.level ? `Nv ${m.level}` : "";
        const cp = document.createElement("span");
        cp.className = "cp";
        cp.textContent = m.combatPower ? m.combatPower.toLocaleString(lang) : "—";
        cp.title = m.gearScore ? `Item level ${m.gearScore.toLocaleString(lang)}` : "";
        li.append(icon, name, lvl, cp);
        return li;
      }),
    );
  };

  const refresh = async () => {
    const members = await invoke("copilot_party");
    render(Array.isArray(members) ? members : []);
  };
  // For checking the layout from DevTools with sample members.
  window.copilotGroupRender = render;

  refresh();
  setInterval(refresh, 1000);
})();

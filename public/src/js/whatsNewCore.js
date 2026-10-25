// aion-copilot: the pure part of the Novidades page and of the update pills —
// versions compared, a changelog date in pt-BR, what the updater's state
// reads as and which pills the bar shows — for painel.js, hub.js (and the
// tests). No DOM here.
(() => {
  const MONTHS = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];

  // "1.10.0" > "1.9.2": by number, part by part; anything after a "-" or "+"
  // (a pre-release) is left out. Not a version: compared as text.
  const parse = (v) => {
    const m = /^v?(\d+)\.(\d+)\.(\d+)/.exec(String(v ?? "").trim());
    return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
  };
  const compareVersions = (a, b) => {
    const pa = parse(a);
    const pb = parse(b);
    if (!pa || !pb) return String(a ?? "").localeCompare(String(b ?? ""));
    for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] < pb[i] ? -1 : 1;
    return 0;
  };
  const isNewer = (a, b) => !!parse(a) && !!parse(b) && compareVersions(a, b) > 0;

  // "2026-10-24" -> "24 de outubro"; another year than `now`'s gets it too.
  // Read from the text, never through Date: "2026-10-24" as a Date is UTC
  // midnight, the 23rd in Brazil.
  const formatDate = (iso, now = new Date()) => {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(iso ?? ""));
    if (!m) return String(iso ?? "");
    const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])];
    if (month < 1 || month > 12) return String(iso);
    const text = `${day} de ${MONTHS[month - 1]}`;
    return year === now.getFullYear() ? text : `${text} de ${year}`;
  };

  // An item's chip: its text and class (the colour).
  const TAGS = {
    novo: { text: "Novo", cls: "novo" },
    melhorado: { text: "Melhorado", cls: "melhorado" },
    corrigido: { text: "Corrigido", cls: "corrigido" },
  };
  const tagChip = (tag) => TAGS[tag] || { text: String(tag || ""), cls: "outro" };

  // A newer version the player can take: found, downloading or downloaded.
  const OFFERED = ["disponível", "baixando", "pronta"];
  const updateOffered = (status) => !!status?.latest && OFFERED.includes(status.state);

  // The updater's state (copilot_update_status), as one line, and its tone:
  // "ok", "warn", "error" or "" (neutral).
  const updateLine = (status) => {
    const s = status || {};
    const v = s.latest || "";
    switch (s.state) {
      case "indisponível":
        return { text: "Atualização automática indisponível nesta versão.", tone: "" };
      case "verificando":
        return { text: "Procurando versão nova…", tone: "" };
      case "atualizado":
        return { text: "Você está na versão mais nova.", tone: "ok" };
      case "disponível":
        return { text: `Versão ${v} disponível.`, tone: "warn" };
      case "ignorada":
        return { text: `Você pulou a versão ${v}.`, tone: "" };
      case "baixando":
        return { text: `Baixando a versão ${v}… ${Number(s.progress) || 0}%`, tone: "warn" };
      case "pronta":
        // Outside "auto" it goes in by itself only after the click.
        if (!s.asked && (s.mode === "notify" || s.mode === "off")) {
          return { text: `Versão ${v} baixada e conferida. Instala quando você clicar em Atualizar agora.`, tone: "ok" };
        }
        return { text: `Versão ${v} baixada e conferida. Instala quando o AION 2 fechar ou quando você sair do app.`, tone: "ok" };
      case "instalando":
        return { text: `Instalando a versão ${v}. O app fecha e abre de novo sozinho.`, tone: "warn" };
      case "erro":
        return { text: `Não deu para atualizar: ${s.error || "erro desconhecido"}`, tone: "error" };
      default:
        return { text: s.mode === "off" ? "Desligada: só procura quando você pedir." : "Ainda não procurou versão nova.", tone: "" };
    }
  };

  // Next to the installed version, in a few words: a newer one on its way,
  // or how updates arrive.
  const versionFoot = (status) => {
    const s = status || {};
    if (s.state === "indisponível") return "sem atualização automática";
    if (s.state === "instalando") return `instalando a ${s.latest}`;
    if (s.state === "pronta") return `${s.latest} pronta para instalar`;
    if (s.state === "baixando") return `baixando a ${s.latest}`;
    if (s.state === "disponível") return `${s.latest} disponível`;
    if (s.mode === "notify") return "avisa quando sair versão nova";
    if (s.mode === "off") return "atualização desligada";
    return "atualiza sozinho";
  };

  // The bar's pills: "Novidades da X" while this version's notes are unread
  // (`whatsNew` from copilot_whats_new), "Atualização X" while a newer
  // version is found or downloaded.
  const hubPills = (whatsNew, status) => {
    const pills = [];
    if (whatsNew?.lastSeen && isNewer(whatsNew.current, whatsNew.lastSeen)) {
      pills.push({ kind: "news", view: "novidades", text: `Novidades da ${whatsNew.current}` });
    }
    if (status?.latest && (status.state === "disponível" || status.state === "pronta")) {
      pills.push({ kind: "update", view: "ajustes", text: `Atualização ${status.latest}` });
    }
    return pills;
  };

  window.copilotWhatsNew = { compareVersions, isNewer, formatDate, tagChip, updateOffered, updateLine, versionFoot, hubPills };
})();

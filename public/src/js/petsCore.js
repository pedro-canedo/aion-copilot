// aion-copilot: the Mascotes overlay's pure part — what a pet is called and
// how a species in view stands against your collection — for pets.js (and
// the tests). No DOM here.
(() => {
  const MAX_LEVEL = 3;
  const lang = "pt-BR";

  // Your name for a pet, else the name of a species known to give it, else
  // its id.
  const petLabel = (p) => p?.name || p?.speciesName || `Mascote #${p?.pet}`;

  const soulsText = (souls, estimated) => {
    if (souls == null) return "";
    const n = Number(souls).toLocaleString(lang);
    return `${estimated ? "~" : ""}${n} ${souls === 1 ? "alma" : "almas"}`;
  };

  // The chip of a species row (`status` from copilot_pet_radar): its text,
  // its class (the colour) and what it means.
  const speciesStatus = (s) => {
    const level = Math.min(Number(s?.level) || 0, MAX_LEVEL);
    switch (s?.status) {
      case "missing":
        return { cls: "missing", text: "Falta na coleção", title: "Você ainda não tem este mascote: as almas dele vêm desta criatura." };
      case "upgrade":
        return { cls: "upgrade", text: `Nv.${level} · upar`, title: `Você o tem no nível ${level} de ${MAX_LEVEL}: vale caçar para subir.` };
      case "complete":
        return { cls: "complete", text: "Completo", title: `Já está no nível ${MAX_LEVEL}.` };
      case "unread":
        return { cls: "unread", text: "Nv.? · coleção não lida", title: "O mascote é conhecido, mas sua coleção ainda não foi lida (login ou carregamento de zona)." };
      default:
        return {
          cls: "unknown",
          text: "Mascote ?",
          title: "Ainda não se sabe se esta criatura dá mascote: o app aprende quando uma alma cai logo depois de você derrotar uma.",
        };
    }
  };

  // The line under a species' name: its pet (by name, unless that is the
  // species' own) and your souls of it, or that it is not known to give one.
  const speciesSub = (s) => {
    if (s?.pet == null) return "ainda não se sabe se dá mascote";
    const parts = [s.petName && s.petName !== s.name ? s.petName : `Mascote #${s.pet}`];
    if (s.souls != null && s.level != null) parts.push(soulsText(s.souls, s.estimated));
    return parts.join(" · ");
  };

  // What the overlay draws from, without the ages that tick every second:
  // a change of it is a reason to redraw.
  const viewKey = (view) => {
    if (!view) return "";
    const strip = (list) => (list || []).map(({ seenAgoMs, ...rest }) => rest);
    return JSON.stringify({ ...view, nearby: strip(view.nearby), species: strip(view.species) });
  };

  window.copilotPetsCore = { MAX_LEVEL, petLabel, soulsText, speciesStatus, speciesSub, viewKey };
})();

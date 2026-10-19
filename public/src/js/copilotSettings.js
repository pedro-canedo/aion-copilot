// aion-copilot: Settings for the people who just use it. The meter's own
// technical settings (network adapter and port, plugins, advanced, themes and
// textures) stay for developer mode: debug builds, or AION_COPILOT_DEV=1.
(() => {
  if ((document.documentElement.dataset.a2view || "main") !== "settings") return;
  const invoke = (cmd, args) => window.__TAURI__?.core?.invoke?.(cmd, args) ?? Promise.resolve(null);

  const DEV_SECTIONS = ["connection", "account", "plugins", "advanced"];
  // Rows by their label's i18n key; anything not listed here stays visible.
  const DEV_ROWS = [
    "settings.defaultMeterMode.label",
    "settings.partyStat.label",
    "settings.playerLimit.label",
    "settings.detailsMonitor.label",
    "settings.showHealModes",
    "settings.showTotalDps",
    "settings.roundDps",
    "settings.showDpsSuffix",
    "settings.showSuspendBtn",
    "settings.showLockBtn",
    "settings.theme.label",
    "settings.barTexture.label",
    "settings.textureOffset",
    "settings.meterLayout.label",
    "settings.titleOpacity.label",
    "settings.meterSaturation.label",
    "settings.barGap.label",
    "settings.bossNameSize.label",
    "settings.bold.label",
  ];

  const mark = () => {
    for (const id of DEV_SECTIONS) {
      document.querySelector(`.settingsNavItem[data-section="${id}"]`)?.classList.add("copilotDevOnly");
      document.querySelector(`.settingsSection[data-section="${id}"]`)?.classList.add("copilotDevOnly");
    }
    for (const key of DEV_ROWS) {
      for (const label of document.querySelectorAll(`[data-i18n="${key}"]`)) {
        (label.closest(".settingsRow") || label.parentElement)?.classList.add("copilotDevOnly");
      }
    }
  };

  // The first section a user may see, if the open one is a developer's.
  const showFirstVisible = () => {
    const active = document.querySelector(".settingsNavItem.isActive");
    if (active && !active.classList.contains("copilotDevOnly")) return;
    document.querySelector(".settingsNavItem:not(.copilotDevOnly)")?.click();
  };

  const version = () => {
    const el = document.querySelector(".settingsVersionValue");
    const v = window.__TAURI__?.app?.getVersion?.();
    if (el && v) v.then((n) => (el.textContent = `aion-copilot v${n}`)).catch(() => {});
  };

  const apply = async () => {
    const dev = await invoke("copilot_dev_mode").catch(() => false);
    document.documentElement.classList.toggle("copilotUserSettings", !dev);
    mark();
    if (!dev) showFirstVisible();
    version();
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", apply);
  else apply();
  // Settings re-renders its labels with the language: mark again after.
  setTimeout(apply, 1500);
})();

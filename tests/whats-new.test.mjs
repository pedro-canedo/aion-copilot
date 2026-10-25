import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

function whatsNewCore() {
  const context = vm.createContext({});
  context.window = context;
  vm.runInContext(readFileSync(new URL("../public/src/js/whatsNewCore.js", import.meta.url), "utf8"), context);
  return context.copilotWhatsNew;
}

const W = whatsNewCore();

test("versions compare by number, part by part, not as text", () => {
  assert.equal(W.compareVersions("1.10.0", "1.9.2"), 1);
  assert.equal(W.compareVersions("1.1.0", "1.1.0"), 0);
  assert.equal(W.compareVersions("1.0.9", "1.1.0"), -1);
  assert.equal(W.compareVersions("v2.0.0", "1.99.99"), 1);
  assert.equal(W.isNewer("1.1.0", "1.0.0"), true);
  assert.equal(W.isNewer("1.1.0", "1.1.0"), false);
  assert.equal(W.isNewer("1.0.0", "1.1.0"), false);
  // Not a version on either side: never newer.
  assert.equal(W.isNewer("1.1.0", null), false);
  assert.equal(W.isNewer("abc", "1.0.0"), false);
});

test("a changelog date reads in pt-BR, the day as written, the year only when another", () => {
  const now = new Date(2026, 9, 24, 23, 59);
  assert.equal(W.formatDate("2026-10-24", now), "24 de outubro");
  assert.equal(W.formatDate("2026-03-01", now), "1 de março");
  assert.equal(W.formatDate("2025-12-31", now), "31 de dezembro de 2025");
  // Never moved a day by a time zone: the text is read, not a Date.
  assert.equal(W.formatDate("2026-01-01", new Date(2026, 0, 1, 0, 0)), "1 de janeiro");
  assert.equal(W.formatDate("2026-13-01", now), "2026-13-01");
  assert.equal(W.formatDate("ontem", now), "ontem");
});

test("each item tag has its chip; an unknown one keeps its text", () => {
  assert.equal(W.tagChip("novo").text, "Novo");
  assert.equal(W.tagChip("melhorado").text, "Melhorado");
  assert.equal(W.tagChip("corrigido").text, "Corrigido");
  assert.equal(W.tagChip("corrigido").cls, "corrigido");
  assert.equal(W.tagChip("removido").cls, "outro");
  assert.equal(W.tagChip("removido").text, "removido");
});

test("the updater's state reads as one line with its tone", () => {
  assert.deepEqual({ ...W.updateLine({ state: "atualizado" }) }, { text: "Você está na versão mais nova.", tone: "ok" });
  assert.equal(W.updateLine({ state: "disponível", latest: "1.2.0" }).text, "Versão 1.2.0 disponível.");
  assert.equal(W.updateLine({ state: "baixando", latest: "1.2.0", progress: 42 }).text, "Baixando a versão 1.2.0… 42%");
  assert.match(W.updateLine({ state: "pronta", latest: "1.2.0" }).text, /AION 2 fechar/);
  // Downloaded in "auto", then switched to "notify": waits for the click.
  assert.match(W.updateLine({ state: "pronta", latest: "1.2.0", mode: "notify" }).text, /clicar em Atualizar agora/);
  assert.match(W.updateLine({ state: "pronta", latest: "1.2.0", mode: "off", asked: true }).text, /AION 2 fechar/);
  assert.equal(W.updateLine({ state: "erro", error: "hash diferente" }).tone, "error");
  assert.match(W.updateLine({ state: "erro", error: "hash diferente" }).text, /hash diferente/);
  assert.match(W.updateLine({ state: "indisponível" }).text, /indisponível/);
  assert.match(W.updateLine({ state: "ocioso", mode: "off" }).text, /Desligada/);
  assert.match(W.updateLine(null).text, /Ainda não/);
});

test("a newer version is offered while found, downloading or downloaded, never when skipped", () => {
  assert.equal(W.updateOffered({ state: "disponível", latest: "1.2.0" }), true);
  assert.equal(W.updateOffered({ state: "baixando", latest: "1.2.0" }), true);
  assert.equal(W.updateOffered({ state: "pronta", latest: "1.2.0" }), true);
  assert.equal(W.updateOffered({ state: "ignorada", latest: "1.2.0" }), false);
  assert.equal(W.updateOffered({ state: "atualizado", latest: null }), false);
  assert.equal(W.updateOffered(null), false);
});

test("the bar's pills: unread notes of this version, and a newer version found or downloaded", () => {
  const kinds = (w, s) => [...W.hubPills(w, s).map((p) => `${p.kind}:${p.view}:${p.text}`)];
  assert.deepEqual(kinds({ current: "1.1.0", lastSeen: "1.0.0" }, null), ["news:novidades:Novidades da 1.1.0"]);
  assert.deepEqual(kinds({ current: "1.1.0", lastSeen: "1.1.0" }, null), []);
  // Never read anything (no lastSeen): no news to point at.
  assert.deepEqual(kinds({ current: "1.1.0", lastSeen: null }, null), []);
  assert.deepEqual(kinds(null, { state: "disponível", latest: "1.2.0" }), ["update:ajustes:Atualização 1.2.0"]);
  assert.deepEqual(kinds(null, { state: "pronta", latest: "1.2.0" }), ["update:ajustes:Atualização 1.2.0"]);
  assert.deepEqual(kinds(null, { state: "ignorada", latest: "1.2.0" }), []);
  assert.deepEqual(kinds(null, { state: "atualizado", latest: null }), []);
  assert.equal(W.hubPills({ current: "1.1.0", lastSeen: "1.0.0" }, { state: "pronta", latest: "1.2.0" }).length, 2);
});

test("next to the installed version: a newer one on its way, or how updates arrive", () => {
  assert.equal(W.versionFoot({ state: "atualizado", mode: "auto" }), "atualiza sozinho");
  assert.equal(W.versionFoot({ state: "atualizado", mode: "notify" }), "avisa quando sair versão nova");
  assert.equal(W.versionFoot({ state: "nunca", mode: "off" }), "atualização desligada");
  assert.equal(W.versionFoot({ state: "baixando", latest: "1.2.0", mode: "auto" }), "baixando a 1.2.0");
  assert.equal(W.versionFoot({ state: "pronta", latest: "1.2.0" }), "1.2.0 pronta para instalar");
  assert.equal(W.versionFoot({ state: "indisponível" }), "sem atualização automática");
  assert.equal(W.versionFoot(null), "atualiza sozinho");
});

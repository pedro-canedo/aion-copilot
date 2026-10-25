# aion-copilot

[![Licença](https://img.shields.io/badge/licen%C3%A7a-GPL--3.0-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/pedro-canedo/aion-copilot)](https://github.com/pedro-canedo/aion-copilot/releases/latest)

Companion para **AION 2** no Windows: medidor de dano, rotina automática, radar de mascotes,
timers, chefes de campo, mapa e PvP em overlays leves sobre o jogo. Tudo é lido direto do
tráfego do jogo, sem conta e sem servidor. Projeto pessoal, gratuito e não comercial.

**Site, guia e download: https://pedro-canedo.github.io/aion-copilot/**

## Instalação

1. Instale o [Npcap](https://npcap.com/#download) com as opções padrão (*WinPcap API-compatible Mode* marcado).
2. Baixe o [instalador da versão mais recente](https://github.com/pedro-canedo/aion-copilot/releases/latest/download/aion-copilot_x64-setup.exe)
   (ou escolha na [página de releases](https://github.com/pedro-canedo/aion-copilot/releases)) e rode. Ele instala por usuário,
   sem pedir administrador, e depois se atualiza sozinho.
3. O instalador ainda não é assinado: no aviso do SmartScreen, clique em **Mais informações → Executar assim mesmo**.
4. Abra o aion-copilot antes de entrar no jogo, para que ele veja os nomes desde a tela de carregamento.

Requisitos: Windows 10/11 64 bits, Npcap e WebView2 (o instalador baixa o WebView2 se faltar).

## O que tem

- **Medidor**: dano, DPS, cura, CP, mortes e ressurreições, modo Auto (chefe mundial, expedição, treino, monstros, PvP), detalhes por habilidade e histórico.
- **Alvo e PvP**: vida do alvo, tempo para matar, GS/CP do jogador e placar de abates e mortes.
- **Rotina**: Odyle, tempo e pontos do Abismo, kinah do dia, presença, missões de desafio, arena e checklist que se preenche e zera sozinho.
- **Mascotes**: radar por espécie contra a sua coleção ("Falta na coleção", "upar", "Completo"), com o vínculo espécie → mascote aprendido no jogo.
- **Timers e chefes de campo**: eventos agendados, resets e a volta de cada chefe.
- **Mapa**: posição, camadas e pontos de coleta que você rotula uma vez.
- **Grupo**, **Painel** (histórico, personagens, PvP) e **OBS** (fonte de navegador local).

O guia completo de uso está no [site](https://pedro-canedo.github.io/aion-copilot/#guia), e a
documentação técnica em [COPILOT.md](COPILOT.md).

## Compilar

```bash
npm install
npm run dev:offline      # modo desenvolvimento
npm run build:offline    # instalador em src-tauri/target/release/bundle/nsis/
cd src-tauri && cargo test --lib --no-default-features --features desktop,obs
node --test tests/*.test.mjs
```

O build `offline` deixa de fora todos os serviços online do upstream (conta, uploads, atualizador).

## Créditos e licença

- Fork do [A2Tools DPS Meter](https://github.com/taengu/A2Tools-DPS-Meter) (GPL-3.0), que faz a captura
  e o medidor base. Este projeto segue sob a [GPL-3.0](LICENSE).
- Dados do mapa do [AION2 Interactive Map](https://github.com/aion2-interactive-map/aion2-interactive-map)
  de tc-imba, sob CC BY-NC 4.0 (ver [`src/data/map/ATTRIBUTION.md`](src/data/map/ATTRIBUTION.md)).
- Fontes Sora e JetBrains Mono, sob SIL OFL 1.1.

Projeto de fã, sem vínculo com a NCSOFT nem com o Shugo.GG. AION e a arte do jogo são marcas e
propriedade da NCSOFT. O app só lê de forma passiva o tráfego que já chega ao seu PC, mas ferramentas
de terceiros são uma zona cinzenta nos termos de uso do jogo: use por sua conta e risco.

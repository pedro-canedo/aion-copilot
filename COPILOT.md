# aion-copilot

Companion pessoal para AION 2 (uso pessoal, não comercial). É um fork do
[A2Tools DPS Meter](https://github.com/taengu/A2Tools-DPS-Meter) (GPL-3.0) com visual próprio
("Éter") e recursos extras. O README do [upstream](https://github.com/taengu/A2Tools-DPS-Meter) continua valendo para o núcleo.

## V1 — o que tem

**Barra aion-copilot** (sempre na tela, começa no canto superior direito, arrastável)
- Botão Painel e lista suspensa "Overlays" para ligar/desligar Medidor, Alvo, Grupo, Chefes, Timers, Mapa, Mascotes e Rotina
  (cada um lembra se estava aberto); "Iniciar com o Windows" (pega os nomes desde a tela de login).

**Painel** (janela normal): Início (saudação com o personagem, resets, "Não perca", última luta,
números, chefes que voltam primeiro, rotina de hoje, personagens), Histórico de lutas separado em
Chefes mundiais / Expedições / Outros chefes / Treino (busca e abrir detalhes), Personagens (classe,
CP), **PvP** (abates e mortes por jogador, `copilot-pvp.json`), Rotina e Chefes de campo.

**Medidor (overlay)**
- Cabeçalho: `‹ Dano causado ⌄ ›` (Dano causado / DPS / Cura), **Meu DPS** ao vivo e o tempo da luta;
  abaixo, a pílula Auto com o contexto, a escolha da visão e Histórico · copiar imagem · Top 5/10/Todos
  · zerar. Vazio diz o que espera ("Nada desde 04:47: …").
- **Histórico no overlay**: abas Expedições / Mundo / Treino / PvP com as lutas recentes (clique abre
  os detalhes) e "Ver histórico completo".
- **Cartão ao passar o mouse** numa linha: DPS, dano, parte e todas as habilidades (dano, % crítico,
  golpes, parte), com rolagem.
- **PvP / Abismo**: no Abismo (reconhecido pelos monstros abissais ao redor; o ID do mapa fica salvo em
  `copilot-maps.json`) ou quando o Auto vê PvP, a lista vira o placar **Seu lado × Oponentes** (dano,
  CP, mortes na sessão, K/D, barra de força). Golpe de jogador em jogador conta como dano PvP quando a
  skill já foi vista acertando monstros (ataque) e você está envolvido; entre você e o grupo continua cura.
- CP no estilo do jogo (espadas cruzadas, azul, "105,8K"; número inteiro no tooltip). O CP de
  qualquer jogador vem do fim do registro de spawn `45 36` (marcador `21 f4 cb 1f`, dois u32, CP u64),
  conferido com o Shugo; fica salvo com os nomes da sessão.
- Abas (agora uma escolha) Chefes / Monstros / Treino / Todos e **Auto**: a aba segue o tipo do último alvo atacado e o
  botão mostra o que reconheceu — Chefe mundial, Chefe de expedição, Chefe, Expedição (mostra o dano
  do grupo em todos os alvos), Treino (cada boneco com seu contador), Monstros ou PvP.
- Chefe mundial: você, seu grupo, os **4 maiores de fora** (marcados "≥": o servidor só manda o dano
  contínuo deles, então é um piso) e "Outros" = o resto do HP que o chefe perdeu. Um alvo grande em
  mundo aberto sem código (o app abriu depois do spawn) vira chefe mundial quando perde muito mais HP
  do que os golpes vistos.
- Alvo morto some do medidor 30 s depois (a luta fica no histórico).
- Linha: ícone da classe · nick · CP · dano (k/M) · cura · % do dano total. CP: do grupo, do seu
  registro próprio ou dos personagens salvos (o seu aparece mesmo sozinho).
- Caveira = mortes na luta (zera a cada luta/tentativa); asas = ressurreições
  (`PersonalData.deaths/revives`).
- Nomes: o jogo só manda nomes em telas de carregamento; o app guarda os nomes da sessão do jogo
  (`copilot-names.json`, por PID do AION 2) e se reconhece pelo último personagem da mesma classe.
- Cura em verde só para quem curou; "—" para os demais.
- Sua linha destacada pela borda (pelo nick, nunca "Você").
- Cabeçalho: transparência rápida (slider, salva na hora), alça de arrastar, cadeado (click-through;
  o próprio botão continua clicável), Rotina, histórico, reset, configurações.
- Interface no idioma do Windows na primeira execução (pt-BR → português).
- Herdado do A2Tools: detalhes por jogador (skills, crítico, golpes múltiplos, perfeito…), histórico
  de lutas, modos de alvo (chefe / último alvo / treino / todos), DoTs, invocações somadas ao dono.

**Rotina (overlay próprio)**
- Cabeçalho com o personagem e a contagem até o reset diário (07:00 UTC) e semanal (quarta, 07:00 UTC) — servidores Global.
- **Agora**, preenchido pelo jogo: Energia de Odyle (`60 (+10) / 840`), Tempo no Abismo
  (`3 h 28 min (+1 h) / 7 h`, o valor que o jogo mandou por último) e Pontos do Abismo da semana
  (`35,31k / 1,5M`), com barra, "faltam" e cor perto do limite / no limite. Vem da lista de contadores
  (`0b 61` no login e ao carregar zona, `0c 61` a cada mudança) e de `a9 ff` (pontos), decodificados em
  `src-tauri/src/capture/counters.rs`; o último estado fica salvo por personagem em
  `copilot-routine-state.json` e aparece antes do próximo login ("atualizado há X"). Sem dados: "abra o jogo".
  Também do login: **Kinah ganho hoje** (`a5 ff`: u64 ganho + u64 relógio do servidor; o limite diário
  não vem no pacote, então sem barra; zera no reset diário) e **Presença diária** ("Feita" quando um
  evento de presença foi marcado depois do reset diário, com o dia e recompensas a resgatar), dos
  registros de progresso `83 56` (varint n + n × 17 bytes: id u32, valor u32, ms u64, resgatados u8;
  ids 510xxxxxx = eventos de presença, valor = dias). Esses pacotes só chegam no login (nenhum de novo
  em 1 h de jogo): mostram "atualizado há X". O item "Presença diária" do checklist marca sozinho.
  **Arena**: rating da arena mais jogada (Solo antes de qualquer partida), vitórias/derrotas e % e as
  outras arenas jogadas, de `2a e3` no login (varint n + n × 14 bytes: arena u8 1 Solo / 2 Cooperação /
  3 Tática, rating u32, u32 ?, vitórias u8, derrotas u8, 3 bytes ?; conferido 1527 com 3 V / 2 D).
  Não são as entradas da semana, que seguem não achadas.
  **Missões de desafio** (hoje) e **partidas de arena nesta semana**: registros de progresso que o jogo
  soma para sempre (300100300 missões concluídas, 231111400 partidas de arena), lidos no login (`83 56`)
  e ao vivo (`84 56`: varint n + n × 17 bytes, a cada mudança). O "último ms" de cada um diz se mudou
  desde o reset (diário 07:00 UTC; semanal quarta 07:00 UTC): se não, o valor é a base; se sim, a base
  vem de uma leitura anterior do mesmo período (mesma sessão ou o arquivo por personagem). Sem base
  conhecida mostra "?" em vez de chutar. Os itens "Missões de desafio" e "Arena" do checklist sobem
  sozinhos (nunca descem). Após uma partida, rating e V/D da linha Arena dizem "(até o login)".
  Os quadros de **Suprimento** (`56 e2`: diário de 24 h e semanal de 7 dias, 10 pedidos cada) e os
  demais contadores (ids sem significado confirmado) só aparecem no modo desenvolvedor: qual campo diz
  "entregue" não foi confirmado (a captura tinha 0 entregues).
  O Painel mostra o mesmo no card "Rotina de hoje" e na aba Rotina (`public/src/js/routineAuto.js`).
- Checklist diário/semanal com contadores (`Arena 0/80` cria um contador), zera sozinho nos resets.
- **Não perca**: aviso em destaque quando há diárias pendentes a menos de 3 h do reset, ou semanais
  a menos de 24 h (o tooltip lista quais).
- Transparência, alça de arrastar, cadeado (trava posição), fechar. Reabre onde foi deixada.

**Grupo (overlay próprio)**
- Membros do grupo (do pacote de party): ícone da classe, nick, nível, CP (item level no tooltip).
- Resumo: quantos membros, CP médio, quantos caídos; caídos com caveira; você destacado.
- Mesma transparência/alça/cadeado/fechar da Rotina; abre pelo ícone de pessoas no cabeçalho do medidor.

**Chefes de campo (overlay próprio)**
- Lê o pacote que o jogo manda ao abrir o mapa (M): quem está vivo e quando cada chefe morto volta.
- Contagem regressiva por chefe (destaque laranja nos últimos 5 min), nomes em pt; guarda o último
  estado para sobreviver a reinícios. "voltou?" = o horário passou, abra o mapa para confirmar.

**Timers (overlay próprio)**
- "Timers · Am. do Sul": eventos agendados (Shugo Festa, Invasão Dimensional, Fenda Espaço-Temporal,
  Vigilante Kairah, Cerco de Artefatos, Chefes do Cerco, Senhor Guardião Nahma "aprox."), a próxima
  volta de cada chefe de campo conhecido e os resets. Linha: ícone · nome · hora local · contagem;
  o que está acontecendo aparece no topo em âmbar ("Acontecendo agora, faltam N min"; eventos com entrada
  limitada mostram "Entrada aberta, fecha em N min" e, fechada a entrada, já passam para o próximo início).
  As voltas de chefes de campo lidas aqui também ficam salvas (as mesmas do overlay Chefes e do Painel).
- As regras ficam em `src/data/copilot_timers.json` (cada uma com seu fuso, `utcOffsetMin`), embutidas
  no binário e entregues pelo comando `copilot_timer_rules`; as contas (próximo início, ativo agora,
  resets) ficam em `public/src/js/timersCore.js`, que a Rotina e o Painel também usam. Horas no
  relógio do Windows. O Painel mostra os próximos 4 no card Timers.

**Alvo (overlay próprio)**
- Alvo atual do medidor: nome (marca de chefe), barra de vida em % e absoluta (quando o jogo informa),
  tempo de luta, DPS do grupo, **tempo estimado para matar** e o seu lugar (ranking, DPS, % do dano).
- PvP: o jogador em que você bateu por último — classe, GS, CP, vida e o seu placar contra ele
  (⚔ abates / 💀 mortes). Abate/morte = a morte confirmada (3 s) cujo último golpe nos 10 s antes foi
  seu / dele, sem monstro batendo nos 3 s finais; membros do grupo nunca contam.

**Mapa (overlay próprio)**
- `public/map.html` + `public/src/js/map.js` + `public/src/map.css`: painel com o nome da região (clique
  para escolher o mapa à mão ou voltar ao automático), botão "N camadas" que abre CAMADAS (Mostrar tudo /
  Ocultar tudo, grupos DESTAQUES … OBJETOS, cada camada com ícone e contagem "do mapa +aprendidos";
  clicar no grupo marca/desmarca todas), o mapa em canvas sobre os tiles (sem tiles: grade e marcadores),
  N/S/L/O, você em laranja (anel de erro quando estimada, seta da direção quando o jogo diz), marcadores
  aprendidos vazados e tracejados, o que está em vista com borda verde, nomes ao passar o mouse,
  roda = zoom, arrastar = mover (solta o "centralizar em mim"), duplo clique aproxima, legenda, nota
  de precisão e crédito. Camadas escolhidas, zoom e centro ficam por mundo no localStorage;
  `copilot_map_state` é pedido a cada 1 s com `since`. Vazio: jogo fechado / mapa desconhecido
  ("ande um pouco ou abra o mapa no jogo") / sem tiles.
- Posições do que o jogo manda (`src-tauri/src/capture/world_position.rs`): NPCs (`41 36`), jogadores
  (`45 36`), objetos (`34 36`/`35 36`: monólitos, pontos de coleta com hora de spawn e "esgotado"),
  saídas (`42 36`) e movimentos (`1c/1b/1a/29/28 37`). Sua posição exata só vem de outros registros:
  efeito que você aplicou (`2a/2b 38`, posição de quem lança), golpe de outro em você (`02/03/1c 38`,
  posição do alvo) ou empurrão (`15/16 38`); andando, é estimada pela mediana do que chegou nos
  últimos 2 s (erro típico ~20 m) — `combat/world_tracker.rs`.
- `src-tauri/src/copilot_map.rs`: mundo atual (monólitos em vista contra os do tc-imba, monstros do
  Abismo com o andar pelo último mapa aberto, lugares já aprendidos, último mapa aberto desde o último
  carregamento; ou escolhido à mão, só para ver, pois só se aprende no mundo reconhecido; carregar uma
  zona esquece o mundo e o que estava em vista, e nada visto em expedição é aprendido), projeção em
  pixels do mapa,
  lugares aprendidos por mundo em `copilot-map-nodes.json` (com tempo de volta dos pontos de coleta) e
  tiles baixados sob demanda para `map-tiles/` (nunca no instalador; `public/src/js/mapTiles.js`).
- O que é cada objeto (o tc-imba não tem pontos de coleta): clicar num objeto aprendido/em vista abre
  "O que é isto?" e o usuário diz a camada (RECURSO/COLECIONÁVEIS; "Outro objeto" desfaz), salva em
  `copilot-map-labels.json`; `object_layer(código, variante)` a aplica na hora de mostrar, então o já aprendido muda junto.
  Pontos de coleta dividem um código (1500000 em Verteron, 1590001 no Abismo): o `34 36` de máscara 0x0004 com 46 bytes
  após o código traz, depois da contagem de coletas, a **variante** u32 = o tipo de recurso; então eles são ditos por
  "código:variante" (chave do arquivo; os demais por "código", arquivos antigos seguem valendo; limpar uma variante limpa
  também o rótulo do código inteiro). O nó aprendido guarda a variante mais recente. Sem rótulo: "Ponto de coleta ·
  clique para dizer qual é"; modo dev mostra `código:variante`.
- Tiles: decisão do usuário, para uso pessoal. Cada imagem é baixada do site do tc-imba na primeira
  vez que aparece (a página busca, o CSP libera só esse site; o Rust guarda) e fica em `map-tiles/`;
  nada é redistribuído. A arte dos mapas é da NCSOFT.
- Dados do tc-imba AION2 Interactive Map (CC BY-NC 4.0) em `src/data/map/` (ver `ATTRIBUTION.md`),
  gerados por `tools/map/convert_tcimba.py`. Comandos: `copilot_map_state`, `copilot_map_layers`,
  `copilot_map_markers`, `copilot_map_worlds`, `copilot_map_pick_world`, `copilot_map_label_object`, `copilot_map_tile`,
  `copilot_map_tile_store`. Detalhes técnicos (origem da posição, como o mundo foi reconhecido) só no
  modo desenvolvedor.

**Mascotes (overlay próprio)**
- `public/pets.html` + `public/src/js/pets.js` (visual da Rotina; partes puras em `petsCore.js`, testadas
  em `tests/pets-core.test.mjs`), pedido a cada 1 s (`copilot_pet_radar`).
  Aba **Perto**: todos os monstros em vista (tipos 0x04/0x05/0x0c/0x0d), agrupados por espécie (código NPC):
  nome pt-BR (ou "Criatura #código"), "×N" em vista, distância/seta do mais perto e um chip com o mascote
  que a espécie dá contra a sua coleção: "Falta na coleção" (vermelho), "Nv.N · upar" (âmbar),
  "Completo" (verde), "Nv.? · coleção não lida" ou "Mascote ?" (cinza; ainda não se sabe se dá mascote). Ordem: criaturas com
  alma (chip "Alma") primeiro, depois falta, upar, não lida, desconhecido, completo; depois nível e
  distância. Nota curta (uma linha): "Mascote ?: ainda não aprendido (N já conhecidas)";
  o título do chip explica que o app aprende quando uma alma cai logo depois de você derrotar uma.
  Aba **Coleção**: chips Nv.3 / Nv.2 / Nv.1 / sem, lista por nível, filtro "Faltando subir"; nome com
  lápis para renomear (Enter salva).
  Vazio: "Nenhuma criatura por perto…" só sem nenhum monstro em vista; jogo fechado diz isso.
- Coleção: `00 90` no login (um instante antes do registro próprio `33 36`, então espera ele para saber
  de quem é; se ele não vier em 10 s, fica do personagem já nomeado): u32 mascote ativo, u8 n,
  n × (u32 id, u32 id de novo, u32 nível), u8 m, m × (u32 id, u32 almas), resto ignorado. Conferido:
  115 níveis e as almas iguais ao jogo (`capture/testdata/pet_collection_v1.hex`). Mascote com almas e
  sem nível = nível 0 (não tem). Guardada por personagem em `copilot-pets.json` (mostra antes do
  próximo login, "coleção de dd/mm hh:mm").
- Criatura com a alma de um mascote: spawn `41 36` tipo 0x0c (códigos 2930xxx) com
  `0a 00 00 00 <id u32> 01 00 00 00` depois da posição (id 1001-1400, exigido uma vez só); os outros
  tipo 0x0c (2931xxx) têm `ff…` ali. Posição/saída/zona pelo `combat/world_tracker.rs` (campo `pet`).
  Um mesmo código carrega mascotes diferentes (2930392: 1291 e 1064; 2930374: 1001, 1092 e 1124), então
  para elas vale só o mascote do próprio spawn (não entram no aprendizado abaixo).
- `0d 90` = `01 <id u32> <qtd u32>` (logo após um abate): soma almas ao mascote na coleção em memória e
  marca "estimado desde o login" (`~N almas`) até a próxima coleção.
- Espécie → mascote (o spawn de monstro comum não traz mascote): aprendido. Um `04 8d` (dono do saque)
  que nomeia você, sobre um monstro em vista sem mascote no spawn, conta como abate seu; um `0d 90` até
  3 s depois liga o código da espécie ao mascote. Um abate seu de criatura com alma entra na janela com o
  mascote do spawn: a alma desse mascote é dela (não ensina nada); outra alma com ela na janela é ambígua.
  Um abate seu de entidade cujo spawn não foi visto entra como espécie desconhecida (nunca aprendida),
  deixando a janela ambígua. Nenhum abate seu na janela, ou espécies diferentes nela, não ensina nada; um abate ensina uma vez só; um link já aprendido não é trocado. O `42 36` (morte)
  chega ~4 s depois do `04 8d` (3,5 s depois das almas), por isso não serve de âncora. Captura de
  2026-10-18 16:59:46: entidade 40539 (2100555, "Espírito da Floresta da Serra"), `04 8d` ELLAO e
  `0d 90` do 1048 544 ms depois — o replay do dia inteiro aprende só 2100555 → 1048. Guardado em
  `copilot-pet-species.json` (código → id; carregado ao abrir, só cresce, salvo a cada 10 s).
- Nomes: os seus (`copilot-pet-names.json`, comando `copilot_pet_name`); sem nome seu, o mascote usa o
  nome de uma espécie que sabidamente o dá (Coleção, Perto, mascote ativo), senão "Mascote #id". Os
  dados do jogo não têm nome de mascote nem quantas almas cada nível pede (nada disso é inventado
  aqui). `03 90` (mascote ativo de outro jogador) é ignorado.
- Mapa: camada "Mascotes" (DESTAQUES, pata) com as criaturas em vista; tooltip "Mascote #id · seu nível X".
- Código: `src-tauri/src/capture/pets.rs` (decoders, `PetBook` e `SpeciesBook`), gancho do `04 8d` em
  `capture/stream_processor.rs` (`DataStorage::note_corpse_owner`), `src-tauri/src/copilot_pets.rs`
  (radar por espécie, rumo, arquivos por personagem, espécies, nomes). Entidade e código NPC só no modo
  desenvolvedor.

**Configurações**
- Quem só usa o app vê apenas o essencial: Medidor e Aparência (só as linhas do dia a dia), Streaming
  e Atalhos. As configurações técnicas (Conexão com adaptador e porta, Conta, Plugins, Avançado, temas
  e texturas, modos e limites do medidor) ficam só no modo desenvolvedor: build de debug ou
  `AION_COPILOT_DEV=1` (comando `copilot_dev_mode`; `public/src/js/copilotSettings.js`, regra
  `.copilotDevOnly` em `src/copilot.css`). Escondidas, continuam valendo em segundo plano (o valor
  salvo ou o padrão).

**OBS (fonte de navegador)**
- Configurações → Streaming: liga um servidor local (porta 18731) com o medidor ao vivo para o OBS.
  Incluído no build pessoal pela feature Cargo `obs`, sem nenhum dos serviços online do A2Tools.

**Instalador** (`aion-copilot_<versão>_x64-setup.exe`, NSIS, pt-BR)
- Instala por usuário em `%LOCALAPPDATA%\aion-copilot` (sem pedir administrador), com atalho no
  Menu Iniciar e desinstalador (que preserva as configurações).
- Inclui o WebView2 (bootstrapper embutido). O Npcap não vai junto: a licença gratuita não permite
  redistribuir o instalador dele. Se faltar (detecta pelas duas visões do registro e pela DLL), o
  instalador avisa e abre https://npcap.com/#download.

## 1.1 — atualização automática e Novidades

**Atualização pelo GitHub** (`src-tauri/src/copilot_update.rs`, feature Cargo `selfupdate`)
- 20 s depois de abrir e depois a cada 6 h, pede `api.github.com/repos/pedro-canedo/aion-copilot/releases/latest`
  (`User-Agent: aion-copilot/<versão>`, `If-None-Match` com a ETag guardada; um 304 reaproveita o release guardado em
  `copilot.update.release`). Tira `copilot-v`/`v` da tag, ignora rascunho, pré-release e versão com sufixo
  (`1.2.1-test`) e compara por semver com a versão do `tauri.conf.json`.
- O release só vale com os dois assets exatos `aion-copilot_<versão>_x64-setup.exe` e `….exe.sha256`
  (`"<hex>  <arquivo>"`, a primeira palavra são os 64 hex).
- `copilot.update.mode`: **auto** (padrão) baixa sozinho; **notify** só avisa e instala no clique; **off** só verifica
  quando pedido (`copilot_update_check` funciona em qualquer modo). `copilot.update.skip` = versão recusada (a
  seguinte volta a aparecer).
- Instalar: baixa o `.sha256`, baixa o exe para `%TEMP%\aion-copilot-update\` calculando o hash durante o download
  (mesmo padrão do updater do upstream, que é `online`), hash diferente apaga o arquivo e para; hash certo salva as
  configurações, roda `"<exe>" /P /R /UPDATE` (por usuário, sem UAC, reabre o app) e sai. **Nunca com o AION 2
  aberto**: o instalador fecha o app, então um instalador conferido espera o jogo fechar (verifica a cada 30 s) ou
  o app sair (aí roda sem `/R`).
- Comandos `copilot_update_status` (`{current, latest, state, progress, error, mode}`; `state` = indisponível, ocioso,
  verificando, atualizado, disponível, ignorada, baixando, pronta, instalando ou erro), `copilot_update_check`,
  `copilot_update_install`, `copilot_update_skip(version)`; eventos `copilot-update-status` (a cada mudança) e
  `copilot-update-progress` (`{pct}`). Sem a feature `selfupdate` os comandos existem e dizem "indisponível".
- O hash vem do mesmo release: protege contra arquivo corrompido, não contra a conta do GitHub comprometida.

**Novidades** (`src-tauri/src/copilot_changelog.rs`)
- `src/data/copilot_changelog.json` embutido: `{versions:[{version, date, title, items:[{tag: novo|melhorado|corrigido,
  text}]}]}`, a mais nova primeiro. Testes: parseia, versões estritamente decrescentes, a primeira é a do
  `tauri.conf.json`, tags válidas.
- `copilot_whats_new()` → `{current, lastSeen, versions}`; `copilot_whats_new_seen()` grava `copilot.lastSeenVersion`.
  Instalação nova grava a versão atual em silêncio na primeira abertura.
- Painel, aba **Novidades** (`data-view="novidades"`): a mais nova primeiro, data em pt-BR, chips Novo/Melhorado/
  Corrigido, "Sua versão" na versão instalada e, no topo, "Atualização X disponível · Atualizar agora". Abrir a aba
  chama `copilot_whats_new_seen()`.
- Barra: pílula "Novidades da X" enquanto `lastSeenVersion` < versão atual e "Atualização X" com versão disponível
  ou baixada; o clique abre o Painel na aba certa pela configuração `copilot.painel.view` (o Painel a lê ao abrir ou
  ouve com ele aberto, e a limpa). A lógica pura está em `public/src/js/whatsNewCore.js` (`tests/whats-new.test.mjs`).

**Ajustes** (aba do Painel, `data-view="ajustes"`)
- Sons: preset e volume em `copilot.sound.preset` / `copilot.sound.volume` (0..1), "Testar" chama `copilot_play_sound`.
- Atualizações: modo (`copilot.update.mode`), versão instalada, "Verificar agora", "Atualizar agora", "Pular esta
  versão", estado e progresso pelos eventos `copilot-update-status` / `copilot-update-progress`.
- Inicialização: "Iniciar com o Windows" (`copilot_autostart`, o mesmo da barra).
- Atalhos: mostra os atalhos do medidor e abre as configurações dele para mudar.

**Fundação** (usada pelas próximas fases)
- `src-tauri/src/copilot_window.rs`: a tabela de janelas do copilot (label, página, tamanho, posição inicial, reabrir).
  Toda janela nova precisa caber em `src-tauri/capabilities/default.json` (glob `copilot-*`; um teste confere).
- `src-tauri/src/copilot_store.rs`: documentos do usuário (`checklist`, `auras`, `alerts`) em `copilot-<nome>.json`,
  com rev (`copilot_store_get`/`copilot_store_put`, evento `copilot-store-changed`).
- `src-tauri/src/copilot_sound.rs`: sons Sino/Ping/Alerta/Suave sintetizados (`copilot_play_sound(preset, volume)`).
- `src-tauri/src/platform/{win32,unsupported}/copilot.rs`: o que o copilot pede ao sistema (som, PID do jogo,
  instalador da atualização, iniciar com o Windows).
- `public/src/js/copilotBus.js`, carregado por toda janela do copilot: `copilotBus.settings.get/peek/set/on` (o mapa
  de configurações do Rust, evento `setting-changed`) e `copilotBus.store.get/put/subscribe` (o store de documentos).
  Fora do app devolve os valores padrão e não grava nada (`tests/copilot-bus.test.mjs`).
- `overlayChrome.js`: transparência e cadeado de cada overlay em `copilot.ov.<label>.opacity` / `.locked` (label da
  janela). Na primeira abertura da 1.1 copia uma vez o que a 1.0 guardava no localStorage
  (`copilot.<label>.opacity/.locked`, que continuam lá) e marca `copilot.ov.migrated.<label>=1`
  (`tests/overlay-chrome-settings.test.mjs`).

## Desenvolvimento

```bash
npm install
npm run dev:offline          # app em modo dev, sem recursos online do A2Tools (com OBS e atualização)
npm run build:offline        # gera src-tauri/target/release/bundle/nsis/*.exe
cd src-tauri && cargo test --no-default-features --features desktop,obs,selfupdate
```

### Publicar uma versão (release)

`scripts/copilot-release.ps1` faz os passos 2, 3 e 5 (menos publicar) e imprime os comandos dos passos 4 a 6:

1. Subir a versão **só** no `src-tauri/tauri.conf.json` (o `Cargo.toml` e o `package.json` ficam com a do upstream) e
   escrever a entrada no topo de `src/data/copilot_changelog.json` (texto para o usuário). Atualizar o selo
   `v<versão>` do `site/index.html`. Commitar (sem dar push na `main` ainda: passo 6).
2. Rodar os testes e `npm run build:offline`.
3. `Get-FileHash` → `aion-copilot_<versão>_x64-setup.exe.sha256` (`"<hex>  <arquivo>"`, ASCII sem BOM) e a cópia sem
   versão `aion-copilot_x64-setup.exe` (para o link `releases/latest/download/aion-copilot_x64-setup.exe` do site e do
   README).
4. `git tag copilot-v<versão>` e `git push origin copilot-v<versão>`. **Nunca** `git push --tags`: o `git fetch upstream`
   traz as tags `v2.0.x` do upstream.
5. `gh release create copilot-v<versão>` com o exe, o `.sha256` e a cópia sem versão (o script gera as notas a partir
   do changelog).
6. **Só depois do release publicado**: `git push origin main`. O `pages.yml` publica o site a cada push na `main` que
   mexe em `site/**`; antes do release, o botão "Baixar para Windows" e o link do README dariam 404 e o selo
   mostraria uma versão que ainda não existe.

Quem está na 1.0.0 instala a 1.1.0 à mão uma vez; dali em diante o app se atualiza.

### Testar sem jogar (replay)

1. Use uma gravação no formato de replay do upstream (texto com os pacotes do jogo).
2. Rode com `AION_COPILOT_REPLAY=<caminho>\cap.replay.txt npm run dev:offline`.
   Para inspecionar a UI, adicione `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
   e use o DevTools em `http://localhost:9222`.

### Atualizar com o upstream (correções de opcode a cada patch)

```bash
git fetch upstream
git merge upstream/main
```
As mudanças do aion-copilot ficam concentradas em: `src/copilot.css`, `public/{routine,group,pets}.html`,
`public/src/js/{routine,routineAuto,group,bosses,target,timers,timersCore,overlayChrome,copilot,pets}.js`, `public/{bosses,target,timers}.html`, `src/data/copilot_timers.json`, `public/src/routine.css`, `src-tauri/src/copilot.rs`,
`src-tauri/src/copilot_map.rs`, `src-tauri/src/copilot_pets.rs`, `src-tauri/src/capture/pets.rs`, `src-tauri/src/capture/world_position.rs`, `src-tauri/src/combat/world_tracker.rs`,
`src/data/map/`, `tools/map/`, `public/src/js/{mapTiles,map}.js`, `public/map.html`, `public/src/map.css`,
`public/{hub,painel}.html`, `public/src/js/{hub,painel,copilotView,copilotPvp,copilotSettings,copilotBus,whatsNewCore}.js`,
`src-tauri/windows/hooks.nsh`, `src-tauri/src/{copilot_window,copilot_store,copilot_sound,copilot_update,copilot_changelog}.rs`,
`src-tauri/src/platform/{win32,unsupported}/copilot.rs`, `src/data/copilot_changelog.json`, `scripts/copilot-release.ps1`,
trechos marcados com `aion-copilot` no código.

## Dados de terceiros

Nada do conteúdo do Shugo.GG (bundle de dados, traduções, agenda) é usado aqui.

## Fora da V1 (próximos passos)

- PvP: sem sinal de facção decodificado, "seu lado" é você + grupo e "oponentes" quem você atacou ou
  quem atacou você; dano dos oponentes em membros do grupo ainda não entra. GS de quem não é do grupo
  não foi encontrado. Frames de arena.
- Builds de desenvolvimento gravam pacotes em `%TEMP%\copilot-frames.txt` e `copilot-spawns.txt`
  (para achar campos novos contra valores conhecidos); o release não.
- Morte real conferida (arena, 2026-10-18 17:08/17:09): o HP in-place pegou as duas mortes. Na arena o
  jogo dá outro id ao seu personagem e só o anuncia (`33 36`) depois da 1ª morte; o histórico PvP olha
  de novo os 2 min anteriores quando seu id muda (`PVP_RENAMED_LOOKBACK_MS`).
- Mortes de jogadores vêm da leitura de HP in-place (o evento `42 36` só chega para monstros); os
  logs `copilot-death` (fonte e tempo até voltar) servem para calibrar falsos positivos.
- Ameaça (threat): o jogo não manda e o Shugo só estima com coeficientes do bundle pago; precisa de
  dados próprios por skill.
- Rotina: missões e partidas de arena achadas (acima). **Suprimento**:
  quadros e limites lidos, falta uma captura com uma entrega feita para saber o campo de "entregue"
  (bit por pedido ou o u32 do pedido). **Limite diário de kinah** não vem no pacote. Arena: `2a e3` (rating e
  vitórias/derrotas, já na Rotina) não traz as entradas. `0b e3` traz 3 valores (ids 1001,
  1002, 3001) de significado desconhecido. Entradas de masmorras/expedições devem ser contadores da lista
  `0b 61` (contagens 7/14/21…), mas sem saber qual id é qual.
- Mascotes: almas por nível e nomes dos mascotes não são conhecidos; `0d 90` (ganho de almas) visto
  poucas vezes — confirmar em mais abates. Não se sabe se toda espécie comum dá um mascote só (as
  criaturas com alma não dão); o aprendizado é lento (uma alma a cada ~300 abates).

# Dados do mapa

Os marcadores em `markers/` vêm do **AION2 Interactive Map**, de Yihao Liu (tc-imba):
<https://github.com/aion2-interactive-map/aion2-interactive-map> (pasta `/public/data`,
commit `89ce3984d569ff1a03ee6311d064231211212981`, de 2026-04-22).

Esses dados são licenciados sob a **Creative Commons Atribuição-NãoComercial 4.0
Internacional (CC BY-NC 4.0)**: <https://creativecommons.org/licenses/by-nc/4.0/deed.pt-br>.
O aion-copilot é um projeto pessoal e não comercial; estes dados não podem ser usados
para fins comerciais.

## O que foi alterado

Convertidos por `tools/map/convert_tcimba.py` a partir de `markers/*.yaml` e `maps.yaml`:

- só a posição e o tipo de cada marcador foram mantidos; ids, imagens, nomes em chinês
  dos teleportes e números dos monólitos foram descartados;
- os nomes em inglês de vilas, campos de batalha, guarnições e masmorras seladas viraram
  palavras separadas ("DawnLegionBase" → "Dawn Legion Base");
- a coordenada y passou a ser medida a partir do topo do mapa (o YAML mede a partir da base);
- os tipos foram agrupados nas camadas do aion-copilot (`layers.json`), com nomes em pt-BR.

`worlds.json` (ids dos mundos, nomes e tabela de setores) e `layers.json` (catálogo de
camadas) são do aion-copilot. Nada vem do host `data-aion2.tc-imba.com` (sem licença
declarada) nem de dados do QuestLog.

## Imagens do mapa

As imagens (tiles) não fazem parte do aplicativo: são baixadas do site do tc-imba na primeira
vez que aparecem, para uso pessoal, e ficam na pasta de dados do app (`map-tiles/`). A arte do
mapa é do jogo: © NCSOFT. Linha de crédito exibida no mapa:

> Mapa: tc-imba AION2 Interactive Map (CC BY-NC 4.0); arte © NCSOFT

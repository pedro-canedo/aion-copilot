"""Converte os dados do tc-imba AION2 Interactive Map (CC BY-NC 4.0) para o
formato compacto do mapa do aion-copilot (src/data/map/).

Fonte: https://github.com/aion2-interactive-map/aion2-interactive-map, só a
pasta /public/data (a licença CC BY-NC 4.0 cobre só ela). Nada do host
data-aion2.tc-imba.com (sem licença) nem de dados derivados do QuestLog.

Uso (precisa do PyYAML):
    git clone --depth 1 --filter=blob:none --sparse <repo> tcimba
    git -C tcimba sparse-checkout set public/data
    python -I tools/map/convert_tcimba.py tcimba/public/data src/data/map [commit]

Gera:
- worlds.json: os mapas, com a tabela de setores [x0, y0, nx, ny, size] (o
  quadro de coordenadas do jogo) e os tiles que o site do tc-imba publica;
- layers.json: o catálogo de camadas (grupos e nomes em pt-BR);
- markers/<id do mundo>.json: os marcadores do repositório, em pixels do mapa
  a partir do canto superior esquerdo (y para baixo). O YAML do repositório
  mede y a partir do canto inferior esquerdo (Leaflet CRS.Simple); aqui ele é
  invertido uma vez, para que só exista uma convenção.

O mundo, a tabela de setores e os nomes de mapa são fatos do jogo, escritos à
mão abaixo; o que vem do tc-imba são os marcadores (posição e tipo).
"""
import json
import os
import re
import sys

import yaml

SECTOR = 102000
# id do mundo (tabela Map do jogo), chave do tc-imba, nome, lado, setores,
# tiles publicados (None quando o site não tem), observações.
WORLDS = [
    (1010, "World_L_A", "Verteron", "light", [-408000, -408000, 8, 8, SECTOR], True, []),
    (1011, "World_L_B", "Eltnen", "light", [-408000, -408000, 8, 8, SECTOR], False, ["semTiles"]),
    (1110, "World_D_A", "Altgard", "dark", [-408000, -408000, 8, 8, SECTOR], True, []),
    (1111, "World_D_B", "Morheim", "dark", [-408000, -408000, 8, 8, SECTOR], False, ["semTiles"]),
    (1000, "World_L_Starter", "Poeta", "light", [-204000, -204000, 4, 4, SECTOR], True, []),
    (1100, "World_D_Starter", "Ishalgen", "dark", [-329748, -257999.78, 5, 5, SECTOR], True, ["setoresAssimetricos"]),
    (20, "Abyss_Reshanta_A", "Piso Inferior do Abismo de Reshanta", "abyss", [-204000, -204000, 4, 4, SECTOR], True,
     ["projecaoConferida"]),
    # Os ids 22/23 e a tabela 2x2 são do jogo; qual andar é qual foi deduzido
    # dos nomes dos artefatos de Reshanta, não conferido. O repositório chama
    # os tiles 2x2 de Abyss_Reshanta_B e tem um Abyss_Reshanta_C 1x1: os nomes
    # não batem, então nenhum tile é ligado a eles.
    (22, "Abyss_Reshanta_C", "Piso Intermediário do Abismo de Reshanta", "abyss", [-102000, -102000, 2, 2, SECTOR], False,
     ["semTiles", "nomeDeduzido"]),
    (23, "Abyss_Reshanta_D", "Piso Superior do Abismo de Reshanta", "abyss", [-102000, -102000, 2, 2, SECTOR], False,
     ["semTiles", "nomeDeduzido"]),
]

# Grupos na ordem do painel de camadas.
GROUPS = [
    ("destaques", "DESTAQUES"),
    ("lugares", "LUGARES"),
    ("teleporte", "TELEPORTE"),
    ("masmorras", "MASMORRAS"),
    ("colecionaveis", "COLECIONÁVEIS"),
    ("recurso", "RECURSO"),
    ("servicos", "SERVIÇOS"),
    ("npcs", "NPCS"),
    ("missoes", "MISSÕES"),
    ("monstros", "MONSTROS"),
    ("objetos", "OBJETOS"),
]

# Camadas: id, grupo, nome, cor, glifo, subtipos do tc-imba, códigos de objeto
# (34 36) que caem nela. Códigos conferidos: 1100228/1100229 batem com os
# monólitos do tc-imba no Abismo (mediana 1,5 e 3,6 px); 1590001 e 1500000 são
# pontos de coleta (somem depois de 3 coletas e voltam), sem nome conhecido.
LAYERS = [
    ("fieldBoss", "destaques", "Chefes de campo", "#ff9f5a", "crown", [], []),
    ("named", "destaques", "Monstros nomeados", "#ffb86b", "star", [], []),
    # Criaturas com a alma de um mascote (spawn 41 36 tipo 0x0c com o id do mascote; só ao vivo).
    ("pet", "destaques", "Mascotes", "#ff8fd0", "paw", [], []),
    ("village", "lugares", "Vilas", "#8fd3ff", "home", ["village"], []),
    ("battlefield", "lugares", "Campos de batalha", "#ff6b6b", "swords", ["battlefield"], []),
    ("occupation", "lugares", "Guarnições", "#c792ea", "flag", ["occupation"], []),
    ("teleport", "teleporte", "Kibelisks", "#5ad1c6", "portal", ["teleport"], []),
    ("seal", "masmorras", "Masmorras seladas", "#f5d76e", "seal", ["seal"], []),
    ("dungeon", "masmorras", "Entradas de masmorra", "#b48cff", "door", [], []),
    ("monolith", "colecionaveis", "Rastros Empíreos", "#7ee0ff", "monolith", ["monolithMaterial"], [1100228, 1100229]),
    ("hiddenCube", "colecionaveis", "Cubos escondidos", "#ffd36b", "cube", ["hiddenCube"], []),
    ("gatherOdyle", "recurso", "Odyle", "#9be7ff", "crystal", ["gatheringOdyle"], []),
    ("gatherOrichalcum", "recurso", "Minério de Oricalco", "#d4a373", "ore", ["gatheringOrichalcumOre"], []),
    ("gatherYggdrasil", "recurso", "Tora de Yggdrasil", "#8bc34a", "wood", ["gatheringYggdrasilLog"], []),
    ("gatherSapphire", "recurso", "Safira", "#4f8cff", "gem", ["gatheringSapphireGemstone"], []),
    ("gatherDiamond", "recurso", "Diamante", "#e0f2ff", "gem", ["gatheringDiamondGemstone"], []),
    ("gatherRuby", "recurso", "Rubi", "#ff5c7a", "gem", ["gatheringRubyGemstone"], []),
    ("gatherAria", "recurso", "Ária", "#a6e3a1", "herb", ["gatheringAria"], []),
    ("gatherTargena", "recurso", "Targena", "#c3e88d", "herb", ["gatheringTargena"], []),
    ("gatherCoriolus", "recurso", "Coriolus", "#f78c6c", "food", ["gatheringCoriolus"], []),
    ("gatherKukuru", "recurso", "Kukuru", "#ffcb6b", "food", ["gatheringKukuru"], []),
    ("gatherMela", "recurso", "Mela", "#ff8fab", "food", ["gatheringMela"], []),
    ("gatherInina", "recurso", "Inina", "#89ddff", "herb", ["gatheringInina"], []),
    ("gatherCypri", "recurso", "Cypri", "#b2ccd6", "herb", ["gatheringCypri"], []),
    ("gatherUnknown", "recurso", "Pontos de coleta", "#9aa7b4", "leaf", [], [1590001, 1500000]),
    ("services", "servicos", "Serviços", "#8fd3ff", "bag", [], []),
    ("npc", "npcs", "NPCs", "#cfd8dc", "person", [], []),
    ("quest", "missoes", "Missões", "#ffe082", "scroll", [], []),
    ("monster", "monstros", "Monstros", "#ef5350", "paw", [], []),
    ("object", "objetos", "Objetos", "#90a4ae", "box", [], []),
]


def words(key):
    """'DawnLegionBase' -> 'Dawn Legion Base'; nomes em outra escrita somem."""
    if not key or not re.fullmatch(r"[A-Za-z0-9' ]+", key) or key.isdigit():
        return None
    return re.sub(r"(?<=[a-z])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", " ", key).strip()


def main():
    src, out = sys.argv[1], sys.argv[2]
    commit = sys.argv[3] if len(sys.argv) > 3 else ""
    tiles = {m["name"]: m for m in yaml.safe_load(open(os.path.join(src, "maps.yaml"), encoding="utf-8"))["maps"]}
    by_subtype = {s: layer[0] for layer in LAYERS for s in layer[5]}
    os.makedirs(os.path.join(out, "markers"), exist_ok=True)

    worlds = []
    for wid, key, name, side, sectors, has_tiles, flags in WORLDS:
        x0, y0, nx, ny, size = sectors
        tile = tiles.get(key) if has_tiles else None
        if tile and (tile["tilesCountX"], tile["tilesCountY"]) != (nx, ny):
            raise SystemExit(f"{key}: tiles {tile['tilesCountX']}x{tile['tilesCountY']} != setores {nx}x{ny}")
        height = ny * 1024
        counts = {}
        path = os.path.join(src, "markers", key + ".yaml")
        markers = (yaml.safe_load(open(path, encoding="utf-8")) or {}).get("markers") or [] if os.path.exists(path) else []
        layers = {}
        for m in markers:
            layer = by_subtype.get(m.get("subtype"))
            if layer is None:
                continue
            px, py = round(float(m["x"])), round(height - float(m["y"]))
            label = words(m.get("name")) if layer in ("village", "battlefield", "occupation", "seal") else None
            layers.setdefault(layer, []).append([px, py, label] if label else [px, py])
        for layer, items in layers.items():
            items.sort()
            counts[layer] = len(items)
        if layers:
            with open(os.path.join(out, "markers", f"{wid}.json"), "w", encoding="utf-8", newline="\n") as f:
                json.dump({"world": wid, "layers": layers}, f, ensure_ascii=False, separators=(",", ":"))
                f.write("\n")
        worlds.append({
            "id": wid,
            "key": key,
            "name": name,
            "side": side,
            "sectors": sectors,
            "tiles": {"name": key, "nx": nx, "ny": ny} if tile else None,
            "markers": counts,
            "flags": flags,
        })

    with open(os.path.join(out, "worlds.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump({"source": "tc-imba AION2 Interactive Map /public/data" + (f" @ {commit}" if commit else ""),
                   "worlds": worlds}, f, ensure_ascii=False, indent=1)
        f.write("\n")
    with open(os.path.join(out, "layers.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump({
            "groups": [{"id": g, "name": n} for g, n in GROUPS],
            "layers": [
                {"id": i, "group": g, "name": n, "color": c, "glyph": gl, "objectCodes": codes}
                for i, g, n, c, gl, _, codes in LAYERS
            ],
        }, f, ensure_ascii=False, indent=1)
        f.write("\n")
    for w in worlds:
        print(w["id"], w["key"], w["markers"])


if __name__ == "__main__":
    main()

//! aion-copilot: the map's data. What the game sends (`combat::world_tracker`)
//! placed on the tc-imba AION2 Interactive Map's frame, its markers
//! (CC BY-NC 4.0, `src/data/map/`, see ATTRIBUTION.md there), and the places
//! learned from your own sightings, kept in `copilot-map-nodes.json`. What an
//! object is (which resource) is what you tell the map, kept in
//! `copilot-map-labels.json` and applied when shown: once per code, or for a
//! gathering point (all of them share a code or two) once per code and
//! variant, the resource its spawn says (`ObjectSpawn::variant`).
//!
//! The frame: each world is a grid of `nx` x `ny` sectors of `size` game units
//! from (`x0`, `y0`), drawn as 1024 px tiles, so a pixel is about a metre:
//!
//! ```text
//! px = (X - x0) / size * 1024
//! py = (Y + y0 + ny * size) / size * 1024      from the top
//! ```
//!
//! `py` is `(Y - y0) / size * 1024` when the table is centred (`y0` is
//! `-ny * size / 2`, every world but Ishalgen); the general form is the one
//! QuestLog's projection implies for the game's own coordinates. In the
//! Abyss (world 20) it puts the objects with codes 1100228/1100229 a median
//! 1.5 and 2 px from tc-imba's monoliths; no other world is checked against
//! the game yet.
//!
//! Which world you are in: no record of the game is known to say it (`21 36`
//! names 1010, Verteron, while you stand in the Abyss). So it is told, in
//! order, by objects in view that sit on a world's monoliths, by the monsters
//! around being of the Abyss (the floor: the map last opened, or the lower
//! floor where no other floor's grid reaches), by the world the last zone load
//! named when it is a world of ours (shown, but nothing is learned on its
//! word alone), by objects on places learned
//! in a world before, and by the last map you opened (`01 91`) since the last
//! zone load, each only while your position is inside that world's grid.
//!
//! A zone load forgets the world told and what was seen before it; nothing
//! seen in an expedition is learned, and what waited for a world is learned
//! only where it lies on that world's grid. Places are learned only into the
//! world told, never into one picked by hand: the grids overlap (the Abyss
//! lies inside the land worlds'), and a pick says what you look at, not
//! where you are.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::capture::field_bosses::is_field_boss;
use crate::capture::world_position::Pos;
use crate::combat::data_storage::DataStorage;
use crate::combat::world_tracker::{dist2, Accuracy, EntityKind, Sighting};
use crate::i18n::lookup::NpcLookup;

/// The credit line the map shows.
pub const MAP_CREDIT: &str = "Mapa: tc-imba AION2 Interactive Map (CC BY-NC 4.0); arte © NCSOFT";
/// Where tc-imba's site serves its tiles (`<name>/Res/<name>_<col>_<row>.webp`,
/// row 00 at the top). Downloaded at run time for personal use, never bundled.
pub const TILE_BASE: &str = "https://aion2-interactive-map.github.io/aion2-interactive-map/UI/Map/WorldMap";
/// A tile is 25-700 KB; more than this is not one.
pub const MAX_TILE_BYTES: usize = 4 * 1024 * 1024;
/// Sightings this close (3 m) are one place.
pub const MERGE_UNITS: f32 = 300.0;
/// Monsters wander from where they spawn: one this close (15 m) is the same
/// place.
pub const NPC_MERGE_UNITS: f32 = 1_500.0;
/// Places kept per code and world; past it the least seen go.
const MAX_PER_CODE: usize = 400;
const MAX_NPCS_PER_CODE: usize = 100;
/// Places kept per world (about 1 MB of the file); past it the least seen go.
const MAX_PER_WORLD: usize = 10_000;
/// Respawn samples kept per place.
const MAX_RESPAWNS: usize = 8;
/// A gathering node came back 463 and 707 s after it was used up (Abyss).
const DEFAULT_RESPAWN_S: (u32, u32) = (460, 710);
/// An object this close to a monolith (5 px) sits on it.
const ON_MARKER_PX: f64 = 5.0;
/// Objects seen in this long tell the world...
const DETECT_WINDOW_MS: i64 = 600_000;
/// ...while this close to you (300 m): after a teleport, the old place's
/// objects say nothing about the new one.
const DETECT_RANGE: f32 = 30_000.0;
/// Distinct places that must agree.
const DETECT_NEEDED: usize = 2;
/// A map opened this recently hints at where you are.
const OPENED_MAP_MS: i64 = 1_800_000;
/// Entities sent to the map.
const MAX_ENTITIES: usize = 200;
const MAX_PENDING: usize = 5_000;
/// The Abyss world the Abyss monsters point to (Piso Inferior).
const ABYSS_WORLD: u32 = 20;
/// The PC's clock is not taken as more than a day off the server's.
const MAX_CLOCK_GAP_MS: i64 = 86_400_000;

// ===== Bundled data =====

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileSet {
    pub name: String,
    pub nx: u32,
    pub ny: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorldInfo {
    pub id: u32,
    pub key: String,
    pub name: String,
    pub side: String,
    /// `[x0, y0, nx, ny, size]`.
    pub sectors: [f64; 5],
    pub tiles: Option<TileSet>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub id: String,
    pub group: String,
    pub name: String,
    pub color: String,
    pub glyph: String,
    #[serde(default)]
    pub object_codes: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct WorldsFile {
    worlds: Vec<WorldInfo>,
}

#[derive(Debug, Deserialize)]
pub struct Catalogue {
    pub groups: Vec<GroupInfo>,
    pub layers: Vec<LayerInfo>,
}

/// A marker from the bundled data: pixels from the top left, and a name for
/// some (villages, battlefields).
#[derive(Debug, Clone, PartialEq)]
pub struct Bundled {
    pub px: f64,
    pub py: f64,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MarkersFile {
    world: u32,
    layers: BTreeMap<String, Vec<serde_json::Value>>,
}

const MARKER_FILES: &[&str] = &[
    include_str!("../../src/data/map/markers/1010.json"),
    include_str!("../../src/data/map/markers/1110.json"),
    include_str!("../../src/data/map/markers/20.json"),
];

pub fn worlds() -> &'static [WorldInfo] {
    static WORLDS: OnceLock<Vec<WorldInfo>> = OnceLock::new();
    WORLDS.get_or_init(|| {
        serde_json::from_str::<WorldsFile>(include_str!("../../src/data/map/worlds.json"))
            .map(|f| f.worlds)
            .unwrap_or_default()
    })
}

pub fn world(id: u32) -> Option<&'static WorldInfo> {
    worlds().iter().find(|w| w.id == id)
}

impl WorldInfo {
    /// A floor of the Abyss (Reshanta).
    pub fn is_abyss(&self) -> bool {
        self.side == "abyss"
    }
}

/// A map id that is one of the land worlds (Verteron, Eltnen, Altgard,
/// Morheim, Poeta, Ishalgen), so never the Abyss.
pub fn is_land_world(map_id: i32) -> bool {
    worlds().iter().any(|w| !w.is_abyss() && i64::from(w.id) == i64::from(map_id))
}

pub fn catalogue() -> &'static Catalogue {
    static LAYERS: OnceLock<Catalogue> = OnceLock::new();
    LAYERS.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/data/map/layers.json"))
            .unwrap_or(Catalogue { groups: Vec::new(), layers: Vec::new() })
    })
}

/// The bundled markers of a world, by layer.
pub fn bundled(world: u32) -> Option<&'static BTreeMap<String, Vec<Bundled>>> {
    static MARKERS: OnceLock<HashMap<u32, BTreeMap<String, Vec<Bundled>>>> = OnceLock::new();
    MARKERS
        .get_or_init(|| {
            MARKER_FILES
                .iter()
                .filter_map(|text| serde_json::from_str::<MarkersFile>(text).ok())
                .map(|f| {
                    let layers = f
                        .layers
                        .into_iter()
                        .map(|(layer, items)| {
                            let list = items
                                .iter()
                                .filter_map(|v| {
                                    let a = v.as_array()?;
                                    Some(Bundled {
                                        px: a.first()?.as_f64()?,
                                        py: a.get(1)?.as_f64()?,
                                        name: a.get(2).and_then(|n| n.as_str()).map(str::to_string),
                                    })
                                })
                                .collect();
                            (layer, list)
                        })
                        .collect();
                    (f.world, layers)
                })
                .collect()
        })
        .get(&world)
}

/// The layer an object belongs to: what you said its code and variant are
/// (see `label_object`), else what you said its code is, else the
/// catalogue's, else "object". Taken when shown, so a label moves what was
/// learned before it too.
pub fn object_layer(code: u32, variant: Option<u32>) -> &'static str {
    let cat = catalogue();
    let labels = LABELS.read().unwrap_or_else(|e| e.into_inner());
    let said = |key: String| labels.get(&key).and_then(|id| cat.layers.iter().find(|l| &l.id == id));
    variant
        .and_then(|v| said(label_key(code, Some(v))))
        .or_else(|| said(label_key(code, None)))
        .or_else(|| cat.layers.iter().find(|l| l.object_codes.contains(&code)))
        .map(|l| l.id.as_str())
        .unwrap_or("object")
}

/// A code the catalogue has as a gathering point (`gatherUnknown`'s 1500000
/// and 1590001): one code for every resource, told apart by the variant.
pub fn is_gather_code(code: u32) -> bool {
    catalogue().layers.iter().any(|l| is_gather_layer(&l.id) && l.object_codes.contains(&code))
}

/// The variant that says what an object is: a gathering point's, none for
/// any other code (there it is one value per place, not a type).
pub fn label_variant(code: u32, variant: Option<u32>) -> Option<u32> {
    variant.filter(|_| is_gather_code(code))
}

/// A label's key in `copilot-map-labels.json`: "code" or "code:variant".
pub fn label_key(code: u32, variant: Option<u32>) -> String {
    match variant {
        Some(v) => format!("{code}:{v}"),
        None => code.to_string(),
    }
}

// ===== What you said an object is =====

/// Layer ids by `label_key`, as you said them (`copilot-map-labels.json`).
static LABELS: RwLock<BTreeMap<String, String>> = RwLock::new(BTreeMap::new());
/// The groups whose layers an object can be said to be.
const LABEL_GROUPS: &[&str] = &["recurso", "colecionaveis", "objetos"];

/// `copilot-map-labels.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectLabels {
    pub version: u32,
    /// Layer id by `label_key`: "code", or "code:variant" for a gathering
    /// point (files of before variants have codes only, and still read).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

impl Default for ObjectLabels {
    fn default() -> Self {
        Self { version: 1, labels: BTreeMap::new() }
    }
}

/// Whether an object can be said to be of `layer`: one of RECURSO,
/// COLECIONÁVEIS or OBJETOS.
pub fn labelable(layer: &str) -> bool {
    catalogue().layers.iter().any(|l| l.id == layer && LABEL_GROUPS.contains(&l.group.as_str()))
}

/// The labels in use, as loaded from the file.
pub fn set_labels(labels: ObjectLabels) {
    *LABELS.write().unwrap_or_else(|e| e.into_inner()) = labels.labels;
}

/// Say what an object is (`None` or "object" forgets it): a gathering
/// point by its code and variant (`label_variant`), anything else by its
/// code. Forgetting a gathering point's variant forgets a label of its whole
/// code too (one from before variants, which made every resource the same).
/// The labels to save, or None for a layer an object cannot be (nothing
/// changes).
pub fn label_object(code: u32, variant: Option<u32>, layer: Option<&str>) -> Option<ObjectLabels> {
    let layer = layer.filter(|l| *l != "object");
    if layer.is_some_and(|l| !labelable(l)) {
        return None;
    }
    let variant = label_variant(code, variant);
    let mut labels = LABELS.write().unwrap_or_else(|e| e.into_inner());
    match layer {
        Some(l) => {
            labels.insert(label_key(code, variant), l.to_string());
        }
        None => {
            labels.remove(&label_key(code, variant));
            labels.remove(&label_key(code, None));
        }
    }
    Some(ObjectLabels { labels: labels.clone(), ..ObjectLabels::default() })
}

pub fn load_labels(path: &Path) -> ObjectLabels {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_labels(path: &Path, labels: &ObjectLabels) -> bool {
    match serde_json::to_string(labels) {
        Ok(json) => crate::atomic_file::write(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

/// The layer an NPC code belongs to: field bosses, named monsters, monsters.
pub fn npc_layer(code: u32, npcs: &NpcLookup) -> &'static str {
    let code = code as i32;
    if is_field_boss(code) {
        "fieldBoss"
    } else if npcs.is_boss(code) {
        "named"
    } else {
        "monster"
    }
}

fn is_gather_layer(layer: &str) -> bool {
    layer.starts_with("gather")
}

// ===== Projection =====

/// Map pixels (from the top left) of a world position.
pub fn project(sectors: [f64; 5], x: f64, y: f64) -> (f64, f64) {
    let [x0, y0, _, ny, size] = sectors;
    ((x - x0) / size * 1024.0, (y + y0 + ny * size) / size * 1024.0)
}

/// The world position (x, y) of map pixels: `project` undone.
pub fn unproject(sectors: [f64; 5], px: f64, py: f64) -> (f64, f64) {
    let [x0, y0, _, ny, size] = sectors;
    (px * size / 1024.0 + x0, py * size / 1024.0 - y0 - ny * size)
}

/// Whether a world position falls on a world's grid.
pub fn on_grid(sectors: [f64; 5], x: f64, y: f64) -> bool {
    let (px, py) = project(sectors, x, y);
    let [_, _, nx, ny, _] = sectors;
    (0.0..nx * 1024.0).contains(&px) && (0.0..ny * 1024.0).contains(&py)
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

// ===== Learned places =====

/// One place a code was seen at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Times seen.
    pub n: u32,
    pub first_ms: i64,
    pub last_ms: i64,
    /// When it was last used up, until it is seen back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gone_ms: Option<i64>,
    /// Seconds it took to come back, the latest last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub respawn_s: Vec<u32>,
    /// An object's variant, the latest seen (a gathering point can come back
    /// as another resource); none in files from before it was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldNodes {
    /// By object code (`34 36`).
    #[serde(default)]
    pub objects: BTreeMap<u32, Vec<Node>>,
    /// By NPC code (`41 36`, monsters only).
    #[serde(default)]
    pub npcs: BTreeMap<u32, Vec<Node>>,
}

impl WorldNodes {
    /// Places kept, of every code.
    pub fn places(&self) -> usize {
        self.objects.values().chain(self.npcs.values()).map(Vec::len).sum()
    }

    /// Past `cap` places, the least seen (the oldest of them) go.
    fn trim(&mut self, cap: usize) {
        while self.places() > cap {
            let objects = self.objects.iter().map(|(code, list)| (true, *code, list));
            let npcs = self.npcs.iter().map(|(code, list)| (false, *code, list));
            let least = objects
                .chain(npcs)
                .flat_map(|(object, code, list)| list.iter().enumerate().map(move |(i, n)| ((n.n, n.last_ms), object, code, i)))
                .min_by_key(|(key, ..)| *key)
                .map(|(_, object, code, i)| (object, code, i));
            let Some((object, code, i)) = least else { break };
            let lists = if object { &mut self.objects } else { &mut self.npcs };
            if let Some(list) = lists.get_mut(&code) {
                list.remove(i);
                if list.is_empty() {
                    lists.remove(&code);
                }
            }
        }
    }
}

/// `copilot-map-nodes.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapNodes {
    pub version: u32,
    #[serde(default)]
    pub worlds: BTreeMap<u32, WorldNodes>,
}

impl Default for MapNodes {
    fn default() -> Self {
        Self { version: 1, worlds: BTreeMap::new() }
    }
}

impl MapNodes {
    /// Add a sighting in `world`, the PC's clock taken as the server's (see
    /// `learn_with_gap`).
    pub fn learn(&mut self, world: u32, s: &Sighting) {
        self.learn_with_gap(world, s, 0);
    }

    /// Add a sighting in `world`. A place within `MERGE_UNITS` of a known one
    /// (`NPC_MERGE_UNITS` for a monster) is that one, whatever its variant
    /// (the latest is kept); a node seen spawned
    /// after it was used up gives a respawn time. `clock_gap_ms` is the PC's
    /// clock minus the server's: when it was used up is the PC's time, its
    /// spawn time the server's, and the respawn is measured on the PC's.
    pub fn learn_with_gap(&mut self, world: u32, s: &Sighting, clock_gap_ms: i64) {
        let w = self.worlds.entry(world).or_default();
        let (list, merge, cap) = match s.kind {
            EntityKind::Object => (w.objects.entry(s.code).or_default(), MERGE_UNITS, MAX_PER_CODE),
            EntityKind::Npc => (w.npcs.entry(s.code).or_default(), NPC_MERGE_UNITS, MAX_NPCS_PER_CODE),
            _ => return,
        };
        let nearest = list
            .iter_mut()
            .map(|n| (dist2([n.x, n.y, n.z], s.pos), n))
            .filter(|(d, _)| *d <= merge)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n);
        if let Some(node) = nearest {
            node.variant = s.variant.or(node.variant);
            if s.used_up {
                node.gone_ms = Some(s.at_ms);
                return;
            }
            if let Some(gone) = node.gone_ms.take() {
                match s.spawned_ms.map(|t| t + clock_gap_ms) {
                    // Seen again before it came back (another spawn): still gone.
                    Some(back) if back < gone => node.gone_ms = Some(gone),
                    Some(back) => {
                        let secs = (back - gone) / 1000;
                        if (30..=7_200).contains(&secs) {
                            node.respawn_s.push(secs as u32);
                            if node.respawn_s.len() > MAX_RESPAWNS {
                                node.respawn_s.remove(0);
                            }
                        }
                    }
                    // Back, with nothing saying since when.
                    None => {}
                }
            }
            // The place drifts toward the newest sightings, a little.
            let weight = 1.0 / (node.n.min(19) + 1) as f32;
            node.x += (s.pos[0] - node.x) * weight;
            node.y += (s.pos[1] - node.y) * weight;
            node.z += (s.pos[2] - node.z) * weight;
            node.n = node.n.saturating_add(1);
            node.last_ms = node.last_ms.max(s.at_ms);
            return;
        }
        if list.len() >= cap {
            // The least seen, the oldest of them, makes room.
            if let Some(i) = (0..list.len()).min_by_key(|&i| (list[i].n, list[i].last_ms)) {
                list.remove(i);
            }
        }
        list.push(Node {
            x: s.pos[0],
            y: s.pos[1],
            z: s.pos[2],
            n: u32::from(!s.used_up),
            first_ms: s.at_ms,
            last_ms: s.at_ms,
            gone_ms: s.used_up.then_some(s.at_ms),
            respawn_s: Vec::new(),
            variant: s.variant,
        });
        w.trim(MAX_PER_WORLD);
    }

    /// The range of respawn times seen for a code in a world (seconds), or
    /// the default for a gathering node (of the layer `variant` puts it in)
    /// when none was seen yet.
    pub fn respawn_window(&self, world: u32, code: u32, variant: Option<u32>) -> Option<(u32, u32)> {
        let samples: Vec<u32> = self
            .worlds
            .get(&world)
            .and_then(|w| w.objects.get(&code))
            .map(|list| list.iter().flat_map(|n| n.respawn_s.iter().copied()).collect())
            .unwrap_or_default();
        match (samples.iter().min(), samples.iter().max()) {
            (Some(&lo), Some(&hi)) => Some((lo, hi)),
            _ => is_gather_layer(object_layer(code, variant)).then_some(DEFAULT_RESPAWN_S),
        }
    }
}

pub fn load_nodes(path: &Path) -> MapNodes {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_nodes(path: &Path, nodes: &MapNodes) -> bool {
    match serde_json::to_string(nodes) {
        Ok(json) => crate::atomic_file::write(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

// ===== Which world =====

/// What else says where you are, besides what is in view.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hints {
    /// In an expedition: on none of these maps.
    pub dungeon: bool,
    /// The monsters around are of the Abyss (`copilot::zone_view`).
    pub abyss: bool,
    /// The map last opened in the game (`01 91`) and when.
    pub opened_map: Option<(u32, i64)>,
    /// The map the last zone load named (`21 36`), when it is one of ours.
    pub loaded_map: Option<u32>,
    /// Your position, when known.
    pub own: Option<Pos>,
    /// Zone loads seen and when the last one was (`WorldTracker::loads`).
    pub loads: u64,
    pub loaded_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub world: u32,
    /// "monoliths", "zoneLoad", "learned", "abyss", "openedMap" or "pinned".
    pub by: &'static str,
}

/// What `storage` says besides the objects in view, at `now`. Read after
/// the sightings are taken, so that a zone load in between is seen.
pub fn hints(storage: &DataStorage, npcs: &NpcLookup, abyss_maps: &std::collections::HashSet<i32>, now: i64) -> Hints {
    let zone = crate::copilot::zone_view(storage, npcs, abyss_maps);
    let (own, (loads, loaded_ms)) = storage.with_world(|w| (w.own_position(now), w.loads()));
    // A map opened before the last load says nothing about where it led.
    let opened_map = storage
        .boss_maps_since(u64::MAX)
        .and_then(|(_, maps)| maps.last().map(|m| (m.map_id, m.seen_ms)))
        .filter(|(_, at)| now - at <= OPENED_MAP_MS && loaded_ms.is_none_or(|l| *at >= l));
    let own = own.filter(|p| p.accuracy != Accuracy::Unknown).map(|p| p.pos);
    // Instances and dungeons load under ids of their own: only a world's id counts.
    let loaded_map = storage
        .current_map_id()
        .and_then(|id| u32::try_from(id).ok())
        .filter(|id| world(*id).is_some());
    Hints { dungeon: zone.kind == "dungeon", abyss: zone.kind == "abyss", opened_map, loaded_map, own, loads, loaded_ms }
}

/// The world that `seen` (recent object sightings: code and place) and
/// `hints` point to; see the module's notes for the order.
pub fn detect(seen: &[(u32, Pos)], nodes: &MapNodes, hints: &Hints) -> Option<Detection> {
    if hints.dungeon {
        return None;
    }
    let mut places: Vec<(u32, Pos)> = Vec::new();
    for &(code, pos) in seen {
        if !places.iter().any(|(c, p)| *c == code && dist2(*p, pos) <= MERGE_UNITS) {
            places.push((code, pos));
        }
    }
    let inside = |w: &WorldInfo| match hints.own {
        Some(p) => on_grid(w.sectors, f64::from(p[0]), f64::from(p[1])),
        None => places.iter().all(|(_, p)| on_grid(w.sectors, f64::from(p[0]), f64::from(p[1]))),
    };
    let best = |scores: Vec<(u32, usize)>| -> Option<u32> {
        let mut scores = scores;
        scores.sort_by(|a, b| b.1.cmp(&a.1));
        match scores.as_slice() {
            [(w, n), rest @ ..] if *n >= DETECT_NEEDED && rest.first().is_none_or(|(_, m)| m < n) => Some(*w),
            _ => None,
        }
    };
    // 1. Objects on a world's monoliths.
    let monoliths = worlds()
        .iter()
        .filter(|w| inside(w))
        .filter_map(|w| {
            let marks = bundled(w.id)?.get("monolith")?;
            let n = places
                .iter()
                .filter(|(_, p)| {
                    let (px, py) = project(w.sectors, f64::from(p[0]), f64::from(p[1]));
                    marks.iter().any(|m| (m.px - px).hypot(m.py - py) <= ON_MARKER_PX)
                })
                .count();
            Some((w.id, n))
        })
        .collect();
    if let Some(world) = best(monoliths) {
        return Some(Detection { world, by: "monoliths" });
    }
    if hints.own.is_none() && places.is_empty() {
        return None;
    }
    // 2. Abyss monsters around: the floor of the map last opened, when you
    // are on its grid; else the lower floor, only where no other floor's grid
    // reaches (the middle and upper floors sit inside its centre and have no
    // monoliths). Places learned are no help there: what was seen on another
    // floor may have been learned into the lower one.
    if hints.abyss {
        let floors: Vec<&WorldInfo> = worlds().iter().filter(|w| w.is_abyss() && inside(w)).collect();
        if let Some(w) = hints.opened_map.and_then(|(id, _)| floors.iter().find(|w| w.id == id)) {
            return Some(Detection { world: w.id, by: "openedMap" });
        }
        return match floors.as_slice() {
            [w] if w.id == ABYSS_WORLD => Some(Detection { world: ABYSS_WORLD, by: "abyss" }),
            _ => None,
        };
    }
    // 3. The world the last zone load named, when you stand on its grid. Not
    // in the Abyss (above): there it has named the land world too.
    if let Some(w) = hints.loaded_map.and_then(world).filter(|w| inside(w)) {
        return Some(Detection { world: w.id, by: "zoneLoad" });
    }
    // 4. Objects on places learned in a world.
    let learned = nodes
        .worlds
        .iter()
        .filter(|(id, _)| world(**id).is_some_and(|w| inside(w)))
        .map(|(id, w)| {
            let n = places
                .iter()
                .filter(|(code, p)| {
                    w.objects.get(code).is_some_and(|list| list.iter().any(|n| dist2([n.x, n.y, n.z], *p) <= MERGE_UNITS))
                })
                .count();
            (*id, n)
        })
        .collect();
    if let Some(world) = best(learned) {
        return Some(Detection { world, by: "learned" });
    }
    // 5. The map last opened.
    let (id, _) = hints.opened_map?;
    world(id).filter(|w| inside(w)).map(|w| Detection { world: w.id, by: "openedMap" })
}

/// The map's state between calls: the learned store, sightings waiting for a
/// world, what was seen lately and the world it says.
#[derive(Debug, Default)]
pub struct MapState {
    pub nodes: MapNodes,
    pub dirty: bool,
    pending: Vec<Sighting>,
    recent: VecDeque<(i64, u32, Pos)>,
    detected: Option<Detection>,
    pinned: Option<u32>,
    seq: u64,
    /// The zone loads seen at the last call.
    loads: u64,
    /// The PC's clock minus the server's, at most: the least gap between an
    /// object seen and its spawn time this session (one that spawns in view
    /// gives the gap itself, give or take the ping).
    clock_gap_ms: Option<i64>,
}

impl MapState {
    pub fn new(nodes: MapNodes) -> Self {
        Self { nodes, ..Self::default() }
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// The world you are in: the one picked by hand, or the one detected.
    pub fn world(&self) -> Option<Detection> {
        match self.pinned {
            Some(world) => Some(Detection { world, by: "pinned" }),
            None => self.detected,
        }
    }

    /// The learned places to write, when something was learned since they
    /// were last taken; `save_failed` marks them unsaved again.
    pub fn take_unsaved(&mut self) -> Option<MapNodes> {
        std::mem::take(&mut self.dirty).then(|| self.nodes.clone())
    }

    /// The write of what `take_unsaved` gave failed: try again next time.
    pub fn save_failed(&mut self) {
        self.dirty = true;
    }

    /// What an object is was said (`label_object`): what is shown changed.
    pub fn relabelled(&mut self) {
        self.seq += 1;
    }

    /// Pick the world by hand (`None` goes back to detecting it). It is
    /// only shown: places are still learned into the world told.
    pub fn pin(&mut self, world: Option<u32>) {
        if self.pinned != world {
            self.pinned = world;
            self.seq += 1;
        }
    }

    /// Take the tracker's sightings in: tell the world again, then learn
    /// them (and the ones that waited for a world) into it (the one told,
    /// not the one picked by hand). `hints` are read
    /// after the sightings were taken.
    pub fn ingest(&mut self, sightings: Vec<Sighting>, hints: &Hints, now: i64) {
        if hints.loads != self.loads {
            // A zone load: the world told and what was seen are the old zone's.
            self.loads = hints.loads;
            self.recent.clear();
            self.pending.clear();
            if self.detected.take().is_some() {
                self.seq += 1;
            }
        }
        // Seen before a load that came after they were taken: the old zone's.
        let sightings: Vec<Sighting> =
            sightings.into_iter().filter(|s| hints.loaded_ms.is_none_or(|at| s.at_ms >= at)).collect();
        for s in &sightings {
            let Some(spawned) = s.spawned_ms.filter(|_| !s.used_up) else { continue };
            let gap = s.at_ms - spawned;
            if gap >= -MAX_CLOCK_GAP_MS {
                self.clock_gap_ms = Some(self.clock_gap_ms.map_or(gap, |g| g.min(gap)));
            }
        }
        if hints.dungeon {
            // An expedition is on none of these maps, even with a world picked
            // by hand: nothing seen there is kept.
            self.recent.clear();
            self.pending.clear();
            if self.detected.take().is_some() {
                self.seq += 1;
            }
            return;
        }
        for s in &sightings {
            if s.kind == EntityKind::Object && !s.used_up {
                self.recent.push_back((s.at_ms, s.code, s.pos));
            }
        }
        while self.recent.front().is_some_and(|(at, _, _)| now - at > DETECT_WINDOW_MS) {
            self.recent.pop_front();
        }
        if let Some(own) = hints.own {
            self.recent.retain(|(_, _, p)| dist2(*p, own) <= DETECT_RANGE);
        }
        let seen: Vec<(u32, Pos)> = self.recent.iter().map(|(_, c, p)| (*c, *p)).collect();
        let found = detect(&seen, &self.nodes, hints).or_else(|| {
            // Nothing says otherwise: stay, while you are inside its grid.
            self.detected.filter(|d| {
                let own = hints.own.or_else(|| seen.last().map(|(_, p)| *p));
                own.is_none_or(|p| world(d.world).is_some_and(|w| on_grid(w.sectors, f64::from(p[0]), f64::from(p[1]))))
            })
        });
        if found.map(|d| d.world) != self.detected.map(|d| d.world) {
            if let Some(d) = found {
                tracing::info!("aion-copilot: map world {} ({})", d.world, d.by);
            }
            self.seq += 1;
        }
        self.detected = found;
        match self.detected {
            // A zone load's word alone shows a world but teaches it nothing.
            Some(d) if d.by != "zoneLoad" => {
                let Some(sectors) = world(d.world).map(|w| w.sectors) else { return };
                let waiting = std::mem::take(&mut self.pending);
                // Off the world's grid: seen somewhere else.
                let mut learned = false;
                for s in waiting.iter().chain(&sightings) {
                    if on_grid(sectors, f64::from(s.pos[0]), f64::from(s.pos[1])) {
                        self.nodes.learn_with_gap(d.world, s, self.clock_gap_ms.unwrap_or(0));
                        learned = true;
                    }
                }
                if learned {
                    self.dirty = true;
                    self.seq += 1;
                }
            }
            _ => {
                self.pending.extend(sightings);
                let over = self.pending.len().saturating_sub(MAX_PENDING);
                self.pending.drain(..over);
            }
        }
    }
}

// ===== Views =====

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldView {
    pub id: u32,
    pub name: String,
    /// "light", "dark" or "abyss".
    pub side: String,
    pub sectors: [f64; 5],
    /// Map size in pixels.
    pub width: f64,
    pub height: f64,
    pub tiles: Option<TileSet>,
    /// How it was told (developer mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_by: Option<&'static str>,
}

impl WorldView {
    pub fn of(w: &WorldInfo, detected_by: Option<&'static str>) -> Self {
        Self {
            id: w.id,
            name: w.name.clone(),
            side: w.side.clone(),
            sectors: w.sectors,
            width: w.sectors[2] * 1024.0,
            height: w.sectors[3] * 1024.0,
            tiles: w.tiles.clone(),
            detected_by,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeView {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// On the map, when the world is known.
    pub px: Option<f64>,
    pub py: Option<f64>,
    pub heading: Option<f32>,
    /// "exact", "estimated" or "unknown" (only where you were).
    pub accuracy: &'static str,
    pub age_ms: i64,
    /// How far off an estimate may be, in metres.
    pub radius_m: Option<f32>,
    /// Which kind of record placed you (developer mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityView {
    /// "npc", "player", "object" or "unknown".
    pub kind: &'static str,
    pub code: u32,
    pub name: Option<String>,
    pub layer: Option<&'static str>,
    pub px: f64,
    pub py: f64,
    /// A gathering point's variant (`label_variant`): what it is said by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<u32>,
    /// A wild creature carrying a pet's soul: the pet (layer "pet").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<u32>,
    /// Your level of that pet (0 = not owned), filled in by the command
    /// from `copilot_pets`; None without a collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet_level: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapStateView {
    /// Moves with what is shown (`view_seq`); handed back as `since`.
    pub seq: u64,
    pub now_ms: i64,
    pub world: Option<WorldView>,
    pub me: Option<MeView>,
    /// The nearest to you first (at most 200); empty while the world is unknown.
    pub entities: Vec<EntityView>,
    /// Learned places in this world, by layer.
    pub learned: BTreeMap<&'static str, usize>,
    /// Whether the game is open (the page says so when it is not).
    pub game: bool,
    /// The world picked by hand, if any (the page's menu shows it).
    pub pinned: Option<u32>,
}

/// The map's live state, or None when nothing changed since `since`. The
/// learned counts are taken after the world tracker is let go: the capture
/// needs it for every record.
pub fn map_state(
    storage: &DataStorage,
    npcs: &NpcLookup,
    map: &MapState,
    since: Option<u64>,
    dev: bool,
    game: bool,
    now: i64,
) -> Option<MapStateView> {
    let detection = map.world();
    let mut view = storage.with_world(|tracker| {
        let own = tracker.own_position(now);
        let mut live: Vec<(i32, &crate::combat::world_tracker::Live)> = tracker.in_view(now).map(|(id, l)| (*id, l)).collect();
        let seq = view_seq(tracker.seq() + map.seq(), own.as_ref(), live.len(), game);
        if since == Some(seq) {
            return None;
        }
        let info = detection.and_then(|d| world(d.world));
        let place = |x: f32, y: f32| info.map(|w| project(w.sectors, f64::from(x), f64::from(y)));
        let me = own.as_ref().map(|p| {
            let at = place(p.pos[0], p.pos[1]);
            MeView {
                x: p.pos[0],
                y: p.pos[1],
                z: p.pos[2],
                px: at.map(|a| round1(a.0)),
                py: at.map(|a| round1(a.1)),
                heading: p.heading,
                accuracy: p.accuracy.as_str(),
                age_ms: p.age_ms,
                radius_m: p.radius.map(|r| (r / 100.0).round()),
                source: if dev { p.source.map(|s| s.as_str()) } else { None },
            }
        });
        let mut entities = Vec::new();
        if let Some(w) = info {
            let centre = own.as_ref().map(|p| p.pos);
            match centre {
                Some(c) => live.sort_by(|a, b| dist2(a.1.pos, c).total_cmp(&dist2(b.1.pos, c))),
                None => live.sort_by(|a, b| b.1.last_ms.cmp(&a.1.last_ms)),
            }
            for (id, l) in live.into_iter().take(MAX_ENTITIES) {
                let (px, py) = project(w.sectors, f64::from(l.pos[0]), f64::from(l.pos[1]));
                let (name, layer) = match l.kind {
                    EntityKind::Npc if l.pet.is_some() => {
                        let name = npcs.get_npc_name(l.code as i32);
                        ((!name.is_empty()).then_some(name), Some("pet"))
                    }
                    EntityKind::Npc => {
                        let name = npcs.get_npc_name(l.code as i32);
                        ((!name.is_empty()).then_some(name), l.monster.then(|| npc_layer(l.code, npcs)))
                    }
                    EntityKind::Player => (l.name.clone().or_else(|| storage.get_nickname(id)), None),
                    EntityKind::Object => {
                        let layer = object_layer(l.code, l.variant);
                        (layer_name(layer, l.code, label_variant(l.code, l.variant), dev), Some(layer))
                    }
                    EntityKind::Unknown => (None, None),
                };
                entities.push(EntityView {
                    kind: l.kind.as_str(),
                    code: l.code,
                    name,
                    layer,
                    px: round1(px),
                    py: round1(py),
                    variant: if l.kind == EntityKind::Object { label_variant(l.code, l.variant) } else { None },
                    pet: l.pet,
                    pet_level: None,
                });
            }
        }
        Some(MapStateView {
            seq,
            now_ms: now,
            world: info.map(|w| WorldView::of(w, if dev { detection.map(|d| d.by) } else { None })),
            me,
            entities,
            learned: BTreeMap::new(),
            game,
            pinned: map.pinned,
        })
    })?;
    view.learned = detection.map(|d| learned_counts(&map.nodes, d.world, npcs)).unwrap_or_default();
    Some(view)
}

/// The count `map_state` hands out: it moves with every record and every
/// change of the map's state (`changes`), and with what changes with no
/// record arriving: your position (a fix turning into the estimate from what
/// is around, that into the stale fix, the estimate moving as its places age
/// out), what leaves view and the game opening or closing. Below 2^53, a
/// whole number for the page.
pub fn view_seq(changes: u64, own: Option<&crate::combat::world_tracker::OwnPosition>, in_view: usize, game: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    (changes, in_view, game).hash(&mut h);
    own.map(|p| {
        (
            p.accuracy.as_str(),
            p.source.map(|s| s.as_str()),
            p.pos.map(f32::to_bits),
            p.radius.map(f32::to_bits),
            p.heading.map(f32::to_bits),
        )
    })
    .hash(&mut h);
    h.finish() >> 11
}

/// A layer's name for one of its objects; in developer mode the code too
/// for the ones not named yet ("code:variant" for a gathering point).
fn layer_name(layer: &str, code: u32, variant: Option<u32>, dev: bool) -> Option<String> {
    let name = catalogue().layers.iter().find(|l| l.id == layer)?.name.clone();
    Some(if dev && (layer == "object" || layer == "gatherUnknown" && variant.is_some()) {
        format!("{name} #{}", label_key(code, variant))
    } else {
        name
    })
}

/// The learned places of a world that are not on a bundled marker of the
/// same layer, with their layer: an object's by its own variant, so the
/// places of one code can land in different layers.
fn learned_places<'a>(nodes: &'a MapNodes, world: u32, npcs: &NpcLookup) -> Vec<(&'static str, u32, &'a Node, bool)> {
    let Some(w) = nodes.worlds.get(&world) else { return Vec::new() };
    let info = self::world(world);
    let marks = bundled(world);
    let objects = w
        .objects
        .iter()
        .flat_map(|(code, list)| list.iter().map(move |n| (object_layer(*code, n.variant), *code, n, true)));
    let monsters = w
        .npcs
        .iter()
        .flat_map(|(code, list)| list.iter().map(move |n| (npc_layer(*code, npcs), *code, n, false)));
    objects
        .chain(monsters)
        .filter(|(layer, _, n, _)| {
            let (Some(info), Some(marks)) = (info, marks.and_then(|m| m.get(*layer))) else { return true };
            let (px, py) = project(info.sectors, f64::from(n.x), f64::from(n.y));
            !marks.iter().any(|m| (m.px - px).hypot(m.py - py) <= ON_MARKER_PX)
        })
        .collect()
}

fn learned_counts(nodes: &MapNodes, world: u32, npcs: &NpcLookup) -> BTreeMap<&'static str, usize> {
    let mut out = BTreeMap::new();
    for (layer, ..) in learned_places(nodes, world, npcs) {
        *out.entry(layer).or_insert(0) += 1;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerView {
    pub id: String,
    pub name: String,
    pub color: String,
    pub glyph: String,
    /// Bundled + learned (learned places on a bundled marker count once).
    pub count: usize,
    pub bundled: usize,
    pub learned: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub id: String,
    pub name: String,
    pub count: usize,
    pub layers: Vec<LayerView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayersView {
    pub world: WorldView,
    pub credit: &'static str,
    pub groups: Vec<GroupView>,
}

/// The layer catalogue of a world, with counts, in the panel's order.
pub fn layers_view(world_id: u32, nodes: &MapNodes, npcs: &NpcLookup) -> Option<LayersView> {
    let info = world(world_id)?;
    let marks = bundled(world_id);
    let learned = learned_counts(nodes, world_id, npcs);
    let cat = catalogue();
    let groups = cat
        .groups
        .iter()
        .map(|g| {
            let layers: Vec<LayerView> = cat
                .layers
                .iter()
                .filter(|l| l.group == g.id)
                .map(|l| {
                    let b = marks.and_then(|m| m.get(&l.id)).map_or(0, Vec::len);
                    let n = learned.get(l.id.as_str()).copied().unwrap_or(0);
                    LayerView {
                        id: l.id.clone(),
                        name: l.name.clone(),
                        color: l.color.clone(),
                        glyph: l.glyph.clone(),
                        count: b + n,
                        bundled: b,
                        learned: n,
                    }
                })
                .collect();
            GroupView { id: g.id.clone(), name: g.name.clone(), count: layers.iter().map(|l| l.count).sum(), layers }
        })
        .collect();
    Some(LayersView { world: WorldView::of(info, None), credit: MAP_CREDIT, groups })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkerView {
    pub px: f64,
    pub py: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// From your own sightings (not the bundled data).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub learned: bool,
    /// The object's code (learned objects only), for saying what it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u32>,
    /// A gathering point's variant (`label_variant`): what it is said by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<u32>,
    /// Times seen and when last (learned only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ms: Option<i64>,
    /// A used-up node: when it should be back (unix ms, earliest and latest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_from_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_to_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkersView {
    pub world: u32,
    pub markers: BTreeMap<String, Vec<MarkerView>>,
}

/// The markers of `layers` in a world: bundled, then learned.
pub fn markers_view(world_id: u32, layers: &[String], nodes: &MapNodes, npcs: &NpcLookup, dev: bool) -> Option<MarkersView> {
    let info = world(world_id)?;
    let mut markers: BTreeMap<String, Vec<MarkerView>> = layers.iter().map(|l| (l.clone(), Vec::new())).collect();
    if let Some(marks) = bundled(world_id) {
        for (layer, list) in markers.iter_mut() {
            for m in marks.get(layer).into_iter().flatten() {
                list.push(MarkerView {
                    px: m.px,
                    py: m.py,
                    name: m.name.clone(),
                    learned: false,
                    code: None,
                    variant: None,
                    seen: None,
                    last_ms: None,
                    back_from_ms: None,
                    back_to_ms: None,
                });
            }
        }
    }
    for (layer, code, node, object) in learned_places(nodes, world_id, npcs) {
        let Some(list) = markers.get_mut(layer) else { continue };
        let (px, py) = project(info.sectors, f64::from(node.x), f64::from(node.y));
        let variant = if object { label_variant(code, node.variant) } else { None };
        let respawn = if object { nodes.respawn_window(world_id, code, node.variant) } else { None };
        let back = node.gone_ms.zip(respawn).map(|(gone, (lo, hi))| {
            (gone + i64::from(lo) * 1000, gone + i64::from(hi) * 1000)
        });
        let name = if object {
            layer_name(layer, code, variant, dev)
        } else {
            Some(npcs.get_npc_name(code as i32)).filter(|n| !n.is_empty())
        };
        list.push(MarkerView {
            px: round1(px),
            py: round1(py),
            name,
            learned: true,
            code: object.then_some(code),
            variant,
            seen: Some(node.n),
            last_ms: Some(node.last_ms),
            back_from_ms: back.map(|b| b.0),
            back_to_ms: back.map(|b| b.1),
        });
    }
    Some(MarkersView { world: world_id, markers })
}

// ===== Tiles =====

/// A tile for the page: kept already (`data_url`), or where to fetch it
/// (`url`); neither past the grid or for a world without tiles.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapTile {
    pub data_url: Option<String>,
    pub url: Option<String>,
}

/// The tile kept under `app_data`, or its address on tc-imba's site.
pub fn cached_tile(app_data: &Path, world_id: u32, x: u32, y: u32) -> MapTile {
    let Some(file) = tile_file(app_data, world_id, x, y) else { return MapTile::default() };
    match std::fs::read(&file) {
        Ok(bytes) if is_webp(&bytes) => {
            MapTile { data_url: Some(format!("data:image/webp;base64,{}", crate::share::base64(&bytes))), url: None }
        }
        _ => MapTile { data_url: None, url: tile_url(world_id, x, y) },
    }
}

/// Keep a tile the page fetched (a `data:` URL of a WebP image). False when
/// it is no tile of that world, or not a WebP.
pub fn store_tile(app_data: &Path, world_id: u32, x: u32, y: u32, data_url: &str) -> bool {
    let Some(file) = tile_file(app_data, world_id, x, y) else { return false };
    let Some(bytes) = decode_data_url(data_url).filter(|b| b.len() <= MAX_TILE_BYTES && is_webp(b)) else {
        return false;
    };
    file.parent().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
        && crate::atomic_file::write(&file, &bytes).is_ok()
}

/// A tile's file name without the extension (`World_L_A_03_05`), or None
/// past the world's grid or for a world without tiles. Only the one zoom
/// level the site serves exists.
pub fn tile_name(world_id: u32, x: u32, y: u32) -> Option<(String, String)> {
    let tiles = world(world_id)?.tiles.as_ref()?;
    (x < tiles.nx && y < tiles.ny).then(|| (tiles.name.clone(), format!("{}_{x:02}_{y:02}", tiles.name)))
}

pub fn tile_url(world_id: u32, x: u32, y: u32) -> Option<String> {
    let (dir, name) = tile_name(world_id, x, y)?;
    Some(format!("{TILE_BASE}/{dir}/Res/{name}.webp"))
}

/// Where a tile is kept under the app's data folder.
pub fn tile_file(app_data: &Path, world_id: u32, x: u32, y: u32) -> Option<PathBuf> {
    let (dir, name) = tile_name(world_id, x, y)?;
    Some(app_data.join("map-tiles").join(dir).join(format!("{name}.webp")))
}

pub fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

/// The bytes of a `data:<type>;base64,<data>` URL.
pub fn decode_data_url(url: &str) -> Option<Vec<u8>> {
    let (head, data) = url.split_once(',')?;
    if !head.starts_with("data:") || !head.ends_with(";base64") {
        return None;
    }
    base64_decode(data)
}

/// Standard base64, padded or not.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let body = bytes.strip_suffix(b"==").or_else(|| bytes.strip_suffix(b"=")).unwrap_or(&bytes);
    if body.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    for chunk in body.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(code: u32, x: f32, y: f32) -> (u32, Pos) {
        (code, [x, y, 0.0])
    }

    fn sighting(at_ms: i64, code: u32, pos: Pos, used_up: bool, spawned_ms: Option<i64>) -> Sighting {
        Sighting { at_ms, kind: EntityKind::Object, code, pos, used_up, spawned_ms, variant: None }
    }

    fn gather(at_ms: i64, code: u32, variant: u32, pos: Pos) -> Sighting {
        Sighting { variant: Some(variant), ..sighting(at_ms, code, pos, false, None) }
    }

    #[test]
    fn the_bundled_data_reads_and_names_every_world_and_layer() {
        assert_eq!(worlds().len(), 9);
        let verteron = world(1010).unwrap();
        assert_eq!((verteron.name.as_str(), verteron.sectors), ("Verteron", [-408000.0, -408000.0, 8.0, 8.0, 102000.0]));
        assert_eq!(bundled(1010).unwrap()["monolith"].len(), 560);
        assert_eq!(bundled(20).unwrap()["monolith"].len(), 199);
        assert!(bundled(1011).is_none());
        let cat = catalogue();
        assert_eq!(cat.groups.first().map(|g| g.name.as_str()), Some("DESTAQUES"));
        assert_eq!(cat.groups.last().map(|g| g.name.as_str()), Some("OBJETOS"));
        assert!(cat.layers.iter().all(|l| cat.groups.iter().any(|g| g.id == l.group)));
        assert_eq!(
            (object_layer(1_100_228, None), object_layer(1_590_001, None), object_layer(1_100_016, None)),
            ("monolith", "gatherUnknown", "object")
        );
        assert!(is_gather_code(1_500_000) && is_gather_code(1_590_001) && !is_gather_code(1_100_016) && !is_gather_code(1_100_228));
    }

    #[test]
    fn a_monolith_seen_in_the_abyss_lands_on_tc_imbas_marker() {
        // Code 1100228 at (16636.0, -43830.2), from the capture.
        let w = world(20).unwrap();
        let (px, py) = project(w.sectors, 16636.0, -43830.2);
        let nearest = bundled(20).unwrap()["monolith"].iter().map(|m| (m.px - px).hypot(m.py - py)).fold(f64::MAX, f64::min);
        assert!(nearest < 0.5, "{nearest} px");
        // A centred table: py is (Y - y0) / size * 1024.
        assert!((py - (-43830.2 + 204000.0) / 102000.0 * 1024.0).abs() < 1e-9);
    }

    #[test]
    fn an_off_centre_table_measures_y_from_its_own_top() {
        // Ishalgen: [-329748, -257999.78, 5, 5, 102000]. Its top edge is at
        // y = -(y0 + ny * size), its bottom at -y0.
        let s = world(1100).unwrap().sectors;
        let (px, py) = project(s, 0.0, 0.0);
        assert!((px - 329748.0 / 102000.0 * 1024.0).abs() < 1e-6);
        assert!((py - 252000.22 / 102000.0 * 1024.0).abs() < 1e-6);
        assert!(project(s, 0.0, -252000.22).1.abs() < 1e-6);
        assert!((project(s, 0.0, 257999.78).1 - 5120.0).abs() < 1e-6);
        let (x, y) = unproject(s, px, py);
        assert!(x.abs() < 1e-6 && y.abs() < 1e-6);
        assert!(on_grid(s, 0.0, 0.0) && !on_grid(s, 0.0, 300_000.0));
    }

    #[test]
    fn objects_on_the_abyss_monoliths_say_the_abyss() {
        let objects = [
            seen(1_100_229, 35190.6, -26284.1),
            seen(1_100_228, 33721.2, -27284.1),
            seen(1_590_001, 41859.3, -21269.1),
            seen(1_100_229, 26094.7, -35285.4),
        ];
        let d = detect(&objects, &MapNodes::default(), &Hints::default()).unwrap();
        assert_eq!(d, Detection { world: 20, by: "monoliths" });
        // One is not enough.
        assert_eq!(detect(&objects[..1], &MapNodes::default(), &Hints::default()), None);
    }

    #[test]
    fn objects_on_verterons_monoliths_say_verteron_not_altgard() {
        let w = world(1010).unwrap();
        let objects: Vec<(u32, Pos)> = bundled(1010).unwrap()["monolith"][..3]
            .iter()
            .map(|m| {
                let (x, y) = unproject(w.sectors, m.px + 1.0, m.py - 1.0);
                (1_100_228, [x as f32, y as f32, 0.0])
            })
            .collect();
        assert_eq!(detect(&objects, &MapNodes::default(), &Hints::default()), Some(Detection { world: 1010, by: "monoliths" }));
    }

    #[test]
    fn the_world_a_zone_load_named_is_taken_when_you_stand_on_it() {
        let nodes = MapNodes::default();
        let here = Hints { own: Some([30_000.0, -20_000.0, 0.0]), loaded_map: Some(1010), ..Hints::default() };
        assert_eq!(detect(&[], &nodes, &here), Some(Detection { world: 1010, by: "zoneLoad" }));
        // Off its grid, or with nowhere known, it says nothing.
        let away = Hints { own: Some([900_000.0, 0.0, 0.0]), ..here.clone() };
        assert_eq!(detect(&[], &nodes, &away), None);
        let nowhere = Hints { own: None, ..here.clone() };
        assert_eq!(detect(&[], &nodes, &nowhere), None);
        // Nor in an expedition.
        let dungeon = Hints { dungeon: true, ..here };
        assert_eq!(detect(&[], &nodes, &dungeon), None);
    }

    #[test]
    fn learned_places_then_abyss_monsters_then_the_opened_map_tell_the_world() {
        let mut nodes = MapNodes::default();
        for (i, x) in [10_000.0, 12_000.0].into_iter().enumerate() {
            nodes.learn(20, &sighting(i as i64, 1_590_001, [x, 5_000.0, 0.0], false, None));
        }
        let objects = [seen(1_590_001, 10_100.0, 5_000.0), seen(1_590_001, 12_000.0, 5_100.0)];
        assert_eq!(detect(&objects, &nodes, &Hints::default()), Some(Detection { world: 20, by: "learned" }));
        let me = Hints { own: Some([30_000.0, -20_000.0, 0.0]), ..Hints::default() };
        assert_eq!(detect(&[], &MapNodes::default(), &me), None);
        // The lower floor's edge, where no other floor's grid reaches.
        let abyss = Hints { abyss: true, own: Some([150_000.0, -150_000.0, 0.0]), ..Hints::default() };
        assert_eq!(detect(&[], &MapNodes::default(), &abyss), Some(Detection { world: 20, by: "abyss" }));
        // In the Abyss but far outside its grid: not the Abyss.
        let away = Hints { abyss: true, own: Some([-300_000.0, 0.0, 0.0]), ..Hints::default() };
        assert_eq!(detect(&[], &MapNodes::default(), &away), None);
        let opened = Hints { opened_map: Some((1010, 0)), ..me };
        assert_eq!(detect(&[], &MapNodes::default(), &opened), Some(Detection { world: 1010, by: "openedMap" }));
    }

    #[test]
    fn nearby_sightings_merge_into_one_place_and_far_ones_do_not() {
        let mut nodes = MapNodes::default();
        nodes.learn(20, &sighting(1_000, 1_590_001, [100.0, 100.0, 0.0], false, None));
        nodes.learn(20, &sighting(2_000, 1_590_001, [300.0, 100.0, 0.0], false, None));
        nodes.learn(20, &sighting(3_000, 1_590_001, [500.0, 100.0, 0.0], false, None));
        let list = &nodes.worlds[&20].objects[&1_590_001];
        // 2 m from the first: merged (and pulled halfway). The third is 3 m
        // from the moved place: merged too. A fourth 10 m away is not.
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].n, list[0].first_ms, list[0].last_ms), (3, 1_000, 3_000));
        nodes.learn(20, &sighting(4_000, 1_590_001, [1_500.0, 100.0, 0.0], false, None));
        assert_eq!(nodes.worlds[&20].objects[&1_590_001].len(), 2);
        // Another code, another world: their own.
        nodes.learn(1010, &sighting(5_000, 1_590_001, [100.0, 100.0, 0.0], false, None));
        assert_eq!(nodes.worlds[&1010].objects[&1_590_001].len(), 1);
    }

    #[test]
    fn a_node_used_up_and_back_gives_its_respawn_time() {
        let mut nodes = MapNodes::default();
        let at = [39_085.3, -16_951.3, 13_062.5];
        nodes.learn(20, &sighting(1_000_000, 1_590_001, at, false, Some(900_000)));
        // Used up, then seen spawned 707 s later (as in the capture).
        nodes.learn(20, &sighting(1_100_000, 1_590_001, at, true, None));
        assert_eq!(nodes.worlds[&20].objects[&1_590_001][0].gone_ms, Some(1_100_000));
        // Before anything came back, the default window.
        assert_eq!(nodes.respawn_window(20, 1_590_001, None), Some((460, 710)));
        nodes.learn(20, &sighting(1_900_000, 1_590_001, at, false, Some(1_807_000)));
        let node = &nodes.worlds[&20].objects[&1_590_001][0];
        assert_eq!((node.gone_ms, node.respawn_s.as_slice()), (None, &[707][..]));
        assert_eq!(nodes.respawn_window(20, 1_590_001, None), Some((707, 707)));
        // Not a gathering node, nothing seen: no guess.
        assert_eq!(nodes.respawn_window(20, 1_100_016, None), None);
    }

    #[test]
    fn the_least_seen_place_makes_room_when_a_code_is_full() {
        let mut nodes = MapNodes::default();
        for i in 0..MAX_PER_CODE {
            let s = sighting(i as i64, 7, [i as f32 * 1_000.0, 0.0, 0.0], false, None);
            nodes.learn(1, &s);
            if i == 5 {
                nodes.learn(1, &s);
            }
        }
        nodes.learn(1, &sighting(9_999, 7, [0.0, 50_000.0, 0.0], false, None));
        let list = &nodes.worlds[&1].objects[&7];
        assert_eq!(list.len(), MAX_PER_CODE);
        // The first (seen once, oldest) went; the one seen twice stayed.
        assert!(list.iter().all(|n| n.x != 0.0 || n.y != 0.0) && list.iter().any(|n| n.n == 2));
    }

    #[test]
    fn learned_places_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("copilot-map-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("copilot-map-nodes.json");
        let mut nodes = MapNodes::default();
        nodes.learn(20, &sighting(1, 1_590_001, [1.0, 2.0, 3.0], true, None));
        assert!(save_nodes(&path, &nodes));
        assert_eq!(load_nodes(&path), nodes);
        assert_eq!(load_nodes(&dir.join("missing.json")), MapNodes::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sightings_wait_for_a_world_and_are_learned_once_it_is_known() {
        let mut map = MapState::new(MapNodes::default());
        let gather = sighting(1_000, 1_590_001, [41_859.3, -21_269.1, 0.0], false, None);
        map.ingest(vec![gather], &Hints::default(), 1_000);
        assert!(map.world().is_none() && map.nodes.worlds.is_empty());
        let seq = map.seq();
        let monoliths = vec![
            sighting(2_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(2_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
        ];
        map.ingest(monoliths, &Hints::default(), 2_000);
        assert_eq!(map.world(), Some(Detection { world: 20, by: "monoliths" }));
        assert!(map.seq() > seq && map.dirty);
        let w = &map.nodes.worlds[&20];
        assert_eq!((w.objects.len(), w.objects[&1_590_001].len()), (3, 1));
        // Nothing new in view, still inside the grid: the world stays.
        map.ingest(Vec::new(), &Hints { own: Some([30_000.0, -20_000.0, 0.0]), ..Hints::default() }, 900_000);
        assert_eq!(map.world().map(|d| d.world), Some(20));
        // Outside it: unknown again. A pick by hand wins.
        map.ingest(Vec::new(), &Hints { own: Some([-300_000.0, 0.0, 0.0]), ..Hints::default() }, 901_000);
        assert_eq!(map.world(), None);
        map.pin(Some(1010));
        assert_eq!(map.world(), Some(Detection { world: 1010, by: "pinned" }));
    }

    #[test]
    fn after_a_teleport_the_old_places_objects_no_longer_vote() {
        let mut map = MapState::new(MapNodes::default());
        let abyss = vec![
            sighting(1_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(1_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
        ];
        map.ingest(abyss, &Hints::default(), 1_000);
        assert_eq!(map.world().map(|d| d.world), Some(20));
        // Two Verteron monoliths that also fall inside the Abyss's grid, far
        // from the Abyss ones, a few metres apart.
        let w = world(1010).unwrap();
        let places: Vec<Pos> = bundled(1010).unwrap()["monolith"]
            .iter()
            .map(|m| {
                let (x, y) = unproject(w.sectors, m.px, m.py);
                [x as f32, y as f32, 0.0]
            })
            .filter(|p| on_grid(world(20).unwrap().sectors, f64::from(p[0]), f64::from(p[1])))
            .filter(|p| dist2(*p, [35_190.6, -26_284.1, 0.0]) > 100_000.0)
            .collect();
        let (a, b) = places
            .iter()
            .flat_map(|a| places.iter().map(move |b| (*a, *b)))
            .find(|(a, b)| a != b && dist2(*a, *b) < 10_000.0)
            .unwrap();
        let verteron = vec![sighting(60_000, 1_100_228, a, false, None), sighting(60_000, 1_100_228, b, false, None)];
        map.ingest(verteron, &Hints { own: Some(a), ..Hints::default() }, 60_000);
        assert_eq!(map.world(), Some(Detection { world: 1010, by: "monoliths" }));
    }

    #[test]
    fn the_map_state_changes_when_your_position_ages_into_an_estimate() {
        let storage = DataStorage::new();
        storage.set_local_player_id(Some(14667));
        crate::clock::set_override(Some(10_000));
        // Your buff, from the capture.
        let hex = "302a38cb720111c305caea2101b80b000000000000a2491e4ea1010000cb7201001494f346478ab3c684c66946";
        let buff: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        storage.note_world_packet(&buff);
        crate::clock::set_override(None);
        let npcs = NpcLookup::new();
        let mut map = MapState::new(MapNodes::default());
        map.pin(Some(20));
        let first = map_state(&storage, &npcs, &map, None, false, true, 11_000).unwrap();
        // The pick by hand is the user's own: told outside developer mode too.
        assert_eq!((first.pinned, first.world.as_ref().map(|w| w.detected_by)), (Some(20), Some(None)));
        let me = first.me.clone().unwrap();
        assert_eq!((me.accuracy, me.age_ms, me.source), ("exact", 1_000, None));
        let (px, py) = project(world(20).unwrap().sectors, 31178.04, -22981.139);
        assert!((me.px.unwrap() - px).abs() < 0.1 && (me.py.unwrap() - py).abs() < 0.1);
        assert_eq!(map_state(&storage, &npcs, &map, Some(first.seq), false, true, 12_000), None);
        let later = map_state(&storage, &npcs, &map, Some(first.seq), false, true, 20_000).unwrap();
        assert_eq!(later.me.unwrap().accuracy, "estimated");
        // Developer mode names the record that placed you, and how the world was told.
        let dev = map_state(&storage, &npcs, &map, None, true, true, 11_000).unwrap();
        assert_eq!((dev.me.unwrap().source, dev.world.unwrap().detected_by), (Some("abnormal"), Some("pinned")));
    }

    #[test]
    fn the_map_state_says_when_the_game_closes_with_nothing_else_changing() {
        let storage = DataStorage::new();
        let npcs = NpcLookup::new();
        let map = MapState::new(MapNodes::default());
        let open = map_state(&storage, &npcs, &map, None, false, true, 1_000).unwrap();
        assert!(open.game && open.world.is_none() && open.me.is_none());
        let closed = map_state(&storage, &npcs, &map, Some(open.seq), false, false, 2_000).unwrap();
        assert!(!closed.game);
        assert_eq!(map_state(&storage, &npcs, &map, Some(closed.seq), false, false, 3_000), None);
    }

    #[test]
    fn layer_counts_put_learned_monoliths_on_the_bundled_ones_once() {
        let mut nodes = MapNodes::default();
        // On a bundled monolith (16636.0, -43830.2), and a place of its own.
        nodes.learn(20, &sighting(1, 1_100_228, [16_636.0, -43_830.2, 0.0], false, None));
        nodes.learn(20, &sighting(1, 1_590_001, [41_859.3, -21_269.1, 0.0], false, None));
        let npcs = NpcLookup::new();
        let view = layers_view(20, &nodes, &npcs).unwrap();
        let layer = |id: &str| view.groups.iter().flat_map(|g| &g.layers).find(|l| l.id == id).unwrap().clone();
        assert_eq!((layer("monolith").count, layer("monolith").learned), (199, 0));
        assert_eq!((layer("gatherUnknown").count, layer("gatherUnknown").learned), (1, 1));
        assert_eq!(view.credit, MAP_CREDIT);
        let markers = markers_view(20, &["gatherUnknown".into(), "monolith".into()], &nodes, &npcs, false).unwrap();
        assert_eq!(markers.markers["monolith"].len(), 199);
        let gather = &markers.markers["gatherUnknown"][0];
        assert!(gather.learned && gather.seen == Some(1) && gather.name.as_deref() == Some("Pontos de coleta"));
    }

    #[test]
    fn tiles_are_named_by_column_and_row_inside_the_grid_only() {
        assert_eq!(
            tile_url(1010, 3, 5).as_deref(),
            Some("https://aion2-interactive-map.github.io/aion2-interactive-map/UI/Map/WorldMap/World_L_A/Res/World_L_A_03_05.webp")
        );
        assert_eq!(tile_url(1010, 8, 0), None);
        assert_eq!(tile_url(1011, 0, 0), None);
        assert_eq!(
            tile_file(Path::new("data"), 20, 1, 2),
            Some(Path::new("data").join("map-tiles").join("Abyss_Reshanta_A").join("Abyss_Reshanta_A_01_02.webp"))
        );
    }

    #[test]
    fn a_fetched_tile_is_kept_and_then_comes_from_the_disk() {
        let dir = std::env::temp_dir().join(format!("copilot-map-tiles-{}", std::process::id()));
        let fresh = cached_tile(&dir, 20, 1, 2);
        assert_eq!((fresh.data_url, fresh.url), (None, tile_url(20, 1, 2)));
        let webp = format!("data:image/webp;base64,{}", crate::share::base64(b"RIFF\x10\0\0\0WEBPVP8 "));
        assert!(!store_tile(&dir, 20, 1, 2, "data:text/html;base64,PGh0bWw+"));
        assert!(!store_tile(&dir, 20, 9, 9, &webp));
        assert!(store_tile(&dir, 20, 1, 2, &webp));
        let kept = cached_tile(&dir, 20, 1, 2);
        assert_eq!((kept.data_url.as_deref(), kept.url), (Some(webp.as_str()), None));
        assert_eq!(cached_tile(&dir, 1011, 0, 0), MapTile::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_an_expedition_no_world_is_told() {
        let objects = [seen(1_100_229, 35190.6, -26284.1), seen(1_100_228, 33721.2, -27284.1)];
        let hints = Hints { dungeon: true, ..Hints::default() };
        assert_eq!(detect(&objects, &MapNodes::default(), &hints), None);
    }

    #[test]
    fn in_the_abyss_the_map_last_opened_tells_the_floor_and_the_centre_alone_does_not() {
        // The centre of the lower floor is also the middle and upper floors'.
        let centre = Hints { abyss: true, own: Some([30_000.0, -20_000.0, 0.0]), ..Hints::default() };
        assert_eq!(detect(&[], &MapNodes::default(), &centre), None);
        let middle = Hints { opened_map: Some((22, 0)), ..centre.clone() };
        assert_eq!(detect(&[], &MapNodes::default(), &middle), Some(Detection { world: 22, by: "openedMap" }));
        // Places of the middle floor once learned into the lower one do not
        // outvote it.
        let mut nodes = MapNodes::default();
        let objects = [seen(1_590_001, 10_100.0, 5_000.0), seen(1_590_001, 12_000.0, 5_100.0)];
        for (code, p) in objects {
            nodes.learn(20, &sighting(1, code, p, false, None));
        }
        assert_eq!(detect(&objects, &nodes, &middle), Some(Detection { world: 22, by: "openedMap" }));
        assert_eq!(detect(&objects, &nodes, &centre), None);
        // A land world's map opened says nothing in the Abyss.
        let land = Hints { opened_map: Some((1010, 0)), ..centre };
        assert_eq!(detect(&[], &MapNodes::default(), &land), None);
        assert!(is_land_world(1010) && is_land_world(1111) && !is_land_world(20) && !is_land_world(22) && !is_land_world(600_011));
    }

    #[test]
    fn nothing_seen_in_an_expedition_is_learned_or_left_to_vote() {
        let mut map = MapState::new(MapNodes::default());
        map.pin(Some(20));
        let dungeon = Hints { dungeon: true, ..Hints::default() };
        // Objects in the expedition that happen to sit on Abyss monoliths.
        let inside = vec![
            sighting(1_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(1_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
            sighting(1_000, 1_590_001, [41_859.3, -21_269.1, 0.0], false, None),
        ];
        map.ingest(inside, &dungeon, 1_000);
        assert!(map.nodes.worlds.is_empty() && !map.dirty, "not even into the world picked by hand");
        // Out of it, with no world picked: the expedition's objects are gone.
        map.pin(None);
        map.ingest(Vec::new(), &Hints::default(), 2_000);
        assert_eq!(map.world(), None);
        assert!(map.pending.is_empty() && map.recent.is_empty());
    }

    #[test]
    fn a_world_picked_by_hand_is_shown_but_nothing_is_learned_into_it() {
        // In the Abyss, looking at Verteron, whose grid holds the Abyss's.
        let mut map = MapState::new(MapNodes::default());
        map.pin(Some(1010));
        let gather = sighting(1_000, 1_590_001, [41_859.3, -21_269.1, 0.0], false, None);
        assert!(on_grid(world(1010).unwrap().sectors, 41_859.3, -21_269.1));
        map.ingest(vec![gather.clone()], &Hints::default(), 1_000);
        assert_eq!(map.world(), Some(Detection { world: 1010, by: "pinned" }));
        assert!(map.nodes.worlds.is_empty() && !map.dirty);
        assert_eq!(map.pending.len(), 1, "it waits for the world told");
        // The Abyss told: what waited goes there, still not into Verteron.
        let monoliths = vec![
            sighting(2_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(2_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
        ];
        map.ingest(monoliths, &Hints::default(), 2_000);
        assert_eq!(map.world(), Some(Detection { world: 1010, by: "pinned" }));
        assert!(!map.nodes.worlds.contains_key(&1010));
        assert_eq!(map.nodes.worlds[&20].objects[&1_590_001].len(), 1);
        assert!(map.pending.is_empty() && map.dirty);
    }

    #[test]
    fn a_world_told_by_a_zone_load_is_shown_but_nothing_is_learned_into_it() {
        let mut map = MapState::new(MapNodes::default());
        let gather = sighting(1_000, 1_590_001, [41_859.3, -21_269.1, 0.0], false, None);
        let hints = Hints { loaded_map: Some(1010), own: Some([41_000.0, -21_000.0, 0.0]), ..Hints::default() };
        map.ingest(vec![gather], &hints, 1_000);
        assert_eq!(map.world(), Some(Detection { world: 1010, by: "zoneLoad" }));
        assert!(map.nodes.worlds.is_empty() && !map.dirty);
        assert_eq!(map.pending.len(), 1, "it waits for a world told by what is in view");
    }

    #[test]
    fn learned_places_are_taken_to_save_once_and_again_after_a_failed_write() {
        let mut map = MapState::new(MapNodes::default());
        assert_eq!(map.take_unsaved(), None);
        let monoliths = vec![
            sighting(1_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(1_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
        ];
        map.ingest(monoliths, &Hints::default(), 1_000);
        let taken = map.take_unsaved().unwrap();
        assert_eq!(taken.worlds[&20].objects.len(), 2);
        assert_eq!(map.take_unsaved(), None);
        map.save_failed();
        assert_eq!(map.take_unsaved(), Some(taken));
    }

    #[test]
    fn a_zone_load_forgets_the_world_and_what_was_seen_before_it() {
        let mut map = MapState::new(MapNodes::default());
        let abyss = vec![
            sighting(1_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(1_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
        ];
        map.ingest(abyss, &Hints::default(), 1_000);
        assert_eq!(map.world().map(|d| d.world), Some(20));
        let seq = map.seq();
        // A load at 5 s, into a place on the same grid. The batch straddles
        // it: what came before is the old zone's, what came after waits.
        let loaded = Hints { loads: 1, loaded_ms: Some(5_000), own: Some([30_000.0, -20_000.0, 0.0]), ..Hints::default() };
        let batch = vec![
            sighting(4_000, 1_590_001, [30_100.0, -20_000.0, 0.0], false, None),
            sighting(6_000, 1_590_001, [30_200.0, -20_000.0, 0.0], false, None),
        ];
        map.ingest(batch, &loaded, 6_000);
        assert_eq!(map.world(), None);
        assert!(map.seq() > seq);
        assert_eq!(map.pending.iter().map(|s| s.at_ms).collect::<Vec<_>>(), [6_000]);
        assert!(map.recent.iter().all(|(at, ..)| *at >= 5_000));
        assert!(!map.nodes.worlds[&20].objects.contains_key(&1_590_001));
    }

    #[test]
    fn what_waited_off_the_worlds_grid_is_not_learned_into_it() {
        let mut map = MapState::new(MapNodes::default());
        let far = sighting(1_000, 1_590_001, [-300_000.0, 0.0, 0.0], false, None);
        let near = sighting(1_000, 1_590_001, [41_859.3, -21_269.1, 0.0], false, None);
        map.ingest(vec![far, near], &Hints::default(), 1_000);
        let monoliths = vec![
            sighting(2_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(2_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
        ];
        // You by the monoliths, 300 km from the far one.
        map.ingest(monoliths, &Hints { own: Some([35_000.0, -26_000.0, 0.0]), ..Hints::default() }, 2_000);
        assert_eq!(map.world().map(|d| d.world), Some(20));
        let gathered = &map.nodes.worlds[&20].objects[&1_590_001];
        assert_eq!((gathered.len(), gathered[0].x), (1, 41_859.3));
    }

    #[test]
    fn a_map_opened_before_the_last_zone_load_says_nothing() {
        let storage = DataStorage::new();
        let npcs = NpcLookup::new();
        let none = std::collections::HashSet::new();
        crate::clock::set_override(Some(1_000_000));
        let opened = |at: i64| crate::capture::boss_map::BossMap { map_id: 1010, header: 0, bosses: Vec::new(), seen_ms: at };
        storage.note_boss_map(opened(990_000));
        assert_eq!(hints(&storage, &npcs, &none, 1_000_000).opened_map, Some((1010, 990_000)));
        storage.note_map_load(1110);
        let after = hints(&storage, &npcs, &none, 1_000_000);
        assert_eq!((after.opened_map, after.loads, after.loaded_ms), (None, 1, Some(1_000_000)));
        storage.note_boss_map(opened(1_000_500));
        assert_eq!(hints(&storage, &npcs, &none, 1_001_000).opened_map, Some((1010, 1_000_500)));
        crate::clock::set_override(None);
    }

    #[test]
    fn a_pc_clock_ten_minutes_ahead_still_measures_the_respawn() {
        // The PC's clock is 600 s ahead of the server's. A node spawns in
        // view (the gap itself), is used up, and comes back 590 s later.
        let mut map = MapState::new(MapNodes::default());
        let at = [41_859.3, -21_269.1, 0.0];
        let first = vec![
            sighting(10_000_000, 1_100_229, [35_190.6, -26_284.1, 0.0], false, None),
            sighting(10_000_000, 1_100_228, [33_721.2, -27_284.1, 0.0], false, None),
            sighting(10_000_000, 1_590_001, at, false, Some(9_400_000)),
        ];
        map.ingest(first, &Hints::default(), 10_000_000);
        assert_eq!(map.world().map(|d| d.world), Some(20));
        map.ingest(vec![sighting(10_100_000, 1_590_001, at, true, None)], &Hints::default(), 10_100_000);
        map.ingest(vec![sighting(10_700_000, 1_590_001, at, false, Some(10_090_000))], &Hints::default(), 10_700_000);
        let node = &map.nodes.worlds[&20].objects[&1_590_001][0];
        assert_eq!((node.gone_ms, node.respawn_s.as_slice()), (None, &[590][..]));
        // Taken as the same clock, the server's spawn time falls before it was
        // used up, and the node would stay used up for good.
        let mut nodes = MapNodes::default();
        nodes.learn(20, &sighting(10_100_000, 1_590_001, at, true, None));
        nodes.learn(20, &sighting(10_700_000, 1_590_001, at, false, Some(10_090_000)));
        assert_eq!(nodes.worlds[&20].objects[&1_590_001][0].gone_ms, Some(10_100_000));
    }

    #[test]
    fn monsters_merge_within_15_m_and_a_world_keeps_its_most_seen_places() {
        let mut nodes = MapNodes::default();
        let npc = |at: i64, x: f32| Sighting {
            at_ms: at,
            kind: EntityKind::Npc,
            code: 2_600_011,
            pos: [x, 0.0, 0.0],
            used_up: false,
            spawned_ms: None,
            variant: None,
        };
        nodes.learn(20, &npc(1, 0.0));
        nodes.learn(20, &npc(2, 1_000.0));
        nodes.learn(20, &npc(3, 5_000.0));
        assert_eq!(nodes.worlds[&20].npcs[&2_600_011].len(), 2);
        // Past the cap, the least seen (the oldest of them) go, whatever the code.
        let w = nodes.worlds.get_mut(&20).unwrap();
        w.objects.insert(7, vec![Node { x: 0.0, y: 0.0, z: 0.0, n: 1, first_ms: 0, last_ms: 0, gone_ms: None, respawn_s: Vec::new(), variant: None }]);
        assert_eq!(w.places(), 3);
        w.trim(2);
        assert!(!w.objects.contains_key(&7));
        w.trim(1);
        assert_eq!(w.npcs[&2_600_011].iter().map(|n| n.n).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn a_used_up_node_shows_when_it_should_be_back() {
        let mut nodes = MapNodes::default();
        nodes.learn(20, &sighting(1_000_000, 1_590_001, [41_859.3, -21_269.1, 0.0], true, None));
        let npcs = NpcLookup::new();
        let view = markers_view(20, &["gatherUnknown".into()], &nodes, &npcs, false).unwrap();
        let m = &view.markers["gatherUnknown"][0];
        // No respawn seen yet: the default window, 460-710 s.
        assert_eq!((m.back_from_ms, m.back_to_ms), (Some(1_460_000), Some(1_710_000)));
        let json = serde_json::to_value(m).unwrap();
        assert_eq!((json["backFromMs"].as_i64(), json["backToMs"].as_i64()), (Some(1_460_000), Some(1_710_000)));
    }

    #[test]
    fn objects_not_named_yet_show_their_code_in_developer_mode_only() {
        let name = layer_name("object", 1_100_016, None, false).unwrap();
        assert_eq!(layer_name("object", 1_100_016, None, true), Some(format!("{name} #1100016")));
        assert_eq!(layer_name("gatherUnknown", 1_590_001, None, true).as_deref(), Some("Pontos de coleta"));
        // A gathering point not told yet: code and variant.
        assert_eq!(layer_name("gatherUnknown", 1_590_001, Some(7), true).as_deref(), Some("Pontos de coleta #1590001:7"));
        assert_eq!(layer_name("gatherUnknown", 1_590_001, Some(7), false).as_deref(), Some("Pontos de coleta"));
        assert_eq!(layer_name("nope", 1, None, true), None);
    }

    #[test]
    fn the_map_states_count_moves_when_the_estimate_falls_back_to_the_stale_fix() {
        use crate::capture::world_position::WorldRecord;
        use crate::combat::world_tracker::WorldTracker;
        let mut t = WorldTracker::new();
        t.note(&WorldRecord::Displaced { entity: 1, pos: [0.0; 3] }, Some(1), 10_000);
        for (i, x) in [5_000.0, 6_000.0, 7_000.0].into_iter().enumerate() {
            t.note(&WorldRecord::Moved { entity: 100 + i as i32, pos: [x, 0.0, 0.0], heading: None }, Some(1), 16_000);
        }
        // The estimate from what is around, then (2 s on, nothing new) the
        // stale fix: both estimates, but not the same place.
        let around = t.own_position(17_000).unwrap();
        let stale = t.own_position(18_500).unwrap();
        assert_eq!((around.accuracy, stale.accuracy), (Accuracy::Estimated, Accuracy::Estimated));
        assert_ne!(view_seq(5, Some(&around), 4, true), view_seq(5, Some(&stale), 4, true));
        // Something leaving view moves it too; nothing at all does not.
        assert_ne!(view_seq(5, Some(&stale), 4, true), view_seq(5, Some(&stale), 3, true));
        assert_eq!(view_seq(5, Some(&stale), 4, true), view_seq(5, Some(&stale), 4, true));
        assert!(view_seq(u64::MAX, Some(&stale), usize::MAX, true) < 1 << 53);
    }

    // The labels are the process's: each test below has codes of its own,
    // none of the other tests'.

    #[test]
    fn a_label_moves_an_object_and_what_was_learned_of_it_before() {
        let code = 9_900_001;
        let mut nodes = MapNodes::default();
        nodes.learn(20, &sighting(1_000_000, code, [41_000.0, -21_000.0, 0.0], true, None));
        let npcs = NpcLookup::new();
        let learned = |nodes: &MapNodes, id: &str| {
            let view = layers_view(20, nodes, &npcs).unwrap();
            view.groups.iter().flat_map(|g| &g.layers).find(|l| l.id == id).unwrap().learned
        };
        assert_eq!((object_layer(code, None), learned(&nodes, "object"), learned(&nodes, "gatherRuby")), ("object", 1, 0));
        assert_eq!(nodes.respawn_window(20, code, None), None);
        // Not a gathering code: a variant given is not taken, the code is.
        let saved = label_object(code, Some(5), Some("gatherRuby")).unwrap();
        assert_eq!(saved.labels.get(&code.to_string()).map(String::as_str), Some("gatherRuby"));
        assert!(!saved.labels.contains_key(&format!("{code}:5")));
        assert_eq!((object_layer(code, None), learned(&nodes, "object"), learned(&nodes, "gatherRuby")), ("gatherRuby", 0, 1));
        assert_eq!(object_layer(code, Some(6)), "gatherRuby");
        // A resource now: used up, it should be back in the default window.
        assert_eq!(nodes.respawn_window(20, code, None), Some(DEFAULT_RESPAWN_S));
        let view = markers_view(20, &["gatherRuby".into(), "object".into()], &nodes, &npcs, false).unwrap();
        let m = &view.markers["gatherRuby"][0];
        assert_eq!((m.code, m.name.as_deref(), view.markers["object"].len()), (Some(code), Some("Rubi"), 0));
        assert_eq!(serde_json::to_value(m).unwrap()["code"].as_u64(), Some(u64::from(code)));
        // Cleared, by None or by "object": an object again.
        let saved = label_object(code, None, None).unwrap();
        assert!(!saved.labels.contains_key(&code.to_string()));
        assert_eq!((object_layer(code, None), learned(&nodes, "object")), ("object", 1));
        label_object(code, None, Some("hiddenCube")).unwrap();
        assert_eq!(object_layer(code, None), "hiddenCube");
        label_object(code, None, Some("object")).unwrap();
        assert_eq!(object_layer(code, None), "object");
    }

    #[test]
    fn an_object_can_only_be_said_to_be_a_resource_a_collectible_or_an_object() {
        let code = 9_900_002;
        for layer in ["village", "monster", "fieldBoss", "nope", ""] {
            assert_eq!(label_object(code, None, Some(layer)), None, "{layer}");
        }
        assert_eq!(object_layer(code, None), "object");
        assert!(labelable("gatherOdyle") && labelable("monolith") && labelable("object") && !labelable("teleport"));
        // A label of a layer the catalogue no longer has is not taken.
        LABELS.write().unwrap().insert(code.to_string(), "gatherGone".into());
        assert_eq!(object_layer(code, None), "object");
        LABELS.write().unwrap().remove(&code.to_string());
    }

    #[test]
    fn labels_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("copilot-map-labels-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("copilot-map-labels.json");
        let mut labels = ObjectLabels::default();
        labels.labels.insert(label_key(9_900_003, None), "gatherRuby".into());
        labels.labels.insert(label_key(9_900_004, None), "hiddenCube".into());
        labels.labels.insert(label_key(1_500_000, Some(2_872_447_537)), "gatherRuby".into());
        assert!(save_labels(&path, &labels));
        assert_eq!(load_labels(&path), labels);
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["labels"]["9900003"].as_str(), Some("gatherRuby"));
        assert_eq!(json["labels"]["1500000:2872447537"].as_str(), Some("gatherRuby"));
        // A file from before variants (codes only) reads as it did.
        std::fs::write(&path, r#"{"version":1,"labels":{"9900003":"gatherRuby"}}"#).unwrap();
        assert_eq!(load_labels(&path).labels.get("9900003").map(String::as_str), Some("gatherRuby"));
        assert_eq!(load_labels(&dir.join("missing.json")), ObjectLabels::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_gathering_point_is_told_by_its_variant_and_the_others_of_its_code_stay_unknown() {
        // Verteron's gathering points (1500000): `31 12 36 ab` and `13 dd 47 c5`.
        let (code, ruby, other) = (1_500_000, 0xAB36_1231, 0xC547_DD13);
        let mut nodes = MapNodes::default();
        nodes.learn(1010, &gather(1, code, ruby, [-109_821.0, -108_032.0, 0.0]));
        nodes.learn(1010, &gather(2, code, ruby, [-100_000.0, -100_000.0, 0.0]));
        nodes.learn(1010, &gather(3, code, other, [-90_000.0, -100_000.0, 0.0]));
        let npcs = NpcLookup::new();
        let learned = |nodes: &MapNodes, id: &str| {
            let view = layers_view(1010, nodes, &npcs).unwrap();
            view.groups.iter().flat_map(|g| &g.layers).find(|l| l.id == id).unwrap().learned
        };
        assert_eq!((learned(&nodes, "gatherUnknown"), learned(&nodes, "gatherRuby")), (3, 0));
        let saved = label_object(code, Some(ruby), Some("gatherRuby")).unwrap();
        assert_eq!(saved.labels.get(&format!("{code}:{ruby}")).map(String::as_str), Some("gatherRuby"));
        assert!(!saved.labels.contains_key(&code.to_string()));
        assert_eq!((object_layer(code, Some(ruby)), object_layer(code, Some(other)), object_layer(code, None)), ("gatherRuby", "gatherUnknown", "gatherUnknown"));
        // Both places of that variant move, the other stays a gathering point.
        assert_eq!((learned(&nodes, "gatherUnknown"), learned(&nodes, "gatherRuby")), (1, 2));
        let view = markers_view(1010, &["gatherRuby".into(), "gatherUnknown".into()], &nodes, &npcs, true).unwrap();
        assert!(view.markers["gatherRuby"].iter().all(|m| m.variant == Some(ruby) && m.name.as_deref() == Some("Rubi")));
        let unknown = &view.markers["gatherUnknown"][0];
        assert_eq!((unknown.variant, unknown.name.clone()), (Some(other), Some(format!("Pontos de coleta #{code}:{other}"))));
        assert_eq!(serde_json::to_value(unknown).unwrap()["variant"].as_u64(), Some(u64::from(other)));
        // "Outro objeto" on it: a gathering point again.
        label_object(code, Some(ruby), None).unwrap();
        assert_eq!(object_layer(code, Some(ruby)), "gatherUnknown");

        // In the same test (the labels are the process's, and 1590001 is
        // other tests'): a file from before variants said what the whole
        // code is. It still works, until a variant of it is cleared.
        let (a, b) = (ruby, other);
        let key = code.to_string();
        LABELS.write().unwrap().insert(key.clone(), "gatherOdyle".into());
        assert_eq!((object_layer(code, Some(a)), object_layer(code, Some(b)), object_layer(code, None)), ("gatherOdyle", "gatherOdyle", "gatherOdyle"));
        // One variant told: it wins over the code's label, the other keeps it.
        label_object(code, Some(a), Some("gatherAria")).unwrap();
        assert_eq!((object_layer(code, Some(a)), object_layer(code, Some(b))), ("gatherAria", "gatherOdyle"));
        // Clearing a variant clears the code's label too.
        let saved = label_object(code, Some(a), None).unwrap();
        assert!(!saved.labels.contains_key(&key) && !saved.labels.contains_key(&format!("{code}:{a}")));
        assert_eq!((object_layer(code, Some(a)), object_layer(code, Some(b))), ("gatherUnknown", "gatherUnknown"));
    }

    #[test]
    fn a_place_that_comes_back_as_another_resource_keeps_the_latest_variant() {
        let at = [41_859.3, -21_269.1, 0.0];
        let mut nodes = MapNodes::default();
        nodes.learn(20, &gather(1_000, 1_590_001, 0x0B08_97A4, at));
        // Used up (the tracker carries the variant it had), then back as the other.
        nodes.learn(20, &Sighting { used_up: true, ..gather(2_000, 1_590_001, 0x0B08_97A4, at) });
        nodes.learn(20, &gather(3_000, 1_590_001, 0x6488_979C, [41_900.0, -21_269.1, 0.0]));
        // A sighting that says no variant keeps the one known.
        nodes.learn(20, &sighting(4_000, 1_590_001, at, false, None));
        let list = &nodes.worlds[&20].objects[&1_590_001];
        assert_eq!((list.len(), list[0].n, list[0].variant), (1, 3, Some(0x6488_979C)));
    }

    #[test]
    fn learned_places_from_before_variants_still_load() {
        let old = r#"{"version":1,"worlds":{"20":{"objects":{"1590001":[{"x":1.0,"y":2.0,"z":3.0,"n":4,"firstMs":5,"lastMs":6,"respawnS":[707]}]},"npcs":{}}}}"#;
        let nodes: MapNodes = serde_json::from_str(old).unwrap();
        let node = &nodes.worlds[&20].objects[&1_590_001][0];
        assert_eq!((node.n, node.variant, node.respawn_s.as_slice()), (4, None, &[707][..]));
        assert_eq!(object_layer(1_590_001, node.variant), "gatherUnknown");
        // Written back, a node with no variant says nothing of it.
        assert!(!serde_json::to_string(&nodes).unwrap().contains("variant"));
        let mut with = nodes.clone();
        with.learn(20, &gather(7, 1_590_001, 9, [1.0, 2.0, 3.0]));
        assert!(serde_json::to_string(&with).unwrap().contains(r#""variant":9"#));
    }

    #[test]
    fn a_data_url_decodes_and_only_webp_counts_as_a_tile() {
        assert_eq!(base64_decode("YWI=").unwrap(), b"ab");
        assert_eq!(base64_decode("YWJj").unwrap(), b"abc");
        assert_eq!(base64_decode("YQ").unwrap(), b"a");
        assert_eq!(base64_decode("Y!=="), None);
        let webp = b"RIFF\x10\0\0\0WEBPVP8 ";
        let url = format!("data:image/webp;base64,{}", crate::share::base64(webp));
        let bytes = decode_data_url(&url).unwrap();
        assert!(is_webp(&bytes) && bytes == webp);
        assert!(!is_webp(b"<html>not found</html>"));
        assert_eq!(decode_data_url("https://example.com/x.webp"), None);
    }
}

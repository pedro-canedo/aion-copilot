//! aion-copilot: the pet radar — the monsters in view by species, each with
//! the pet whose soul it gives (when known) against your own collection, and
//! the wild creatures that carry a pet's soul — and the collection itself,
//! kept per character (`copilot-pets.json`) so it shows before the next
//! login. Names for pets are yours (`copilot-pet-names.json`): the game data
//! has none, so a pet with no name of yours takes the name of a species
//! known to give it.
//!
//! Which species gives which pet is learned (`capture::pets::SpeciesBook`)
//! and kept for good in `copilot-pet-species.json` (NPC code -> pet id).
//!
//! Decoding is in `capture::pets`; where things are, in
//! `combat::world_tracker` (spawns, moves and `42 36` keep each creature's
//! place; a zone load clears them).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::capture::pets::{Pet, MAX_LEVEL};
use crate::capture::world_position::Pos;
use crate::combat::data_storage::DataStorage;
use crate::combat::world_tracker::{dist2, Accuracy, EntityKind, Live, OwnPosition};
use crate::i18n::lookup::NpcLookup;

/// A user name for a pet, at most this long (characters).
const MAX_NAME: usize = 40;
/// The radar lists at most this many.
const MAX_NEARBY: usize = 60;

// ===== Kept per character =====

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptPet {
    pub id: u32,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub souls: u32,
    #[serde(default)]
    pub estimated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptCollection {
    #[serde(default)]
    pub active: u32,
    #[serde(default)]
    pub pets: Vec<KeptPet>,
    /// When the game sent it (unix ms).
    #[serde(default)]
    pub seen_ms: i64,
}

impl KeptCollection {
    fn of(c: &crate::capture::pets::Collection) -> Self {
        KeptCollection {
            active: c.active,
            pets: c
                .pets
                .iter()
                .map(|(id, p)| KeptPet { id: *id, level: p.level, souls: p.souls, estimated: p.estimated })
                .collect(),
            seen_ms: c.seen_ms,
        }
    }

    fn pets(&self) -> BTreeMap<u32, Pet> {
        self.pets.iter().map(|p| (p.id, Pet { level: p.level, souls: p.souls, estimated: p.estimated })).collect()
    }
}

/// `copilot-pets.json`: the last collection per character name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetsFile {
    /// The character seen last, to show when the game hasn't said who you are.
    #[serde(default)]
    pub last: Option<String>,
    #[serde(default)]
    pub characters: BTreeMap<String, KeptCollection>,
}

pub fn load_pets_file(path: &Path) -> PetsFile {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Keep the collection the game sent for the character it belongs to, once
/// the game has said whose. Returns whether the file changed.
pub fn remember_pets(storage: &DataStorage, path: &Path) -> bool {
    let book = storage.pet_book();
    let (Some(owner), Some(c)) = (book.owned_by().filter(|n| !n.is_empty()), book.collection.as_ref()) else {
        return false;
    };
    let kept = KeptCollection::of(c);
    let mut file = load_pets_file(path);
    if file.characters.get(owner) == Some(&kept) && file.last.as_deref() == Some(owner) {
        return false;
    }
    file.characters.insert(owner.to_string(), kept);
    file.last = Some(owner.to_string());
    match serde_json::to_string_pretty(&file) {
        Ok(json) => crate::atomic_file::write(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

// ===== Names =====

/// `copilot-pet-names.json`: pet id -> your name for it.
pub type PetNames = BTreeMap<u32, String>;

pub fn load_names(path: &Path) -> PetNames {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Name a pet (`None` or blank forgets the name). False for an id that is no
/// pet's or when the file could not be written.
pub fn set_name(path: &Path, pet: u32, name: Option<&str>) -> bool {
    if !crate::capture::pets::PET_IDS.contains(&pet) {
        return false;
    }
    let mut names = load_names(path);
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => {
            names.insert(pet, n.chars().take(MAX_NAME).collect());
        }
        None => {
            names.remove(&pet);
        }
    }
    match serde_json::to_string_pretty(&names) {
        Ok(json) => crate::atomic_file::write(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

// ===== Species =====

/// `copilot-pet-species.json`: NPC code -> the pet whose soul it gives.
pub type SpeciesFile = BTreeMap<u32, u32>;

pub fn load_species(path: &Path) -> SpeciesFile {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Add what this session learned to the file (never drops what it holds, so
/// a save before the file was read back cannot empty it). Returns whether
/// the file changed.
pub fn remember_species(storage: &DataStorage, path: &Path) -> bool {
    let (learned, _) = storage.pet_species();
    if learned.is_empty() {
        return false;
    }
    let kept = load_species(path);
    let mut file = kept.clone();
    file.extend(learned);
    if file == kept {
        return false;
    }
    match serde_json::to_string_pretty(&file) {
        Ok(json) => crate::atomic_file::write(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

/// The game data's name for an NPC code, when it has one.
pub fn npc_name(npcs: &NpcLookup, code: u32) -> Option<String> {
    i32::try_from(code).ok().map(|c| npcs.get_npc_name(c)).filter(|n| !n.trim().is_empty())
}

/// For each pet a known species gives, that species' name (the lowest code
/// with a name, when several give it): what a pet you have not named is
/// called.
pub fn species_names(learned: &BTreeMap<u32, u32>, name_of: impl Fn(u32) -> Option<String>) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    for (code, pet) in learned {
        if out.contains_key(pet) {
            continue;
        }
        if let Some(name) = name_of(*code) {
            out.insert(*pet, name);
        }
    }
    out
}

// ===== Whose collection shows =====

/// The collection to show: the game's this session when it is the shown
/// character's, else the one kept for that character.
pub struct Shown {
    pub character: Option<String>,
    pub pets: Option<BTreeMap<u32, Pet>>,
    pub active: Option<u32>,
    pub seen_ms: Option<i64>,
    /// From the game this session (else kept).
    pub live: bool,
}

/// As the Routine does: the game's owner, else the character the game named,
/// else the one seen last.
pub fn shown(storage: &DataStorage, file: &PetsFile) -> Shown {
    let book = storage.pet_book();
    let owner = book.owned_by().map(str::to_string);
    let character = owner
        .clone()
        .or_else(|| storage.local_character_name().filter(|n| !n.is_empty()))
        .or_else(|| file.last.clone());
    match book.collection.as_ref().filter(|_| owner.is_some() && owner == character) {
        Some(c) => Shown {
            character,
            pets: Some(c.pets.clone()),
            active: (c.active != 0).then_some(c.active),
            seen_ms: Some(c.seen_ms),
            live: true,
        },
        None => {
            let kept = character.as_ref().and_then(|n| file.characters.get(n));
            Shown {
                pets: kept.map(KeptCollection::pets),
                active: kept.map(|k| k.active).filter(|a| *a != 0),
                seen_ms: kept.map(|k| k.seen_ms),
                character,
                live: false,
            }
        }
    }
}

// ===== Direction =====

/// Compass points, clockwise from north, as the map labels them (L = east,
/// O = west).
const POINTS: [&str; 8] = ["N", "NE", "L", "SE", "S", "SO", "O", "NO"];

/// The bearing (degrees clockwise from north) and compass point from `from`
/// to `to`. Game x grows east and game y grows south: the map projects y
/// downwards with north at the top (`copilot_map::project`).
pub fn compass(from: Pos, to: Pos) -> (f32, &'static str) {
    let east = to[0] - from[0];
    let north = from[1] - to[1];
    let bearing = east.atan2(north).to_degrees().rem_euclid(360.0);
    let point = POINTS[(((bearing + 22.5) / 45.0) as usize) % 8];
    (bearing, point)
}

// ===== Views =====

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Level 1 or more.
    pub owned: usize,
    /// Every pet in the collection (with a level or souls).
    pub known: usize,
    /// Count at level 0, 1, 2 and 3.
    pub by_level: [usize; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbyPet {
    pub pet: u32,
    pub name: Option<String>,
    /// Your level of it (0 = not owned); None without a collection.
    pub level: Option<u32>,
    pub souls: Option<u32>,
    pub estimated: bool,
    /// From you, horizontally; None while where you are is unknown.
    pub distance_m: Option<f32>,
    /// Degrees clockwise from north, and the compass point.
    pub bearing: Option<f32>,
    pub direction: Option<&'static str>,
    /// Since it last moved or came into view.
    pub seen_ago_ms: i64,
    /// The name of a species known to give it (for a pet you have not named).
    pub species_name: Option<String>,
    /// Developer mode only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPet {
    pub pet: u32,
    pub name: Option<String>,
    pub level: u32,
    pub souls: u32,
    pub estimated: bool,
    /// The name of a species known to give it (for a pet you have not named).
    pub species_name: Option<String>,
}

/// Where a species' pet stands in your collection.
pub mod status {
    /// Not in the collection yet (level 0).
    pub const MISSING: &str = "missing";
    /// Owned below the top level: worth hunting to level it up.
    pub const UPGRADE: &str = "upgrade";
    /// At the top level.
    pub const COMPLETE: &str = "complete";
    /// The pet is known, the collection is not (not read yet).
    pub const UNREAD: &str = "unread";
    /// Which pet the species gives is not known yet.
    pub const UNKNOWN: &str = "unknown";
}

/// The monsters of one species in view.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbySpecies {
    pub code: u32,
    /// The game data's name, else "Criatura #code".
    pub name: String,
    /// How many are in view.
    pub count: usize,
    /// The nearest one: from you, horizontally (None while where you are is
    /// unknown), degrees clockwise from north and the compass point.
    pub distance_m: Option<f32>,
    pub bearing: Option<f32>,
    pub direction: Option<&'static str>,
    /// Since one of them last moved or came into view.
    pub seen_ago_ms: i64,
    /// A wild creature carrying a pet's soul (its spawn names the pet).
    pub soul: bool,
    /// The pet whose soul it gives, when known.
    pub pet: Option<u32>,
    /// Your name for that pet, else a species name for it.
    pub pet_name: Option<String>,
    /// Your level of the pet (0 = not owned); None without a collection or a pet.
    pub level: Option<u32>,
    pub souls: Option<u32>,
    pub estimated: bool,
    /// One of `status::*`.
    pub status: &'static str,
    /// Developer mode only: the nearest one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetRadarView {
    pub character: Option<String>,
    pub active: Option<u32>,
    pub active_name: Option<String>,
    /// Whether a collection is known for the character.
    pub has_collection: bool,
    /// From the game this session (else kept from an earlier one).
    pub live: bool,
    /// When the collection was read (unix ms).
    pub seen_ms: Option<i64>,
    pub summary: Summary,
    /// "exact" or "estimated"; None while unknown.
    pub position: Option<&'static str>,
    pub nearby: Vec<NearbyPet>,
    /// Every monster in view, by species.
    pub species: Vec<NearbySpecies>,
    /// How many species are known to give a pet.
    pub learned_species: usize,
    /// Things in view seen only moving: their spawn came before the app
    /// started, so what they are isn't known until they show up again.
    pub unidentified: usize,
    pub collection: Vec<CollectionPet>,
    /// Whether the game is open.
    pub game: bool,
}

pub fn summary(pets: &BTreeMap<u32, Pet>) -> Summary {
    let mut s = Summary { known: pets.len(), ..Summary::default() };
    for p in pets.values() {
        s.by_level[p.level.min(3) as usize] += 1;
    }
    s.owned = s.known - s.by_level[0];
    s
}

/// The pet creatures among `live`, nearest-priority first: not owned, then
/// lower level, then nearer. `own`: where you are (a stale fix is not used).
pub fn nearby<'a>(
    live: impl Iterator<Item = (i32, &'a Live)>,
    pets: Option<&BTreeMap<u32, Pet>>,
    names: &PetNames,
    species_names: &BTreeMap<u32, String>,
    own: Option<&OwnPosition>,
    now: i64,
    dev: bool,
) -> Vec<NearbyPet> {
    let me = own.filter(|p| p.accuracy != Accuracy::Unknown).map(|p| p.pos);
    let mut out: Vec<NearbyPet> = live
        .filter_map(|(entity, l)| {
            let pet = l.pet?;
            let mine = pets.map(|p| p.get(&pet).copied().unwrap_or_default());
            let (bearing, direction) = match me {
                Some(m) => {
                    let (b, d) = compass(m, l.pos);
                    (Some(b.round()), Some(d))
                }
                None => (None, None),
            };
            Some(NearbyPet {
                pet,
                name: names.get(&pet).cloned(),
                level: mine.map(|p| p.level),
                souls: mine.map(|p| p.souls),
                estimated: mine.is_some_and(|p| p.estimated),
                distance_m: me.map(|m| (dist2(m, l.pos) / 100.0).round()),
                bearing,
                direction,
                seen_ago_ms: (now - l.last_ms).max(0),
                species_name: species_names.get(&pet).cloned(),
                entity: dev.then_some(entity),
                code: dev.then_some(l.code),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let key = |p: &NearbyPet| (p.level.map_or(1, |l| u8::from(l > 0)), p.level.unwrap_or(0));
        key(a)
            .cmp(&key(b))
            .then(a.distance_m.unwrap_or(f32::MAX).total_cmp(&b.distance_m.unwrap_or(f32::MAX)))
            .then(a.pet.cmp(&b.pet))
    });
    out.truncate(MAX_NEARBY);
    out
}

/// Where a pet stands in the collection (`status::*`).
fn pet_status(pet: Option<u32>, mine: Option<Pet>) -> &'static str {
    match (pet, mine) {
        (None, _) => status::UNKNOWN,
        (Some(_), None) => status::UNREAD,
        (Some(_), Some(p)) if p.level == 0 => status::MISSING,
        (Some(_), Some(p)) if p.level < MAX_LEVEL => status::UPGRADE,
        _ => status::COMPLETE,
    }
}

/// The order the radar lists species in, after the soul creatures: the
/// pets you lack, the ones to level up, the ones whose level is not read
/// yet, the species not known, and the pets already complete.
fn status_rank(status: &str) -> u8 {
    match status {
        status::MISSING => 0,
        status::UPGRADE => 1,
        status::UNREAD => 2,
        status::UNKNOWN => 3,
        _ => 4,
    }
}

/// Every monster among `live`, grouped by species (the soul creatures by
/// species and pet), each with the pet it gives: the one its spawn names,
/// else the one `learned` says. `pet_names`: what each pet is called;
/// `name_of`: the game data's name for a code. Soul creatures first, then
/// by `status_rank`, lower levels first, then nearer.
#[allow(clippy::too_many_arguments)]
pub fn nearby_species<'a>(
    live: impl Iterator<Item = (i32, &'a Live)>,
    pets: Option<&BTreeMap<u32, Pet>>,
    learned: &BTreeMap<u32, u32>,
    pet_names: &BTreeMap<u32, String>,
    name_of: impl Fn(u32) -> Option<String>,
    own: Option<&OwnPosition>,
    now: i64,
    dev: bool,
) -> Vec<NearbySpecies> {
    let me = own.filter(|p| p.accuracy != Accuracy::Unknown).map(|p| p.pos);
    // (code, the spawn's pet) -> (count, the nearest one (the lowest id
    // without a place of yours) with its squared distance, the newest sighting).
    let mut groups: BTreeMap<(u32, Option<u32>), (usize, (i32, Pos, f32), i64)> = BTreeMap::new();
    for (entity, l) in live {
        if l.kind != EntityKind::Npc || !l.monster || l.code == 0 {
            continue;
        }
        let d = me.map_or(0.0, |m| dist2(m, l.pos));
        let g = groups.entry((l.code, l.pet)).or_insert((0, (entity, l.pos, d), l.last_ms));
        g.0 += 1;
        if d < g.1 .2 || (d == g.1 .2 && entity < g.1 .0) {
            g.1 = (entity, l.pos, d);
        }
        g.2 = g.2.max(l.last_ms);
    }
    let mut out: Vec<NearbySpecies> = groups
        .into_iter()
        .map(|((code, spawn_pet), (count, (entity, pos, d), last_ms))| {
            let pet = spawn_pet.or_else(|| learned.get(&code).copied());
            let mine = pet.and_then(|pet| pets.map(|p| p.get(&pet).copied().unwrap_or_default()));
            let (bearing, direction) = match me {
                Some(m) => {
                    let (b, dir) = compass(m, pos);
                    (Some(b.round()), Some(dir))
                }
                None => (None, None),
            };
            NearbySpecies {
                code,
                name: name_of(code).unwrap_or_else(|| format!("Criatura #{code}")),
                count,
                distance_m: me.map(|_| (d / 100.0).round()),
                bearing,
                direction,
                seen_ago_ms: (now - last_ms).max(0),
                soul: spawn_pet.is_some(),
                pet,
                pet_name: pet.and_then(|p| pet_names.get(&p).cloned()),
                level: mine.map(|p| p.level),
                souls: mine.map(|p| p.souls),
                estimated: mine.is_some_and(|p| p.estimated),
                status: pet_status(pet, mine),
                entity: dev.then_some(entity),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        let key = |s: &NearbySpecies| (!s.soul, status_rank(s.status), s.level.unwrap_or(0));
        key(a)
            .cmp(&key(b))
            .then(a.distance_m.unwrap_or(f32::MAX).total_cmp(&b.distance_m.unwrap_or(f32::MAX)))
            .then(a.code.cmp(&b.code))
    });
    out.truncate(MAX_NEARBY);
    out
}

/// The radar and the collection, for the Pets overlay.
pub fn radar(storage: &DataStorage, file: &PetsFile, names: &PetNames, npcs: &NpcLookup, dev: bool, game: bool, now: i64) -> PetRadarView {
    let shown = shown(storage, file);
    let (learned, _) = storage.pet_species();
    let by_species = species_names(&learned, |code| npc_name(npcs, code));
    // What each pet is called: your name, else a species'.
    let mut pet_names = by_species.clone();
    pet_names.extend(names.iter().map(|(k, v)| (*k, v.clone())));
    let (near, species, position, unidentified) = storage.with_world(|t| {
        let own = t.own_position(now);
        let position = own.as_ref().filter(|p| p.accuracy != Accuracy::Unknown).map(|p| p.accuracy.as_str());
        let in_view = || t.in_view(now).map(|(id, l)| (*id, l));
        let pets = shown.pets.as_ref();
        (
            nearby(in_view(), pets, names, &by_species, own.as_ref(), now, dev),
            nearby_species(in_view(), pets, &learned, &pet_names, |code| npc_name(npcs, code), own.as_ref(), now, dev),
            position,
            in_view().filter(|(_, l)| l.kind == crate::combat::world_tracker::EntityKind::Unknown).count(),
        )
    });
    let collection = shown
        .pets
        .as_ref()
        .map(|pets| {
            pets.iter()
                .map(|(id, p)| CollectionPet {
                    pet: *id,
                    name: names.get(id).cloned(),
                    level: p.level,
                    souls: p.souls,
                    estimated: p.estimated,
                    species_name: by_species.get(id).cloned(),
                })
                .collect()
        })
        .unwrap_or_default();
    PetRadarView {
        active_name: shown.active.and_then(|a| pet_names.get(&a).cloned()),
        active: shown.active,
        has_collection: shown.pets.is_some(),
        live: shown.live && game,
        seen_ms: shown.seen_ms,
        summary: shown.pets.as_ref().map(summary).unwrap_or_default(),
        character: shown.character,
        position,
        nearby: near,
        species,
        learned_species: learned.len(),
        unidentified,
        collection,
        game,
    }
}

/// Your level of each pet, for the map's tooltips (empty without a collection).
pub fn levels(storage: &DataStorage, file: &PetsFile) -> BTreeMap<u32, u32> {
    shown(storage, file).pets.map(|p| p.into_iter().map(|(id, p)| (id, p.level)).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::world_tracker::EntityKind;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// A real session (COPILOT_PET_REPLAY=<frame dump>; skipped without it):
    /// the login's collection reaches the radar under your name, and the
    /// creatures carrying a pet's soul seen on the way are listed.
    #[test]
    fn a_real_login_fills_the_radar() {
        let Ok(path) = std::env::var("COPILOT_PET_REPLAY") else { return };
        use crate::capture::stream_processor::StreamProcessor;
        use crate::i18n::lookup::{NpcLookup, SkillLookup};
        use std::sync::Arc;
        let storage = Arc::new(DataStorage::new());
        let mut processor = StreamProcessor::new(storage.clone(), Arc::new(SkillLookup::new()), Arc::new(NpcLookup::new()));
        let mut last = 0;
        let mut creatures = std::collections::BTreeMap::new();
        // COPILOT_PET_REPLAY_UNTIL=<unix ms>: stop there, to see the radar at that moment.
        let until = std::env::var("COPILOT_PET_REPLAY_UNTIL").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(i64::MAX);
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let parts: Vec<&str> = line.trim().splitn(3, '|').collect();
            if parts.len() != 3 {
                continue;
            }
            if let Ok(ts) = parts[0].parse::<i64>() {
                if ts > until {
                    break;
                }
                processor.set_override_timestamp(Some(ts));
                crate::clock::set_override(Some(ts));
                last = ts;
            }
            let hex = parts[2].trim();
            let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
            processor.consume_stream(&bytes);
            storage.with_world(|t| {
                for (id, l) in t.in_view(last) {
                    if let Some(pet) = l.pet {
                        creatures.insert(*id, pet);
                    }
                }
            });
        }
        let view = radar(&storage, &PetsFile::default(), &PetNames::default(), &NpcLookup::new(), true, true, last);
        crate::clock::set_override(None);
        for s in &view.species {
            eprintln!("pet replay species: {:?}", s);
        }
        eprintln!(
            "pet replay: {:?} active {:?} summary {:?}, {} in the collection, creatures seen {:?}, species learned {:?}",
            view.character,
            view.active,
            view.summary,
            view.collection.len(),
            creatures,
            storage.pet_species().0
        );
        assert!(view.has_collection, "the login's collection is read");
        assert_eq!(view.character.as_deref(), Some("ELLAO"));
        assert!(view.summary.owned > 0);
    }

    fn login(storage: &DataStorage) {
        let packet = hex(include_str!("capture/testdata/pet_collection_v1.hex"));
        storage.note_pet_packet(crate::capture::pets::parse(&packet, 1_000).unwrap());
    }

    fn creature(pet: u32, pos: Pos, last_ms: i64) -> Live {
        Live {
            kind: EntityKind::Npc,
            code: 2_930_392,
            monster: true,
            name: None,
            pet: Some(pet),
            variant: None,
            pos,
            heading: None,
            last_ms,
        }
    }

    fn at(pos: Pos) -> OwnPosition {
        OwnPosition { pos, accuracy: Accuracy::Exact, age_ms: 0, radius: None, heading: None, source: None }
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("copilot-pets-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{name}.json"));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn the_compass_has_north_at_minus_y_and_east_at_plus_x() {
        let o = [0.0, 0.0, 0.0];
        assert_eq!(compass(o, [0.0, -100.0, 0.0]), (0.0, "N"));
        assert_eq!(compass(o, [100.0, 0.0, 0.0]).1, "L");
        assert_eq!(compass(o, [0.0, 100.0, 0.0]).1, "S");
        assert_eq!(compass(o, [-100.0, 0.0, 0.0]).1, "O");
        let (b, d) = compass(o, [100.0, -100.0, 0.0]);
        assert!((b - 45.0).abs() < 0.01 && d == "NE");
        assert_eq!(compass(o, [-100.0, 100.0, 0.0]).1, "SO");
        assert_eq!(compass(o, [-100.0, -100.0, 0.0]).1, "NO");
        assert_eq!(compass(o, [100.0, 100.0, 0.0]).1, "SE");
        // 20° east of north is still north.
        assert_eq!(compass(o, [36.4, -100.0, 0.0]).1, "N");
    }

    #[test]
    fn the_radar_lists_missing_pets_first_then_lower_levels_then_nearer() {
        let pets: BTreeMap<u32, Pet> = [
            (1001, Pet { level: 3, souls: 0, estimated: false }),
            (1002, Pet { level: 1, souls: 4, estimated: false }),
            (1003, Pet { level: 1, souls: 9, estimated: true }),
            (1004, Pet { level: 0, souls: 2, estimated: false }),
        ]
        .into_iter()
        .collect();
        let names: PetNames = [(1002, "Fofo".to_string())].into_iter().collect();
        let live = [
            (1, creature(1001, [0.0, -500.0, 0.0], 9_000)),
            (2, creature(1002, [3_000.0, 0.0, 0.0], 9_000)),
            (3, creature(1003, [1_000.0, 0.0, 0.0], 9_000)),
            (4, creature(1004, [0.0, 9_000.0, 0.0], 8_000)),
            // Not in the collection at all: not owned either.
            (5, creature(1200, [0.0, 2_000.0, 0.0], 9_000)),
            // No pet: not listed.
            (6, Live { pet: None, ..creature(1001, [0.0; 3], 9_000) }),
        ];
        let me = at([0.0, 0.0, 0.0]);
        let list = nearby(live.iter().map(|(e, l)| (*e, l)), Some(&pets), &names, &BTreeMap::new(), Some(&me), 10_000, false);
        let order: Vec<u32> = list.iter().map(|p| p.pet).collect();
        assert_eq!(order, [1200, 1004, 1003, 1002, 1001]);
        assert_eq!((list[0].level, list[0].souls, list[0].distance_m, list[0].direction), (Some(0), Some(0), Some(20.0), Some("S")));
        assert_eq!((list[1].distance_m, list[1].seen_ago_ms), (Some(90.0), 2_000));
        assert!(list[2].estimated && list[2].direction == Some("L"));
        assert_eq!((list[3].name.as_deref(), list[3].souls, list[3].distance_m), (Some("Fofo"), Some(4), Some(30.0)));
        assert_eq!((list[4].level, list[4].direction, list[4].bearing), (Some(3), Some("N"), Some(0.0)));
        assert!(list.iter().all(|p| p.entity.is_none() && p.code.is_none()), "developer mode only");
        let dev = nearby(live.iter().map(|(e, l)| (*e, l)), Some(&pets), &names, &BTreeMap::new(), Some(&me), 10_000, true);
        assert_eq!((dev[0].entity, dev[0].code), (Some(5), Some(2_930_392)));
    }

    #[test]
    fn without_a_position_or_a_collection_the_radar_still_lists_what_is_near() {
        let live = [(1, creature(1001, [0.0, -500.0, 0.0], 0)), (2, creature(1002, [0.0; 3], 0))];
        let stale = OwnPosition { accuracy: Accuracy::Unknown, ..at([0.0; 3]) };
        let list = nearby(live.iter().map(|(e, l)| (*e, l)), None, &PetNames::new(), &BTreeMap::new(), Some(&stale), 0, false);
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|p| p.level.is_none() && p.distance_m.is_none() && p.direction.is_none()));
    }

    #[test]
    fn a_wild_pet_creature_spawn_shows_on_the_radar_with_your_level() {
        let storage = DataStorage::new();
        login(&storage);
        storage.set_local_identity_from_game(14667, Some("ELLAO".into()));
        storage.note_world_packet(&hex(include_str!("capture/testdata/world_npc_spawn_pet_v1.hex")));
        // Another kind-0x0c spawn without a pet: not on the radar.
        storage.note_world_packet(&hex(include_str!("capture/testdata/world_npc_spawn_pet_none.hex")));
        let view = radar(&storage, &PetsFile::default(), &PetNames::new(), &NpcLookup::new(), false, true, crate::clock::now_ms());
        assert_eq!(view.character.as_deref(), Some("ELLAO"));
        assert!(view.live && view.has_collection);
        assert_eq!(view.active, Some(1066));
        assert_eq!((view.summary.owned, view.summary.by_level), (115, [view.summary.known - 115, 86, 14, 15]));
        assert_eq!(view.nearby.len(), 1);
        let p = &view.nearby[0];
        let mine = view.collection.iter().find(|c| c.pet == 1291).map(|c| c.level).unwrap_or(0);
        assert_eq!((p.pet, p.level), (1291, Some(mine)));
        assert_eq!(view.collection.len(), view.summary.known);
    }

    #[test]
    fn a_soul_gain_counts_on_the_radar_as_an_estimate() {
        let storage = DataStorage::new();
        login(&storage);
        storage.set_local_identity_from_game(14667, Some("ELLAO".into()));
        storage.note_pet_packet(crate::capture::pets::parse(&hex("0f0d90010504000001000000"), 2_000).unwrap());
        let view = radar(&storage, &PetsFile::default(), &PetNames::new(), &NpcLookup::new(), false, true, 3_000);
        let before = {
            let s = DataStorage::new();
            login(&s);
            s.set_local_identity_from_game(14667, Some("ELLAO".into()));
            s.pet_book().collection.unwrap().pets.get(&1029).copied().unwrap_or_default()
        };
        let p = view.collection.iter().find(|c| c.pet == 1029).unwrap();
        assert_eq!((p.souls, p.estimated), (before.souls + 1, true));
    }

    #[test]
    fn the_collection_is_kept_per_character_and_shows_before_the_next_login() {
        let path = temp("copilot-pets");
        let storage = DataStorage::new();
        assert!(!remember_pets(&storage, &path), "nothing yet");
        login(&storage);
        assert!(!remember_pets(&storage, &path), "whose is not known yet");
        storage.set_local_identity_from_game(14667, Some("ELLAO".into()));
        assert!(remember_pets(&storage, &path));
        assert!(!remember_pets(&storage, &path), "unchanged");

        // The next start: nothing from the game yet.
        let file = load_pets_file(&path);
        assert_eq!(file.last.as_deref(), Some("ELLAO"));
        let fresh = DataStorage::new();
        let view = radar(&fresh, &file, &PetNames::new(), &NpcLookup::new(), false, true, 0);
        assert_eq!(view.character.as_deref(), Some("ELLAO"));
        assert!(!view.live && view.has_collection);
        assert_eq!((view.active, view.summary.owned), (Some(1066), 115));
        assert_eq!(view.collection.iter().find(|c| c.pet == 1013).map(|c| c.souls), Some(37));
        assert_eq!(levels(&fresh, &file).get(&1003), Some(&2));

        // Another character with nothing kept: no collection shown.
        fresh.set_local_identity_from_game(4294, Some("Alt".into()));
        let view = radar(&fresh, &file, &PetNames::new(), &NpcLookup::new(), false, true, 0);
        assert_eq!(view.character.as_deref(), Some("Alt"));
        assert!(!view.has_collection && view.collection.is_empty());
        // The game closed: not live.
        let view = radar(&storage, &file, &PetNames::new(), &NpcLookup::new(), false, false, 0);
        assert!(!view.live && view.has_collection);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn names_are_set_trimmed_and_cleared() {
        let path = temp("copilot-pet-names");
        assert!(set_name(&path, 1003, Some("  Fofo  ")));
        assert!(set_name(&path, 1013, Some("Rex")));
        assert_eq!(load_names(&path).get(&1003).map(String::as_str), Some("Fofo"));
        assert!(set_name(&path, 1003, Some("   ")));
        assert!(set_name(&path, 1013, None));
        assert!(load_names(&path).is_empty());
        assert!(!set_name(&path, 7, Some("not a pet")));
        assert!(set_name(&path, 1003, Some(&"x".repeat(100))));
        assert_eq!(load_names(&path)[&1003].chars().count(), MAX_NAME);
        let _ = std::fs::remove_file(&path);
    }

    // ===== Species =====

    /// 2026-10-18 16:59, ELLAO (entity 8301) on entity 40539, an "Espírito
    /// da Floresta da Serra" (kind 0x04, code 2100555): its spawn, HP 0, its
    /// loot record naming ELLAO, souls of pet 1048 544 ms later, and its death
    /// (`42 36` flag 3) 3.5 s after those.
    const SPAWN_40539: (i64, &str) = (
        1_792_353_581_657,
        "6c4136dbbc020420004b0d2000000259646bc561bfcc46000c9746c4af134306690194a70794a70764000000640000000000000000000000000000000000000064000000640000000100000000000000000000000000000000000000060001001b00000079e5939700",
    );
    const HP0_40539: (i64, &str) = (1_792_353_586_672, "14008ddbbc020201000000000000000000");
    const GONE_40539: (i64, &str) = (1_792_353_586_780, "0b3538dbbc020000");
    const LOOT_40539: (i64, &str) =
        (1_792_353_586_780, "28048ddbbc02a7c8f000ed407b0505454c4c414f0750415445544153010000000000000100");
    const SOULS_1048: (i64, &str) = (1_792_353_587_324, "0f0d90011804000001000000");
    const DEATH_40539: (i64, &str) = (1_792_353_590_815, "0b4236dbbc020003");
    /// The same session: entity 51315, "Implacável Kandhak" (kind 0x0c, code
    /// 2101109), and the loot record ELLAO got for it (at 17:00:21, moved
    /// here into the window of the souls above).
    const SPAWN_51315: (i64, &str) = (
        1_792_353_581_700,
        "c3014136f390020c3000750f200000024bd4c5c3908eba46002897460000e14200500198ffcd0198ffcd01640000006400000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000602110281969800ffffffffffffffff8075d52abb030000f3900201044bd4c5c3908eba4600289746110384969800ffffffffffffffff8075d52abb030000f39002014bd4c5c3908eba460028974606002f000000cb4f0f00b4c7b6fa01a4170000",
    );
    const LOOT_51315: (i64, &str) =
        (1_792_353_586_900, "28048df390026e17e800ed407b0505454c4c414f0750415445544153010000000000000100");

    fn replay(storage: &std::sync::Arc<DataStorage>, frames: &[(i64, &str)]) {
        use crate::capture::stream_processor::StreamProcessor;
        use crate::i18n::lookup::SkillLookup;
        use std::sync::Arc;
        let mut processor = StreamProcessor::new(storage.clone(), Arc::new(SkillLookup::new()), Arc::new(NpcLookup::new()));
        for (ts, frame) in frames {
            processor.set_override_timestamp(Some(*ts));
            processor.consume_stream(&hex(frame));
        }
        processor.set_override_timestamp(None);
    }

    #[test]
    fn creatures_seen_only_moving_are_counted_not_listed() {
        // A move (1a 37) of something whose spawn came before the app.
        let storage = DataStorage::new();
        storage.note_world_packet(&hex("191a379782030000481086c7046213c600d42246fa81"));
        let view = radar(&storage, &PetsFile::default(), &PetNames::default(), &NpcLookup::new(), false, true, crate::clock::now_ms());
        assert!(view.species.is_empty());
        assert_eq!(view.unidentified, 1);
        assert_eq!(serde_json::to_value(&view).unwrap()["unidentified"], 1);
    }

    fn ellao() -> std::sync::Arc<DataStorage> {
        let storage = std::sync::Arc::new(DataStorage::new());
        login(&storage);
        storage.set_local_identity_from_game(8301, Some("ELLAO".into()));
        storage
    }

    fn npcs() -> NpcLookup {
        let npcs = NpcLookup::new();
        npcs.load_from_json(
            r#"{"2100555": {"name": "Espírito da Floresta da Serra", "isBoss": false}, "2101109": {"name": "Implacável Kandhak", "isBoss": true}}"#,
        );
        npcs
    }

    #[test]
    fn souls_right_after_your_kill_teach_the_radar_which_pet_a_species_gives() {
        let storage = ellao();
        replay(&storage, &[SPAWN_40539, HP0_40539, GONE_40539, LOOT_40539, SOULS_1048]);
        assert_eq!(storage.pet_species().0, [(2_100_555, 1048)].into_iter().collect());

        // Still in view until its death: listed with its pet.
        let view = radar(&storage, &PetsFile::default(), &PetNames::new(), &npcs(), false, true, SOULS_1048.0);
        assert_eq!(view.learned_species, 1);
        assert!(view.nearby.is_empty(), "no soul creature");
        let [s] = &view.species[..] else { panic!("{:?}", view.species) };
        assert_eq!((s.code, s.name.as_str(), s.count, s.soul), (2_100_555, "Espírito da Floresta da Serra", 1, false));
        // Level 1 with 2 souls at the login, one more from the kill.
        assert_eq!((s.pet, s.level, s.souls, s.estimated, s.status), (Some(1048), Some(1), Some(3), true, status::UPGRADE));
        assert_eq!(s.pet_name.as_deref(), Some("Espírito da Floresta da Serra"), "no name of yours: the species'");
        // The collection calls the pet by the species too.
        let p = view.collection.iter().find(|p| p.pet == 1048).unwrap();
        assert_eq!((p.name.as_deref(), p.species_name.as_deref()), (None, Some("Espírito da Floresta da Serra")));
        // Your own name for it wins on the radar.
        let names: PetNames = [(1048, "Fofo".to_string())].into_iter().collect();
        let view = radar(&storage, &PetsFile::default(), &names, &npcs(), false, true, SOULS_1048.0);
        assert_eq!(view.species[0].pet_name.as_deref(), Some("Fofo"));

        // Its death takes it off the radar; the link stays.
        replay(&storage, &[DEATH_40539]);
        let view = radar(&storage, &PetsFile::default(), &PetNames::new(), &npcs(), false, true, DEATH_40539.0);
        assert!(view.species.is_empty());
        assert_eq!(storage.pet_species().0.get(&2_100_555), Some(&1048));
    }

    #[test]
    fn souls_with_no_kill_of_yours_just_before_teach_nothing() {
        // No loot record.
        let storage = ellao();
        replay(&storage, &[SPAWN_40539, HP0_40539, GONE_40539, SOULS_1048, DEATH_40539]);
        assert!(storage.pet_species().0.is_empty());
        // The loot was someone else's.
        let storage = std::sync::Arc::new(DataStorage::new());
        storage.set_local_identity_from_game(4294, Some("Alt".into()));
        replay(&storage, &[SPAWN_40539, LOOT_40539, SOULS_1048]);
        assert!(storage.pet_species().0.is_empty());
        // Yours, but more than 3 s before the souls.
        let storage = ellao();
        replay(&storage, &[SPAWN_40539, LOOT_40539, (LOOT_40539.0 + 3_100, SOULS_1048.1)]);
        assert!(storage.pet_species().0.is_empty());
        // The death alone (it comes after the souls anyway).
        let storage = ellao();
        replay(&storage, &[SPAWN_40539, (SOULS_1048.0 - 1_000, DEATH_40539.1), SOULS_1048]);
        assert!(storage.pet_species().0.is_empty());
    }

    #[test]
    fn souls_after_kills_of_two_species_teach_nothing() {
        let storage = ellao();
        replay(&storage, &[SPAWN_40539, SPAWN_51315, LOOT_40539, LOOT_51315, SOULS_1048]);
        assert!(storage.pet_species().0.is_empty());
        // Both still in view, neither with a pet known.
        let view = radar(&storage, &PetsFile::default(), &PetNames::new(), &npcs(), true, true, SOULS_1048.0);
        let listed: Vec<(u32, Option<u32>, &str)> = view.species.iter().map(|s| (s.code, s.pet, s.status)).collect();
        assert_eq!(listed, [(2_100_555, None, status::UNKNOWN), (2_101_109, None, status::UNKNOWN)]);
        assert_eq!(view.species[0].entity, Some(40539), "developer mode");
    }

    #[test]
    fn souls_after_a_kill_of_yours_whose_spawn_was_missed_teach_nothing() {
        // Entity 51315's spawn never seen: its species is unknown, so the
        // window holds 40539's and another.
        let storage = ellao();
        replay(&storage, &[SPAWN_40539, LOOT_40539, LOOT_51315, SOULS_1048]);
        assert!(storage.pet_species().0.is_empty());
    }

    #[test]
    fn a_soul_creatures_own_soul_is_not_learned_for_a_regular_kill_beside_it() {
        let storage = DataStorage::new();
        storage.set_local_identity_from_game(14667, Some("ELLAO".into()));
        storage.note_world_packet(&hex(include_str!("capture/testdata/world_npc_spawn_pet_v1.hex")));
        storage.note_world_packet(&hex(include_str!("capture/testdata/world_npc_spawn_pet_none.hex")));
        let regular = storage.with_world(|w| w.live().iter().find(|(_, l)| l.code == 2_931_240).map(|(e, _)| *e)).unwrap();
        // Both yours, then the soul creature's own soul (pet 1291).
        storage.note_corpse_owner(regular, 14667, "ELLAO");
        storage.note_corpse_owner(28734, 14667, "ELLAO");
        storage.note_pet_packet(crate::capture::pets::parse(&hex("0f0d90010b05000001000000"), 0).unwrap());
        storage.settle_pet_souls();
        assert!(storage.pet_species().0.is_empty());
        // The regular kill's own souls (pet 1048) are still its.
        storage.note_pet_packet(crate::capture::pets::parse(&hex("0f0d90011804000001000000"), 0).unwrap());
        storage.settle_pet_souls();
        assert_eq!(storage.pet_species().0, [(2_931_240, 1048)].into_iter().collect());
    }

    #[test]
    fn a_soul_creature_lists_first_with_its_spawns_pet_and_is_not_learned() {
        let storage = DataStorage::new();
        login(&storage);
        storage.set_local_identity_from_game(14667, Some("ELLAO".into()));
        storage.note_world_packet(&hex(include_str!("capture/testdata/world_npc_spawn_pet_v1.hex")));
        storage.note_world_packet(&hex(include_str!("capture/testdata/world_npc_spawn_pet_none.hex")));
        // One code carries several pets: only the spawn's own is trusted,
        // even after a kill of yours and its souls.
        storage.note_corpse_owner(28734, 14667, "ELLAO");
        storage.note_pet_packet(crate::capture::pets::parse(&hex("0f0d90010b05000001000000"), 0).unwrap());
        assert!(storage.pet_species().0.is_empty());
        let view = radar(&storage, &PetsFile::default(), &PetNames::new(), &NpcLookup::new(), false, true, crate::clock::now_ms());
        let listed: Vec<(u32, &str, bool, Option<u32>, &str)> =
            view.species.iter().map(|s| (s.code, s.name.as_str(), s.soul, s.pet, s.status)).collect();
        // 1291 is not in that collection: missing.
        assert_eq!(
            listed,
            [(2_930_392, "Criatura #2930392", true, Some(1291), status::MISSING), (2_931_240, "Criatura #2931240", false, None, status::UNKNOWN)]
        );
        assert!(view.species.iter().all(|s| s.entity.is_none()), "developer mode only");
    }

    fn monster(code: u32, pos: Pos, last_ms: i64) -> Live {
        Live { code, pet: None, ..creature(0, pos, last_ms) }
    }

    #[test]
    fn species_group_by_code_and_sort_soul_missing_upgrade_unknown_complete() {
        let pets: BTreeMap<u32, Pet> = [
            (1001, Pet { level: 3, souls: 0, estimated: false }),
            (1002, Pet { level: 2, souls: 4, estimated: false }),
            (1003, Pet { level: 1, souls: 9, estimated: false }),
        ]
        .into_iter()
        .collect();
        let learned: BTreeMap<u32, u32> = [(10, 1001), (11, 1002), (12, 1003), (13, 1004)].into_iter().collect();
        let live = [
            (1, monster(10, [0.0, -500.0, 0.0], 9_000)),
            (2, monster(11, [3_000.0, 0.0, 0.0], 9_000)),
            (3, monster(11, [1_000.0, 0.0, 0.0], 8_000)),
            (4, monster(12, [0.0, 900.0, 0.0], 9_000)),
            (5, monster(13, [0.0, 9_000.0, 0.0], 9_000)),
            (6, monster(14, [0.0, 100.0, 0.0], 9_500)),
            (7, creature(1003, [0.0, 20_000.0, 0.0], 9_000)),
            // Not monsters: a summon, a player, a creature whose spawn was missed.
            (8, Live { monster: false, ..monster(15, [0.0; 3], 9_000) }),
            (9, Live { kind: EntityKind::Player, ..monster(0, [0.0; 3], 9_000) }),
            (10, Live { kind: EntityKind::Unknown, ..monster(0, [0.0; 3], 9_000) }),
        ];
        let name_of = |code: u32| (code == 11).then(|| "Zaif da Colina".to_string());
        let me = at([0.0, 0.0, 0.0]);
        let list = nearby_species(live.iter().map(|(e, l)| (*e, l)), Some(&pets), &learned, &BTreeMap::new(), name_of, Some(&me), 10_000, true);
        let order: Vec<(u32, &str)> = list.iter().map(|s| (s.code, s.status)).collect();
        assert_eq!(
            order,
            [
                (2_930_392, status::UPGRADE),
                (13, status::MISSING),
                (12, status::UPGRADE),
                (11, status::UPGRADE),
                (14, status::UNKNOWN),
                (10, status::COMPLETE),
            ]
        );
        assert!(list[0].soul && list[0].pet == Some(1003));
        let zaif = &list[3];
        assert_eq!((zaif.name.as_str(), zaif.count, zaif.distance_m, zaif.direction), ("Zaif da Colina", 2, Some(10.0), Some("L")));
        assert_eq!((zaif.entity, zaif.seen_ago_ms, zaif.level), (Some(3), 1_000, Some(2)));
        assert_eq!((list[4].name.as_str(), list[4].pet, list[4].level), ("Criatura #14", None, None));
        // No collection: the pet is known, its level not.
        let unread = nearby_species(live.iter().map(|(e, l)| (*e, l)), None, &learned, &BTreeMap::new(), |_| None, None, 10_000, false);
        assert!(unread.iter().all(|s| s.distance_m.is_none() && s.direction.is_none() && s.entity.is_none()));
        assert_eq!(unread.iter().find(|s| s.code == 10).map(|s| s.status), Some(status::UNREAD));
        assert_eq!(unread.iter().find(|s| s.code == 14).map(|s| s.status), Some(status::UNKNOWN));
    }

    #[test]
    fn a_pet_with_no_name_of_yours_takes_a_named_species_name() {
        let learned: BTreeMap<u32, u32> = [(2_930_373, 1231), (2_100_812, 1231), (2_100_555, 1048), (2_930_374, 1001)].into_iter().collect();
        let names = species_names(&learned, |code| npc_name(&npcs(), code).or_else(|| (code == 2_100_812).then(|| "Espírito da Pedra da Colina".into())));
        let got: Vec<(u32, &str)> = names.iter().map(|(p, n)| (*p, n.as_str())).collect();
        assert_eq!(got, [(1048, "Espírito da Floresta da Serra"), (1231, "Espírito da Pedra da Colina")]);
    }

    #[test]
    fn the_species_file_only_grows() {
        let path = temp("copilot-pet-species");
        let storage = DataStorage::new();
        assert!(!remember_species(&storage, &path), "nothing learned");
        std::fs::write(&path, r#"{"2100812": 1231}"#).unwrap();
        // Learned this session, the file not read back: what it holds stays.
        storage.seed_pet_species(&[(2_100_555, 1048)].into_iter().collect());
        assert!(remember_species(&storage, &path));
        let kept = load_species(&path);
        assert_eq!(kept, [(2_100_555, 1048), (2_100_812, 1231)].into_iter().collect());
        assert!(!remember_species(&storage, &path), "unchanged");
        // The next start: seeded from the file.
        let fresh = DataStorage::new();
        fresh.seed_pet_species(&load_species(&path));
        assert_eq!(fresh.pet_species().0, kept);
        let _ = std::fs::remove_file(&path);
    }
}

//! aion-copilot: your pet collection, for the pet radar (`copilot_pets`).
//!
//! ```text
//! <len varint> 00 90 <active pet u32> <n u8> (<pet u32> <pet u32, again> <level u32>) x n
//!                    <m u8> (<pet u32> <souls u32>) x m  <other data, not read>
//!                                                 the collection: at login, before the self record
//! <len varint> 0d 90 01 <pet u32> <count u32>     souls gained (seen once, right after a kill)
//! ```
//!
//! The login of 2026-10-18 11:58 (`testdata/pet_collection_v1.hex`) lists 115
//! pets with a level (86 at 1, 14 at 2, 15 at 3) and 98 with souls: every
//! level and every soul count matched what the game showed (16 of the soul
//! entries are 0). A pet with souls
//! and no level (1007 there) is one not owned yet: level 0. `0d 90` was seen
//! once (pet 1029, +1) and is a candidate only: what it adds is marked as an
//! estimate until the next collection replaces it.
//!
//! Wild creatures carrying a pet's soul are NPC spawns (`41 36`) of kind 0x0c
//! (codes 2930xxx) with `0a 00 00 00 <pet u32> 01 00 00 00` after their place
//! (89 bytes after the kind in all 32 seen); other kind-0x0c spawns (2931xxx)
//! have `ff` bytes there instead (`spawn_pet`).
//!
//! The ids are the game's pet ids (1001-1400 seen); no names or soul
//! thresholds are known, so none are made up here.
//!
//! Which monster gives which pet's soul is learned (`SpeciesBook`): a regular
//! monster's spawn carries no pet. The soul creatures' codes are no guide
//! either: they bear no relation to the pets' ids, and one code carries
//! several pets (in the captures of 2026-10-18, 2930392 carried 1291 and
//! 1064, 2930374 carried 1001, 1092 and 1124), so only each spawn's own pet
//! is trusted for them, and they are not learned. On 2026-10-18 16:59:46
//! entity 40539 (code
//! 2100555) took its last hit and read HP 0, a `04 8d` named ELLAO its loot
//! owner 108 ms later, the `0d 90` (pet 1048, +1) came 544 ms after that and
//! its `42 36` death (flag 3) only 3.5 s after the souls: so a kill is
//! anchored on the loot record, not the death. All three kills of the
//! player's in that capture had such a record, each about 4 s before the
//! death.

use std::collections::{BTreeMap, VecDeque};

const COLLECTION: [u8; 2] = [0x00, 0x90];
const SOUL_GAIN: [u8; 2] = [0x0D, 0x90];
/// The pet ids seen: 1001-1400.
pub const PET_IDS: std::ops::RangeInclusive<u32> = 1001..=1400;
/// The highest level a pet has.
pub const MAX_LEVEL: u32 = 3;
/// A collection whose self record never came (the pet screen opened
/// mid-session, say) is the named character's after this long.
pub const SELF_RECORD_WAIT_MS: i64 = 10_000;

/// One pet of the collection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pet {
    /// 0 = not owned yet.
    pub level: u32,
    pub souls: u32,
    /// Souls added by `0d 90` since the collection was read.
    pub estimated: bool,
}

/// The collection as the game sent it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Collection {
    pub active: u32,
    pub pets: BTreeMap<u32, Pet>,
    /// When it was read (unix ms).
    pub seen_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PetPacket {
    Collection(Collection),
    SoulGain { pet: u32, count: u32 },
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn varint_len(b: &[u8]) -> Option<usize> {
    for (i, x) in b.iter().take(5).enumerate() {
        if x & 0x80 == 0 {
            return Some(i + 1);
        }
    }
    None
}

fn parse_collection(body: &[u8], now_ms: i64) -> Option<Collection> {
    let active = u32_at(body, 0)?;
    if active != 0 && !PET_IDS.contains(&active) {
        return None;
    }
    let n = usize::from(*body.get(4)?);
    let mut i = 5;
    let mut pets = BTreeMap::new();
    for _ in 0..n {
        let (id, again, level) = (u32_at(body, i)?, u32_at(body, i + 4)?, u32_at(body, i + 8)?);
        if id != again || !PET_IDS.contains(&id) || level > MAX_LEVEL {
            return None;
        }
        pets.insert(id, Pet { level, ..Pet::default() });
        i += 12;
    }
    let m = usize::from(*body.get(i)?);
    i += 1;
    for _ in 0..m {
        let (id, souls) = (u32_at(body, i)?, u32_at(body, i + 4)?);
        if !PET_IDS.contains(&id) {
            return None;
        }
        pets.entry(id).or_default().souls = souls;
        i += 8;
    }
    // What follows is other data, not read.
    (!pets.is_empty()).then_some(Collection { active, pets, seen_ms: now_ms })
}

/// The pet data in a packet (`<len varint> <opcode> <body>`), or None.
pub fn parse(packet: &[u8], now_ms: i64) -> Option<PetPacket> {
    let i = varint_len(packet)?;
    let opcode = packet.get(i..i + 2)?;
    let body = &packet[i + 2..];
    if opcode == COLLECTION {
        parse_collection(body, now_ms).map(PetPacket::Collection)
    } else if opcode == SOUL_GAIN {
        if body.len() != 9 || body[0] != 1 {
            return None;
        }
        let (pet, count) = (u32_at(body, 1)?, u32_at(body, 5)?);
        (PET_IDS.contains(&pet) && count > 0 && count < 10_000).then_some(PetPacket::SoulGain { pet, count })
    } else {
        None
    }
}

/// The pet whose soul a kind-0x0c spawn carries: `rest` is what follows its
/// place and heading. Only when `0a 00 00 00 <pet> 01 00 00 00` is there
/// exactly once with a pet id.
pub fn spawn_pet(rest: &[u8]) -> Option<u32> {
    let mut found = None;
    for (i, w) in rest.windows(12).enumerate() {
        if w[..4] == [0x0A, 0, 0, 0] && w[8..] == [1, 0, 0, 0] {
            let pet = u32_at(rest, i + 4)?;
            if PET_IDS.contains(&pet) {
                if found.is_some() {
                    return None;
                }
                found = Some(pet);
            }
        }
    }
    found
}

/// The collection the game has sent, and whose it is.
///
/// Whose: at login the collection comes a moment before the self record that
/// names who logged in (as the counters list does), so it waits for that
/// record (`awaits_self_record`); one the record never follows is the named
/// character's after `SELF_RECORD_WAIT_MS`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PetBook {
    pub collection: Option<Collection>,
    pub owner: Option<String>,
    pub awaits_self_record: bool,
    /// Moves with every change.
    pub seq: u64,
}

impl PetBook {
    /// Take in a packet. `named`: who the game has said you are, if it has.
    /// Returns whether it changed anything.
    pub fn note(&mut self, packet: PetPacket, named: Option<&str>, now_ms: i64) -> bool {
        self.settle(named, now_ms);
        match packet {
            PetPacket::Collection(c) => {
                self.collection = Some(c);
                self.awaits_self_record = true;
            }
            PetPacket::SoulGain { pet, count } => {
                let Some(c) = self.collection.as_mut() else { return false };
                let p = c.pets.entry(pet).or_default();
                p.souls = p.souls.saturating_add(count);
                p.estimated = true;
            }
        }
        self.seq += 1;
        true
    }

    /// The game said who you are (a self record): a collection waiting for
    /// it is theirs.
    pub fn note_self_record(&mut self, name: Option<&str>) {
        if self.collection.is_some() && (self.awaits_self_record || self.owner.is_none()) {
            self.owner = name.map(str::to_string);
            self.awaits_self_record = false;
            self.seq += 1;
        }
    }

    /// A collection the self record never followed: the named character's.
    pub fn settle(&mut self, named: Option<&str>, now_ms: i64) {
        let waited = self.collection.as_ref().is_some_and(|c| now_ms - c.seen_ms >= SELF_RECORD_WAIT_MS);
        if self.awaits_self_record && waited && named.is_some() {
            self.owner = named.map(str::to_string);
            self.awaits_self_record = false;
            self.seq += 1;
        }
    }

    /// Whose the collection is, once known.
    pub fn owned_by(&self) -> Option<&str> {
        self.owner.as_deref().filter(|_| !self.awaits_self_record && self.collection.is_some())
    }
}

/// A soul gain this soon after a kill of yours is that kill's.
pub const SOUL_WINDOW_MS: i64 = 3_000;
/// Kills kept to match soul gains against.
const MAX_KILLS: usize = 64;
/// A soul gain read just before the loot record of its kill (both in one
/// read of the stream, where the loot records are scanned for last) waits
/// this long for it.
const SAME_READ_MS: i64 = 250;

/// The code of a kill of yours whose species is not known (its spawn was
/// never seen): never learned, and with a kill of a known species in the
/// same window it makes the window ambiguous.
pub const UNKNOWN_CODE: u32 = 0;

/// A monster you killed (the game named you its loot owner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kill {
    entity: i32,
    /// `UNKNOWN_CODE` when the spawn was never seen.
    code: u32,
    /// The pet a soul creature's spawn named: its own soul, never learned.
    pet: Option<u32>,
    at_ms: i64,
    /// A soul gain was already put down to it.
    used: bool,
}

/// Which monster species gives which pet's soul: NPC code -> pet id, learned
/// from a soul gain right after a kill of yours (of a monster whose spawn
/// named no pet). Kept in `copilot-pet-species.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpeciesBook {
    pub species: BTreeMap<u32, u32>,
    kills: VecDeque<Kill>,
    /// The last soul gain no kill was found for: (pet, when).
    unmatched: Option<(u32, i64)>,
    /// Soul gains read in this read of the stream, put down at its end
    /// (`settle`), once every loot record in it has been seen: a soul and
    /// two loot records in one read must be matched against both.
    pending: Vec<(u32, i64)>,
    /// Moves with every change of `species`.
    pub seq: u64,
}

impl SpeciesBook {
    /// Links kept from earlier sessions; what this one learned wins.
    pub fn seed(&mut self, kept: &BTreeMap<u32, u32>) {
        let mut changed = false;
        for (code, pet) in kept {
            if PET_IDS.contains(pet) && !self.species.contains_key(code) {
                self.species.insert(*code, *pet);
                changed = true;
            }
        }
        if changed {
            self.seq += 1;
        }
    }

    /// A monster of `code` you killed (`UNKNOWN_CODE` when its spawn was
    /// never seen), with the pet its spawn named when it is a soul creature.
    /// The loot record that says so is seen again on every read of the
    /// buffer it sits in: only the first counts, so a re-read cannot stretch
    /// the window.
    pub fn note_kill(&mut self, entity: i32, code: u32, pet: Option<u32>, now_ms: i64) {
        if self.kills.iter().any(|k| k.entity == entity && now_ms - k.at_ms <= SOUL_WINDOW_MS * 4) {
            return;
        }
        self.kills.push_back(Kill { entity, code, pet, at_ms: now_ms, used: false });
        while self.kills.len() > MAX_KILLS {
            self.kills.pop_front();
        }
        if let Some((pet, at)) = self.unmatched.take().filter(|(_, at)| (0..=SAME_READ_MS).contains(&(now_ms - at))) {
            tracing::debug!("aion-copilot: pet {pet} souls {} ms before the loot record", now_ms - at);
            // Matched at the end of this read, with every kill in it.
            self.pending.push((pet, now_ms));
        }
    }

    /// Souls of `pet` gained, read now: matched at the end of the read.
    pub fn defer_soul_gain(&mut self, pet: u32, now_ms: i64) {
        self.pending.push((pet, now_ms));
    }

    /// The end of a read: the soul gains read in it are matched against the
    /// kills, all of this read's now known. Returns the codes learned.
    pub fn settle(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending).into_iter().filter_map(|(pet, at)| self.note_soul_gain(pet, at)).collect()
    }

    /// Souls of `pet` gained: the kill of yours in the last
    /// `SOUL_WINDOW_MS` gave them, when every such kill is of one species.
    /// A soul creature of that very pet killed in the window gave them: it
    /// takes them and nothing is learned. Two species in the window (a soul
    /// creature of another pet, or a kill whose species is unknown, counting
    /// as one), or none, teach nothing; nor does a second soul gain for the
    /// same kill. A species already linked to another pet keeps that link.
    /// Returns the code learned.
    pub fn note_soul_gain(&mut self, pet: u32, now_ms: i64) -> Option<u32> {
        let recent = |k: &Kill| !k.used && (0..=SOUL_WINDOW_MS).contains(&(now_ms - k.at_ms));
        if let Some(k) = self.kills.iter_mut().find(|k| recent(k) && k.pet == Some(pet)) {
            tracing::debug!("aion-copilot: pet {pet} souls from soul creature {} ({}): not learned", k.entity, k.code);
            k.used = true;
            self.unmatched = None;
            return None;
        }
        // A soul creature's code carries several pets: never learned.
        let mut codes: Vec<u32> =
            self.kills.iter().filter(|k| recent(k)).map(|k| if k.pet.is_some() { UNKNOWN_CODE } else { k.code }).collect();
        codes.sort_unstable();
        codes.dedup();
        self.unmatched = codes.is_empty().then_some((pet, now_ms));
        let [code] = codes[..] else {
            if codes.len() > 1 {
                tracing::debug!("aion-copilot: pet {pet} souls after kills of {codes:?}: not learned (ambiguous)");
            }
            return None;
        };
        if code == UNKNOWN_CODE {
            tracing::debug!("aion-copilot: pet {pet} souls after a kill of an unknown species: not learned");
            return None;
        }
        for k in self.kills.iter_mut().filter(|k| recent(k)) {
            k.used = true;
        }
        match self.species.get(&code) {
            Some(&known) if known == pet => None,
            Some(&known) => {
                tracing::debug!("aion-copilot: {code} gave pet {pet} souls, kept as {known}");
                None
            }
            None => {
                tracing::info!("aion-copilot: monster {code} gives pet {pet} souls (learned)");
                self.species.insert(code, pet);
                self.seq += 1;
                Some(code)
            }
        }
    }

    /// The pet a species gives, once learned.
    pub fn pet_of(&self, code: u32) -> Option<u32> {
        self.species.get(&code).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn login() -> Collection {
        match parse(&hex(include_str!("testdata/pet_collection_v1.hex")), 5) {
            Some(PetPacket::Collection(c)) => c,
            r => panic!("{r:?}"),
        }
    }

    #[test]
    fn the_login_collection_has_every_level_and_soul_count_the_game_showed() {
        let c = login();
        assert_eq!((c.active, c.seen_ms), (1066, 5));
        let leveled: Vec<u32> = c.pets.values().filter(|p| p.level > 0).map(|p| p.level).collect();
        assert_eq!(leveled.len(), 115);
        let at = |l: u32| leveled.iter().filter(|&&x| x == l).count();
        assert_eq!((at(1), at(2), at(3)), (86, 14, 15));
        // 98 soul entries, 16 of them at 0.
        assert_eq!(c.pets.values().filter(|p| p.souls > 0).count(), 82);
        assert_eq!(c.pets[&1003].level, 2);
        assert_eq!(c.pets[&1013].souls, 37);
        // Souls but no level: not owned yet.
        assert_eq!(c.pets[&1007], Pet { level: 0, souls: 1, estimated: false });
    }

    #[test]
    fn a_collection_with_a_mismatched_id_or_level_is_not_read() {
        // active 1066, one pet 1003/1004 level 2, no souls.
        assert_eq!(parse(&hex("1a00902a04000001eb030000ec0300000200000000"), 0), None);
        // Level 9.
        assert_eq!(parse(&hex("1a00902a04000001eb030000eb0300000900000000"), 0), None);
        // Cut before the soul count.
        assert_eq!(parse(&hex("1a00902a04000001eb030000eb03000002000000"), 0), None);
        let Some(PetPacket::Collection(c)) = parse(&hex("1a00902a04000001eb030000eb0300000200000001ed0300000500000077"), 0) else {
            panic!()
        };
        assert_eq!((c.pets[&1003].level, c.pets[&1005].souls), (2, 5));
    }

    #[test]
    fn a_soul_gain_reads_its_pet_and_count() {
        assert_eq!(parse(&hex("0f0d90010504000001000000"), 0), Some(PetPacket::SoulGain { pet: 1029, count: 1 }));
        assert_eq!(parse(&hex("0f0d90020504000001000000"), 0), None);
        assert_eq!(parse(&hex("0f0d900105040000010000"), 0), None);
        // 0c 90 is something else.
        assert_eq!(parse(&hex("0e0c90fe410000bd040000"), 0), None);
    }

    #[test]
    fn a_soul_gain_adds_to_the_collection_and_marks_it_an_estimate() {
        let mut book = PetBook::default();
        // Nothing to add to yet.
        assert!(!book.note(PetPacket::SoulGain { pet: 1013, count: 2 }, Some("Main"), 0));
        book.note(PetPacket::Collection(login()), Some("Main"), 0);
        book.note(PetPacket::SoulGain { pet: 1013, count: 2 }, Some("Main"), 1_000);
        let c = book.collection.as_ref().unwrap();
        assert_eq!(c.pets[&1013], Pet { level: c.pets[&1013].level, souls: 39, estimated: true });
        // A pet not listed yet starts at level 0.
        book.note(PetPacket::SoulGain { pet: 1399, count: 1 }, Some("Main"), 1_000);
        assert_eq!(book.collection.as_ref().unwrap().pets[&1399], Pet { level: 0, souls: 1, estimated: true });
        // The next collection replaces the estimates.
        book.note(PetPacket::Collection(login()), Some("Main"), 2_000);
        assert!(!book.collection.as_ref().unwrap().pets[&1013].estimated);
    }

    #[test]
    fn the_collection_waits_for_the_self_record_to_know_whose_it_is() {
        let mut book = PetBook::default();
        book.note(PetPacket::Collection(login()), Some("Main"), 1_000);
        assert_eq!(book.owned_by(), None, "the self record comes a moment later");
        book.note_self_record(Some("Alt"));
        assert_eq!(book.owned_by(), Some("Alt"));
        // A self record later (a zone load) does not change it.
        book.note_self_record(Some("Other"));
        assert_eq!(book.owned_by(), Some("Alt"));
        // One the record never followed: the named character's, after a while.
        let mut book = PetBook::default();
        book.note(PetPacket::Collection(login()), Some("Main"), 1_000);
        book.settle(Some("Main"), 5_000);
        assert_eq!(book.owned_by(), None);
        book.settle(Some("Main"), 1_000 + SELF_RECORD_WAIT_MS);
        assert_eq!(book.owned_by(), Some("Main"));
    }

    #[test]
    fn a_wild_pet_creature_carries_its_pet_id_after_its_place() {
        use crate::capture::world_position::{parse as world, WorldRecord};
        let r = world(&hex(include_str!("testdata/world_npc_spawn_pet_v1.hex")));
        let Some(WorldRecord::Npc { entity, kind, code, pet, .. }) = r else { panic!("{r:?}") };
        assert_eq!((entity, kind, code, pet), (28734, 0x0C, 2_930_392, Some(1291)));
        // Another kind-0x0c spawn has ff bytes there: no pet.
        let r = world(&hex(include_str!("testdata/world_npc_spawn_pet_none.hex")));
        let Some(WorldRecord::Npc { kind, code, pet, .. }) = r else { panic!("{r:?}") };
        assert_eq!((kind, code, pet), (0x0C, 2_931_240, None));
    }

    #[test]
    fn the_pet_pattern_must_be_there_once_with_a_pet_id() {
        let one = hex("0003000101000a0000000b0500000100000000e6");
        assert_eq!(spawn_pet(&one), Some(1291));
        // Out of range.
        assert_eq!(spawn_pet(&hex("0a000000ffff00000100000000")), None);
        // Twice, different: not trusted.
        assert_eq!(spawn_pet(&hex("0a0000000b050000010000000a0000000c05000001000000")), None);
        // Not closed by 01 00 00 00.
        assert_eq!(spawn_pet(&hex("0a0000000b05000002000000")), None);
        assert_eq!(spawn_pet(&[]), None);
    }

    fn learned(book: &SpeciesBook) -> Vec<(u32, u32)> {
        book.species.iter().map(|(c, p)| (*c, *p)).collect()
    }

    #[test]
    fn a_soul_gain_right_after_your_kill_links_the_species_to_the_pet() {
        // 2026-10-18 16:59:46: the loot record of entity 40539, then 1048 +1
        // 544 ms later.
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        assert_eq!(book.note_soul_gain(1048, 1_544), Some(2_100_555));
        assert_eq!(learned(&book), [(2_100_555, 1048)]);
        assert_eq!((book.pet_of(2_100_555), book.seq), (Some(1048), 1));
        // Two of one species in the window are still one species.
        let mut book = SpeciesBook::default();
        book.note_kill(1, 2_100_813, None, 0);
        book.note_kill(2, 2_100_813, None, 2_000);
        assert_eq!(book.note_soul_gain(1100, 2_500), Some(2_100_813));
    }

    #[test]
    fn a_soul_gain_with_no_kill_of_yours_just_before_teaches_nothing() {
        let mut book = SpeciesBook::default();
        assert_eq!(book.note_soul_gain(1048, 10_000), None);
        // Too long after.
        book.note_kill(40539, 2_100_555, None, 20_000);
        assert_eq!(book.note_soul_gain(1048, 20_000 + SOUL_WINDOW_MS + 1), None);
        // Before the kill (by more than one read of the stream).
        book.note_kill(40540, 2_100_555, None, 30_000 + SAME_READ_MS + 1);
        assert!(book.species.is_empty() && book.seq == 0);
    }

    #[test]
    fn kills_of_two_species_in_the_window_teach_nothing() {
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        book.note_kill(40540, 2_100_813, None, 1_200);
        assert_eq!(book.note_soul_gain(1048, 1_544), None);
        assert!(book.species.is_empty());
    }

    #[test]
    fn one_kill_teaches_once_and_a_reread_does_not_move_it() {
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        // The same loot record seen again on a later read of its buffer.
        book.note_kill(40539, 2_100_555, None, 3_500);
        assert_eq!(book.note_soul_gain(1048, 4_100), None, "the kill is from 1000, not 3500");
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        assert_eq!(book.note_soul_gain(1048, 1_500), Some(2_100_555));
        // A second pet's souls for the same kill: not put down to it.
        assert_eq!(book.note_soul_gain(1063, 1_600), None);
        assert_eq!(learned(&book), [(2_100_555, 1048)]);
    }

    #[test]
    fn a_soul_gain_read_just_before_its_loot_record_still_counts() {
        // Both in one read: the loot records are scanned after the packets.
        let mut book = SpeciesBook::default();
        assert_eq!(book.note_soul_gain(1048, 5_000), None);
        book.note_kill(40539, 2_100_555, None, 5_000);
        book.settle();
        assert_eq!(learned(&book), [(2_100_555, 1048)]);
    }

    #[test]
    fn souls_read_with_loot_records_are_matched_once_the_read_is_over() {
        // A regular kill in an earlier read; then, in one read, a soul
        // creature's souls and its loot record (scanned last): its own.
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        assert!(book.settle().is_empty());
        book.defer_soul_gain(1291, 2_000);
        book.note_kill(28734, 2_930_392, Some(1291), 2_000);
        assert!(book.settle().is_empty());
        assert!(book.species.is_empty());
        // The regular kill's own souls, later: learned.
        book.defer_soul_gain(1048, 2_400);
        assert_eq!(book.settle(), vec![2_100_555]);
        // One read with the souls and two loot records, the regular one first.
        let mut book = SpeciesBook::default();
        book.defer_soul_gain(1291, 5_000);
        book.note_kill(40539, 2_100_555, None, 5_000);
        book.note_kill(28734, 2_930_392, Some(1291), 5_000);
        assert!(book.settle().is_empty());
        assert!(book.species.is_empty());
    }

    #[test]
    fn a_learned_link_is_kept_and_an_earlier_session_fills_in() {
        let mut book = SpeciesBook::default();
        book.note_kill(1, 2_100_555, None, 0);
        book.note_soul_gain(1048, 100);
        // Another pet for the same species later: the first link stays.
        book.note_kill(2, 2_100_555, None, 10_000);
        assert_eq!(book.note_soul_gain(1050, 10_100), None);
        assert_eq!(book.pet_of(2_100_555), Some(1048));
        // What an earlier session kept fills in, and does not replace (nor
        // does a pet id out of range get in).
        let kept: BTreeMap<u32, u32> = [(2_100_555, 1099), (2_100_812, 1077), (5, 9)].into_iter().collect();
        book.seed(&kept);
        assert_eq!(learned(&book), [(2_100_555, 1048), (2_100_812, 1077)]);
    }

    #[test]
    fn a_soul_creatures_own_soul_is_not_put_down_to_a_regular_kill_beside_it() {
        // A regular kill and a soul creature of pet 1291 in one window: its
        // soul is the soul creature's, and the regular kill keeps its own.
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        book.note_kill(28734, 2_930_392, Some(1291), 1_500);
        assert_eq!(book.note_soul_gain(1291, 2_000), None);
        assert!(book.species.is_empty());
        assert_eq!(book.note_soul_gain(1048, 2_100), Some(2_100_555));
        assert_eq!(learned(&book), [(2_100_555, 1048)]);
        // The regular kill's soul first: two species in the window.
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        book.note_kill(28734, 2_930_392, Some(1291), 1_500);
        assert_eq!(book.note_soul_gain(1048, 2_000), None);
        assert_eq!(book.note_soul_gain(1291, 2_100), None);
        assert!(book.species.is_empty());
        // A soul creature alone and another pet's souls: not learned either.
        let mut book = SpeciesBook::default();
        book.note_kill(28734, 2_930_392, Some(1291), 1_000);
        assert_eq!(book.note_soul_gain(1048, 1_500), None);
        assert!(book.species.is_empty() && book.seq == 0);
    }

    #[test]
    fn a_soul_creatures_soul_read_before_its_loot_record_is_not_learned() {
        // The souls, then (same read) the soul creature's loot record, then a
        // regular kill's: the soul creature takes them.
        let mut book = SpeciesBook::default();
        assert_eq!(book.note_soul_gain(1291, 5_000), None);
        book.note_kill(28734, 2_930_392, Some(1291), 5_000);
        book.note_kill(40539, 2_100_555, None, 5_010);
        assert!(book.settle().is_empty());
        assert!(book.species.is_empty());
        // The regular kill's souls then come on their own: learned.
        assert_eq!(book.note_soul_gain(1048, 5_500), Some(2_100_555));
    }

    #[test]
    fn a_kill_whose_species_is_unknown_makes_the_window_ambiguous() {
        // The one kill of a known species and one whose spawn was never seen.
        let mut book = SpeciesBook::default();
        book.note_kill(40539, 2_100_555, None, 1_000);
        book.note_kill(51315, UNKNOWN_CODE, None, 1_200);
        assert_eq!(book.note_soul_gain(1048, 1_544), None);
        assert!(book.species.is_empty());
        // Alone: nothing learned, no code 0 kept.
        let mut book = SpeciesBook::default();
        book.note_kill(51315, UNKNOWN_CODE, None, 1_200);
        assert_eq!(book.note_soul_gain(1048, 1_544), None);
        // Through the catch-up path too.
        assert_eq!(book.note_soul_gain(1050, 9_000), None);
        book.note_kill(51316, UNKNOWN_CODE, None, 9_000);
        book.note_kill(40540, 2_100_555, None, 9_010);
        assert!(book.species.is_empty() && book.seq == 0);
    }
}

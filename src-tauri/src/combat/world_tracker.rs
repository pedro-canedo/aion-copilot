//! aion-copilot: what is around you and where you are, for the map, from the
//! records `capture::world_position` reads.
//!
//! Your own place is exact only for a moment after a record that carries it
//! (an abnormal you cast, a hit someone lands on you, a push); in between it
//! is estimated from what the server is updating around you: it only sends
//! what is within view (spawns come in a median 40 m from you), so the
//! median of the places it sent in the last 2 s lands a median 19 m from
//! you (p90 33 m) in a live Abyss capture, the 20-30 m the Shugo app quotes.
//!
//! Spawns of monsters and objects, and objects used up, are also kept as
//! sightings for the map's store of learned places (`copilot_map`), which
//! takes them with `take_sightings`.
//!
//! A zone load (`21 36`) clears it all: what was in view stays behind, and
//! the new zone hands out new entity ids, so a move for a reused id would
//! otherwise drag an old entry (a monolith, say) along with a player.

use std::collections::{HashMap, VecDeque};

use crate::capture::world_position::{self, Pos, WorldRecord};

/// A fix this fresh is where you are.
pub const EXACT_MS: i64 = 5_000;
/// The places sent this recently count for the estimate...
pub const NEIGHBOUR_WINDOW_MS: i64 = 2_000;
/// ...when there are this many.
pub const NEIGHBOURS_NEEDED: usize = 3;
/// With too few around, a fix this fresh still says roughly where you are.
pub const STALE_FIX_MS: i64 = 15_000;
/// Your facing from your own casts, while this fresh.
const HEADING_MS: i64 = 10_000;
/// No record says a player left: one not heard from in this long is gone.
const PLAYER_GONE_MS: i64 = 60_000;
/// NPCs and objects leave with a record; these only clear what was missed.
const NPC_GONE_MS: i64 = 600_000;
const OBJECT_GONE_MS: i64 = 1_800_000;
/// Farther than anything in view (about 50 m) from a new fix: left behind.
const LEFT_BEHIND: f32 = 30_000.0;
const PRUNE_EVERY_MS: i64 = 5_000;
const MAX_LIVE: usize = 3_000;
const MAX_SIGHTINGS: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Npc,
    Player,
    Object,
    /// Moving, but its spawn came before the capture did.
    Unknown,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Npc => "npc",
            EntityKind::Player => "player",
            EntityKind::Object => "object",
            EntityKind::Unknown => "unknown",
        }
    }
}

/// Something in view.
#[derive(Debug, Clone, PartialEq)]
pub struct Live {
    pub kind: EntityKind,
    /// The NPC or object code; 0 for a player.
    pub code: u32,
    /// An NPC that is a monster (not a summon or an effect).
    pub monster: bool,
    /// A player's name.
    pub name: Option<String>,
    /// The pet whose soul a wild creature carries (`capture::pets`).
    pub pet: Option<u32>,
    /// An object's variant: which resource a gathering point is
    /// (`ObjectSpawn::variant`).
    pub variant: Option<u32>,
    pub pos: Pos,
    pub heading: Option<f32>,
    pub last_ms: i64,
}

/// Which kind of record placed you.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixSource {
    /// An abnormal you cast (`2a 38`, `2b 38`).
    Abnormal,
    /// A cast or hit on you (`02 38`, `03 38`, `1c 38`).
    Hit,
    /// A push or pull (`15 38`, `16 38`).
    Displaced,
}

impl FixSource {
    pub fn as_str(self) -> &'static str {
        match self {
            FixSource::Abnormal => "abnormal",
            FixSource::Hit => "hit",
            FixSource::Displaced => "displaced",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub pos: Pos,
    pub at_ms: i64,
    pub source: FixSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accuracy {
    Exact,
    Estimated,
    /// Only an old fix: where you were.
    Unknown,
}

impl Accuracy {
    pub fn as_str(self) -> &'static str {
        match self {
            Accuracy::Exact => "exact",
            Accuracy::Estimated => "estimated",
            Accuracy::Unknown => "unknown",
        }
    }
}

/// Where you are, and how sure.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnPosition {
    pub pos: Pos,
    pub accuracy: Accuracy,
    /// Since the newest record it comes from.
    pub age_ms: i64,
    /// For an estimate from what is around: the median distance of those
    /// places from it (game units).
    pub radius: Option<f32>,
    /// Degrees, from your own casts.
    pub heading: Option<f32>,
    /// The fix it comes from, when it does.
    pub source: Option<FixSource>,
}

/// A monster or object spawn, or an object used up, for the learned store.
#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub at_ms: i64,
    pub kind: EntityKind,
    pub code: u32,
    pub pos: Pos,
    pub used_up: bool,
    /// An object's spawn time (unix ms), from the server.
    pub spawned_ms: Option<i64>,
    /// An object's variant (`ObjectSpawn::variant`).
    pub variant: Option<u32>,
}

#[derive(Debug, Default)]
pub struct WorldTracker {
    live: HashMap<i32, Live>,
    fix: Option<Fix>,
    heading: Option<(f32, i64)>,
    sightings: VecDeque<Sighting>,
    pruned_ms: i64,
    seq: u64,
    /// Zone loads seen, and when the last one was.
    loads: u64,
    loaded_ms: Option<i64>,
}

impl WorldTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves with every change, for `copilot_map_state(since)`.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn live(&self) -> &HashMap<i32, Live> {
        &self.live
    }

    pub fn last_fix(&self) -> Option<Fix> {
        self.fix
    }

    /// What is in view at `now`: `live` without what has not been heard from
    /// in too long, which only a record arriving prunes (none does with the
    /// game closed or behind a loading screen).
    pub fn in_view(&self, now: i64) -> impl Iterator<Item = (&i32, &Live)> {
        self.live.iter().filter(move |(_, l)| now - l.last_ms <= keep_ms(l.kind))
    }

    /// A zone load: forget what was in view, where you were and the
    /// sightings not taken yet.
    pub fn note_load(&mut self, now: i64) {
        self.live.clear();
        self.fix = None;
        self.heading = None;
        self.sightings.clear();
        self.loads += 1;
        self.loaded_ms = Some(now);
        self.seq += 1;
    }

    /// Zone loads seen, and when the last one was.
    pub fn loads(&self) -> (u64, Option<i64>) {
        (self.loads, self.loaded_ms)
    }

    /// Take a record in; `local` is your entity id when known.
    pub fn note(&mut self, record: &WorldRecord, local: Option<i32>, now: i64) {
        let mut changed = false;
        if let Some(me) = local {
            if let Some(pos) = world_position::own_position(record, me) {
                let source = match record {
                    WorldRecord::CasterAt { .. } => FixSource::Abnormal,
                    WorldRecord::Displaced { .. } => FixSource::Displaced,
                    _ => FixSource::Hit,
                };
                self.live.retain(|_, l| dist2(l.pos, pos) <= LEFT_BEHIND);
                self.fix = Some(Fix { pos, at_ms: now, source });
                changed = true;
            }
            if let WorldRecord::Cast { source, facing: Some(facing), .. } = *record {
                if source == me {
                    self.heading = Some((facing, now));
                }
            }
        }
        if let Some((entity, pos)) = world_position::other_position(record, local) {
            if let Some(l) = self.live.get_mut(&entity) {
                l.pos = pos;
                l.last_ms = now;
                changed = true;
            }
        }
        changed |= match record {
            WorldRecord::Npc { entity, kind, code, pos, heading, pet, .. } => {
                let monster = world_position::is_monster_kind(*kind);
                if monster {
                    self.sight(Sighting {
                        at_ms: now,
                        kind: EntityKind::Npc,
                        code: *code,
                        pos: *pos,
                        used_up: false,
                        spawned_ms: None,
                        variant: None,
                    });
                }
                self.live.insert(*entity, Live {
                    kind: EntityKind::Npc,
                    code: *code,
                    monster,
                    name: None,
                    pet: *pet,
                    variant: None,
                    pos: *pos,
                    heading: Some(*heading),
                    last_ms: now,
                });
                true
            }
            WorldRecord::Player { entity, name, pos, heading, .. } => {
                if Some(*entity) != local {
                    self.live.insert(*entity, Live {
                        kind: EntityKind::Player,
                        code: 0,
                        monster: false,
                        name: Some(name.clone()),
                        pet: None,
                        variant: None,
                        pos: *pos,
                        heading: Some(*heading),
                        last_ms: now,
                    });
                }
                true
            }
            WorldRecord::Object(o) => {
                self.sight(Sighting {
                    at_ms: now,
                    kind: EntityKind::Object,
                    code: o.code,
                    pos: o.pos,
                    used_up: false,
                    spawned_ms: o.spawned_ms,
                    variant: o.variant,
                });
                self.live.insert(o.entity, Live {
                    kind: EntityKind::Object,
                    code: o.code,
                    monster: false,
                    name: None,
                    pet: None,
                    variant: o.variant,
                    pos: o.pos,
                    heading: Some(o.yaw),
                    last_ms: now,
                });
                true
            }
            WorldRecord::ObjectGone { entity, used_up } => match self.live.remove(entity) {
                Some(l) => {
                    if *used_up {
                        self.sight(Sighting {
                            at_ms: now,
                            kind: EntityKind::Object,
                            code: l.code,
                            pos: l.pos,
                            used_up: true,
                            spawned_ms: None,
                            variant: l.variant,
                        });
                    }
                    true
                }
                None => false,
            },
            WorldRecord::NpcGone { entity, .. } => self.live.remove(entity).is_some(),
            WorldRecord::Moved { entity, pos, heading } if Some(*entity) != local => {
                let l = self.live.entry(*entity).or_insert_with(|| Live {
                    kind: EntityKind::Unknown,
                    code: 0,
                    monster: false,
                    name: None,
                    pet: None,
                    variant: None,
                    pos: *pos,
                    heading: None,
                    last_ms: now,
                });
                l.pos = *pos;
                l.heading = heading.or(l.heading);
                l.last_ms = now;
                true
            }
            _ => false,
        };
        changed |= self.prune(now);
        if changed {
            self.seq += 1;
        }
    }

    fn sight(&mut self, s: Sighting) {
        self.sightings.push_back(s);
        while self.sightings.len() > MAX_SIGHTINGS {
            self.sightings.pop_front();
        }
    }

    /// The sightings since the last call, oldest first.
    pub fn take_sightings(&mut self) -> Vec<Sighting> {
        self.sightings.drain(..).collect()
    }

    /// Forget what has not been heard from in too long; true when anything went.
    fn prune(&mut self, now: i64) -> bool {
        if now >= self.pruned_ms && now - self.pruned_ms < PRUNE_EVERY_MS {
            return false;
        }
        self.pruned_ms = now;
        let before = self.live.len();
        self.live.retain(|_, l| now - l.last_ms <= keep_ms(l.kind));
        if self.live.len() > MAX_LIVE {
            let mut ages: Vec<i64> = self.live.values().map(|l| l.last_ms).collect();
            ages.sort_unstable();
            let cut = ages[self.live.len() - MAX_LIVE];
            self.live.retain(|_, l| l.last_ms >= cut);
        }
        self.live.len() != before
    }

    /// Where you are at `now`: see the module's notes. None before anything
    /// placed you.
    pub fn own_position(&self, now: i64) -> Option<OwnPosition> {
        let heading = self.heading.filter(|(_, at)| now - at <= HEADING_MS).map(|(h, _)| h);
        if let Some(fix) = self.fix.filter(|f| now - f.at_ms <= EXACT_MS) {
            return Some(OwnPosition {
                pos: fix.pos,
                accuracy: Accuracy::Exact,
                age_ms: now - fix.at_ms,
                radius: None,
                heading,
                source: Some(fix.source),
            });
        }
        let around: Vec<(Pos, i64)> = self
            .live
            .values()
            .filter(|l| now - l.last_ms <= NEIGHBOUR_WINDOW_MS && l.last_ms <= now)
            .map(|l| (l.pos, l.last_ms))
            .collect();
        if around.len() >= NEIGHBOURS_NEEDED {
            let places: Vec<Pos> = around.iter().map(|(p, _)| *p).collect();
            let pos = median_place(&places);
            let mut spread: Vec<f32> = places.iter().map(|p| dist2(*p, pos)).collect();
            let radius = median(&mut spread);
            let newest = around.iter().map(|(_, at)| *at).max().unwrap_or(now);
            return Some(OwnPosition {
                pos,
                accuracy: Accuracy::Estimated,
                age_ms: now - newest,
                radius: Some(radius),
                heading,
                source: None,
            });
        }
        let fix = self.fix?;
        let age_ms = now - fix.at_ms;
        Some(OwnPosition {
            pos: fix.pos,
            accuracy: if age_ms <= STALE_FIX_MS { Accuracy::Estimated } else { Accuracy::Unknown },
            age_ms,
            radius: None,
            heading,
            source: Some(fix.source),
        })
    }
}

/// How long something not heard from stays in view.
fn keep_ms(kind: EntityKind) -> i64 {
    match kind {
        EntityKind::Player | EntityKind::Unknown => PLAYER_GONE_MS,
        EntityKind::Npc => NPC_GONE_MS,
        EntityKind::Object => OBJECT_GONE_MS,
    }
}

/// Horizontal distance.
pub fn dist2(a: Pos, b: Pos) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// The median of each coordinate.
pub fn median_place(places: &[Pos]) -> Pos {
    let mut out = [0.0; 3];
    for (axis, slot) in out.iter_mut().enumerate() {
        let mut v: Vec<f32> = places.iter().map(|p| p[axis]).collect();
        *slot = median(&mut v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::world_position::{parse, ObjectSpawn};

    const ME: i32 = 14667;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn npc(entity: i32, pos: Pos) -> WorldRecord {
        WorldRecord::Npc { entity, kind: 0x04, code: 2_600_011, owner: None, pos, heading: 0.0, pet: None }
    }

    fn object(entity: i32, code: u32, pos: Pos) -> WorldRecord {
        WorldRecord::Object(ObjectSpawn {
            entity,
            mask: 4,
            code,
            pos,
            yaw: 0.0,
            state: [1, 1, 1],
            spawned_ms: Some(1_000),
            gathered: Some(0),
            variant: None,
        })
    }

    #[test]
    fn a_fresh_fix_is_exact_then_what_is_around_takes_over() {
        let mut t = WorldTracker::new();
        // Your buff, from the capture: (31178, -22981, 14962).
        let buff = parse(&hex("302a38cb720111c305caea2101b80b000000000000a2491e4ea1010000cb7201001494f346478ab3c684c66946")).unwrap();
        t.note(&buff, Some(ME), 10_000);
        let p = t.own_position(12_000).unwrap();
        assert_eq!((p.accuracy, p.age_ms, p.source), (Accuracy::Exact, 2_000, Some(FixSource::Abnormal)));
        assert!((p.pos[0] - 31178.04).abs() < 0.1);
        // Seven seconds on, three monsters come into view around a new place.
        for (i, (x, y)) in [(32_000.0, -22_000.0), (33_000.0, -21_000.0), (34_000.0, -23_000.0)].into_iter().enumerate() {
            t.note(&npc(100 + i as i32, [x, y, 15_000.0]), Some(ME), 16_500);
        }
        let p = t.own_position(17_000).unwrap();
        assert_eq!((p.accuracy, p.age_ms, p.source), (Accuracy::Estimated, 500, None));
        assert_eq!(p.pos, [33_000.0, -22_000.0, 15_000.0]);
        assert!((p.radius.unwrap() - 1_000.0).abs() < 1.0);
        // Nothing new around: the old fix says roughly where, then only where you were.
        let p = t.own_position(20_000).unwrap();
        assert_eq!((p.accuracy, p.age_ms), (Accuracy::Estimated, 10_000));
        assert_eq!(t.own_position(40_000).unwrap().accuracy, Accuracy::Unknown);
    }

    #[test]
    fn nothing_placed_yet_is_no_position_and_two_neighbours_are_not_enough() {
        let mut t = WorldTracker::new();
        assert_eq!(t.own_position(0), None);
        t.note(&npc(1, [0.0, 0.0, 0.0]), Some(ME), 1_000);
        t.note(&npc(2, [100.0, 0.0, 0.0]), Some(ME), 1_000);
        assert_eq!(t.own_position(1_500), None);
    }

    #[test]
    fn a_hit_on_you_places_you_but_your_own_cast_places_your_target() {
        let mut t = WorldTracker::new();
        // Danïka (f171) in view, then your cast on her.
        t.note(&WorldRecord::Moved { entity: 14577, pos: [0.0; 3], heading: None }, Some(ME), 1_000);
        let cast = parse(&hex("270238cb72003ad5ea005402f171ac0d43c0b9daef464a39d3c6ecdc55469e550296aa03")).unwrap();
        t.note(&cast, Some(ME), 1_100);
        assert_eq!(t.last_fix(), None);
        assert!((t.live()[&14577].pos[0] - 30701.36).abs() < 0.1);
        // Facing from your cast, for the marker.
        let hit = parse(&hex("280338f17100cb72c88d030110a18a8e51f775e746e2c6d2c6dd775546de02010000000100")).unwrap();
        t.note(&hit, Some(ME), 1_200);
        let p = t.own_position(1_300).unwrap();
        assert_eq!((p.accuracy, p.source), (Accuracy::Exact, Some(FixSource::Hit)));
        assert!((p.pos[0] - 29626.98).abs() < 0.1 && (p.heading.unwrap() - -3.0477).abs() < 0.001);
        // Without knowing who you are, nothing places you.
        let mut t = WorldTracker::new();
        t.note(&hit, None, 1_200);
        assert_eq!(t.last_fix(), None);
    }

    #[test]
    fn a_fix_far_away_forgets_what_was_around_the_old_place() {
        let mut t = WorldTracker::new();
        t.note(&npc(1, [30_000.0, -20_000.0, 0.0]), Some(ME), 1_000);
        t.note(&npc(2, [-120_000.0, -100_000.0, 0.0]), Some(ME), 1_000);
        t.note(&WorldRecord::Displaced { entity: ME, pos: [30_500.0, -20_000.0, 0.0] }, Some(ME), 2_000);
        assert!(t.live().contains_key(&1) && !t.live().contains_key(&2));
    }

    #[test]
    fn objects_used_up_become_sightings_with_their_place_and_leave_view() {
        let mut t = WorldTracker::new();
        t.note(&object(32353, 1_590_001, [41_859.0, -21_269.0, 12_757.0]), Some(ME), 1_000);
        t.note(&parse(&hex("0c3536e1fc01040300")).unwrap(), Some(ME), 2_000);
        assert!(t.live().is_empty());
        let s = t.take_sightings();
        assert_eq!(s.len(), 2);
        assert!(!s[0].used_up && s[0].spawned_ms == Some(1_000));
        assert!(s[1].used_up && s[1].code == 1_590_001 && s[1].pos[0] == 41_859.0 && s[1].at_ms == 2_000);
        assert!(t.take_sightings().is_empty());
    }

    #[test]
    fn a_gathering_points_variant_is_kept_in_view_and_on_its_sightings() {
        let mut t = WorldTracker::new();
        let spawn = parse(&hex(include_str!("../capture/testdata/world_object_gather_1590001_a.hex").trim())).unwrap();
        t.note(&spawn, Some(ME), 1_000);
        assert_eq!(t.live()[&32353].variant, Some(0x0B08_97A4));
        t.note(&parse(&hex("0c3536e1fc01040300")).unwrap(), Some(ME), 2_000);
        let s = t.take_sightings();
        assert_eq!(s.iter().map(|s| (s.used_up, s.variant)).collect::<Vec<_>>(), [(false, Some(0x0B08_97A4)), (true, Some(0x0B08_97A4))]);
    }

    #[test]
    fn summons_are_not_learned_and_npcs_out_of_view_are_forgotten() {
        let mut t = WorldTracker::new();
        let summon = parse(&hex(include_str!("../capture/testdata/world_npc_spawn_summon.hex").trim())).unwrap();
        t.note(&summon, Some(ME), 1_000);
        assert!(t.take_sightings().is_empty());
        t.note(&npc(33627, [0.0; 3]), Some(ME), 1_000);
        let seq = t.seq();
        t.note(&parse(&hex("0b4236db86020001")).unwrap(), Some(ME), 1_100);
        assert!(!t.live().contains_key(&33627) && t.seq() > seq);
    }

    #[test]
    fn players_not_heard_from_in_a_minute_are_gone() {
        let mut t = WorldTracker::new();
        t.note(&WorldRecord::Moved { entity: 5, pos: [0.0; 3], heading: Some(90.0) }, Some(ME), 0);
        t.note(&object(6, 1_100_228, [0.0; 3]), Some(ME), 0);
        t.note(&npc(7, [0.0; 3]), Some(ME), 70_000);
        assert!(!t.live().contains_key(&5) && t.live().contains_key(&6) && t.live().contains_key(&7));
        // Your own moves are not someone else's.
        t.note(&WorldRecord::Moved { entity: ME, pos: [0.0; 3], heading: None }, Some(ME), 70_000);
        assert!(!t.live().contains_key(&ME));
    }

    #[test]
    fn what_was_not_heard_from_leaves_view_even_with_no_record_arriving() {
        let mut t = WorldTracker::new();
        t.note(&WorldRecord::Moved { entity: 5, pos: [0.0; 3], heading: None }, Some(ME), 0);
        t.note(&npc(7, [0.0; 3]), Some(ME), 0);
        t.note(&object(6, 1_100_228, [0.0; 3]), Some(ME), 0);
        let ids = |now: i64| {
            let mut v: Vec<i32> = t.in_view(now).map(|(id, _)| *id).collect();
            v.sort_unstable();
            v
        };
        assert_eq!(ids(60_000), [5, 6, 7]);
        // Nothing pruned them (no record came), but a player is gone in a
        // minute, an NPC in 10 and an object in 30.
        assert_eq!(ids(60_001), [6, 7]);
        assert_eq!(ids(600_001), [6]);
        assert_eq!(ids(1_800_001), Vec::<i32>::new());
        assert_eq!(t.live().len(), 3);
    }

    #[test]
    fn a_zone_load_forgets_what_was_in_view_and_where_you_were() {
        let mut t = WorldTracker::new();
        t.note(&object(6, 1_100_228, [0.0; 3]), Some(ME), 1_000);
        t.note(&WorldRecord::Displaced { entity: ME, pos: [100.0, 0.0, 0.0] }, Some(ME), 1_000);
        let seq = t.seq();
        t.note_load(2_000);
        assert!(t.live().is_empty() && t.last_fix().is_none() && t.own_position(2_000).is_none());
        assert!(t.take_sightings().is_empty(), "the old zone's sightings are not the new one's");
        assert_eq!(t.loads(), (1, Some(2_000)));
        assert!(t.seq() > seq);
        // The id handed out again moves something new, not the old monolith.
        t.note(&WorldRecord::Moved { entity: 6, pos: [5.0, 5.0, 0.0], heading: None }, Some(ME), 3_000);
        assert_eq!((t.live()[&6].kind, t.live()[&6].code), (EntityKind::Unknown, 0));
    }

    #[test]
    fn the_median_place_takes_each_coordinate_on_its_own() {
        let places = [[1.0, 50.0, 7.0], [3.0, 10.0, 8.0], [2.0, 30.0, 9.0], [100.0, 20.0, 10.0]];
        assert_eq!(median_place(&places), [2.5, 25.0, 8.5]);
    }
}

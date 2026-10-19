//! aion-copilot: where things are, for the map. The records that carry a
//! world position (x, y, z f32 in game units, about a centimetre; x and y
//! horizontal, z up), read from a live Abyss capture (2026-10-18):
//!
//! ```text
//! <len> 41 36 <entity> <mask u16> <gate u8> [gate & 1: <len u8> <owner name>]
//!             <npc code u32> <u8> 02 <x y z f32> <heading f32> ...                  an NPC comes into view
//! <len> 45 36 <entity> <mask1 u32> <mask2 u8> <len u8> <name> <level u32> <u8> <u8>
//!             [mask2 & 0x10: <u8>] <flags u8> <sub-mask u8, bits 0x12> <x y z f32> <heading f32> ...   a player
//! <len> 34 36 <entity> <mask u16> <object code u32> <x y z f32> <0 f32> <yaw f32> <state 3 bytes>
//!             <spawned u64, unix ms> [mask & 1: 4 bytes] <gathered u64> ...           a world object
//!             (mask 0x0004, 46 bytes after the code: ... <gathered u64> <variant u32> <3 bytes>)
//! <len> 35 36 <entity> 01 01 00 (left view) | 04 03 00 (used up)                      an object goes
//! <len> 42 36 <entity> 00 <reason u8: 01 left view, 03 and 07 dead>                   an NPC goes
//! <len> 1c 37 | 1b 37 <entity> <kind u8> [kind & 1: <u8>] <x y z f32> <yaw u16> ...    moves
//! <len> 1a 37 <entity> <u8> <u8> <x y z f32> <yaw u16>
//! <len> 29 37 | 28 37 <entity> <u8> <u8> <x y z f32> ...
//! <len> 2a 38 | 2b 38 ... <caster> ... <x y z f32>                                    an abnormal lands: the CASTER's place
//! <len> 02 38 <source> <u8> <skill u32> <u8> <flags u8> <target> <facing f32>
//!             [flags & 2: <x y z f32>] <4-8 bytes>                                    a cast: the TARGET's place
//! <len> 03 38 <source> <u8> <target> <skill u32> <u8> <u32> <x y z f32> <varint> <6 bytes>   a hit: the target's
//! <len> 1c 38 <source> <u8> <skill u32> <u8> <u8> <target> <facing f32> <x y z f32> <5 bytes> the target's
//! <len> 15 38 <entity> <x y z f32> <2 bytes>                                          pushed or pulled to here
//! <len> 16 38 <entity> <by entity> <u32> <x y z f32> <3 bytes>                        moved by another's skill
//! ```
//!
//! The server never sends your own walking (it is the client's to send, and
//! encrypted): in 488 s of play, your id was in none of the 5,000 moves. So
//! where you are comes only from what other records carry, and the two
//! families point opposite ways:
//!
//! - an abnormal (`2a 38`, `2b 38`) carries where its caster stood, whoever
//!   it landed on: one you cast is your position (369 in that capture,
//!   flight and buffs among them);
//! - a cast or hit (`02 38` with flags & 2, `03 38`, `1c 38`) carries where
//!   its target stood: one aimed at you by someone else is your position
//!   (a median 2 cm from the abnormal fixes 500 ms around it). Your own cast
//!   carries your target's place, not yours (a median 57 cm from the
//!   target's last move, 648 cm from you); a cast of yours on yourself
//!   sometimes carries a small offset such as (94, -33, 0), no place at all.
//!
//! A `41 36` code is an NPC table id; the `34 36` codes are another table
//! (1100228 and 1100229 land on the Empyrean Trace monoliths of the tc-imba
//! map, a median 1.5 and 3.6 px off). 41 36 kinds 0x04, 0x05, 0x0c and 0x0d
//! are monsters, 0x5f a summon, 0x1c-0x1f an effect.
//!
//! The `34 36` variant: gathering points share one code (about 95 places of
//! 1500000 in Verteron, 190 of 1590001 in the Abyss), and the u32 after the
//! gather count tells which resource: 1500000 carries six values shared by
//! many places (`31 12 36 ab` at 14, `13 dd 47 c5` at 9, ...), 1590001 two
//! (`a4 97 08 0b` at 29, `9c 97 88 64` at 28). For other codes (1100016, say)
//! it is one value per place, an instance's, not a type. Read only in the
//! layout it was seen in: mask 0x0004 with 46 bytes after the code.

use super::abnormal;
use super::stream_processor::read_varint;

/// A world position (x, y, z).
pub type Pos = [f32; 3];

/// Farther than any map reaches (the largest is ±408,000): a misread.
const MAX_COORD: f32 = 1_000_000.0;
/// 2024-01-01 to 2100-01-01: a spawn time in unix ms.
const SPAWNED_FROM: u64 = 1_704_067_200_000;
const SPAWNED_TO: u64 = 4_102_444_800_000;

#[derive(Debug, Clone, PartialEq)]
pub enum WorldRecord {
    /// `41 36`. `owner` is the player a summon belongs to, when it says;
    /// `pet` the pet whose soul a wild creature carries (`capture::pets`).
    Npc { entity: i32, kind: u8, code: u32, owner: Option<String>, pos: Pos, heading: f32, pet: Option<u32> },
    /// `45 36`.
    Player { entity: i32, name: String, level: u32, pos: Pos, heading: f32 },
    /// `34 36`.
    Object(ObjectSpawn),
    /// `35 36`: `used_up` when it was gathered empty, otherwise out of view.
    ObjectGone { entity: i32, used_up: bool },
    /// `42 36`: 1 out of view, 3 and 7 dead.
    NpcGone { entity: i32, reason: u8 },
    /// A move of someone else (`1c 37`, `1b 37`, `1a 37`, `29 37`, `28 37`).
    Moved { entity: i32, pos: Pos, heading: Option<f32> },
    /// An abnormal landed: `caster` stood at `pos`.
    CasterAt { caster: i32, target: i32, pos: Pos },
    /// A cast or a hit from `source` on `target`, who stood at `pos` when it
    /// says; `facing` is where the source faced (degrees), when it says.
    Cast { opcode: [u8; 2], source: i32, target: i32, facing: Option<f32>, pos: Option<Pos> },
    /// `15 38`, `16 38`: `entity` was moved to `pos` by a skill.
    Displaced { entity: i32, pos: Pos },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectSpawn {
    pub entity: i32,
    pub mask: u16,
    pub code: u32,
    pub pos: Pos,
    /// Degrees.
    pub yaw: f32,
    pub state: [u8; 3],
    /// When the server spawned it (unix ms): days ago for a fixed object,
    /// minutes for a gathering node that came back.
    pub spawned_ms: Option<i64>,
    /// How many gathers it has given already.
    pub gathered: Option<u64>,
    /// Which resource a gathering point is (see the module's notes); for
    /// other codes an instance's own value. Only for mask 0x0004 records of
    /// 46 bytes after the code.
    pub variant: Option<u32>,
}

/// Whether a `41 36` kind is a monster (not a summon or an effect).
pub fn is_monster_kind(kind: u8) -> bool {
    matches!(kind, 0x04 | 0x05 | 0x0C | 0x0D)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn varint(&mut self) -> Option<i32> {
        let v = read_varint(self.b, self.at);
        if v.length <= 0 {
            return None;
        }
        self.at += v.length as usize;
        Some(v.value)
    }
    fn bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let out = self.b.get(self.at..self.at + N)?.try_into().ok()?;
        self.at += N;
        Some(out)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes::<1>()?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.bytes()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.bytes()?)).filter(|v| v.is_finite())
    }
    fn pos(&mut self) -> Option<Pos> {
        let pos = [self.f32()?, self.f32()?, self.f32()?];
        pos.iter().all(|v| v.abs() < MAX_COORD).then_some(pos)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        (self.at + n <= self.b.len()).then(|| self.at += n)
    }
    fn left(&self) -> usize {
        self.b.len().saturating_sub(self.at)
    }
}

/// A u16 yaw in degrees.
fn yaw(raw: u16) -> f32 {
    f32::from(raw) * 360.0 / 65536.0
}

/// Read one framed packet (its length varint included). `None` for any other
/// record, and for one that does not read as its layout says.
pub fn parse(packet: &[u8]) -> Option<WorldRecord> {
    let len = read_varint(packet, 0);
    if len.length <= 0 {
        return None;
    }
    let o = len.length as usize;
    let op: [u8; 2] = packet.get(o..o + 2)?.try_into().ok()?;
    let mut r = Reader { b: packet, at: o + 2 };
    match op {
        [0x41, 0x36] => {
            let entity = r.varint()?;
            let kind = r.u16()? as u8;
            let owner = if r.u8()? & 0x01 != 0 {
                let n = r.u8()? as usize;
                let name = r.b.get(r.at..r.at + n)?;
                r.skip(n)?;
                Some(String::from_utf8_lossy(name).into_owned())
            } else {
                None
            };
            let code = r.u32()?;
            r.u8()?;
            if r.u8()? != 0x02 {
                return None;
            }
            let pos = r.pos()?;
            let heading = r.f32()?;
            let pet = if kind == 0x0C { super::pets::spawn_pet(r.b.get(r.at..).unwrap_or_default()) } else { None };
            Some(WorldRecord::Npc { entity, kind, code, owner, pos, heading, pet })
        }
        [0x45, 0x36] => {
            let entity = r.varint()?;
            r.u32()?;
            let mask2 = r.u8()?;
            if mask2 & 0x01 == 0 {
                return None;
            }
            let n = r.u8()? as usize;
            let name = String::from_utf8(r.b.get(r.at..r.at + n)?.to_vec()).ok()?;
            r.skip(n)?;
            let level = r.u32()?;
            r.skip(if mask2 & 0x10 != 0 { 3 } else { 2 })?;
            r.u8()?;
            if r.u8()? & 0x12 != 0x12 {
                return None;
            }
            let pos = r.pos()?;
            let heading = r.f32()?;
            (!name.is_empty() && pos != [0.0; 3]).then_some(WorldRecord::Player { entity, name, level, pos, heading })
        }
        [0x34, 0x36] => {
            let entity = r.varint()?;
            let mask = r.u16()?;
            let code = r.u32()?;
            let pos = r.pos()?;
            if r.f32()? != 0.0 {
                return None;
            }
            let yaw = r.f32()?;
            let state = r.bytes::<3>()?;
            let spawned_ms = r.u64().filter(|t| (SPAWNED_FROM..SPAWNED_TO).contains(t)).map(|t| t as i64);
            // Mask bit 1 has 4 bytes of its own before the count.
            let gathered = if mask & 0x01 != 0 { None } else { r.u64().filter(|_| spawned_ms.is_some()) };
            // Then the variant and 3 bytes (`07 00 00`, `08 00 00`, `07 01 00`).
            let variant = if mask == 0x0004 && gathered.is_some() && r.left() == 7 { r.u32() } else { None };
            Some(WorldRecord::Object(ObjectSpawn { entity, mask, code, pos, yaw, state, spawned_ms, gathered, variant }))
        }
        [0x35, 0x36] => {
            let entity = r.varint()?;
            let reason = r.u8()?;
            r.skip(2)?;
            (r.left() == 0).then_some(WorldRecord::ObjectGone { entity, used_up: reason == 0x04 })
        }
        [0x42, 0x36] => {
            let entity = r.varint()?;
            if r.u8()? != 0 {
                return None;
            }
            let reason = r.u8()?;
            (r.left() == 0).then_some(WorldRecord::NpcGone { entity, reason })
        }
        [0x1C, 0x37] | [0x1B, 0x37] => {
            let entity = r.varint()?;
            if r.u8()? & 0x01 != 0 {
                r.u8()?;
            }
            let pos = r.pos()?;
            let heading = Some(yaw(r.u16()?));
            Some(WorldRecord::Moved { entity, pos, heading })
        }
        [0x1A, 0x37] => {
            let entity = r.varint()?;
            r.skip(2)?;
            let pos = r.pos()?;
            let heading = Some(yaw(r.u16()?));
            Some(WorldRecord::Moved { entity, pos, heading })
        }
        [0x29, 0x37] | [0x28, 0x37] => {
            let entity = r.varint()?;
            r.skip(2)?;
            let pos = r.pos()?;
            Some(WorldRecord::Moved { entity, pos, heading: None })
        }
        abnormal::ADDED | abnormal::CHANGED => {
            let applied = match abnormal::parse(packet)? {
                abnormal::Record::Added(a) | abnormal::Record::Changed(a) => a,
                _ => return None,
            };
            let mut tail = Reader { b: packet, at: packet.len().checked_sub(12)? };
            Some(WorldRecord::CasterAt { caster: applied.caster, target: applied.entity, pos: tail.pos()? })
        }
        [0x02, 0x38] => {
            let source = r.varint()?;
            r.u8()?;
            r.u32()?;
            r.u8()?;
            let flags = r.u8()?;
            let target = r.varint()?;
            let facing = r.f32()?;
            let pos = if flags & 0x02 != 0 { Some(r.pos()?) } else { None };
            if !(4..=8).contains(&r.left()) {
                return None;
            }
            Some(WorldRecord::Cast { opcode: op, source, target, facing: Some(facing), pos })
        }
        [0x03, 0x38] => {
            let source = r.varint()?;
            r.u8()?;
            let target = r.varint()?;
            r.u32()?;
            r.u8()?;
            r.u32()?;
            let pos = r.pos()?;
            r.varint()?;
            (r.left() == 6).then_some(WorldRecord::Cast { opcode: op, source, target, facing: None, pos: Some(pos) })
        }
        [0x1C, 0x38] => {
            let source = r.varint()?;
            r.u8()?;
            r.u32()?;
            r.skip(2)?;
            let target = r.varint()?;
            let facing = r.f32()?;
            let pos = r.pos()?;
            (r.left() == 5).then_some(WorldRecord::Cast { opcode: op, source, target, facing: Some(facing), pos: Some(pos) })
        }
        [0x15, 0x38] => {
            let entity = r.varint()?;
            let pos = r.pos()?;
            (r.left() == 2).then_some(WorldRecord::Displaced { entity, pos })
        }
        [0x16, 0x38] => {
            let entity = r.varint()?;
            r.varint()?;
            r.u32()?;
            let pos = r.pos()?;
            (r.left() == 3).then_some(WorldRecord::Displaced { entity, pos })
        }
        _ => None,
    }
}

/// Where you stood, if `record` says: see the module's notes for which
/// records place you and why.
pub fn own_position(record: &WorldRecord, local: i32) -> Option<Pos> {
    match *record {
        WorldRecord::CasterAt { caster, pos, .. } if caster == local => Some(pos),
        WorldRecord::Cast { source, target, pos: Some(pos), .. } if target == local && source != local => Some(pos),
        WorldRecord::Displaced { entity, pos } if entity == local => Some(pos),
        _ => None,
    }
}

/// Someone else's position that `record` gives besides their own spawn and
/// moves: the caster of an abnormal, the target of your (or anyone's) cast.
pub fn other_position(record: &WorldRecord, local: Option<i32>) -> Option<(i32, Pos)> {
    match *record {
        WorldRecord::CasterAt { caster, pos, .. } if Some(caster) != local => Some((caster, pos)),
        WorldRecord::Cast { source, target, pos: Some(pos), .. } if Some(target) != local && source != target => {
            Some((target, pos))
        }
        WorldRecord::Displaced { entity, pos } if Some(entity) != local => Some((entity, pos)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// You in the capture (ELLAO, the killer named in its 04 8d records).
    const ME: i32 = 14667;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn near(a: Pos, b: Pos) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.1)
    }

    #[test]
    fn a_monster_coming_into_view_reads_its_code_place_and_heading() {
        let r = parse(&hex(
            "6c4136cc84020420004bac27000002c36fe846b67091c600502b4600e89e4300e201e0d403e0d40364000000640000000000000000000000\
             00000000000000002c0100002c0100000100000000000000000000000000000000000000060001002d0000008693df2200",
        ));
        let Some(WorldRecord::Npc { entity, kind, code, owner, pos, heading, pet: None }) = r else { panic!("{r:?}") };
        // The Abyss Aberrant Swarn (Enxame Mutante do Abismo).
        assert_eq!((entity, kind, code, owner), (33356, 0x04, 2_600_011, None));
        assert!(near(pos, [29751.88, -18616.355, 10964.0]) && (heading - 317.8125).abs() < 0.01);
        assert!(is_monster_kind(kind));
    }

    #[test]
    fn a_summon_with_its_owners_name_reads_past_the_name() {
        let r = parse(&hex(include_str!("testdata/world_npc_spawn_summon.hex")));
        let Some(WorldRecord::Npc { entity, kind, code, owner, pos, .. }) = r else { panic!("{r:?}") };
        assert_eq!((entity, kind, code, owner.as_deref()), (24824, 0x5F, 2_920_738, Some("Danïka")));
        assert!(near(pos, [30701.361, -27036.645, 13687.23]));
        assert!(!is_monster_kind(kind));
    }

    #[test]
    fn a_player_record_places_them_after_the_name_with_or_without_the_extra_byte() {
        // mask2 0x17: one more byte before the flags.
        let r = parse(&hex(include_str!("testdata/world_player_spawn_named.hex")));
        let Some(WorldRecord::Player { entity, name, level, pos, heading }) = r else { panic!("{r:?}") };
        assert_eq!((entity, name.as_str(), level), (14577, "Danïka", 32));
        assert!(near(pos, [28322.416, -24988.205, 13161.54]) && (heading - 319.299).abs() < 0.01);
        // mask2 0x07 (in the Verteron part of the capture).
        let r = parse(&hex(include_str!("testdata/world_player_spawn_short.hex")));
        let Some(WorldRecord::Player { name, level, pos, .. }) = r else { panic!("{r:?}") };
        assert_eq!((name.as_str(), level), ("MrDarkstein", 13));
        assert!((pos[0] - -119_741.8).abs() < 1.0 && (pos[1] - -103_383.5).abs() < 1.0);
        // The zero-filled record that comes before a map load names nobody.
        assert_eq!(parse(&hex("184536d74800000000070146000000000000000000")), None);
    }

    #[test]
    fn a_monolith_and_a_gathering_node_read_their_code_place_and_spawn_time() {
        let r = parse(&hex(
            "413436bda1010c00c5c910008d7609474258cdc6cf7d15460000000000b0f44201010115975b39a10100000000000000000000\
             8262e3cf07004747a10700",
        ));
        let Some(WorldRecord::Object(o)) = r else { panic!("{r:?}") };
        assert_eq!((o.entity, o.mask, o.code, o.state, o.gathered), (20669, 0x0C, 1_100_229, [1, 1, 1], Some(0)));
        assert!(near(o.pos, [35190.55, -26284.13, 9567.452]) && (o.yaw - 122.34375).abs() < 0.001);
        // Placed about four days before (the server's start).
        assert_eq!(o.spawned_ms, Some(1_791_963_666_197));

        let r = parse(&hex(
            "3d3436e1fc010400f142180045832347232aa6c63c56474600000000000000000101011043184ea10100000000000000000000\
             a497080b070000",
        ));
        let Some(WorldRecord::Object(o)) = r else { panic!("{r:?}") };
        assert_eq!((o.entity, o.code, o.spawned_ms), (32353, 1_590_001, Some(1_792_311_575_312)));
        assert!(near(o.pos, [41859.27, -21269.068, 12757.559]));
        // The monolith's record is of another layout: no variant read.
        assert_eq!(parse(&hex("413436bda1010c00c5c910008d7609474258cdc6cf7d15460000000000b0f44201010115975b39a101000000000000000000008262e3cf07004747a10700")).and_then(|r| match r {
            WorldRecord::Object(o) => Some(o.variant),
            _ => None,
        }), Some(None));
    }

    #[test]
    fn a_gathering_point_reads_which_resource_it_is() {
        let object = |s: &str| match parse(&hex(s)) {
            Some(WorldRecord::Object(o)) => o,
            r => panic!("{r:?}"),
        };
        // Abyss, 1590001: `a4 97 08 0b` (29 places) and `9c 97 88 64` (28).
        let a = object(include_str!("testdata/world_object_gather_1590001_a.hex"));
        let b = object(include_str!("testdata/world_object_gather_1590001_b.hex"));
        assert_eq!((a.code, a.mask, a.gathered, a.variant), (1_590_001, 0x0004, Some(0), Some(0x0B08_97A4)));
        assert_eq!((b.code, b.variant), (1_590_001, Some(0x6488_979C)));
        assert!(near(b.pos, [37250.22, 9636.19, 37688.57]));
        // Verteron, 1500000: `31 12 36 ab` (14 places).
        let v = object(include_str!("testdata/world_object_gather_1500000.hex"));
        assert_eq!((v.code, v.mask, v.variant), (1_500_000, 0x0004, Some(0xAB36_1231)));
        assert!(near(v.pos, [-109821.0, -108032.0, 24280.19]));
        // Cut by a byte: not the layout, no variant.
        let cut = include_str!("testdata/world_object_gather_1500000.hex").trim();
        assert_eq!(object(&format!("3c{}", &cut[2..cut.len() - 2])).variant, None);
    }

    #[test]
    fn an_object_with_mask_bit_one_keeps_its_spawn_time_but_no_gather_count() {
        let r = parse(&hex(
            "413436ee91010100e1c8100000ecc7460092064700bee1460000000000000000010101c6935b39a101000000000000a88a732e\
             f66b651f00000000070000",
        ));
        let Some(WorldRecord::Object(o)) = r else { panic!("{r:?}") };
        assert_eq!((o.code, o.pos, o.spawned_ms, o.gathered, o.variant), (1_100_001, [25590.0, 34450.0, 28895.0], Some(1_791_963_665_350), None, None));
    }

    #[test]
    fn objects_and_npcs_leave_with_a_reason() {
        assert_eq!(parse(&hex("0c3536e1fc01010100")), Some(WorldRecord::ObjectGone { entity: 32353, used_up: false }));
        assert_eq!(parse(&hex("0c3536e9ab01040300")), Some(WorldRecord::ObjectGone { entity: 21993, used_up: true }));
        assert_eq!(parse(&hex("0b4236db86020001")), Some(WorldRecord::NpcGone { entity: 33627, reason: 1 }));
        assert_eq!(parse(&hex("0b4236ccda010003")), Some(WorldRecord::NpcGone { entity: 27980, reason: 3 }));
    }

    #[test]
    fn moves_of_other_entities_read_their_place_and_yaw() {
        let moved = |s: &str| match parse(&hex(s)) {
            Some(WorldRecord::Moved { entity, pos, heading }) => (entity, pos, heading),
            r => panic!("{r:?}"),
        };
        let (e, p, h) = moved("1b1c3797fb0104de97ed46b98348c600582846e038e03801");
        assert!(e == 32151 && near(p, [30411.934, -12832.931, 10774.0]) && (h.unwrap() - 79.98).abs() < 0.01);
        // Kind 5 has one more byte before the place.
        let (e, p, _) = moved("1a1b37d0770503cdf3884680fa1ec758dc294620312031");
        assert!(e == 15312 && near(p, [17529.9, -40698.5, 10871.086]));
        let (e, p, h) = moved("191a37f7b8010000f1a3f1462a2551c600a828469276");
        assert!(e == 23671 && near(p, [30929.97, -13385.291, 10794.0]) && (h.unwrap() - 166.74).abs() < 0.01);
        let (e, p, h) = moved("242937f1710303dbe1de46af9bc4c6ba424e466b0fe044bbbac0c45201ce430fe3");
        assert!(e == 14577 && near(p, [28528.928, -25165.842, 13200.682]) && h.is_none());
        let (e, p, _) = moved("242837f17105038859e046d3dec5c695fe4e46d66bdf44062ec0c4cf7ee0430fe3");
        assert!(e == 14577 && near(p, [28716.766, -25327.412, 13247.646]));
    }

    #[test]
    fn an_abnormal_you_cast_places_you_whoever_it_lands_on() {
        // A buff on yourself.
        let own = parse(&hex("302a38cb720111c305caea2101b80b000000000000a2491e4ea1010000cb7201001494f346478ab3c684c66946")).unwrap();
        assert!(near(own_position(&own, ME).unwrap(), [31178.04, -22981.139, 14961.629]));
        // A change of it, a moment later.
        let changed = parse(&hex("2f2b38cb7211c305caea2101b80b0000000000002c4c1e4ea1010000cb7201021308f746561db2c6ed7e6946")).unwrap();
        assert!(near(own_position(&changed, ME).unwrap(), [31620.037, -22798.668, 14943.731]));
        // A debuff of yours on Danïka: still your place.
        let on_her = parse(&hex(
            "342a38f1710113960127285d098813000000000000db7a1e4ea1010000cb720137b7ef0000f775e746e2c6d2c6dd775546",
        ))
        .unwrap();
        assert!(near(own_position(&on_her, ME).unwrap(), [29626.982, -26979.441, 13661.966]));
        assert_eq!(other_position(&on_her, Some(ME)), None);
    }

    #[test]
    fn an_abnormal_someone_else_puts_on_you_places_them_not_you() {
        let r = parse(&hex(
            "352a38cb720113db05b7552c091027000000000000cd8d1e4ea1010000f8c1010841d5ea0000b9daef464a39d3c6ecdc5546",
        ))
        .unwrap();
        assert_eq!(own_position(&r, ME), None);
        let (who, pos) = other_position(&r, Some(ME)).unwrap();
        assert!(who == 24824 && near(pos, [30701.361, -27036.645, 13687.23]));
    }

    #[test]
    fn a_cast_on_you_by_someone_else_places_you() {
        let r = parse(&hex("250238f17100c88d03011002cb7220ce2c43f775e746e2c6d2c6dd7755469e550200")).unwrap();
        assert!(near(own_position(&r, ME).unwrap(), [29626.982, -26979.441, 13661.966]));
        // Their hit landing on you: the same place.
        let r = parse(&hex("280338f17100cb72c88d030110a18a8e51f775e746e2c6d2c6dd775546de02010000000100")).unwrap();
        assert!(near(own_position(&r, ME).unwrap(), [29626.982, -26979.441, 13661.966]));
        let r = parse(&hex("261c38e15c0176c7d5000e02cb7200000000eba6cf466a6db1c6ea9661466ea3913f02")).unwrap();
        assert!(near(own_position(&r, ME).unwrap(), [26579.459, -22710.707, 14437.729]));
    }

    #[test]
    fn a_cast_on_a_monster_places_the_monster() {
        // Your cast on Danïka (f171) carries her place, not yours.
        let r = parse(&hex("270238cb72003ad5ea005402f171ac0d43c0b9daef464a39d3c6ecdc55469e550296aa03")).unwrap();
        assert_eq!(own_position(&r, ME), None);
        let (who, pos) = other_position(&r, Some(ME)).unwrap();
        assert!(who == 14577 && near(pos, [30701.361, -27036.645, 13687.23]));
        let Some(WorldRecord::Cast { facing: Some(facing), .. }) = parse(&hex("270238cb72003ad5ea005402f171ac0d43c0b9daef464a39d3c6ecdc55469e550296aa03")) else {
            panic!()
        };
        assert!((facing - -3.0477).abs() < 0.001);
        // Your hit on her, too.
        let r = parse(&hex("280338cb7200f171e5cde50061c2f2e7472a3cf346e6cfd4c6eb2256469601010000000100")).unwrap();
        assert_eq!(own_position(&r, ME), None);
        assert!(near(other_position(&r, Some(ME)).unwrap().1, [31134.082, -27239.95, 13704.729]));
    }

    #[test]
    fn a_cast_of_yours_on_yourself_is_no_place() {
        // No position at all (flags 0).
        let r = parse(&hex("1b0238cb72005efcea005300cb72946729409e550296aa03")).unwrap();
        assert!(matches!(r, WorldRecord::Cast { pos: None, .. }));
        assert_eq!(own_position(&r, ME), None);
        // A small offset that is no place on any map.
        let r = parse(&hex("260238cb720024e2e4002a02cb7264d9144311c7bc42361b04c200000000904e01f403")).unwrap();
        assert_eq!((own_position(&r, ME), other_position(&r, Some(ME))), (None, None));
    }

    #[test]
    fn being_pushed_or_pulled_places_you() {
        let r = parse(&hex("161538cb722851224746a7a1c6009c28460201")).unwrap();
        assert!(near(own_position(&r, ME).unwrap(), [41553.156, -20691.637, 10791.0]));
        let r = parse(&hex("1d1638cb7298748bd69a3b053bbd46714c1747900be046040101")).unwrap();
        assert!(near(own_position(&r, ME).unwrap(), [24221.51, 38732.44, 28677.78]));
        // Someone else pulled: their place.
        let r = parse(&hex("171538858d018f8e8b46505c12c700a42a460201")).unwrap();
        assert_eq!(own_position(&r, ME), None);
        assert_eq!(other_position(&r, Some(ME)).map(|(e, _)| e), Some(18053));
    }

    #[test]
    fn other_records_and_cut_ones_are_not_positions() {
        // A map load, a teleport-less zone record, a damage tick.
        assert_eq!(parse(&hex("34213601000000f2030000dd7f3c00000000006868d047d0c62c470098da46fa63284300000000000000000000004f0000")), None);
        assert_eq!(parse(&hex("111d37f7b80126f7029076907601")), None);
        assert_eq!(parse(&hex("0f008dcb72010106716d0300")), None);
        // Cut short.
        assert_eq!(parse(&hex("1b1c3797fb0104de97ed46b98348")), None);
        assert_eq!(parse(&hex("413436bda1010c00c5c910008d76")), None);
        assert_eq!(parse(&hex("0c3536e1fc010101")), None);
    }
}

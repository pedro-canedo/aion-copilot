//! aion-copilot: the game's counters — entry counts, energies and timed zones
//! — and the week's Abyss points, for the Routine.
//!
//! ```text
//! <len varint> 0b 61 <n varint> <entry> x n      the whole list: login, zone load, relog
//! <len varint> 0c 61 <u8: 00 or 01> <entry> <reason u8> [<delta u32le> when reason is 01]
//!                                                 one counter changed
//! <len varint> a9 ff <gained u64le> <limit u64le> the week's Abyss points, on each gain
//! <len varint> a5 ff <earned u64le> <server ms u64le>  kinah earned today, at login
//! <len varint> 83 56 <n varint> <record> x n <...>  progress records, at login (39 KB)
//! <len varint> 84 56 <n varint> <record> x n        records that changed, live
//! <len varint> 56 e2 <n varint> <board> x n          supply boards, at login
//! <len varint> 2a e3 <n varint> <arena> x n          arena records, at login
//!
//! entry = <flags u8> <id u32le>
//!         [flags & 01: time u64le, ms LEFT]  [flags & 02: charged time u64le, ms]
//!         [flags & 04: count varint]         [flags & 08: charged count varint]
//! ```
//!
//! A field the flags leave out is 0 (flags `00`: all four are). The login list
//! of 2026-10-14 21:16:50 (`testdata/counters_list_v1.hex`) holds 73 entries
//! and ends exactly after the last; only flags 00, 01, 04 and 0c were seen.
//! Bit 02 was never on the wire: that it is a u64 after the time is inferred
//! from the time in the Abyss "+1 h" the game showed later (see
//! `Counter::split_time`). The `0c 61` reasons seen: 01 an addition (the
//! delta is how much), 02 a set, 03 a spend or clear; the entry always holds
//! the new values, so the reason is not needed to keep up.
//!
//! What an id is: 60000001 Odyle energy (count = basic, charged = charged),
//! 3 time in the Abyss (time = left, charged time = extra, out of 7 h). The
//! others are not confirmed and are not named here.
//!
//! The login packets (2026-10-18 11:59:40, `testdata/progress_login_v1.hex`,
//! `testdata/supply_login_v1.hex`), checked against what another companion
//! showed for the same character (only its values, as ground truth):
//!
//! ```text
//! record  = <id u32> <value u32> <last ms u64> <claimed u8>      17 bytes
//!           2338 of them, then ~130 bytes not read. The 510xxxxxx ids are
//!           the attendance events: value = days attended, claimed = days
//!           rewarded, last = when the last day was marked.
//! board   = <id u32> <slot u8> <start ms u64> <end ms u64> <n varint>
//!           <request> x n, and a byte of flags after the 1st, 9th, 17th...
//!           request of the packet (counted across boards)
//! request = <board id u32> <request id u32> <u32>
//! ```
//!
//! ```text
//! arena   = <arena u8> <rating u32> <u32> <wins u8> <losses u8> <3 bytes>   14 bytes
//! ```
//!
//! Arena 1 is the solo one, 2 the cooperation one, 3 the tactics one; the
//! login of 11:59:40 read 1527 with 3 wins and 2 losses for the first and
//! 1500 with none for the others, as the other companion showed. Only those
//! two bytes were ever not 0 after the rating, so how wide wins and losses
//! are (and what the u32 and the last 3 bytes hold) is not known: past 255
//! they would read wrong. They are not the week's entries.
//!
//! Two records are running totals the Routine counts from a reset (a
//! `Tally`): their last ms says whether they moved since it, so the count is
//! the total less what it was then.
//! - 300100300, the missions ever completed, per day: +1 on each of two
//!   missions done at 17:00:53 and 17:02:39 (`84 56`, one record each), 11
//!   -> 13 between two logins.
//! - 231111400, the arena matches played, per week: 5 at login (3 wins and
//!   2 losses in `2a e3`), +1 when a match ended at 17:09:31.
//!
//! A board's window says what it is: 24 h the daily emergency supply, 7 days
//! the weekly one (10 requests each), the rest open-ended. Which field says
//! a request was delivered is NOT confirmed (all were 0 with 0 delivered), so
//! the boards are for developer mode only. The kinah's daily limit is not in
//! its packet. None of these three came again in an hour of play.
//!
//! The upstream meter reads Odyle energy from the same packets with its own
//! reader (`StreamProcessor::parse_odyle_energy_packet`), left as it is.

use std::collections::BTreeMap;

/// Odyle energy.
pub const ODYLE_ENERGY: u32 = 60_000_001;
/// Time in the Abyss.
pub const ABYSS_TIME: u32 = 3;

const LIST: [u8; 2] = [0x0B, 0x61];
const UPDATE: [u8; 2] = [0x0C, 0x61];
const ABYSS_POINTS: [u8; 2] = [0xA9, 0xFF];
const KINAH_TODAY: [u8; 2] = [0xA5, 0xFF];
const PROGRESS: [u8; 2] = [0x83, 0x56];
const PROGRESS_UPDATE: [u8; 2] = [0x84, 0x56];
/// The progress record counting the missions ever completed.
pub const MISSIONS_DONE: u32 = 300_100_300;
/// The progress record counting the arena matches ever played.
pub const ARENA_PLAYED: u32 = 231_111_400;
const RECORD: usize = 17;
const DAY_MS: i64 = 86_400_000;
/// The daily reset, 07:00 UTC (Global servers); the weekly one is Wednesday's.
const DAILY_RESET_MS: i64 = 7 * 3_600_000;
const WEDNESDAY: i64 = 3;
const SUPPLY: [u8; 2] = [0x56, 0xE2];
const ARENA: [u8; 2] = [0x2A, 0xE3];
/// An arena record's size.
const ARENA_RECORD: usize = 14;
/// More entries than this is not a list (the game sends 73).
const MAX_ENTRIES: u64 = 1_000;
/// The progress records' attendance events: ids 510xxxxxx.
const ATTENDANCE_GROUP: u32 = 510;
/// Times read as unix ms must fall in here (2017..2128).
const PLAUSIBLE_MS: std::ops::Range<u64> = 1_500_000_000_000..5_000_000_000_000;

/// One counter as the game last sent it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counter {
    pub id: u32,
    /// The time left, ms (a timed zone's: 25,200,000 when untouched).
    pub time_ms: u64,
    /// Extra time charged on top, ms.
    pub charged_ms: u64,
    pub count: u32,
    /// Charged on top of the count (Odyle energy from items).
    pub charged: u32,
    /// When the meter read it (unix ms).
    pub seen_ms: i64,
}

impl Counter {
    /// Flags `01` alone with a time past a u32: two u32 instead (time,
    /// charged time). Every time seen had its high half 0, so the data can't
    /// tell that reading from bit 02's; a time this long (49 days) isn't one.
    fn split_time(&mut self) {
        if self.charged_ms == 0 && self.time_ms > u64::from(u32::MAX) {
            self.charged_ms = self.time_ms >> 32;
            self.time_ms &= u64::from(u32::MAX);
        }
    }
}

/// The week's Abyss points.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AbyssPoints {
    pub gained: u64,
    /// 1,500,000 so far.
    pub limit: u64,
    pub seen_ms: i64,
}

/// Kinah earned today (since the daily reset), as the game said at login.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KinahToday {
    pub earned: u64,
    /// The server's clock when it said it.
    pub server_ms: i64,
    pub seen_ms: i64,
}

/// One attendance event: days attended, days rewarded, when last marked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttendanceEvent {
    pub id: u32,
    pub days: u32,
    pub claimed: u32,
    /// Unix ms; 0 never.
    pub last_ms: i64,
}

/// One progress record: an id's value, when it last changed, how much of it
/// was claimed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgressRecord {
    pub id: u32,
    pub value: u32,
    /// Unix ms; 0 never.
    pub last_ms: i64,
    pub claimed: u8,
}

impl ProgressRecord {
    fn attendance(&self) -> Option<AttendanceEvent> {
        (self.id / 1_000_000 == ATTENDANCE_GROUP).then_some(AttendanceEvent {
            id: self.id,
            days: self.value,
            claimed: u32::from(self.claimed),
            last_ms: self.last_ms,
        })
    }
}

/// A running total, and what it was at the last reset when known.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub total: u32,
    /// When it last moved (unix ms; 0 never).
    pub last_ms: i64,
    pub seen_ms: i64,
    /// The reset `start` is for.
    pub reset_ms: i64,
    /// The total at that reset; None when it isn't known (the first look
    /// since the reset came after it moved).
    pub start: Option<u32>,
}

/// The last daily reset at or before `ms`.
pub fn last_daily_reset(ms: i64) -> i64 {
    (ms - DAILY_RESET_MS).div_euclid(DAY_MS) * DAY_MS + DAILY_RESET_MS
}

/// The last weekly reset (Wednesday's) at or before `ms`.
pub fn last_weekly_reset(ms: i64) -> i64 {
    let daily = last_daily_reset(ms);
    // 1970-01-01 was a Thursday (4).
    let weekday = (daily.div_euclid(DAY_MS) + 4).rem_euclid(7);
    daily - (weekday - WEDNESDAY).rem_euclid(7) * DAY_MS
}

/// The reset a tallied record counts from, None for any other record.
fn tally_reset(id: u32) -> Option<fn(i64) -> i64> {
    match id {
        MISSIONS_DONE => Some(last_daily_reset),
        ARENA_PLAYED => Some(last_weekly_reset),
        _ => None,
    }
}

/// One supply board; what isn't confirmed is kept as two guesses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupplyBoard {
    pub id: u32,
    pub slot: u8,
    pub start_ms: i64,
    pub end_ms: i64,
    pub requests: u32,
    /// Requests whose flag bit is on (a guess at "delivered": unconfirmed).
    pub flagged: u32,
    /// Requests whose third u32 isn't 0 (the other guess: unconfirmed).
    pub with_value: u32,
}

/// One arena's rating, wins and losses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArenaRecord {
    /// 1 solo, 2 cooperation, 3 tactics.
    pub arena: u8,
    pub rating: u32,
    pub wins: u32,
    pub losses: u32,
}

/// A login packet's records, and when read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot<T> {
    pub items: Vec<T>,
    pub seen_ms: i64,
}

/// What a counters packet carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterPacket {
    /// The whole list: what it leaves out is gone.
    List(Vec<Counter>),
    /// One counter's new values; `delta` is how much was added (reason 01).
    Update { counter: Counter, reason: u8, delta: Option<u32> },
    AbyssPoints(AbyssPoints),
    KinahToday(KinahToday),
    /// The progress records the meter keeps (attendance events, missions):
    /// all of them at login (`full`), else the ones that changed.
    Progress { records: Snapshot<ProgressRecord>, full: bool },
    Supply(Snapshot<SupplyBoard>),
    Arena(Snapshot<ArenaRecord>),
}

fn varint(b: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let x = *b.get(i)?;
        i += 1;
        v |= u64::from(x & 0x7F) << shift;
        if x & 0x80 == 0 {
            return Some((v, i));
        }
        shift += 7;
        if shift > 35 {
            return None;
        }
    }
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// The entry at `i`, and where the next begins.
pub fn parse_entry(b: &[u8], mut i: usize, now_ms: i64) -> Option<(Counter, usize)> {
    let flags = *b.get(i)?;
    if flags & !0x0F != 0 {
        return None;
    }
    let mut c = Counter { id: u32_at(b, i + 1)?, seen_ms: now_ms, ..Counter::default() };
    i += 5;
    if flags & 0x01 != 0 {
        c.time_ms = u64_at(b, i)?;
        i += 8;
    }
    if flags & 0x02 != 0 {
        c.charged_ms = u64_at(b, i)?;
        i += 8;
    } else if flags & 0x01 != 0 {
        c.split_time();
    }
    if flags & 0x04 != 0 {
        let (v, next) = varint(b, i)?;
        c.count = u32::try_from(v).ok()?;
        i = next;
    }
    if flags & 0x08 != 0 {
        let (v, next) = varint(b, i)?;
        c.charged = u32::try_from(v).ok()?;
        i = next;
    }
    Some((c, i))
}

/// A `0b 61` body: every entry, or None unless the count's entries fill it
/// exactly (excerpts and misreads leave the counters as they were).
fn parse_list(body: &[u8], now_ms: i64) -> Option<Vec<Counter>> {
    let (n, mut i) = varint(body, 0)?;
    if n == 0 || n > MAX_ENTRIES {
        return None;
    }
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let (c, next) = parse_entry(body, i, now_ms)?;
        out.push(c);
        i = next;
    }
    (i == body.len()).then_some(out)
}

/// A `0c 61` body: its one entry, the reason, and the delta after reason 01;
/// None unless that fills it exactly, as a list must.
fn parse_update(body: &[u8], now_ms: i64) -> Option<CounterPacket> {
    if *body.first()? > 1 {
        return None;
    }
    let (counter, i) = parse_entry(body, 1, now_ms)?;
    let reason = *body.get(i)?;
    let delta = if reason == 1 { u32_at(body, i + 1) } else { None };
    let end = i + 1 + if delta.is_some() { 4 } else { 0 };
    (end == body.len()).then_some(CounterPacket::Update { counter, reason, delta })
}

/// An `a9 ff` body: gained, then the limit.
fn parse_abyss_points(body: &[u8], now_ms: i64) -> Option<AbyssPoints> {
    if body.len() != 16 {
        return None;
    }
    let (gained, limit) = (u64_at(body, 0)?, u64_at(body, 8)?);
    // Generous bounds, only to turn away bytes that merely look like it.
    (limit > 0 && limit < 1_000_000_000 && gained <= limit * 2).then_some(AbyssPoints { gained, limit, seen_ms: now_ms })
}

/// An `a5 ff` body: kinah earned today, then the server's clock.
fn parse_kinah_today(body: &[u8], now_ms: i64) -> Option<KinahToday> {
    if body.len() != 16 {
        return None;
    }
    let (earned, server_ms) = (u64_at(body, 0)?, u64_at(body, 8)?);
    (earned < 1_000_000_000_000 && PLAUSIBLE_MS.contains(&server_ms))
        .then_some(KinahToday { earned, server_ms: server_ms as i64, seen_ms: now_ms })
}

/// The records worth keeping: the attendance events and the tallies.
fn kept_record(r: &ProgressRecord) -> bool {
    r.id / 1_000_000 == ATTENDANCE_GROUP || tally_reset(r.id).is_some()
}

/// An `83 56` body: the records kept among its records. The records must
/// fit and every time be 0 or a date; what follows them isn't read. An
/// `84 56` body (`exact`) must hold its records and nothing else.
fn parse_progress(body: &[u8], exact: bool) -> Option<Vec<ProgressRecord>> {
    let (n, start) = varint(body, 0)?;
    if n == 0 || n > 100_000 {
        return None;
    }
    let end = start.checked_add(RECORD.checked_mul(usize::try_from(n).ok()?)?)?;
    if end > body.len() || (exact && end != body.len()) {
        return None;
    }
    let mut out = Vec::new();
    for i in (start..end).step_by(RECORD) {
        let (id, value, last) = (u32_at(body, i)?, u32_at(body, i + 4)?, u64_at(body, i + 8)?);
        if last != 0 && !PLAUSIBLE_MS.contains(&last) {
            return None;
        }
        let record = ProgressRecord { id, value, last_ms: last as i64, claimed: body[i + 16] };
        if kept_record(&record) {
            out.push(record);
        }
    }
    Some(out)
}

/// A `56 e2` body: every board; None unless each request names its board
/// and the boards fill the body exactly.
fn parse_supply(body: &[u8]) -> Option<Vec<SupplyBoard>> {
    let (n, mut i) = varint(body, 0)?;
    if n == 0 || n > 64 {
        return None;
    }
    let mut out = Vec::new();
    // Requests so far, across boards: a flags byte follows every 8th from the 1st.
    let mut seen = 0usize;
    let mut flags = 0u8;
    for _ in 0..n {
        let id = u32_at(body, i)?;
        let slot = *body.get(i + 4)?;
        let (start, end) = (u64_at(body, i + 5)?, u64_at(body, i + 13)?);
        let (count, next) = varint(body, i + 21)?;
        i = next;
        if count > 1_000 {
            return None;
        }
        let mut board = SupplyBoard {
            id,
            slot,
            start_ms: start as i64,
            end_ms: end as i64,
            requests: count as u32,
            ..SupplyBoard::default()
        };
        for _ in 0..count {
            if u32_at(body, i)? != id {
                return None;
            }
            let value = u32_at(body, i + 8)?;
            i += 12;
            if seen % 8 == 0 {
                flags = *body.get(i)?;
                i += 1;
            }
            board.flagged += u32::from((flags >> (seen % 8)) & 1);
            board.with_value += u32::from(value != 0);
            seen += 1;
        }
        out.push(board);
    }
    (i == body.len()).then_some(out)
}

/// A `2a e3` body: every arena; None unless the records fill it exactly.
fn parse_arena(body: &[u8]) -> Option<Vec<ArenaRecord>> {
    let (n, i) = varint(body, 0)?;
    if n == 0 || n > 16 || body.len() != i + n as usize * ARENA_RECORD {
        return None;
    }
    let out: Vec<ArenaRecord> = body[i..]
        .chunks_exact(ARENA_RECORD)
        .map(|r| ArenaRecord {
            arena: r[0],
            rating: u32::from_le_bytes([r[1], r[2], r[3], r[4]]),
            wins: u32::from(r[9]),
            losses: u32::from(r[10]),
        })
        .collect();
    // A rating is in the hundreds or thousands; anything else isn't this.
    out.iter().all(|r| r.arena != 0 && r.rating < 100_000).then_some(out)
}

/// The counters in a packet (`<len varint> <opcode> <body>`), or None when it
/// carries none or doesn't read cleanly.
pub fn parse(packet: &[u8], now_ms: i64) -> Option<CounterPacket> {
    let (_, i) = varint(packet, 0)?;
    let opcode: [u8; 2] = packet.get(i..i + 2)?.try_into().ok()?;
    let body = &packet[i + 2..];
    match opcode {
        LIST => parse_list(body, now_ms).map(CounterPacket::List),
        UPDATE => parse_update(body, now_ms),
        ABYSS_POINTS => parse_abyss_points(body, now_ms).map(CounterPacket::AbyssPoints),
        KINAH_TODAY => parse_kinah_today(body, now_ms).map(CounterPacket::KinahToday),
        PROGRESS => parse_progress(body, false)
            .map(|items| CounterPacket::Progress { records: Snapshot { items, seen_ms: now_ms }, full: true }),
        PROGRESS_UPDATE => parse_progress(body, true)
            .filter(|items| !items.is_empty())
            .map(|items| CounterPacket::Progress { records: Snapshot { items, seen_ms: now_ms }, full: false }),
        SUPPLY => parse_supply(body).map(|items| CounterPacket::Supply(Snapshot { items, seen_ms: now_ms })),
        ARENA => parse_arena(body).map(|items| CounterPacket::Arena(Snapshot { items, seen_ms: now_ms })),
        _ => None,
    }
}

/// The counters the game has sent, and whose they are.
///
/// Whose: a login's list comes some seconds before the self record naming
/// who logged in, so a list has no owner until that record
/// (`awaits_self_record`); an update in a session the game has already named
/// takes that name. As `DataStorage`'s Odyle energy does.
///
/// A login sends the progress records a few ms before the list, then the
/// kinah and the supply boards: the first of the records or the list opens
/// the login (`login_ms`), and when the self record names someone else, what
/// was read before that is the other character's and goes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterBook {
    pub counters: BTreeMap<u32, Counter>,
    pub abyss_points: Option<AbyssPoints>,
    pub kinah_today: Option<KinahToday>,
    pub attendance: Option<Snapshot<AttendanceEvent>>,
    /// The tallied records (missions, arena matches) by id.
    pub tallies: BTreeMap<u32, Tally>,
    pub supply: Option<Snapshot<SupplyBoard>>,
    pub arena: Option<Snapshot<ArenaRecord>>,
    /// Whose; while `awaits_self_record`, still the one before the list.
    pub owner: Option<String>,
    pub awaits_self_record: bool,
    /// When the login waiting for the self record began.
    pub login_ms: i64,
    /// Moves with every change.
    pub seq: u64,
}

impl CounterBook {
    /// Take in a packet. `named`: who the game has said you are, if it has.
    pub fn note(&mut self, packet: CounterPacket, named: Option<&str>) {
        match packet {
            CounterPacket::List(list) => {
                // A new login or a zone load: possibly another character,
                // which the self record will say (the Abyss points, not in
                // the list, go then if it is another).
                self.open_login(list.first().map_or(0, |c| c.seen_ms));
                self.counters = list.into_iter().map(|c| (c.id, c)).collect();
            }
            CounterPacket::Progress { records, full: true } => {
                // Only at login, just before the list (none left to show:
                // the list opens it).
                if !records.items.is_empty() {
                    self.open_login(records.seen_ms);
                }
                let items = records.items.iter().filter_map(ProgressRecord::attendance).collect();
                self.attendance = Some(Snapshot { items, seen_ms: records.seen_ms });
                for r in &records.items {
                    self.note_tally(r, records.seen_ms, false);
                }
            }
            CounterPacket::Progress { records, full: false } => {
                self.name_if_unowned(named);
                for r in &records.items {
                    if let Some(event) = r.attendance() {
                        // Marked now: the day's attendance, live.
                        let snapshot = self.attendance.get_or_insert_with(Snapshot::default);
                        match snapshot.items.iter_mut().find(|e| e.id == event.id) {
                            Some(e) => *e = event,
                            None => snapshot.items.push(event),
                        }
                        snapshot.seen_ms = records.seen_ms;
                    } else {
                        self.note_tally(r, records.seen_ms, true);
                    }
                }
            }
            CounterPacket::Update { counter, .. } => {
                self.name_if_unowned(named);
                self.counters.insert(counter.id, counter);
            }
            CounterPacket::AbyssPoints(points) => {
                self.name_if_unowned(named);
                self.abyss_points = Some(points);
            }
            CounterPacket::KinahToday(kinah) => {
                self.name_if_unowned(named);
                self.kinah_today = Some(kinah);
            }
            CounterPacket::Supply(snapshot) => {
                self.name_if_unowned(named);
                self.supply = Some(snapshot);
            }
            CounterPacket::Arena(snapshot) => {
                self.name_if_unowned(named);
                self.arena = Some(snapshot);
            }
        }
        self.seq += 1;
    }

    /// A tallied record as read at `seen_ms`: untouched since its reset, its
    /// value is where the count starts. A `live` change continues what this
    /// session read: the start already known for the same reset, or, past a
    /// reset, the total last read before it (every change since came live).
    /// A login's record may be another character's, so it never takes a
    /// start from the book (the per-character file gives it back).
    fn note_tally(&mut self, r: &ProgressRecord, seen_ms: i64, live: bool) {
        let Some(reset) = tally_reset(r.id) else { return };
        let reset_ms = reset(seen_ms);
        let start = if r.last_ms < reset_ms {
            Some(r.value)
        } else if live {
            self.tallies.get(&r.id).and_then(|t| match t.reset_ms.cmp(&reset_ms) {
                std::cmp::Ordering::Equal => t.start,
                std::cmp::Ordering::Less if t.seen_ms < reset_ms => Some(t.total),
                _ => None,
            })
        } else {
            None
        };
        self.tallies.insert(r.id, Tally { total: r.value, last_ms: r.last_ms, seen_ms, reset_ms, start });
    }

    pub fn tally(&self, id: u32) -> Option<&Tally> {
        self.tallies.get(&id)
    }

    fn open_login(&mut self, at_ms: i64) {
        if !self.awaits_self_record {
            self.awaits_self_record = true;
            self.login_ms = at_ms;
        }
    }

    fn name_if_unowned(&mut self, named: Option<&str>) {
        if self.owner.is_none() && !self.awaits_self_record {
            self.owner = named.map(str::to_string);
        }
    }

    /// The game said who you are (a self record): a list waiting for it is
    /// theirs, and so is anything read before anyone was named.
    pub fn note_self_record(&mut self, name: Option<&str>) {
        let has_any = !self.counters.is_empty()
            || self.abyss_points.is_some()
            || self.kinah_today.is_some()
            || self.attendance.is_some()
            || !self.tallies.is_empty()
            || self.supply.is_some()
            || self.arena.is_some();
        if has_any && (self.awaits_self_record || self.owner.is_none()) {
            if self.owner.is_some() && self.owner.as_deref() != name {
                // Read before this login: the one before's.
                let login_ms = self.login_ms;
                let before = |seen: i64| seen < login_ms;
                if self.abyss_points.is_some_and(|p| before(p.seen_ms)) {
                    self.abyss_points = None;
                }
                if self.kinah_today.is_some_and(|k| before(k.seen_ms)) {
                    self.kinah_today = None;
                }
                if self.attendance.as_ref().is_some_and(|a| before(a.seen_ms)) {
                    self.attendance = None;
                }
                self.tallies.retain(|_, t| !before(t.seen_ms));
                if self.supply.as_ref().is_some_and(|s| before(s.seen_ms)) {
                    self.supply = None;
                }
                if self.arena.as_ref().is_some_and(|a| before(a.seen_ms)) {
                    self.arena = None;
                }
            }
            self.owner = name.map(str::to_string);
            self.awaits_self_record = false;
            self.seq += 1;
        }
    }

    pub fn get(&self, id: u32) -> Option<&Counter> {
        self.counters.get(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn login_list() -> Vec<Counter> {
        match parse(&hex(include_str!("testdata/counters_list_v1.hex")), 1_000) {
            Some(CounterPacket::List(list)) => list,
            other => panic!("not a list: {other:?}"),
        }
    }

    fn by_id(list: &[Counter], id: u32) -> Counter {
        *list.iter().find(|c| c.id == id).unwrap()
    }

    /// A packet of `body`, with its length in front as the game puts it.
    fn packet(body: &str) -> Vec<u8> {
        let mut p = hex(body);
        p.insert(0, (p.len() + 1) as u8);
        p
    }

    #[test]
    fn the_login_list_reads_as_its_73_entries_and_ends_exactly_after_them() {
        let list = login_list();
        assert_eq!(list.len(), 73);
        assert!(list.iter().all(|c| c.seen_ms == 1_000));
        let ids: std::collections::BTreeSet<u32> = list.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), 73, "every id once");
        assert!(ids.contains(&60_015_101) && ids.contains(&10_000_012));
    }

    #[test]
    fn the_login_list_has_the_odyle_energy_the_game_showed() {
        let odyle = by_id(&login_list(), ODYLE_ENERGY);
        assert_eq!((odyle.count, odyle.charged, odyle.time_ms, odyle.charged_ms), (30, 840, 0, 0));
    }

    #[test]
    fn the_login_list_has_the_time_left_in_the_abyss_and_the_full_timed_zones() {
        let list = login_list();
        let abyss = by_id(&list, ABYSS_TIME);
        assert_eq!((abyss.time_ms, abyss.charged_ms, abyss.count), (18_596_000, 0, 0));
        for id in [9, 13, 201, 202, 203, 204, 205, 206] {
            assert_eq!(by_id(&list, id).time_ms, 25_200_000, "id {id}: 7 h, untouched");
        }
    }

    #[test]
    fn a_counter_the_list_sends_without_fields_is_all_zero_and_one_with_a_count_has_it() {
        let list = login_list();
        let four = by_id(&list, 4);
        assert_eq!((four.count, four.charged, four.time_ms, four.charged_ms), (0, 0, 0, 0));
        assert_eq!(by_id(&list, 10).count, 14);
        let first = by_id(&list, 1);
        assert_eq!((first.count, first.charged), (10, 1));
    }

    #[test]
    fn a_time_and_a_charged_time_read_in_flag_order() {
        // id 3: 12,511,000 ms left (3 h 28 min) and 3,600,000 charged (+1 h).
        let entry = hex("03 03000000 18e7be0000000000 80ee360000000000");
        let (c, end) = parse_entry(&entry, 0, 5).unwrap();
        assert_eq!((c.id, c.time_ms, c.charged_ms, c.seen_ms), (3, 12_511_000, 3_600_000, 5));
        assert_eq!(end, entry.len());
    }

    #[test]
    fn a_time_alone_past_a_u32_reads_as_time_and_charged_time() {
        // Flags 01 with the charged time in the high half instead of bit 02.
        let entry = hex("01 03000000 18e7be00 80ee3600");
        let (c, _) = parse_entry(&entry, 0, 0).unwrap();
        assert_eq!((c.time_ms, c.charged_ms), (12_511_000, 3_600_000));
    }

    #[test]
    fn an_unknown_flag_is_not_an_entry() {
        assert_eq!(parse_entry(&hex("10 03000000"), 0, 0), None);
    }

    #[test]
    fn a_list_excerpt_whose_count_says_more_is_not_read() {
        // The upstream Odyle tests' three-entry excerpts keep the count of 73.
        assert_eq!(parse(&packet("0b61490401879303c80604028793030e04038793030e"), 0), None);
        // Bytes left after the last entry: not this list either.
        assert_eq!(parse(&packet("0b6101 0403000000 05 ff"), 0), None);
        assert!(matches!(parse(&packet("0b6101 0403000000 05"), 0), Some(CounterPacket::List(l)) if l[0].count == 5));
    }

    #[test]
    fn updates_carry_the_new_values_and_the_delta_of_an_addition() {
        let update = |p: &str| match parse(&hex(p), 7) {
            Some(CounterPacket::Update { counter, reason, delta }) => (counter, reason, delta),
            other => panic!("{p}: {other:?}"),
        };
        // A recharge item, 2026-10-14 21:24:45: 840 to 880 charged.
        let (c, reason, delta) = update("140c61010c018793031ef0060128000000");
        assert_eq!((c.id, c.count, c.charged, c.seen_ms, reason, delta), (ODYLE_ENERGY, 30, 880, 7, 1, Some(40)));
        // Charged energy alone: spent to 1755, then 240 added to 1995.
        let (c, reason, delta) = update("0f0c61000801879303db0d03");
        assert_eq!((c.count, c.charged, reason, delta), (0, 1755, 3, None));
        let (c, _, delta) = update("130c61010801879303cb0f01f0000000");
        assert_eq!((c.charged, delta), (1995, Some(240)));
        // Set: id 12 to 14 (2026-10-17 05:30:29).
        let (c, reason, _) = update("0e0c6100040c0000000e02");
        assert_eq!((c.id, c.count, reason), (12, 14, 2));
        // Cleared: 60000009 to 0.
        let (c, reason, _) = update("0d0c6100000987930303");
        assert_eq!((c.id, c.count, c.charged, reason), (60_000_009, 0, 0, 3));
    }

    #[test]
    fn an_update_with_bytes_left_after_it_is_not_read() {
        assert!(parse(&hex("0e0c6100040c0000000e02"), 0).is_some());
        assert_eq!(parse(&hex("0f0c6100040c0000000e02ff"), 0), None);
        assert_eq!(parse(&hex("150c61010c018793031ef0060128000000ff"), 0), None);
    }

    #[test]
    fn the_login_arena_records_are_rating_wins_and_losses_per_arena() {
        // The login of 2026-10-18 11:59:40.
        let p = parse(&hex("312ae30301f705000000000000030200000002dc05000000000000000000000003dc050000000000000000000000"), 9);
        let Some(CounterPacket::Arena(Snapshot { items, seen_ms: 9 })) = p else { panic!("not read: {p:?}") };
        assert_eq!(
            items,
            vec![
                ArenaRecord { arena: 1, rating: 1_527, wins: 3, losses: 2 },
                ArenaRecord { arena: 2, rating: 1_500, wins: 0, losses: 0 },
                ArenaRecord { arena: 3, rating: 1_500, wins: 0, losses: 0 },
            ]
        );
        assert_eq!(parse(&hex("302ae30301f705000000000000030200000002dc05000000000000000000000003dc0500000000000000000000"), 9), None, "short");
        assert_eq!(parse(&hex("102ae30100ffffffff00000000000000"), 9), None, "arena 0, no rating");
    }

    #[test]
    fn the_weeks_abyss_points_are_gained_and_limit() {
        // 2026-10-17 05:36:08.
        let p = parse(&hex("16a9ff9e8800000000000060e3160000000000"), 9);
        assert_eq!(p, Some(CounterPacket::AbyssPoints(AbyssPoints { gained: 34_974, limit: 1_500_000, seen_ms: 9 })));
        assert_eq!(parse(&hex("0ea9ff9e88000000000000"), 9), None, "too short");
    }

    fn login_packet(fixture: &str, at: i64) -> CounterPacket {
        parse(&hex(fixture), at).unwrap_or_else(|| panic!("not read"))
    }

    #[test]
    fn the_kinah_earned_today_is_the_first_u64_and_the_servers_clock_the_second() {
        // The login of 2026-10-18 11:59:40: 7,507 kinah, as at 05:39 (offline since).
        let p = parse(&hex("16a5ff531d000000000000078f864fa1010000"), 9);
        assert_eq!(p, Some(CounterPacket::KinahToday(KinahToday { earned: 7_507, server_ms: 1_792_335_580_935, seen_ms: 9 })));
        assert_eq!(parse(&hex("0ea5ff531d0000000000"), 9), None, "too short");
        assert_eq!(parse(&hex("16a5ff531d000000000000ffffffffffffffff"), 9), None, "no date");
    }

    #[test]
    fn the_login_progress_records_hold_the_three_attendance_events() {
        let CounterPacket::Progress { records, full: true } = login_packet(include_str!("testdata/progress_login_v1.hex"), 4) else {
            panic!("not progress");
        };
        assert_eq!(records.seen_ms, 4);
        assert_eq!(
            records.items.iter().filter_map(ProgressRecord::attendance).collect::<Vec<_>>(),
            vec![
                // 7 of 7, last marked 2026-10-11: over.
                AttendanceEvent { id: 510_320_100, days: 7, claimed: 7, last_ms: 1_791_733_752_263 },
                // Marked today 07:02:57 UTC, after the 07:00 reset.
                AttendanceEvent { id: 510_330_100, days: 14, claimed: 14, last_ms: 1_792_306_977_387 },
                AttendanceEvent { id: 510_340_100, days: 5, claimed: 5, last_ms: 1_792_306_977_387 },
            ]
        );
    }

    #[test]
    fn progress_records_that_dont_fit_or_hold_no_date_are_not_read() {
        // One record: id 510000001, 2 days, a date, 1 claimed; then a tail.
        let record = "01 81fb651e 02000000 078f864fa1010000 01";
        assert!(matches!(parse(&packet(&format!("8356 {record}")), 0), Some(CounterPacket::Progress { records, full: true }) if records.items.len() == 1));
        assert!(parse(&packet(&format!("8356 {record} 17 65 a3")), 0).is_some(), "what follows isn't read");
        assert_eq!(parse(&packet("8356 02 81fb651e 02000000 078f864fa1010000 01"), 0), None, "says 2, holds 1");
        assert_eq!(parse(&packet("8356 01 81fb651e 02000000 ffffffffffffffff 01"), 0), None, "not a date");
    }

    #[test]
    fn the_resets_are_at_seven_utc_daily_and_on_wednesdays() {
        // 2026-10-18 (a Sunday) 07:00 UTC.
        let reset = 1_792_306_800_000;
        assert_eq!(last_daily_reset(reset), reset);
        assert_eq!(last_daily_reset(reset + 1), reset);
        assert_eq!(last_daily_reset(reset - 1), reset - DAY_MS);
        assert_eq!(last_daily_reset(1_792_353_759_800), reset, "17:02 in Brasília");
        // Wednesday 2026-10-14 07:00 UTC.
        let wednesday = reset - 4 * DAY_MS;
        assert_eq!(last_weekly_reset(reset), wednesday);
        assert_eq!(last_weekly_reset(wednesday), wednesday);
        assert_eq!(last_weekly_reset(wednesday - 1), wednesday - 7 * DAY_MS);
        assert_eq!(last_weekly_reset(wednesday + 7 * DAY_MS), wednesday + 7 * DAY_MS);
    }

    #[test]
    fn the_missions_done_today_are_the_total_less_its_value_at_the_reset() {
        let mut book = CounterBook::default();
        // The login of 11:59:40: 11, the last one done on 2026-10-09.
        book.note(login_packet(include_str!("testdata/progress_login_v1.hex"), 1_792_335_580_037), Some("ELLAO"));
        book.note_self_record(Some("ELLAO"));
        let m = book.tallies[&MISSIONS_DONE];
        assert_eq!((m.total, m.start, m.reset_ms), (11, Some(11), 1_792_306_800_000));
        assert_eq!(book.attendance.as_ref().map(|a| a.items.len()), Some(3), "attendance still read");

        // Done at 17:00:53 and 17:02:39, as the game said them (84 56).
        let live = |body: &str, at| parse(&hex(body), at).unwrap();
        book.note(live("18845601cc2ae3110c000000e73a9a50a101000002", 1_792_353_653_559), Some("ELLAO"));
        book.note(live("18845601cc2ae3110d000000b1ba9b50a101000002", 1_792_353_759_800), Some("ELLAO"));
        let m = book.tallies[&MISSIONS_DONE];
        assert_eq!((m.total, m.start, m.last_ms), (13, Some(11), 1_792_353_745_585));
        // Another record changing is left out; an attendance one is taken in.
        assert_eq!(parse(&hex("18845601787cc60d090000009df1a050a101000000"), 0), None);
        book.note(live("18845601f4046b1e0f000000b1ba9b50a10100000f", 1_792_353_760_000), Some("ELLAO"));
        let a = book.attendance.as_ref().unwrap();
        assert_eq!(a.items.iter().find(|e| e.id == 510_330_100).map(|e| (e.days, e.claimed)), Some((15, 15)));
        assert_eq!((a.items.len(), a.seen_ms), (3, 1_792_353_760_000));

        // A fresh look after a mission today: the day's start isn't known...
        let mut fresh = CounterBook::default();
        fresh.note(live("18845601cc2ae3110d000000b1ba9b50a101000002", 1_792_353_759_800), None);
        assert_eq!(fresh.tallies[&MISSIONS_DONE].start, None);
        // ...unless that day's start was read before.
        let mut kept = book.clone();
        kept.note(live("18845601cc2ae3110e000000b1ba9b50a101000002", 1_792_353_800_000), Some("ELLAO"));
        assert_eq!(kept.tally(MISSIONS_DONE).map(|m| (m.total, m.start)), Some((14, Some(11))));
        // The game left open through the next reset: a mission done after it
        // counts from the total read before it.
        let next_day = 1_792_393_200_000;
        let after_reset = format!("18845601cc2ae3110f000000{}02", hex_u64(next_day as u64 + 600_000));
        kept.note(live(&after_reset, next_day + 600_100), Some("ELLAO"));
        assert_eq!(kept.tally(MISSIONS_DONE).map(|m| (m.total, m.start, m.reset_ms)), Some((15, Some(14), next_day)));
        assert_eq!(parse(&hex("19845601cc2ae3110e000000b1ba9b50a10100000200"), 0), None, "a byte too many");
    }

    fn hex_u64(v: u64) -> String {
        v.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn a_relog_to_another_character_does_not_take_the_first_ones_start() {
        let mut book = CounterBook::default();
        book.note(login_packet(include_str!("testdata/progress_login_v1.hex"), 1_792_335_580_037), Some("ELLAO"));
        book.note_self_record(Some("ELLAO"));
        assert_eq!(book.tallies[&MISSIONS_DONE].start, Some(11));
        // Alt logs in: 40 missions, the last one done today, at 08:00 UTC.
        let today = 1_792_306_800_000_u64 + 3_600_000;
        let alt = format!("8356 02 cc2ae311 28000000 {} 02 e87ac60d c8000000 {} 00", hex_u64(today), hex_u64(today));
        book.note(parse(&packet(&alt), 1_792_336_000_000).unwrap(), Some("ELLAO"));
        book.note_self_record(Some("Alt"));
        assert_eq!(book.owner.as_deref(), Some("Alt"));
        assert_eq!(book.tallies[&MISSIONS_DONE].start, None, "not ELLAO's 11");
        assert_eq!(book.tallies[&ARENA_PLAYED].start, None, "not ELLAO's 5");
    }

    #[test]
    fn the_arena_matches_this_week_count_from_wednesdays_value() {
        let mut book = CounterBook::default();
        // 11:59:40: 5 played, the last on 2026-10-14 before that day's reset.
        book.note(login_packet(include_str!("testdata/progress_login_v1.hex"), 1_792_335_580_037), Some("ELLAO"));
        book.note_self_record(Some("ELLAO"));
        let a = book.tallies[&ARENA_PLAYED];
        assert_eq!((a.total, a.start, a.reset_ms), (5, Some(5), 1_791_961_200_000));
        // A match ended at 17:09:31: three records at once, one of them this.
        for body in [
            "18845601787cc60d0b000000543aa250a101000000",
            "18845601e87ac60d06000000543aa250a101000000",
            "1884560130ffc70d01000000543aa250a101000000",
        ] {
            if let Some(p) = parse(&hex(body), 1_792_354_170_001) {
                book.note(p, Some("ELLAO"));
            }
        }
        let a = book.tallies[&ARENA_PLAYED];
        assert_eq!((a.total, a.start, a.last_ms), (6, Some(5), 1_792_354_171_476));
        assert_eq!(book.tallies.len(), 2, "only the missions and the arena");
    }

    #[test]
    fn the_login_supply_boards_are_a_daily_a_weekly_and_two_open_ended() {
        let CounterPacket::Supply(snapshot) = login_packet(include_str!("testdata/supply_login_v1.hex"), 4) else {
            panic!("not supply");
        };
        let boards: Vec<_> = snapshot.items.iter().map(|b| (b.slot, b.requests, b.start_ms, b.end_ms)).collect();
        assert_eq!(
            boards,
            vec![
                // 07:00 UTC to the next 07:00 UTC: the day's.
                (1, 10, 1_792_306_801_000, 1_792_393_200_999),
                // Wednesday to Wednesday: the week's.
                (2, 10, 1_791_961_201_000, 1_792_566_000_999),
                (3, 54, 1_784_098_801_000, 4_072_575_600_999),
                (4, 54, 1_784_098_801_000, 4_072_575_600_999),
                (5, 0, 0, 0),
                (6, 0, 0, 0),
            ]
        );
        assert!(snapshot.items.iter().all(|b| b.flagged == 0 && b.with_value == 0), "nothing delivered that day");
    }

    #[test]
    fn a_supply_request_naming_another_board_is_not_read() {
        // One board (id 1, slot 1), one request: its id, a request, a value, the flags.
        let board = "01 01000000 01 0000000000000000 0000000000000000 01";
        assert!(matches!(
            parse(&packet(&format!("56e2 {board} 01000000 07000000 05000000 01")), 0),
            Some(CounterPacket::Supply(s)) if (s.items[0].flagged, s.items[0].with_value) == (1, 1)
        ));
        assert_eq!(parse(&packet(&format!("56e2 {board} 02000000 07000000 00000000 00")), 0), None);
        assert_eq!(parse(&packet(&format!("56e2 {board} 01000000 07000000 00000000")), 0), None, "no flags byte");
    }

    #[test]
    fn a_login_switching_character_drops_what_the_one_before_left_and_keeps_its_own() {
        let mut book = CounterBook::default();
        let old = KinahToday { earned: 1, server_ms: 0, seen_ms: 10 };
        book.note(CounterPacket::KinahToday(old), Some("Main"));
        assert_eq!(book.owner.as_deref(), Some("Main"));
        // Alt logs in: progress records, the list, then its kinah, then the self record.
        book.note(login_packet(include_str!("testdata/progress_login_v1.hex"), 100), Some("Main"));
        book.note(CounterPacket::List(login_list()), Some("Main"));
        assert_eq!(book.login_ms, 100, "the records opened the login");
        let new = KinahToday { earned: 7_507, server_ms: 0, seen_ms: 105 };
        book.note(CounterPacket::KinahToday(new), Some("Main"));
        book.note(login_packet(include_str!("testdata/supply_login_v1.hex"), 104), Some("Main"));
        book.note_self_record(Some("Alt"));
        assert_eq!(book.owner.as_deref(), Some("Alt"));
        assert_eq!(book.kinah_today, Some(new));
        assert_eq!(book.attendance.as_ref().map(|a| a.items.len()), Some(3));
        assert_eq!(book.supply.as_ref().map(|s| s.items.len()), Some(6));

        // Back to Main: Alt's kinah from before the login goes.
        book.note(CounterPacket::List(login_list()), Some("Alt"));
        book.note_self_record(Some("Main"));
        assert_eq!((book.kinah_today, book.attendance.is_some(), book.supply.is_some()), (None, false, false));
    }

    #[test]
    fn other_packets_are_not_counters() {
        assert_eq!(parse(&hex("0e0638b115171dd200f700"), 0), None);
        assert_eq!(parse(&hex("03"), 0), None);
    }

    #[test]
    fn a_login_list_waits_for_the_self_record_to_know_whose_it_is() {
        let mut book = CounterBook::default();
        book.note(CounterPacket::AbyssPoints(AbyssPoints { gained: 5, limit: 10, seen_ms: 1 }), Some("Main"));
        assert_eq!(book.owner.as_deref(), Some("Main"), "a session the game had named");
        // A zone load of the same character: its Abyss points stay.
        book.note(CounterPacket::List(login_list()), Some("Main"));
        assert!(book.awaits_self_record);
        book.note_self_record(Some("Main"));
        assert_eq!(book.abyss_points.map(|p| p.gained), Some(5));
        // Switch to another character: the list comes before the self record.
        book.note(CounterPacket::List(login_list()), Some("Main"));
        assert!(book.awaits_self_record);
        let update = Counter { id: ABYSS_TIME, time_ms: 100, ..Counter::default() };
        book.note(CounterPacket::Update { counter: update, reason: 2, delta: None }, Some("Main"));
        assert!(book.awaits_self_record, "still waiting for the self record");
        book.note_self_record(Some("Alt"));
        assert_eq!((book.owner.as_deref(), book.abyss_points), (Some("Alt"), None), "Main's points aren't Alt's");
        assert_eq!(book.get(ABYSS_TIME).unwrap().time_ms, 100);
        // A later self record (a zone load) of the same session changes nothing.
        book.note_self_record(Some("Other"));
        assert_eq!(book.owner.as_deref(), Some("Alt"));
    }

    #[test]
    fn counters_read_before_anyone_is_named_belong_to_the_next_self_record() {
        let mut book = CounterBook::default();
        book.note_self_record(Some("Nobody"));
        assert_eq!(book.owner, None, "nothing to own yet");
        let c = Counter { id: ODYLE_ENERGY, count: 25, ..Counter::default() };
        book.note(CounterPacket::Update { counter: c, reason: 2, delta: None }, None);
        assert_eq!(book.owner, None);
        book.note_self_record(Some("Main"));
        assert_eq!(book.owner.as_deref(), Some("Main"));
    }
}

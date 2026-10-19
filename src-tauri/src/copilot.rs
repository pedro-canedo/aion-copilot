//! aion-copilot: data for the fork's own overlays, kept out of `app.rs` so
//! upstream merges stay small and the logic is testable without Tauri.

use serde::Serialize;

use crate::capture::field_bosses::FIELD_BOSSES;
use crate::combat::data_storage::DataStorage;
use crate::i18n::lookup::NpcLookup;

/// One party member as the Group overlay shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyMemberView {
    pub name: String,
    /// The class as the meter's icon files name it (assets/<job>.png).
    pub job: String,
    pub level: i32,
    pub combat_power: i64,
    pub gear_score: i32,
    pub slot: u8,
    pub hp: Option<i32>,
    pub dead: bool,
    pub is_local: bool,
}

/// Your party as the roster packet states it, in slot order, with whether
/// each member is down (marked dead, or last HP seen at 0) and which one is you.
pub fn party_view(storage: &DataStorage) -> Vec<PartyMemberView> {
    let local = storage.local_player_id().map(|v| v as i32);
    let mut members: Vec<PartyMemberView> = storage
        .get_party_members()
        .into_iter()
        .map(|(name, m)| {
            let entity = storage.find_id_by_nickname(&name);
            let hp = entity.and_then(|id| storage.last_hp_of(id));
            PartyMemberView {
                job: m.job.map(|j| j.class_name().to_string()).unwrap_or_default(),
                level: m.level,
                combat_power: m.combat_power,
                gear_score: m.gear_score,
                slot: m.slot,
                dead: entity.is_some_and(|id| storage.is_entity_dead(id)) || hp == Some(0),
                is_local: entity.is_some() && entity == local,
                hp,
                name,
            }
        })
        .collect();
    members.sort_by_key(|m| m.slot);
    members
}

/// One field boss as the Bosses overlay shows it, from the last time its map
/// was opened in the game (`capture::boss_map`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldBossView {
    /// The game's WorldMapFieldNamed id.
    pub id: u32,
    pub name: String,
    pub map_id: u32,
    pub up: bool,
    /// Unix ms: since when it is up, or when it comes back; 0 when unknown.
    pub time_ms: i64,
    /// When the map was read (unix ms).
    pub seen_ms: i64,
}

/// Every field boss on the maps opened so far: the soonest back first, then
/// the ones up.
pub fn field_bosses(storage: &DataStorage, npcs: &NpcLookup) -> Vec<FieldBossView> {
    let maps = storage.boss_maps_since(u64::MAX).map(|(_, maps)| maps).unwrap_or_default();
    let mut out: Vec<FieldBossView> = maps
        .iter()
        .flat_map(|map| {
            map.bosses.iter().map(move |boss| {
                let name = FIELD_BOSSES
                    .iter()
                    .find(|(_, id)| *id == boss.id)
                    .map(|(code, _)| npcs.get_npc_name(*code))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("Chefe de campo #{}", boss.id));
                FieldBossView {
                    id: boss.id,
                    name,
                    map_id: map.map_id,
                    up: boss.up,
                    time_ms: boss.time_ms,
                    seen_ms: map.seen_ms,
                }
            })
        })
        .collect();
    // Dead ones with a known return first (soonest first), then unknown, then up.
    out.sort_by_key(|b| (b.up, b.time_ms == 0, b.time_ms, b.id));
    out
}

/// The names learned in this game session, saved so a restarted app keeps
/// them: the game only sends players' names on a loading screen, and entity
/// ids stay valid while the same game process runs.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NameSession {
    pub game_pid: u32,
    pub local_player_id: Option<i64>,
    pub names: std::collections::HashMap<i32, String>,
    /// Combat powers seen this session, by name (players' records come only
    /// as they come into view).
    #[serde(default)]
    pub combat_powers: std::collections::HashMap<String, i64>,
}

pub fn snapshot_names(storage: &DataStorage, game_pid: u32) -> NameSession {
    NameSession {
        game_pid,
        local_player_id: storage.local_player_id(),
        names: storage.get_nicknames(),
        combat_powers: storage.known_combat_powers(),
    }
}

/// Put back what a session saved, when it is the same game process: names the
/// app does not know yet, and who you are if it does not know that either.
/// Returns how many names came back.
pub fn restore_names(storage: &DataStorage, session: &NameSession, game_pid: u32) -> usize {
    if session.game_pid != game_pid {
        return 0;
    }
    if storage.local_player_id().is_none() {
        if let Some(id) = session.local_player_id {
            storage.set_local_player_id(Some(id));
        }
    }
    for (name, &cp) in &session.combat_powers {
        if storage.known_combat_power(name).is_none() {
            storage.note_known_combat_power(name, cp);
        }
    }
    let known = storage.get_nicknames();
    let mut restored = 0;
    for (&id, name) in &session.names {
        if !known.contains_key(&id) && !name.is_empty() {
            storage.append_nickname_authoritative(id, name);
            restored += 1;
        }
    }
    restored
}

#[cfg(windows)]
pub fn game_pid() -> Option<u32> {
    crate::platform::window_detector::aion2_pid()
}

#[cfg(not(windows))]
pub fn game_pid() -> Option<u32> {
    None
}

/// What kind of target you hit last, for the meter's tabs to follow it:
/// "worldBoss", "dungeonBoss", "boss" (another), "dungeon" (a monster in an
/// instance), "train" (a training dummy), "mob", "pvp", or "none".
pub fn last_target_kind(storage: &DataStorage, npcs: &NpcLookup) -> &'static str {
    let Some(local) = storage.local_player_id().map(|v| v as i32) else { return "none" };
    let summons = storage.get_summon_data();
    let mobs = storage.get_mob_data();
    let mut best: Option<(i32, i64)> = None;
    let mut seen_damage = 0i64;
    for (&target, data) in &storage.get_combat_snapshot_light() {
        for (&actor, actor_data) in &data.actors {
            if crate::entity::summon_resolver::resolve(actor, &summons) == local
                && best.is_none_or(|(_, t)| actor_data.last_damage_time > t)
            {
                best = Some((target, actor_data.last_damage_time));
                seen_damage = data.total_damage as i64;
            }
        }
    }
    // A player hit since: PvP (their hits on you read as heals, so they
    // never become a meter target).
    if let Some((player, at)) = storage.last_player_hit_by(local) {
        let party = storage.get_party_members();
        let ours = storage.get_nickname(player).is_some_and(|n| party.contains_key(n.trim()));
        if !ours && best.is_none_or(|(_, t)| at > t) {
            return "pvp";
        }
    }
    let Some((target, _)) = best else { return "none" };
    let code = mobs.get(&target).copied();
    let dummy = match code {
        Some(code) => npcs.is_training_dummy(code),
        None => storage.is_hp_reset_dummy(target),
    };
    let instance = storage.current_dungeon_id() != 0;
    // A crowd on a big open-world target the meter has no code for (it came
    // after the spawn): it lost far more than the hits seen. See
    // `DpsCalculator::world_boss_rows`; the Boss view needs the code.
    let crowd = !instance && code.is_none() && {
        let max = storage.get_mob_hp(target).unwrap_or(0) as i64;
        let cur = storage.get_mob_current_hp(target).map_or(-1, |h| h as i64);
        let lost = max - cur;
        max >= 2_000_000 && cur >= 0 && lost > 2 * seen_damage && lost - seen_damage > 1_000_000
    };
    if dummy {
        "train"
    } else if crowd {
        "crowdBoss"
    } else if code.is_some_and(crate::capture::field_bosses::is_field_boss) && !instance {
        "worldBoss"
    } else if storage.is_boss(target) || code.is_some_and(|c| npcs.is_boss(c)) {
        if instance { "dungeonBoss" } else { "boss" }
    } else if instance {
        "dungeon"
    } else {
        "mob"
    }
}

pub use crate::clock::now_ms;

/// A death settles this long before it counts: a misread HP of 0 is gone by then.
pub const PVP_SETTLE_MS: i64 = 3_000;
/// The killing blow is the last hit within this long before the death.
pub const PVP_KILL_WINDOW_MS: i64 = 10_000;
/// A monster that hit you this close to your death killed you, not a player
/// (a stranger's heal reads as a hit from them).
pub const PVP_NPC_GRACE_MS: i64 = 3_000;

/// One player in your PvP record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpOpponent {
    pub name: String,
    #[serde(default)]
    pub job: String,
    pub kills: u32,
    pub deaths: u32,
    pub last_ms: i64,
    #[serde(default)]
    pub events: Vec<PvpEvent>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpEvent {
    pub at_ms: i64,
    /// You killed them (else they killed you).
    pub kill: bool,
    /// Which of your characters.
    #[serde(default)]
    pub character: String,
}

/// A kill or death between you and another player.
#[derive(Debug, Clone, PartialEq)]
pub struct PvpOutcome {
    pub opponent: i32,
    pub name: String,
    pub kill: bool,
    pub at_ms: i64,
}

/// Where the last look at the deaths stopped, and whom it took for you.
#[derive(Debug, Default)]
pub struct PvpTracker {
    pub from_ms: i64,
    pub local: Option<i32>,
}

/// How far back a look goes when the game names you anew.
pub const PVP_RENAMED_LOOKBACK_MS: i64 = 120_000;

/// The settled deaths since the last look that were PvP: yours to another
/// player's last hit, or another player's to yours. Party members are never
/// opponents.
pub fn pvp_outcomes(storage: &DataStorage, tracker: &mut PvpTracker, now: i64) -> Vec<PvpOutcome> {
    let until = now - PVP_SETTLE_MS;
    let from = tracker.from_ms;
    if until < from {
        return Vec::new();
    }
    tracker.from_ms = until + 1;
    let Some(local) = storage.local_player_id().map(|v| v as i32) else { return Vec::new() };
    // The game gives you a new id in an instance (an arena) and may say so
    // only after a death there (2026-10-18: died at 17:08:05, named 17:08:38):
    // deaths read under that id before it was yours are looked at again. Only
    // those that involve the new id count, so none is counted twice.
    let from = if tracker.local.is_some_and(|l| l != local) { from - PVP_RENAMED_LOOKBACK_MS } else { from };
    tracker.local = Some(local);
    let party = storage.get_party_members();
    let named = |x: i32| {
        storage.get_nickname(x).map(|n| n.trim().to_string()).filter(|n| !n.is_empty() && !party.contains_key(n))
    };
    let mut out = Vec::new();
    for (id, at, _) in storage.deaths_between(from, until) {
        let Some((killer, hit_at, npc_at)) = storage.killer_of(id, at) else { continue };
        if at - hit_at > PVP_KILL_WINDOW_MS || at - npc_at <= PVP_NPC_GRACE_MS || killer == id {
            continue;
        }
        let (opponent, kill) = if id == local {
            (killer, false)
        } else if killer == local {
            (id, true)
        } else {
            continue;
        };
        if !storage.is_known_player(opponent) || storage.mob_code(opponent).is_some() {
            continue;
        }
        if let Some(name) = named(opponent) {
            out.push(PvpOutcome { opponent, name, kill, at_ms: at });
        }
    }
    out
}

pub fn load_pvp(path: &std::path::Path) -> Vec<PvpOpponent> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Add `outcomes` to the record at `path`, the latest opponent first.
pub fn record_pvp(storage: &DataStorage, path: &std::path::Path, outcomes: &[PvpOutcome]) {
    if outcomes.is_empty() {
        return;
    }
    let mut list = load_pvp(path);
    let me = storage.local_character_name().unwrap_or_default();
    for o in outcomes {
        let pos = match list.iter().position(|p| p.name == o.name) {
            Some(pos) => pos,
            None => {
                list.push(PvpOpponent { name: o.name.clone(), ..Default::default() });
                list.len() - 1
            }
        };
        let entry = &mut list[pos];
        if let Some(job) = storage.actor_job_of(o.opponent) {
            entry.job = job.class_name().to_string();
        }
        if o.kill {
            entry.kills += 1;
        } else {
            entry.deaths += 1;
        }
        entry.last_ms = entry.last_ms.max(o.at_ms);
        entry.events.push(PvpEvent { at_ms: o.at_ms, kill: o.kill, character: me.clone() });
        if entry.events.len() > 100 {
            entry.events.remove(0);
        }
    }
    list.sort_by(|a, b| b.last_ms.cmp(&a.last_ms));
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(path, json);
    }
}

/// The player you hit last, as the Target overlay shows them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpTargetView {
    pub id: i32,
    pub name: String,
    pub job: String,
    pub gear_score: i32,
    pub combat_power: i64,
    pub hp: Option<i32>,
    pub last_hit_ms: i64,
    pub kills: u32,
    pub deaths: u32,
}

pub fn pvp_target(storage: &DataStorage, record: &[PvpOpponent]) -> Option<PvpTargetView> {
    let local = storage.local_player_id()? as i32;
    let (id, at) = storage.last_player_hit_by(local)?;
    let name = storage.get_nickname(id).map(|n| n.trim().to_string()).filter(|n| !n.is_empty())?;
    if storage.get_party_members().contains_key(&name) {
        return None;
    }
    let rec = record.iter().find(|p| p.name == name);
    Some(PvpTargetView {
        id,
        job: storage.actor_job_of(id).map(|j| j.class_name().to_string()).unwrap_or_default(),
        // The spawn's "gear score" never matched: only the roster's.
        gear_score: storage.get_party_members().get(&name).map_or(0, |m| m.gear_score),
        combat_power: storage.known_combat_power(&name).unwrap_or(0),
        hp: storage.last_hp_of(id),
        last_hit_ms: at,
        kills: rec.map_or(0, |r| r.kills),
        deaths: rec.map_or(0, |r| r.deaths),
        name,
    })
}


/// A character you played while the app was open (the Painel's list).
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSeen {
    pub name: String,
    /// The class as the icon files name it; empty until known.
    pub job: String,
    /// From the party roster: 0 when you were never in a party.
    pub combat_power: i64,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
}

pub fn load_characters(path: &std::path::Path) -> Vec<CharacterSeen> {
    let mut list: Vec<CharacterSeen> = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    list.sort_by_key(|c| std::cmp::Reverse(c.last_seen_ms));
    list
}

/// Note who you are playing now, if the game has said: name, class (from
/// your hits or the party roster) and combat power (party roster only).
pub fn note_character(storage: &DataStorage, path: &std::path::Path, now: i64) -> Option<CharacterSeen> {
    let local = storage.local_player_id()? as i32;
    let name = storage.get_nickname(local).filter(|n| !n.is_empty())?;
    let roster = storage.get_party_members();
    let member = roster.get(&name);
    let job = member
        .and_then(|m| m.job)
        .or_else(|| {
            storage
                .get_combat_snapshot_light()
                .values()
                .find_map(|t| t.actors.get(&local).and_then(|a| a.job))
        })
        .map(|j| j.class_name().to_string());
    let mut list = load_characters(path);
    let entry = match list.iter_mut().find(|c| c.name == name) {
        Some(c) => c,
        None => {
            list.push(CharacterSeen { name: name.clone(), job: String::new(), combat_power: 0, first_seen_ms: now, last_seen_ms: now });
            list.last_mut().unwrap()
        }
    };
    entry.last_seen_ms = now;
    if let Some(job) = job {
        entry.job = job;
    }
    if let Some(cp) = member.map(|m| m.combat_power).filter(|&cp| cp > 0).or_else(|| storage.known_combat_power(&name)) {
        entry.combat_power = cp;
    }
    let seen = entry.clone();
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(path, json);
    }
    Some(seen)
}

/// You, known by id but not yet by name (the app opened after the loading
/// screen that names you): the character you played last of the same class.
/// A real name from the game later replaces it. Returns the name used.
pub fn name_local_from_characters(storage: &DataStorage, characters: &[CharacterSeen]) -> Option<String> {
    let local = storage.local_player_id()? as i32;
    if storage.get_nickname(local).is_some_and(|n| !n.is_empty()) {
        return None;
    }
    let job = storage
        .get_combat_snapshot_light()
        .values()
        .find_map(|t| t.actors.get(&local).and_then(|a| a.job))
        .map(|j| j.class_name().to_string())?;
    let best = characters.iter().filter(|c| c.job == job).max_by_key(|c| c.last_seen_ms)?;
    storage.append_nickname_authoritative(local, &best.name);
    Some(best.name.clone())
}

/// Where you are: "dungeon", "abyss", "world", or "none" before a map load.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneView {
    pub kind: &'static str,
    pub label: &'static str,
    pub map_id: i32,
}

/// Monsters around with Abyss in their name that make a map the Abyss.
pub const ABYSS_NPCS_NEEDED: usize = 3;

/// Your zone. A map is the Abyss once `abyss_maps` has it, or once enough
/// monsters around it are of the Abyss (then its id is for the caller to keep,
/// unless it is a land world's: the load into the Abyss may name Verteron,
/// and a kept 1010 would make all of Verteron the Abyss).
pub fn zone_view(storage: &DataStorage, npcs: &NpcLookup, abyss_maps: &std::collections::HashSet<i32>) -> ZoneView {
    let map_id = storage.current_map_id().unwrap_or(0);
    if storage.current_dungeon_id() != 0 {
        return ZoneView { kind: "dungeon", label: "Expedição", map_id };
    }
    let abyss = (abyss_maps.contains(&map_id) && !crate::copilot_map::is_land_world(map_id)) || {
        let named = storage
            .get_mob_data()
            .values()
            .filter(|&&code| {
                let name = npcs.get_npc_name(code).to_lowercase();
                name.contains("abis") || name.contains("abyss")
            })
            .count();
        named >= ABYSS_NPCS_NEEDED
    };
    if abyss {
        ZoneView { kind: "abyss", label: "Abismo", map_id }
    } else if map_id == 0 {
        // Opened after the last loading screen: no map yet.
        ZoneView { kind: "none", label: "", map_id }
    } else {
        ZoneView { kind: "world", label: "Mundo", map_id }
    }
}

/// One player on the PvP board.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpBoardRow {
    pub id: i32,
    pub name: String,
    pub job: String,
    pub combat_power: i64,
    pub gear_score: i32,
    /// On your side: damage to opponents; an opponent: damage to you.
    pub damage: i64,
    /// Times they died this session (an opponent: to you).
    pub deaths: u32,
    pub is_you: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PvpBoard {
    pub ours: Vec<PvpBoardRow>,
    pub opponents: Vec<PvpBoardRow>,
    pub ours_total: i64,
    pub opponents_total: i64,
    pub first_ms: i64,
    pub last_ms: i64,
    /// This session's kills and deaths (from the record).
    pub kills: u32,
    pub deaths: u32,
}

/// You and your party against the players you fought: damage each side did
/// to the other (as the meter has it now), and this session's kills.
pub fn pvp_board(storage: &DataStorage, record: &[PvpOpponent], since_ms: i64) -> PvpBoard {
    let Some(local) = storage.local_player_id().map(|v| v as i32) else { return PvpBoard::default() };
    let party = storage.get_party_members();
    let names = storage.get_nicknames();
    let ours = |id: i32| id == local || names.get(&id).is_some_and(|n| party.contains_key(n.trim()));
    let targets = storage.pvp_target_ids();
    let fights = storage.get_combat_snapshot_light();
    let mut board = PvpBoard::default();
    let mut ours_dmg: std::collections::HashMap<i32, i64> = std::collections::HashMap::new();
    let mut opp_dmg: std::collections::HashMap<i32, i64> = std::collections::HashMap::new();
    for (&target, fight) in &fights {
        let on_opponent = targets.contains(&target) && !ours(target);
        let on_us = ours(target);
        if !on_opponent && !on_us {
            continue;
        }
        for (&actor, a) in &fight.actors {
            if on_opponent && ours(actor) {
                *ours_dmg.entry(actor).or_default() += a.total_damage;
                opp_dmg.entry(target).or_default();
            } else if on_us && !ours(actor) && storage.is_known_player(actor) {
                *opp_dmg.entry(actor).or_default() += a.total_damage;
            } else {
                continue;
            }
            if board.first_ms == 0 || fight.first_damage_time < board.first_ms {
                board.first_ms = fight.first_damage_time;
            }
            board.last_ms = board.last_ms.max(fight.last_damage_time);
        }
    }
    let session_deaths = |name: &str, kill: bool| -> u32 {
        record.iter().find(|p| p.name == name).map_or(0, |p| {
            p.events.iter().filter(|e| e.kill == kill && e.at_ms >= since_ms).count() as u32
        })
    };
    let row = |id: i32, damage: i64| {
        let name = names.get(&id).cloned().unwrap_or_else(|| format!("#{id}"));
        PvpBoardRow {
            id,
            job: storage.actor_job_of(id).map(|j| j.class_name().to_string()).unwrap_or_default(),
            combat_power: storage.known_combat_power(&name).unwrap_or(0),
            gear_score: 0,
            damage,
            deaths: if id == local { 0 } else { session_deaths(&name, true) },
            is_you: id == local,
            name,
        }
    };
    if !ours_dmg.is_empty() || !opp_dmg.is_empty() {
        ours_dmg.entry(local).or_default();
    }
    board.ours = ours_dmg.iter().map(|(&id, &d)| row(id, d)).collect();
    board.opponents = opp_dmg.iter().map(|(&id, &d)| row(id, d)).collect();
    board.ours.sort_by(|a, b| b.damage.cmp(&a.damage));
    board.opponents.sort_by(|a, b| b.damage.cmp(&a.damage).then(a.name.cmp(&b.name)));
    board.ours_total = board.ours.iter().map(|r| r.damage).sum();
    board.opponents_total = board.opponents.iter().map(|r| r.damage).sum();
    if let Some(you) = board.ours.iter_mut().find(|r| r.is_you) {
        you.deaths = record.iter().flat_map(|p| &p.events).filter(|e| !e.kill && e.at_ms >= since_ms).count() as u32;
    }
    board.kills = record.iter().flat_map(|p| &p.events).filter(|e| e.kill && e.at_ms >= since_ms).count() as u32;
    board.deaths = record.iter().flat_map(|p| &p.events).filter(|e| !e.kill && e.at_ms >= since_ms).count() as u32;
    board
}

/// The Timers overlay's schedule (`src/data/copilot_timers.json`): the
/// game's recurring events, each in the clock its times are known in. Kept in
/// the binary, like the other small tables in `src/data`, and handed to the
/// page as it is; the next start of each is worked out there (`timersCore.js`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSchedule {
    /// The server region the times are for, as the overlay titles it.
    pub region: String,
    pub rules: Vec<TimerRule>,
}

/// One recurring event.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerRule {
    pub id: String,
    /// Shown as is (pt-BR).
    pub name: String,
    /// Which drawing the overlay uses for it.
    pub icon: String,
    pub recurrence: TimerRecurrence,
    /// The rule's clock: minutes east of UTC (South America: -180).
    pub utc_offset_min: i32,
    /// How long entry stays open after the start, when it closes before the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_min: Option<u32>,
    /// How long it is on after the start.
    pub duration_min: u32,
    /// "high", "medium" or "low" ("aprox." on screen).
    pub confidence: String,
}

/// When an event starts, in its rule's clock.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TimerRecurrence {
    /// Every hour at `minute`.
    Hourly { minute: u32 },
    /// Every `every` hours (a divisor of 24) counted from `anchorHour`, at `minute`.
    #[serde(rename_all = "camelCase")]
    EveryHours { every: u32, anchor_hour: u32, minute: u32 },
    /// Every day at `hour:minute`.
    Daily { hour: u32, minute: u32 },
    /// On `days` (0 = Sunday … 6 = Saturday) at `hour:minute`.
    Weekly { days: Vec<u32>, hour: u32, minute: u32 },
}

/// The schedule shipped with the app; empty if the table does not parse
/// (a test keeps it parsing).
pub fn timer_schedule() -> TimerSchedule {
    serde_json::from_str(include_str!("../../src/data/copilot_timers.json")).unwrap_or_default()
}

// ---------- Routine (filled in from the game's counters) ----------

/// Odyle energy's cap, basic energy (560 without a membership, which the
/// packets don't tell apart).
pub const ODYLE_MAX: u32 = 840;
/// Time in the Abyss's cap: 7 h.
pub const ABYSS_TIME_CAP_MS: u64 = 25_200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OdyleView {
    pub basic: u32,
    pub charged: u32,
    pub max: u32,
    pub seen_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbyssTimeView {
    /// As the game last said: it may have gone down since.
    pub remaining_ms: u64,
    pub charged_ms: u64,
    pub cap_ms: u64,
    pub seen_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbyssPointsView {
    /// This week (the game only says when you gain some).
    pub gained: u64,
    pub limit: u64,
    pub seen_ms: i64,
}

/// Kinah earned today, as the game said at login (its daily limit isn't sent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KinahView {
    pub earned: u64,
    pub seen_ms: i64,
}

/// One attendance event: days attended, days rewarded, when last marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceEventView {
    pub id: u32,
    pub days: u32,
    pub claimed: u32,
    pub last_ms: i64,
}

/// The attendance events, as the game said at login: marked today when one's
/// `last_ms` is past the daily reset (worked out by the page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceView {
    pub events: Vec<AttendanceEventView>,
    pub seen_ms: i64,
}

/// One arena: 1 solo, 2 cooperation, 3 tactics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaRecordView {
    pub arena: u8,
    pub rating: u32,
    pub wins: u32,
    pub losses: u32,
}

/// The arenas' ratings, wins and losses, as the game said at login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaView {
    pub records: Vec<ArenaRecordView>,
    pub seen_ms: i64,
}

/// A running total the game keeps (missions done, arena matches played),
/// and its value at the last reset when known: what was done since is
/// `total - start` (worked out by the page against its own clock).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TallyView {
    pub total: u32,
    /// When it last moved (unix ms; 0 never).
    pub last_ms: i64,
    pub seen_ms: i64,
    /// The reset `start` is for.
    pub reset_ms: i64,
    pub start: Option<u32>,
}

impl TallyView {
    fn from(t: &crate::capture::counters::Tally) -> Self {
        TallyView { total: t.total, last_ms: t.last_ms, seen_ms: t.seen_ms, reset_ms: t.reset_ms, start: t.start }
    }
}

/// What the Routine fills in by itself, for one character; also what is
/// kept of it per character, to show before the game sends it again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineAuto {
    #[serde(default)]
    pub odyle: Option<OdyleView>,
    #[serde(default)]
    pub abyss_time: Option<AbyssTimeView>,
    #[serde(default)]
    pub abyss_points: Option<AbyssPointsView>,
    #[serde(default)]
    pub kinah: Option<KinahView>,
    #[serde(default)]
    pub attendance: Option<AttendanceView>,
    #[serde(default)]
    pub arena: Option<ArenaView>,
    /// Missions completed (per day).
    #[serde(default)]
    pub missions: Option<TallyView>,
    /// Arena matches played (per week).
    #[serde(default)]
    pub arena_played: Option<TallyView>,
}

impl RoutineAuto {
    /// From the counters the game sent.
    fn from_book(book: &crate::capture::counters::CounterBook) -> Self {
        use crate::capture::counters::{ABYSS_TIME, ARENA_PLAYED, MISSIONS_DONE, ODYLE_ENERGY};
        RoutineAuto {
            odyle: book
                .get(ODYLE_ENERGY)
                .map(|c| OdyleView { basic: c.count, charged: c.charged, max: ODYLE_MAX, seen_ms: c.seen_ms }),
            abyss_time: book.get(ABYSS_TIME).map(|c| AbyssTimeView {
                remaining_ms: c.time_ms,
                charged_ms: c.charged_ms,
                cap_ms: ABYSS_TIME_CAP_MS,
                seen_ms: c.seen_ms,
            }),
            abyss_points: book
                .abyss_points
                .map(|p| AbyssPointsView { gained: p.gained, limit: p.limit, seen_ms: p.seen_ms }),
            kinah: book.kinah_today.map(|k| KinahView { earned: k.earned, seen_ms: k.seen_ms }),
            // A login without attendance events leaves none to show.
            attendance: book.attendance.as_ref().filter(|a| !a.items.is_empty()).map(|a| AttendanceView {
                events: a
                    .items
                    .iter()
                    .map(|e| AttendanceEventView { id: e.id, days: e.days, claimed: e.claimed, last_ms: e.last_ms })
                    .collect(),
                seen_ms: a.seen_ms,
            }),
            arena: book.arena.as_ref().map(|a| ArenaView {
                records: a
                    .items
                    .iter()
                    .map(|r| ArenaRecordView { arena: r.arena, rating: r.rating, wins: r.wins, losses: r.losses })
                    .collect(),
                seen_ms: a.seen_ms,
            }),
            missions: book.tally(MISSIONS_DONE).map(TallyView::from),
            arena_played: book.tally(ARENA_PLAYED).map(TallyView::from),
        }
    }

    fn is_empty(&self) -> bool {
        self.odyle.is_none()
            && self.abyss_time.is_none()
            && self.abyss_points.is_none()
            && self.kinah.is_none()
            && self.attendance.is_none()
            && self.arena.is_none()
            && self.missions.is_none()
            && self.arena_played.is_none()
    }

    /// Each item from whichever of the two was read later.
    fn merged(&self, other: &RoutineAuto) -> RoutineAuto {
        fn later<T: Clone>(a: &Option<T>, b: &Option<T>, seen: impl Fn(&T) -> i64) -> Option<T> {
            match (a, b) {
                (Some(a), Some(b)) => Some(if seen(b) >= seen(a) { b.clone() } else { a.clone() }),
                (a, b) => a.clone().or_else(|| b.clone()),
            }
        }
        RoutineAuto {
            odyle: later(&self.odyle, &other.odyle, |v| v.seen_ms),
            abyss_time: later(&self.abyss_time, &other.abyss_time, |v| v.seen_ms),
            abyss_points: later(&self.abyss_points, &other.abyss_points, |v| v.seen_ms),
            kinah: later(&self.kinah, &other.kinah, |v| v.seen_ms),
            attendance: later(&self.attendance, &other.attendance, |v| v.seen_ms),
            arena: later(&self.arena, &other.arena, |v| v.seen_ms),
            missions: later_tally(&self.missions, &other.missions),
            arena_played: later_tally(&self.arena_played, &other.arena_played),
        }
    }
}

/// The later of two tallies; where it doesn't know its reset's start and
/// the other read it for the same reset (an earlier login that day), that.
fn later_tally(a: &Option<TallyView>, b: &Option<TallyView>) -> Option<TallyView> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let (mut new, old) = if b.seen_ms >= a.seen_ms { (*b, a) } else { (*a, b) };
            if new.start.is_none() && old.reset_ms == new.reset_ms {
                new.start = old.start;
            }
            Some(new)
        }
        (a, b) => a.or(*b),
    }
}

impl RoutineAuto {
}

/// `copilot-routine-state.json`: the last of it per character name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineFile {
    /// The character seen last, to show when the game hasn't said who you are.
    #[serde(default)]
    pub last: Option<String>,
    #[serde(default)]
    pub characters: std::collections::BTreeMap<String, RoutineAuto>,
}

/// A counter as the game sent it, for developer mode only: what most ids
/// are isn't confirmed, so they are never shown to players.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCounter {
    pub id: u32,
    pub count: u32,
    pub charged: u32,
    pub time_ms: u64,
    pub charged_ms: u64,
    pub seen_ms: i64,
}

/// A supply board as the game sent it, for developer mode only: which field
/// says a request was delivered isn't confirmed (`capture::counters`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSupplyBoard {
    pub id: u32,
    pub slot: u8,
    pub start_ms: i64,
    pub end_ms: i64,
    pub requests: u32,
    pub flagged: u32,
    pub with_value: u32,
    pub seen_ms: i64,
}

/// The Routine's automatic part, for the overlay and the Painel.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineStateView {
    /// Whose it is; None before any character was seen.
    pub name: Option<String>,
    #[serde(flatten)]
    pub auto: RoutineAuto,
    /// Whether any of it came from the game this session (else remembered).
    pub live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_counters: Option<Vec<DevCounter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_supply: Option<Vec<DevSupplyBoard>>,
}

pub fn load_routine_file(path: &std::path::Path) -> RoutineFile {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The Routine now: what the game sent this session over what was kept for
/// the same character. While a login's list waits for the self record that
/// says whose it is, only what was kept shows; read before the game said who
/// you are (the app opened mid-session), it shows alone, under no name, never
/// mixed with a guessed character's. `game_running`: "ao vivo" only while the
/// game is open, else it reads as kept.
pub fn routine_state(storage: &DataStorage, file: &RoutineFile, dev_mode: bool, game_running: bool) -> RoutineStateView {
    let book = storage.counter_book();
    let owner = book.owner.clone().filter(|_| !book.awaits_self_record);
    let live = RoutineAuto::from_book(&book);
    let unowned = !book.awaits_self_record && owner.is_none() && !live.is_empty();
    let name = if unowned {
        None
    } else {
        owner
            .clone()
            .or_else(|| storage.local_character_name().filter(|n| !n.is_empty()))
            .or_else(|| file.last.clone())
    };
    let kept = name.as_ref().and_then(|n| file.characters.get(n)).cloned().unwrap_or_default();
    let ours = owner.is_some() && owner == name && !live.is_empty();
    let dev_counters = dev_mode.then(|| {
        book.counters
            .values()
            .map(|c| DevCounter {
                id: c.id,
                count: c.count,
                charged: c.charged,
                time_ms: c.time_ms,
                charged_ms: c.charged_ms,
                seen_ms: c.seen_ms,
            })
            .collect()
    });
    let dev_supply = dev_mode.then(|| {
        book.supply
            .iter()
            .flat_map(|s| {
                s.items.iter().filter(|b| b.requests > 0).map(|b| DevSupplyBoard {
                    id: b.id,
                    slot: b.slot,
                    start_ms: b.start_ms,
                    end_ms: b.end_ms,
                    requests: b.requests,
                    flagged: b.flagged,
                    with_value: b.with_value,
                    seen_ms: s.seen_ms,
                })
            })
            .collect()
    });
    RoutineStateView {
        name,
        auto: if ours { kept.merged(&live) } else if unowned { live } else { kept },
        live: (ours || unowned) && game_running,
        dev_counters,
        dev_supply,
    }
}

/// Keep what the game sent for the character it belongs to, once the game
/// has said whose. Returns whether the file changed.
pub fn remember_routine(storage: &DataStorage, path: &std::path::Path) -> bool {
    let book = storage.counter_book();
    let Some(owner) = book.owner.clone().filter(|n| !n.is_empty() && !book.awaits_self_record) else {
        return false;
    };
    let live = RoutineAuto::from_book(&book);
    if live.is_empty() {
        return false;
    }
    let mut file = load_routine_file(path);
    let merged = file.characters.get(&owner).cloned().unwrap_or_default().merged(&live);
    if file.characters.get(&owner) == Some(&merged) && file.last.as_ref() == Some(&owner) {
        return false;
    }
    file.characters.insert(owner.clone(), merged);
    file.last = Some(owner);
    match serde_json::to_string_pretty(&file) {
        Ok(json) => crate::atomic_file::write(path, json.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::data_storage::PartyMember;
    use crate::entity::job_class::JobClass;

    #[test]
    fn a_land_world_kept_as_the_abyss_does_not_make_it_the_abyss() {
        let storage = DataStorage::new();
        storage.note_map_load(1010);
        let npcs = NpcLookup::new();
        let kept: std::collections::HashSet<i32> = [1010, 77].into_iter().collect();
        assert_eq!(zone_view(&storage, &npcs, &kept).kind, "world");
        // Another map kept as the Abyss still is.
        storage.note_map_load(77);
        assert_eq!(zone_view(&storage, &npcs, &kept).kind, "abyss");
    }

    fn pvp_hit(actor: i32, target: i32) -> crate::entity::damage_packet::ParsedDamagePacket {
        let mut p = crate::entity::damage_packet::ParsedDamagePacket::new();
        p.set_actor_id(actor);
        p.set_target_id(target);
        p.set_skill_code(11_010_000);
        p.set_damage(500);
        p.set_timestamp(crate::clock::now_ms());
        p
    }

    /// A real capture in front of the training dummies (COPILOT_REPLAY=<replay
    /// file>; skipped without it): the target reads as training, each dummy
    /// keeps its own count, you never die, and you have your combat power.
    #[test]
    fn a_training_dummy_capture_reads_as_training() {
        let Ok(path) = std::env::var("COPILOT_REPLAY") else { return };
        use crate::capture::stream_processor::StreamProcessor;
        use crate::i18n::lookup::SkillLookup;
        use std::sync::Arc;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/data/i18n/npcs");
        let npcs = Arc::new(NpcLookup::new());
        npcs.load_from_json(&std::fs::read_to_string(dir.join("pt.json")).unwrap());
        let storage = Arc::new(DataStorage::new());
        let mut processor = StreamProcessor::new(storage.clone(), Arc::new(SkillLookup::new()), npcs.clone());
        let text = std::fs::read_to_string(&path).unwrap();
        let mut last = 0;
        for line in text.lines() {
            let parts: Vec<&str> = line.trim().splitn(3, '|').collect();
            if parts.len() != 3 {
                continue;
            }
            if let Ok(ts) = parts[0].parse::<i64>() {
                processor.set_override_timestamp(Some(ts));
                crate::clock::set_override(Some(ts));
                last = ts;
            }
            let hex = parts[2].trim();
            let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
            processor.consume_stream(&bytes);
        }
        let local = storage.local_player_id().expect("the self record names you") as i32;
        let mobs = storage.get_mob_data();
        let dummies: Vec<(i32, i64)> = storage
            .get_combat_snapshot_light()
            .iter()
            .filter(|&(&t, _)| match mobs.get(&t) {
                Some(&c) => npcs.is_training_dummy(c),
                // Spawned before the capture began: known by its HP coming back.
                None => storage.is_hp_reset_dummy(t),
            })
            .filter_map(|(&t, f)| f.actors.get(&local).map(|a| (t, a.total_damage)))
            .collect();
        let mine: Vec<(i32, i64, Option<i64>)> = storage.deaths_between(0, last + 60_000).into_iter().filter(|d| d.0 == local).collect();
        if !mine.is_empty() {
            eprintln!("replay: your deaths (at, back) {:?}", mine.iter().map(|d| (d.1, d.2)).collect::<Vec<_>>());
        }
        let deaths = mine.len();
        // COPILOT_REPLAY_DEATHS=<from ms>-<to ms>: every death read in that stretch.
        if let Some((from, to)) = std::env::var("COPILOT_REPLAY_DEATHS").ok().and_then(|v| {
            let (a, b) = v.split_once('-')?;
            Some((a.parse::<i64>().ok()?, b.parse::<i64>().ok()?))
        }) {
            for (actor, at, back) in storage.deaths_between(from, to) {
                eprintln!("replay death: actor {actor} ({:?}) at {at} back {back:?}", storage.get_nickname(actor));
            }
        }
        let kind = last_target_kind(&storage, &npcs);
        let cp = storage.local_profile().combat_power;
        eprintln!("replay: you {local} ({:?}), dummies {dummies:?}, kind {kind}, deaths {deaths}, cp {cp:?}", storage.local_character_name());
        crate::clock::set_override(None);
        assert_eq!(kind, "train");
        assert!(!dummies.is_empty() && dummies.iter().all(|&(_, d)| d > 0), "each dummy hit has your damage");
        assert_eq!(deaths, 0, "nobody dies at a training dummy");
        if storage.local_character_name().is_some() {
            assert!(cp.is_some_and(|c| c > 0), "your combat power, once your record came");
        }
    }

    /// A skill you used on a monster is an attack: on a player outside your
    /// party it is PvP damage on them; a skill never seen on a monster, on
    /// your party, stays a heal.
    #[test]
    fn an_attack_on_another_player_is_pvp_damage() {
        let storage = DataStorage::new();
        storage.set_local_player_id(Some(7983));
        storage.append_nickname_authoritative(7983, "ELLAO");
        storage.append_nickname_authoritative(5000, "RenataRabuda");
        storage.append_nickname_authoritative(6000, "Noriac");
        storage.set_party_roster(vec![
            ("ELLAO".into(), PartyMember { slot: 1, ..Default::default() }),
            ("Noriac".into(), PartyMember { slot: 2, ..Default::default() }),
        ], true);
        storage.append_mob(60_000, 2_100_704);
        storage.append_damage(pvp_hit(7983, 60_000)); // your attack, on a monster
        storage.append_damage(pvp_hit(5000, 60_000)); // she is a player too
        storage.append_damage(pvp_hit(6000, 60_000));
        storage.append_damage(pvp_hit(7983, 5000)); // the same attack, on her
        let mut heal = pvp_hit(6000, 7983); // Noriac's heal on you
        heal.set_skill_code(17_010_000);
        storage.append_damage(heal);
        let fights = storage.get_combat_snapshot_light();
        let on_her = fights.get(&5000).expect("damage on the enemy player");
        assert_eq!(on_her.total_damage, 500);
        assert!(!fights.contains_key(&7983), "a party heal is no damage on you");
        // A new player showing up does not purge it as friendly fire.
        storage.append_damage(pvp_hit(9000, 60_000));
        assert!(storage.get_combat_snapshot_light().contains_key(&5000));
        // She hits back; the board sets your side against hers.
        storage.append_damage(pvp_hit(5000, 7983));
        let record = vec![PvpOpponent {
            name: "RenataRabuda".into(),
            kills: 1,
            events: vec![PvpEvent { at_ms: 2_000, kill: true, character: "ELLAO".into() }],
            ..Default::default()
        }];
        let board = pvp_board(&storage, &record, 1_000);
        let ours: Vec<(&str, i64)> = board.ours.iter().map(|r| (r.name.as_str(), r.damage)).collect();
        let theirs: Vec<(&str, i64, u32)> = board.opponents.iter().map(|r| (r.name.as_str(), r.damage, r.deaths)).collect();
        assert_eq!(ours, vec![("ELLAO", 500)]);
        assert_eq!(theirs, vec![("RenataRabuda", 500, 1)]);
        assert_eq!((board.ours_total, board.opponents_total, board.kills, board.deaths), (500, 500, 1, 0));
    }

    /// An arena gives you a new id, said only after your first death there.
    #[test]
    fn a_death_before_the_game_names_your_new_id_still_counts() {
        let storage = DataStorage::new();
        storage.set_local_player_id(Some(11617));
        storage.append_nickname_authoritative(11617, "ELLAO");
        storage.append_nickname_authoritative(5000, "Aldaric04");
        let mut tracker = PvpTracker { from_ms: 0, local: None };
        assert!(pvp_outcomes(&storage, &mut tracker, 1_000_000).is_empty());
        // In the arena, as 13914: you die to Aldaric04 before the game says
        // 13914 is you; the look in between finds nothing of yours.
        crate::clock::set_override(Some(1_010_000));
        storage.append_damage(pvp_hit(13914, 5000));
        storage.append_damage(pvp_hit(5000, 13914));
        storage.note_player_hp(13914, 9_000);
        storage.note_player_hp(13914, 0);
        assert!(pvp_outcomes(&storage, &mut tracker, 1_020_000).is_empty());
        // 33 s later the self record names 13914 you: that death is yours.
        storage.set_local_player_id(Some(13914));
        storage.append_nickname_authoritative(13914, "ELLAO");
        let got: Vec<(String, bool, i64)> =
            pvp_outcomes(&storage, &mut tracker, 1_045_000).into_iter().map(|o| (o.name, o.kill, o.at_ms)).collect();
        assert_eq!(got, vec![("Aldaric04".to_string(), false, 1_010_000)]);
        assert!(pvp_outcomes(&storage, &mut tracker, 1_050_000).is_empty(), "once");
        crate::clock::set_override(None);
    }

    /// You and a player of the other side trade blows: their kill of you and
    /// yours of them land in the record; a party member is never one, and a
    /// monster's hit just before your death makes it no PvP death.
    #[test]
    fn pvp_kills_and_deaths_are_told_apart() {
        let storage = DataStorage::new();
        storage.set_local_player_id(Some(7983));
        storage.append_nickname_authoritative(7983, "ELLAO");
        storage.append_nickname_authoritative(5000, "Inimigo");
        crate::clock::set_override(Some(1_000_000));
        // They become known players by their class skills.
        storage.append_damage(pvp_hit(7983, 5000));
        storage.append_damage(pvp_hit(5000, 7983));
        storage.note_player_hp(7983, 9_000);
        storage.note_player_hp(7983, 0); // you die to Inimigo
        crate::clock::set_override(Some(1_020_000));
        storage.note_player_hp(7983, 9_000); // revived
        storage.append_damage(pvp_hit(7983, 5000));
        storage.note_player_hp(5000, 8_000);
        storage.note_player_hp(5000, 0); // and get even
        let mut tracker = PvpTracker { from_ms: 0, local: None };
        let got: Vec<(String, bool)> = pvp_outcomes(&storage, &mut tracker, 1_030_000)
            .into_iter().map(|o| (o.name, o.kill)).collect();
        assert_eq!(got, vec![("Inimigo".to_string(), false), ("Inimigo".to_string(), true)]);
        assert!(pvp_outcomes(&storage, &mut tracker, 1_040_000).is_empty(), "each death once");

        // The record keeps both, and the Target overlay reads it.
        let dir = std::env::temp_dir().join(format!("copilot-pvp-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("pvp.json");
        let _ = std::fs::remove_file(&path);
        let outcomes = vec![
            PvpOutcome { opponent: 5000, name: "Inimigo".into(), kill: false, at_ms: 1_000_000 },
            PvpOutcome { opponent: 5000, name: "Inimigo".into(), kill: true, at_ms: 1_020_000 },
        ];
        record_pvp(&storage, &path, &outcomes);
        let rec = load_pvp(&path);
        assert_eq!((rec[0].kills, rec[0].deaths, rec[0].events.len()), (1, 1, 2));
        let view = pvp_target(&storage, &rec).expect("Inimigo is the player you hit last");
        assert_eq!((view.name.as_str(), view.kills, view.deaths), ("Inimigo", 1, 1));

        // A monster hit you just before: that death is no PvP death.
        storage.append_mob(60_000, 2_100_704);
        crate::clock::set_override(Some(1_100_000));
        storage.append_damage(pvp_hit(5000, 7983));
        let mut mob_hit = pvp_hit(60_000, 7983);
        mob_hit.set_skill_code(2_100_704);
        storage.append_damage(mob_hit);
        storage.note_player_hp(7983, 0);
        assert!(pvp_outcomes(&storage, &mut tracker, 1_110_000).is_empty());

        // A party member is no opponent.
        storage.set_party_roster(vec![
            ("ELLAO".into(), PartyMember { slot: 1, ..Default::default() }),
            ("Inimigo".into(), PartyMember { slot: 2, ..Default::default() }),
        ], true);
        assert!(pvp_target(&storage, &rec).is_none());
        crate::clock::set_override(None);
    }

    #[test]
    fn the_party_comes_in_slot_order_with_you_and_the_fallen_marked() {
        let storage = DataStorage::new();
        let member = |slot, job, cp| PartyMember { slot, job: Some(job), combat_power: cp, level: 45, ..Default::default() };
        storage.set_party_roster(vec![
            ("Lysa".into(), member(3, JobClass::Cleric, 69_810)),
            ("ELLAO".into(), member(1, JobClass::Sorcerer, 105_722)),
            ("Kaelith".into(), member(2, JobClass::Gladiator, 106_728)),
        ], true);
        storage.append_nickname_authoritative(1429, "ELLAO");
        storage.append_nickname_authoritative(2260, "Lysa");
        storage.set_local_player_id(Some(1429));
        storage.note_hp(1429, 16_000);
        storage.note_hp(2260, 5_000);
        storage.note_hp(2260, 0);

        let party = party_view(&storage);
        let names: Vec<&str> = party.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["ELLAO", "Kaelith", "Lysa"]);
        assert!(party[0].is_local && !party[1].is_local && !party[2].is_local, "{party:?}");
        assert_eq!(party[0].job, "마도성");
        assert_eq!(party[0].combat_power, 105_722);
        assert!(!party[0].dead);
        assert!(party[2].dead);
        // Kaelith has no entity yet (no spawn seen): no HP, not counted as down.
        assert_eq!((party[1].hp, party[1].dead), (None, false));
    }

    #[test]
    fn bosses_from_an_opened_map_come_soonest_back_first() {
        let hex = include_str!("capture/testdata/boss_map_v1.hex");
        let bytes: Vec<u8> = (0..hex.trim().len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex.trim()[i..i + 2], 16).unwrap())
            .collect();
        let now = 1_791_992_733_074;
        let map = crate::capture::boss_map::parse(&bytes, now).expect("the sample parses");
        let storage = DataStorage::new();
        storage.note_boss_map(map.clone());
        let bosses = field_bosses(&storage, &NpcLookup::new());
        assert_eq!(bosses.len(), map.bosses.len());
        assert!(bosses.iter().all(|b| b.map_id == map.map_id && b.seen_ms == map.seen_ms));
        // Order: dead with a time (ascending), dead without, then up.
        let keys: Vec<(bool, bool, i64)> = bosses.iter().map(|b| (b.up, b.time_ms == 0, b.time_ms)).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert!(bosses.iter().any(|b| b.up) && bosses.iter().any(|b| !b.up));
        // No NPC table loaded: the name falls back to the id.
        assert!(bosses[0].name.starts_with("Chefe de campo #"));
    }

    #[test]
    fn names_come_back_only_for_the_same_game_process() {
        let first = DataStorage::new();
        first.append_nickname_authoritative(11652, "ELLAO");
        first.append_nickname_authoritative(6351, "Yzkra");
        first.set_local_player_id(Some(11652));
        let saved = snapshot_names(&first, 4242);
        let json = serde_json::to_string(&saved).unwrap();
        let session: NameSession = serde_json::from_str(&json).unwrap();

        let other_game = DataStorage::new();
        assert_eq!(restore_names(&other_game, &session, 9999), 0);
        assert!(other_game.get_nicknames().is_empty() && other_game.local_player_id().is_none());

        let restarted = DataStorage::new();
        restarted.append_nickname_authoritative(6351, "Renamed");
        assert_eq!(restore_names(&restarted, &session, 4242), 1);
        assert_eq!(restarted.get_nickname(11652).as_deref(), Some("ELLAO"));
        // A name the app already learned again is not overwritten.
        assert_eq!(restarted.get_nickname(6351).as_deref(), Some("Renamed"));
        assert_eq!(restarted.local_player_id(), Some(11652));
    }

    #[test]
    fn the_character_you_play_is_remembered_with_its_class_and_power() {
        let dir = std::env::temp_dir().join(format!("copilot-chars-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("chars.json");
        let _ = std::fs::remove_file(&path);
        let storage = DataStorage::new();
        assert!(note_character(&storage, &path, 1_000).is_none(), "nobody known yet");
        storage.set_local_player_id(Some(1429));
        storage.append_nickname_authoritative(1429, "ELLAO");
        storage.set_party_roster(vec![(
            "ELLAO".into(),
            PartyMember { slot: 1, job: Some(JobClass::Sorcerer), combat_power: 105_722, ..Default::default() },
        )], true);
        let first = note_character(&storage, &path, 2_000).unwrap();
        assert_eq!((first.job.as_str(), first.combat_power, first.first_seen_ms), ("마도성", 105_722, 2_000));
        let again = note_character(&storage, &path, 9_000).unwrap();
        assert_eq!((again.first_seen_ms, again.last_seen_ms), (2_000, 9_000));
        assert_eq!(load_characters(&path).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn you_get_your_last_character_of_the_same_class_when_the_game_has_not_named_you() {
        use crate::entity::damage_packet::ParsedDamagePacket;
        let storage = DataStorage::new();
        storage.set_local_player_id(Some(11652));
        let mut hit = ParsedDamagePacket::new();
        hit.set_actor_id(11652);
        hit.set_target_id(50_000);
        hit.set_skill_code(15_200_000); // a Sorcerer skill: the class comes from it
        hit.set_damage(500);
        hit.set_timestamp(1_000);
        storage.append_damage(hit);
        let seen = |name: &str, job: &str, last| CharacterSeen { name: name.into(), job: job.into(), combat_power: 0, first_seen_ms: 0, last_seen_ms: last };
        let chars = vec![seen("ELLAO", "마도성", 50), seen("Alt", "검성", 90), seen("OldMage", "마도성", 10)];
        assert_eq!(name_local_from_characters(&storage, &chars).as_deref(), Some("ELLAO"));
        assert_eq!(storage.get_nickname(11652).as_deref(), Some("ELLAO"));
        // Already named: left alone.
        assert_eq!(name_local_from_characters(&storage, &chars), None);
    }

    #[test]
    fn the_timer_schedule_parses_with_every_event_in_south_america_time() {
        let schedule: TimerSchedule = serde_json::from_str(include_str!("../../src/data/copilot_timers.json")).unwrap();
        assert_eq!(schedule, timer_schedule());
        assert_eq!(schedule.region, "Am. do Sul");
        let ids: Vec<&str> = schedule.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "shugo-festa",
                "invasao-dimensional",
                "fenda-espaco-temporal",
                "vigilante-kairah",
                "cerco-artefatos",
                "chefes-do-cerco",
                "senhor-guardiao-nahma",
            ]
        );
        for rule in &schedule.rules {
            assert_eq!(rule.utc_offset_min, -180, "{}", rule.id);
            assert!(rule.duration_min > 0 && rule.duration_min <= 120, "{}", rule.id);
            assert!(rule.entry_min.is_none_or(|e| e < rule.duration_min), "{}", rule.id);
            assert!(["high", "medium", "low"].contains(&rule.confidence.as_str()), "{}", rule.id);
            match &rule.recurrence {
                TimerRecurrence::Hourly { minute } => assert!(*minute < 60),
                TimerRecurrence::EveryHours { every, anchor_hour, minute } => {
                    assert!(*every > 0 && 24 % every == 0 && *anchor_hour < *every && *minute < 60, "{}", rule.id)
                }
                TimerRecurrence::Daily { hour, minute } => assert!(*hour < 24 && *minute < 60),
                TimerRecurrence::Weekly { days, hour, minute } => {
                    assert!(!days.is_empty() && days.iter().all(|d| *d < 7) && *hour < 24 && *minute < 60, "{}", rule.id)
                }
            }
        }
        let nahma = schedule.rules.iter().find(|r| r.id == "senhor-guardiao-nahma").unwrap();
        assert_eq!(nahma.confidence, "low");
        assert_eq!(nahma.recurrence, TimerRecurrence::Weekly { days: vec![0, 5], hour: 21, minute: 0 });
    }

    #[test]
    fn a_timer_rule_reaches_the_page_in_the_shape_timers_core_reads() {
        let rule = timer_schedule().rules.into_iter().find(|r| r.id == "fenda-espaco-temporal").unwrap();
        let json = serde_json::to_value(&rule).unwrap();
        assert_eq!(json["recurrence"], serde_json::json!({ "kind": "everyHours", "every": 3, "anchorHour": 2, "minute": 0 }));
        assert_eq!(json["utcOffsetMin"], -180);
        assert_eq!(json["durationMin"], 10);
        assert!(json.get("entryMin").is_none());
        let shugo = timer_schedule().rules.into_iter().find(|r| r.id == "shugo-festa").unwrap();
        let json = serde_json::to_value(&shugo).unwrap();
        assert_eq!(json["recurrence"], serde_json::json!({ "kind": "hourly", "minute": 0 }));
        assert_eq!(json["entryMin"], 3);
    }

    #[test]
    fn an_unknown_recurrence_kind_is_refused_rather_than_guessed() {
        let bad = r#"{ "region": "x", "rules": [{ "id": "a", "name": "A", "icon": "x", "recurrence": { "kind": "lunar" },
            "utcOffsetMin": 0, "durationMin": 5, "confidence": "low" }] }"#;
        assert!(serde_json::from_str::<TimerSchedule>(bad).is_err());
    }

    fn counters_packet(hex: &str, at: i64) -> crate::capture::counters::CounterPacket {
        let bytes: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect();
        crate::capture::counters::parse(&bytes, at).unwrap()
    }

    fn login_list(at: i64) -> crate::capture::counters::CounterPacket {
        counters_packet(include_str!("capture/testdata/counters_list_v1.hex").trim(), at)
    }

    #[test]
    fn the_routine_fills_in_from_the_login_list_once_the_game_says_whose_it_is() {
        let storage = DataStorage::new();
        storage.note_counters(login_list(1_000));
        let state = routine_state(&storage, &RoutineFile::default(), false, true);
        assert_eq!((state.name.clone(), state.live, state.auto.clone()), (None, false, RoutineAuto::default()), "waits for the self record");

        storage.set_local_identity_from_game(15997, Some("Nullception".into()));
        storage.note_counters(counters_packet("16a9ff9e8800000000000060e3160000000000", 2_000));
        let state = routine_state(&storage, &RoutineFile::default(), false, true);
        assert_eq!(state.name.as_deref(), Some("Nullception"));
        assert!(state.live);
        assert_eq!(state.auto.odyle, Some(OdyleView { basic: 30, charged: 840, max: 840, seen_ms: 1_000 }));
        assert_eq!(
            state.auto.abyss_time,
            Some(AbyssTimeView { remaining_ms: 18_596_000, charged_ms: 0, cap_ms: 25_200_000, seen_ms: 1_000 })
        );
        assert_eq!(state.auto.abyss_points, Some(AbyssPointsView { gained: 34_974, limit: 1_500_000, seen_ms: 2_000 }));
        assert_eq!(state.dev_counters, None, "never outside developer mode");
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["abyssTime"]["remainingMs"], 18_596_000);
        assert_eq!(json["odyle"]["basic"], 30);
        assert!(json.get("devCounters").is_none());

        let dev = routine_state(&storage, &RoutineFile::default(), true, true);
        let counters = dev.dev_counters.unwrap();
        assert_eq!(counters.len(), 73);
        assert!(counters.iter().any(|c| c.id == 9 && c.time_ms == 25_200_000));
    }

    #[test]
    fn the_routine_shows_what_was_kept_for_the_character_until_the_game_sends_it() {
        let dir = std::env::temp_dir().join(format!("copilot-routine-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("copilot-routine-state.json");
        let _ = std::fs::remove_file(&path);

        // A session: the list, then the self record, then an Abyss points gain.
        let storage = DataStorage::new();
        assert!(!remember_routine(&storage, &path), "nothing yet");
        storage.note_counters(login_list(1_000));
        assert!(!remember_routine(&storage, &path), "whose is not known yet");
        storage.set_local_identity_from_game(15997, Some("Nullception".into()));
        assert!(remember_routine(&storage, &path));
        assert!(!remember_routine(&storage, &path), "unchanged");
        storage.note_counters(counters_packet("16a9ff9e8800000000000060e3160000000000", 2_000));
        assert!(remember_routine(&storage, &path));

        // The next start: the game hasn't sent anything yet.
        let file = load_routine_file(&path);
        assert_eq!(file.last.as_deref(), Some("Nullception"));
        let fresh = DataStorage::new();
        let state = routine_state(&fresh, &file, false, true);
        assert_eq!(state.name.as_deref(), Some("Nullception"));
        assert!(!state.live);
        assert_eq!(state.auto.odyle.map(|o| (o.basic, o.charged, o.seen_ms)), Some((30, 840, 1_000)));
        assert_eq!(state.auto.abyss_points.map(|p| p.gained), Some(34_974));

        // Another character logs in: its own list, nothing kept for it, the
        // first one's Abyss points not carried over.
        fresh.note_counters(counters_packet("0b0b61010c018793033c0a", 5_000));
        fresh.set_local_identity_from_game(4294, Some("Alt".into()));
        let state = routine_state(&fresh, &file, false, true);
        assert_eq!(state.name.as_deref(), Some("Alt"));
        assert_eq!(state.auto.odyle.map(|o| (o.basic, o.charged)), Some((60, 10)));
        assert_eq!((state.auto.abyss_time, state.auto.abyss_points), (None, None));
        assert!(remember_routine(&fresh, &path));
        let file = load_routine_file(&path);
        assert_eq!(file.characters.len(), 2);
        assert_eq!(file.characters["Nullception"].abyss_points.map(|p| p.gained), Some(34_974), "kept");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn counters_read_before_the_game_says_who_you_are_show_alone_and_are_not_kept() {
        let dir = std::env::temp_dir().join(format!("copilot-routine-unowned-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("copilot-routine-state.json");
        let _ = std::fs::remove_file(&path);
        let kept_points = AbyssPointsView { gained: 9, limit: 10, seen_ms: 100 };
        let mut file = RoutineFile { last: Some("Guess".into()), ..RoutineFile::default() };
        file.characters.insert("Guess".into(), RoutineAuto { abyss_points: Some(kept_points), ..RoutineAuto::default() });

        // Opened mid-session: a name guessed from the class, then an update,
        // but no self record yet.
        let storage = DataStorage::new();
        storage.set_local_character_name(Some("Guess".into()));
        storage.note_counters(counters_packet("140c61010c018793031ef0060128000000", 2_000));
        let state = routine_state(&storage, &file, false, true);
        assert_eq!(state.name, None, "not the guess");
        assert!(state.live);
        assert_eq!(state.auto.odyle.map(|o| (o.basic, o.charged)), Some((30, 880)));
        assert_eq!(state.auto.abyss_points, None, "the guess's kept points aren't mixed in");
        assert!(!remember_routine(&storage, &path), "whose is not known yet");

        // The self record names them: theirs from then on.
        storage.set_local_identity_from_game(15997, Some("Guess".into()));
        let state = routine_state(&storage, &file, false, true);
        assert_eq!(state.name.as_deref(), Some("Guess"));
        assert_eq!(state.auto.abyss_points, Some(kept_points));
        let _ = std::fs::remove_file(&path);
    }

    fn fixture(text: &str, at: i64) -> crate::capture::counters::CounterPacket {
        let hex: String = text.split_whitespace().collect();
        counters_packet(&hex, at)
    }

    #[test]
    fn the_login_kinah_and_attendance_show_and_are_kept_and_the_supply_only_for_developers() {
        let dir = std::env::temp_dir().join(format!("copilot-routine-login-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("copilot-routine-state.json");
        let _ = std::fs::remove_file(&path);

        // The login of 2026-10-18 11:59:40, in the game's order.
        let storage = DataStorage::new();
        storage.note_counters(fixture(include_str!("capture/testdata/progress_login_v1.hex"), 1_000));
        storage.note_counters(login_list(1_003));
        storage.note_counters(fixture(include_str!("capture/testdata/supply_login_v1.hex"), 1_004));
        storage.note_counters(counters_packet("16a5ff531d000000000000078f864fa1010000", 1_005));
        storage.note_counters(counters_packet(
            "312ae30301f705000000000000030200000002dc05000000000000000000000003dc050000000000000000000000",
            1_006,
        ));
        let state = routine_state(&storage, &RoutineFile::default(), false, true);
        assert_eq!((state.auto.kinah, state.auto.attendance.clone()), (None, None), "waits for the self record");
        assert_eq!(state.auto.arena, None);

        storage.set_local_identity_from_game(15997, Some("ELLAO".into()));
        let state = routine_state(&storage, &RoutineFile::default(), false, true);
        assert_eq!(state.auto.kinah, Some(KinahView { earned: 7_507, seen_ms: 1_005 }));
        let attendance = state.auto.attendance.clone().unwrap();
        assert_eq!(attendance.seen_ms, 1_000);
        assert_eq!(
            attendance.events.iter().map(|e| (e.days, e.claimed, e.last_ms)).collect::<Vec<_>>(),
            vec![(7, 7, 1_791_733_752_263), (14, 14, 1_792_306_977_387), (5, 5, 1_792_306_977_387)]
        );
        assert_eq!(state.dev_supply, None, "never outside developer mode");
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["kinah"]["earned"], 7_507);
        assert_eq!(json["arena"]["records"][0], serde_json::json!({ "arena": 1, "rating": 1_527, "wins": 3, "losses": 2 }));
        assert_eq!(json["arena"]["seenMs"], 1_006);
        // Read "at" 1 s after 1970: the 2026 dates are after its reset, so no start.
        assert_eq!((json["missions"]["total"].clone(), json["missions"]["start"].clone()), (serde_json::json!(11), serde_json::Value::Null));
        assert_eq!(json["arenaPlayed"]["total"], 5);
        assert_eq!(json["attendance"]["events"][1]["lastMs"], 1_792_306_977_387_i64);
        assert!(json.get("devSupply").is_none());

        let dev = routine_state(&storage, &RoutineFile::default(), true, true).dev_supply.unwrap();
        assert_eq!(dev.iter().map(|b| (b.slot, b.requests)).collect::<Vec<_>>(), vec![(1, 10), (2, 10), (3, 54), (4, 54)]);
        assert_eq!(serde_json::to_value(&dev[0]).unwrap()["endMs"], 1_792_393_200_999_i64);

        // Kept for the next start.
        assert!(remember_routine(&storage, &path));
        let file = load_routine_file(&path);
        let kept = routine_state(&DataStorage::new(), &file, false, true);
        assert_eq!((kept.name.as_deref(), kept.live), (Some("ELLAO"), false));
        assert_eq!(kept.auto.kinah.map(|k| k.earned), Some(7_507));
        assert_eq!(kept.auto.attendance.map(|a| a.events.len()), Some(3));
        assert_eq!(kept.auto.arena.map(|a| a.records.len()), Some(3));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn what_the_game_sent_reads_as_kept_once_the_game_is_closed() {
        let storage = DataStorage::new();
        storage.note_counters(login_list(1_000));
        storage.set_local_identity_from_game(15997, Some("Nullception".into()));
        assert!(routine_state(&storage, &RoutineFile::default(), false, true).live);
        let closed = routine_state(&storage, &RoutineFile::default(), false, false);
        assert!(!closed.live);
        assert_eq!(closed.name.as_deref(), Some("Nullception"));
        assert_eq!(closed.auto.odyle.map(|o| (o.basic, o.seen_ms)), Some((30, 1_000)), "still the latest values");
    }

    #[test]
    fn a_kept_item_gives_way_to_a_later_one_and_stays_when_the_game_has_not_sent_it() {
        let attendance = |days, seen_ms| AttendanceView {
            events: vec![AttendanceEventView { id: 510_330_100, days, claimed: days, last_ms: seen_ms }],
            seen_ms,
        };
        let old = RoutineAuto {
            odyle: Some(OdyleView { basic: 10, charged: 0, max: 840, seen_ms: 100 }),
            abyss_time: None,
            abyss_points: Some(AbyssPointsView { gained: 5, limit: 10, seen_ms: 100 }),
            kinah: Some(KinahView { earned: 7, seen_ms: 100 }),
            attendance: Some(attendance(13, 100)),
            arena: None,
            missions: Some(TallyView { total: 11, last_ms: 5, seen_ms: 100, reset_ms: 50, start: Some(11) }),
            arena_played: None,
        };
        let new = RoutineAuto {
            odyle: Some(OdyleView { basic: 20, charged: 0, max: 840, seen_ms: 200 }),
            abyss_time: Some(AbyssTimeView { remaining_ms: 1, charged_ms: 0, cap_ms: 2, seen_ms: 200 }),
            abyss_points: None,
            kinah: None,
            attendance: Some(attendance(14, 200)),
            arena: None,
            // Read later the same day, after one was done: the start is the old one's.
            missions: Some(TallyView { total: 12, last_ms: 150, seen_ms: 200, reset_ms: 50, start: None }),
            arena_played: None,
        };
        let merged = old.merged(&new);
        assert_eq!(merged.odyle.unwrap().basic, 20);
        assert!(merged.abyss_time.is_some());
        assert_eq!(merged.abyss_points.unwrap().gained, 5);
        assert_eq!(merged.kinah.map(|k| k.earned), Some(7));
        assert_eq!(merged.attendance.as_ref().map(|a| a.events[0].days), Some(14));
        assert_eq!(merged.missions.map(|m| (m.total, m.start)), Some((12, Some(11))));
        assert_eq!(new.merged(&old), merged, "order doesn't matter");
    }

    #[test]
    fn a_routine_file_from_before_reads_with_what_it_has() {
        let file: RoutineFile = serde_json::from_str(r#"{ "characters": { "A": { "odyle": { "basic": 1, "charged": 2, "max": 840, "seenMs": 3 } } } }"#).unwrap();
        assert_eq!(file.last, None);
        assert_eq!(file.characters["A"].abyss_time, None);
        assert_eq!(load_routine_file(std::path::Path::new("/nonexistent/copilot-routine-state.json")), RoutineFile::default());
    }
}

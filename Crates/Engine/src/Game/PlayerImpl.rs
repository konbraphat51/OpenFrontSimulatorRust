// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/PlayerImpl.ts: player state, diplomacy, economy and build rules.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Handles: `PlayerRef` (read) and `PlayerMut` (write) borrow the game and deref to the data in
//! [`PlayerImpl`]; players are addressed by small id (index + 1 in `GameImpl.players`), terra
//! nullius is 0. Skipped as client plumbing: `toUpdate`/`toFullUpdate`, `buildableUnits`,
//! `allianceInfo`, `playerProfile`, `allRelationsSorted` (UI only) and the unit/nearby memos
//! (pure caches; the answers are recomputed). `stats()` calls other than attacks and MIRV launches
//! are dropped (see `StatsImpl`).

use super::Game::{
    AllianceId, AllianceRequestId, AttackId, COLORED_TEAMS, Cell, DisconnectSnapshot, Embargo,
    EmojiMessage, EmojiRecipient, GameMode, GameType, Gold, NUM_UNIT_TYPES, PlayerInfo, PlayerType,
    Relation, STRUCTURES, SmallId, Team, Tick, UnitId, UnitParams, UnitType,
};
use super::GameImpl::GameImpl;
use super::GameMap::{GameMapImpl, TileRef, manhattan_dist_fn};
use super::TileSet::TileSet;
use super::TransportShipUtils::{best_shore_deployment_source, can_build_transport_ship};
use crate::Builds::Rules::Change;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Configuration::Config::{CostContext, TroopInput};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::{find_closest_by, simple_hash, to_int, within};
use std::ops::{Deref, DerefMut};

/// Ticks a decay cue stays lit after the last rot.
const DECAY_CUE_GRACE_TICKS: f64 = 30.0;

/// A `targets_` entry.
#[derive(Clone, Copy, Debug)]
pub struct TargetEntry {
    /// Tick the target was set.
    pub tick: Tick,
    /// Targeted player.
    pub target: SmallId,
}

/// A sent donation.
#[derive(Clone, Copy, Debug)]
pub struct Donation {
    /// Recipient.
    pub recipient: SmallId,
    /// Tick sent.
    pub tick: Tick,
}

/// State of one player (humans, nations and tribes alike).
#[derive(Debug)]
pub struct PlayerImpl {
    pub(crate) small_id: SmallId,
    pub(crate) info: PlayerInfo,
    pub(crate) team: Option<Team>,
    pub(crate) last_tile_change: f64,
    pub(crate) tile_change_version: u32,
    /// The player's own random stream (`_pseudo_random`), seeded with `simpleHash(id)`.
    pub pseudo_random: PseudoRandom,
    pub(crate) gold: Gold,
    pub(crate) troops: i64,
    pub(crate) trade_gold: Gold,
    pub(crate) train_gold: Gold,
    pub(crate) piracy_gold: Gold,
    pub(crate) gold_earned: Gold,
    pub(crate) marked_traitor_tick: f64,
    pub(crate) marked_doomsday_clock_tick: f64,
    pub(crate) rotted_at_tick: f64,
    pub(crate) betrayal_count: u32,
    pub(crate) embargoes: OrderedMap<SmallId, Embargo>,
    pub(crate) border_tiles: TileSet,
    pub(crate) units: Vec<UnitId>,
    pub(crate) my_units_version: u32,
    pub(crate) tiles: TileSet,
    pub(crate) past_outgoing_alliance_requests: Vec<AllianceRequestId>,
    pub(crate) targets: Vec<TargetEntry>,
    pub(crate) outgoing_emojis: Vec<EmojiMessage>,
    pub(crate) outgoing_quick_chats: OrderedMap<SmallId, Tick>,
    pub(crate) sent_donations: Vec<Donation>,
    pub(crate) relations: OrderedMap<SmallId, f64>,
    pub(crate) last_delete_unit_tick: f64,
    pub(crate) last_embargo_all_tick: f64,
    pub(crate) incoming_attacks: Vec<AttackId>,
    pub(crate) outgoing_attacks: Vec<AttackId>,
    pub(crate) outgoing_land_attacks: Vec<AttackId>,
    pub(crate) alliances: Vec<AllianceId>,
    pub(crate) spawn_tile: Option<TileRef>,
    pub(crate) is_disconnected: bool,
    pub(crate) disconnect_snapshot: Option<DisconnectSnapshot>,
    pub(crate) num_units_constructed: [i64; NUM_UNIT_TYPES],
    /// `largestClusterBoundingBox` (min, max), set by `PlayerExecution`.
    pub largest_cluster_bounding_box: Option<(Cell, Cell)>,
}

impl PlayerImpl {
    /// `new PlayerImpl(mg, smallID, info, startTroops, team)`.
    pub(crate) fn new(
        game: &GameImpl,
        small_id: SmallId,
        info: PlayerInfo,
        start_troops: f64,
        team: Option<Team>,
    ) -> Self {
        let gold = game
            .config()
            .starting_gold(info.player_type, info.is_lobby_creator);
        let pseudo_random =
            PseudoRandom::for_rules(game.config().rules(), simple_hash(&info.id) as f64);
        Self {
            small_id,
            team,
            last_tile_change: 0.0,
            tile_change_version: 0,
            pseudo_random,
            gold,
            troops: to_int(start_troops),
            trade_gold: 0,
            train_gold: 0,
            piracy_gold: 0,
            gold_earned: 0,
            marked_traitor_tick: -1.0,
            marked_doomsday_clock_tick: -1.0,
            rotted_at_tick: -1.0,
            betrayal_count: 0,
            embargoes: OrderedMap::new(),
            border_tiles: TileSet::new(),
            units: Vec::new(),
            my_units_version: 0,
            tiles: TileSet::new(),
            past_outgoing_alliance_requests: Vec::new(),
            targets: Vec::new(),
            outgoing_emojis: Vec::new(),
            outgoing_quick_chats: OrderedMap::new(),
            sent_donations: Vec::new(),
            relations: OrderedMap::new(),
            last_delete_unit_tick: -1.0,
            last_embargo_all_tick: -1.0,
            incoming_attacks: Vec::new(),
            outgoing_attacks: Vec::new(),
            outgoing_land_attacks: Vec::new(),
            alliances: Vec::new(),
            spawn_tile: None,
            is_disconnected: false,
            disconnect_snapshot: None,
            num_units_constructed: [0; NUM_UNIT_TYPES],
            largest_cluster_bounding_box: None,
            info,
        }
    }

    /// `smallID`.
    pub fn small_id(&self) -> SmallId {
        self.small_id
    }
    /// `info()`.
    pub fn info(&self) -> &PlayerInfo {
        &self.info
    }
    /// `name`.
    pub fn name(&self) -> &str {
        &self.info.name
    }
    /// `id` (the player id string).
    pub fn id(&self) -> &str {
        &self.info.id
    }
    /// `clientID`.
    pub fn client_id(&self) -> Option<&str> {
        self.info.client_id.as_deref()
    }
    /// `type`.
    pub fn player_type(&self) -> PlayerType {
        self.info.player_type
    }
    /// `isLobbyCreator`.
    pub fn is_lobby_creator(&self) -> bool {
        self.info.is_lobby_creator
    }
    /// `units()`: unit ids in the player's list order.
    pub fn units(&self) -> &[UnitId] {
        &self.units
    }
    /// `numTilesOwned`.
    pub fn num_tiles_owned(&self) -> usize {
        self.tiles.len()
    }
    /// `tiles()`.
    pub fn tiles(&self) -> &TileSet {
        &self.tiles
    }
    /// `borderTiles()`.
    pub fn border_tiles(&self) -> &TileSet {
        &self.border_tiles
    }
    /// `isAlive`.
    pub fn is_alive(&self) -> bool {
        !self.tiles.is_empty()
    }
    /// `hasSpawned`.
    pub fn has_spawned(&self) -> bool {
        self.spawn_tile.is_some()
    }
    /// `spawnTile`.
    pub fn spawn_tile(&self) -> Option<TileRef> {
        self.spawn_tile
    }
    /// `setSpawnTile`.
    pub fn set_spawn_tile(&mut self, tile: TileRef) {
        self.spawn_tile = Some(tile);
    }
    /// `team`.
    pub fn team(&self) -> Option<&Team> {
        self.team.as_ref()
    }
    /// `gold`.
    pub fn gold(&self) -> Gold {
        self.gold
    }
    /// `tradeGold`.
    pub fn trade_gold(&self) -> Gold {
        self.trade_gold
    }
    /// `trainGold`.
    pub fn train_gold(&self) -> Gold {
        self.train_gold
    }
    /// `piracyGold`.
    pub fn piracy_gold(&self) -> Gold {
        self.piracy_gold
    }
    /// `goldEarned`.
    pub fn gold_earned(&self) -> Gold {
        self.gold_earned
    }
    /// `troops`.
    pub fn troops(&self) -> f64 {
        self.troops as f64
    }
    /// `alliances()`: alliance ids in order.
    pub fn alliances(&self) -> &[AllianceId] {
        &self.alliances
    }
    /// `outgoingAttacks()`.
    pub fn outgoing_attacks(&self) -> &[AttackId] {
        &self.outgoing_attacks
    }
    /// The raw incoming list (`incomingAttacks()` also filters dead attackers, see [`PlayerRef`]).
    pub fn incoming_attacks_raw(&self) -> &[AttackId] {
        &self.incoming_attacks
    }
    /// `_outgoingLandAttacks`.
    pub fn outgoing_land_attacks(&self) -> &[AttackId] {
        &self.outgoing_land_attacks
    }
    /// `isDisconnected`.
    pub fn is_disconnected(&self) -> bool {
        self.is_disconnected
    }
    /// `disconnectSnapshot`.
    pub fn disconnect_snapshot(&self) -> Option<&DisconnectSnapshot> {
        self.disconnect_snapshot.as_ref()
    }
    /// `lastTileChange`.
    pub fn last_tile_change(&self) -> f64 {
        self.last_tile_change
    }
    /// `tileChangeVersion`.
    pub fn tile_change_version(&self) -> u32 {
        self.tile_change_version
    }
    /// `betrayals`.
    pub fn betrayals(&self) -> u32 {
        self.betrayal_count
    }
    /// `unitsConstructed(type)`.
    pub fn units_constructed(&self, t: UnitType) -> i64 {
        self.num_units_constructed[t.index()]
    }
    /// `hasEmbargoAgainst`.
    pub fn has_embargo_against(&self, other: SmallId) -> bool {
        self.embargoes.contains_key(&other)
    }
    /// `hasManualEmbargoAgainst`: a permanent (non-temporary) embargo.
    pub fn has_manual_embargo_against(&self, other: SmallId) -> bool {
        self.embargoes.get(&other).is_some_and(|e| !e.is_temporary)
    }
    /// `getEmbargoes()`.
    pub fn get_embargoes(&self) -> Vec<Embargo> {
        self.embargoes.values().copied().collect()
    }
    /// `canDeleteUnit` needs the tick: see [`PlayerRef::can_delete_unit`].
    /// `markedTraitorTick`.
    pub fn marked_traitor_tick(&self) -> f64 {
        self.marked_traitor_tick
    }
    /// `markedDoomsdayClockTick`.
    pub fn marked_doomsday_clock_tick(&self) -> f64 {
        self.marked_doomsday_clock_tick
    }
    /// `pastOutgoingAllianceRequests`.
    pub fn past_outgoing_alliance_requests(&self) -> &[AllianceRequestId] {
        &self.past_outgoing_alliance_requests
    }
    /// `_myUnitsVersion` (bumped on every unit list change).
    pub fn my_units_version(&self) -> u32 {
        self.my_units_version
    }
    /// `playerInfo.id` hashed: `simpleHash(id)`.
    pub fn id_hash(&self) -> u32 {
        simple_hash(&self.info.id)
    }
}

/// Read handle on a player.
pub struct PlayerRef<'a> {
    pub(crate) game: &'a GameImpl,
    pub(crate) id: SmallId,
}

impl Deref for PlayerRef<'_> {
    type Target = PlayerImpl;
    fn deref(&self) -> &PlayerImpl {
        self.game.player_impl(self.id)
    }
}

/// Mutating handle on a player.
pub struct PlayerMut<'a> {
    pub(crate) game: &'a mut GameImpl,
    pub(crate) id: SmallId,
}

impl Deref for PlayerMut<'_> {
    type Target = PlayerImpl;
    fn deref(&self) -> &PlayerImpl {
        self.game.player_impl(self.id)
    }
}

impl DerefMut for PlayerMut<'_> {
    fn deref_mut(&mut self) -> &mut PlayerImpl {
        self.game.player_impl_mut(self.id)
    }
}

impl<'a> PlayerRef<'a> {
    /// The game this handle borrows.
    pub fn game(&self) -> &'a GameImpl {
        self.game
    }

    /// `units(type)`: ids of that type in list order.
    pub fn units_of(&self, t: UnitType) -> Vec<UnitId> {
        self.units
            .iter()
            .copied()
            .filter(|&u| self.game.unit_any(u).unit_type == t)
            .collect()
    }

    /// `units(types)`: ids of any of the types in list order.
    pub fn units_of_types(&self, types: &[UnitType]) -> Vec<UnitId> {
        self.units
            .iter()
            .copied()
            .filter(|&u| types.contains(&self.game.unit_any(u).unit_type))
            .collect()
    }

    /// `unitCount(type)`: level-weighted count, construction included as upstream does.
    pub fn unit_count(&self, t: UnitType) -> i64 {
        let mut total = 0i64;
        for &u in &self.units {
            let unit = self.game.unit_any(u);
            if unit.unit_type == t {
                total += unit.level as i64;
            }
        }
        total
    }

    /// `unitsOwned(type)`: under-construction units count 1 each.
    pub fn units_owned(&self, t: UnitType) -> i64 {
        let mut total = 0i64;
        for &u in &self.units {
            let unit = self.game.unit_any(u);
            if unit.unit_type == t {
                if unit.under_construction {
                    total += 1;
                } else {
                    total += unit.level as i64;
                }
            }
        }
        total
    }

    /// The fields `Config.maxTroops`/`troopIncreaseRate` read.
    pub fn troop_input(&self) -> TroopInput {
        let mut city_level_sum = 0.0;
        for &u in &self.units {
            let unit = self.game.unit_any(u);
            if unit.unit_type == UnitType::City
                && (!unit.under_construction
                    || !self
                        .game
                        .config()
                        .rules()
                        .has(Change::CityTroopsFinishedOnly31))
            {
                city_level_sum += unit.level as f64;
            }
        }
        TroopInput {
            player_type: self.info.player_type,
            is_lobby_creator: self.info.is_lobby_creator,
            num_tiles_owned: self.tiles.len() as f64,
            city_level_sum,
            troops: self.troops as f64,
        }
    }

    /// `config.maxTroops(player)`.
    pub fn max_troops(&self) -> f64 {
        self.game.config().max_troops(&self.troop_input())
    }

    /// `config.maxTroopsWithoutCities(player)`.
    pub fn max_troops_without_cities(&self) -> f64 {
        let mut input = self.troop_input();
        input.city_level_sum = 0.0;
        self.game.config().max_troops(&input)
    }

    /// `config.troopIncreaseRate(player)`.
    pub fn troop_increase_rate(&self) -> f64 {
        self.game.config().troop_increase_rate(&self.troop_input())
    }

    /// `config.goldAdditionRate(player)`.
    pub fn gold_addition_rate(&self) -> Gold {
        self.game
            .config()
            .gold_addition_rate(self.info.player_type, self.info.is_lobby_creator)
    }

    /// `unitsConstructed(type)`; before PR 4320 buildings under construction count twice.
    pub fn units_constructed(&self, t: UnitType) -> i64 {
        let built = self.num_units_constructed[t.index()];
        if self
            .game
            .config()
            .rules()
            .has(Change::UnitsConstructedOnce4320)
        {
            return built;
        }
        let constructing = self
            .units
            .iter()
            .filter(|&&u| {
                let unit = self.game.unit_any(u);
                unit.unit_type == t && unit.is_under_construction()
            })
            .count() as i64;
        built + constructing
    }

    /// `unitInfo(type).cost(mg, player, extraUnits)`.
    pub fn unit_cost(&self, t: UnitType, extra_units: i64) -> Gold {
        let owned = |ut: UnitType| self.units_owned(ut).min(self.units_constructed(ut));
        self.game.config().unit_cost(
            t,
            &CostContext {
                player_type: self.info.player_type,
                is_lobby_creator: self.info.is_lobby_creator,
                owned_and_constructed: &owned,
                extra_units,
                mirvs_launched: self.game.stats().num_mirvs_launched(),
            },
        )
    }

    /// `sharesBorderWith(other)`.
    pub fn shares_border_with(&self, other: SmallId) -> bool {
        let map = &self.game.map;
        let mut nbuf = [0; 4];
        for border in self.border_tiles.iter() {
            let n = map.neighbors4_nswe(border, &mut nbuf);
            for &nb in &nbuf[..n] {
                if map.owner_id(nb) == other {
                    return true;
                }
            }
        }
        false
    }

    /// `nearby()`: neighbouring owners (0 = terra nullius) in discovery order.
    pub fn nearby(&self) -> Vec<SmallId> {
        let mut ns: OrderedSet<SmallId> = OrderedSet::new();
        let map = &self.game.map;
        let small_id = self.small_id;
        let skip_fallout = self
            .game
            .config()
            .rules()
            .has(Change::NearbySkipsFallout4422);
        for border in self.border_tiles.iter() {
            map.for_each_neighbor_nswe(border, |neighbor| {
                if map.is_land(neighbor) && !map.is_impassable(neighbor) {
                    if skip_fallout && !map.has_owner(neighbor) && map.has_fallout(neighbor) {
                        return;
                    }
                    let owner = map.owner_id(neighbor);
                    if owner != small_id {
                        ns.insert(owner);
                    }
                }
            });
        }
        if self.game.config().rules().has(Change::AiAttackRework31) {
            for n in self.shore_reachable_neighbors().iter() {
                ns.insert(*n);
            }
        }
        ns.iter().copied().collect()
    }

    fn shore_reachable_neighbors(&self) -> Vec<SmallId> {
        const DX: [i32; 4] = [0, 0, -1, 1];
        const DY: [i32; 4] = [-1, 1, 0, 0];
        let mut ns: OrderedSet<SmallId> = OrderedSet::new();
        let map = &self.game.map;
        let mut shore_idx = 0u32;
        for border in self.border_tiles.iter() {
            if !map.is_shore(border) {
                continue;
            }
            let visit = shore_idx.is_multiple_of(10);
            shore_idx += 1;
            if !visit {
                continue;
            }
            let bx = map.x(border) as i32;
            let by = map.y(border) as i32;
            for d in 0..4 {
                let (dx, dy) = (DX[d], DY[d]);
                let (x1, y1) = (bx + dx, by + dy);
                if !map.is_valid_coord(x1, y1) || !map.is_water(map.tile_ref(x1, y1)) {
                    continue;
                }
                let (nx, ny) = (bx + dx * 5, by + dy * 5);
                if !map.is_valid_coord(nx, ny) {
                    continue;
                }
                let tile = map.tile_ref(nx, ny);
                if !map.is_land(tile) {
                    continue;
                }
                if map.is_impassable(tile) {
                    continue;
                }
                if !map.has_owner(tile) && map.has_fallout(tile) {
                    continue;
                }
                let owner = map.owner_id(tile);
                if owner != self.small_id {
                    ns.insert(owner);
                }
            }
        }
        ns.iter().copied().collect()
    }

    /// `incomingAllianceRequests()`: pending requests addressed to this player.
    pub fn incoming_alliance_requests(&self) -> Vec<AllianceRequestId> {
        self.game
            .pending_requests
            .iter()
            .copied()
            .filter(|&r| self.game.alliance_requests[r as usize].recipient == self.id)
            .collect()
    }

    /// `outgoingAllianceRequests()`: pending requests sent by this player.
    pub fn outgoing_alliance_requests(&self) -> Vec<AllianceRequestId> {
        self.game
            .pending_requests
            .iter()
            .copied()
            .filter(|&r| self.game.alliance_requests[r as usize].requestor == self.id)
            .collect()
    }

    /// `allies()`.
    pub fn allies(&self) -> Vec<SmallId> {
        self.alliances
            .iter()
            .map(|&a| self.game.alliances[a as usize].other(self.id))
            .collect()
    }

    /// `isAlliedWith(other)`.
    pub fn is_allied_with(&self, other: SmallId) -> bool {
        if other == self.id {
            return false;
        }
        self.alliance_with(other).is_some()
    }

    /// `allianceWith(other)`.
    pub fn alliance_with(&self, other: SmallId) -> Option<AllianceId> {
        if other == self.id {
            return None;
        }
        self.alliances.iter().copied().find(|&a| {
            let al = &self.game.alliances[a as usize];
            al.recipient == other || al.requestor == other
        })
    }

    /// `canSendAllianceRequest(other)`.
    pub fn can_send_alliance_request(&self, other: SmallId) -> bool {
        if self.game.config().disable_alliances() {
            return false;
        }
        if other == self.id {
            return false;
        }
        let o = self.game.player(other);
        if self.is_disconnected || o.is_disconnected {
            return false;
        }
        if self.is_friendly(other, false) || !self.is_alive() {
            return false;
        }
        let has_pending = self
            .outgoing_alliance_requests()
            .iter()
            .any(|&r| self.game.alliance_requests[r as usize].recipient == other);
        if has_pending {
            return false;
        }
        let has_incoming = self
            .incoming_alliance_requests()
            .iter()
            .any(|&r| self.game.alliance_requests[r as usize].requestor == other);
        if has_incoming
            && self
                .game
                .config()
                .rules()
                .has(Change::AllianceIncomingBypass3092)
        {
            return true;
        }
        let mut latest: Option<Tick> = None;
        for &r in &self.past_outgoing_alliance_requests {
            let req = &self.game.alliance_requests[r as usize];
            if req.recipient == other && latest.is_none_or(|l| req.tick_created > l) {
                latest = Some(req.tick_created);
            }
        }
        let Some(latest) = latest else {
            return true;
        };
        let delta = self.game.ticks() as f64 - latest as f64;
        delta >= self.game.config().alliance_request_cooldown() as f64
    }

    /// `isTraitor`.
    pub fn is_traitor(&self) -> bool {
        self.get_traitor_remaining_ticks() > 0.0
    }

    /// `getTraitorRemainingTicks`.
    pub fn get_traitor_remaining_ticks(&self) -> f64 {
        if self.marked_traitor_tick < 0.0 {
            return 0.0;
        }
        let elapsed = self.game.ticks() as f64 - self.marked_traitor_tick;
        let duration = self.game.config().traitor_duration() as f64;
        let remaining = duration - elapsed;
        if remaining > 0.0 { remaining } else { 0.0 }
    }

    /// `inDoomsdayClock`.
    pub fn in_doomsday_clock(&self) -> bool {
        self.is_alive() && self.marked_doomsday_clock_tick >= 0.0
    }

    /// `doomsdayClockTicks`.
    pub fn doomsday_clock_ticks(&self) -> f64 {
        if self.in_doomsday_clock() {
            self.game.ticks() as f64 - self.marked_doomsday_clock_tick
        } else {
            0.0
        }
    }

    /// `isDecaying`.
    pub fn is_decaying(&self) -> bool {
        if !self.in_doomsday_clock() || self.rotted_at_tick < 0.0 {
            return false;
        }
        self.game.ticks() as f64 - self.rotted_at_tick <= DECAY_CUE_GRACE_TICKS
    }

    /// `relation(other)`; panics for self like upstream throws.
    pub fn relation(&self, other: SmallId) -> Relation {
        assert!(other != self.id, "cannot get relation with self");
        let r = self.relations.get(&other).copied().unwrap_or(0.0);
        Self::relation_from_value(r)
    }

    /// `relationFromValue`.
    pub fn relation_from_value(value: f64) -> Relation {
        if value < -50.0 {
            Relation::Hostile
        } else if value < 0.0 {
            Relation::Distrustful
        } else if value < 50.0 {
            Relation::Neutral
        } else {
            Relation::Friendly
        }
    }

    /// `allRelationsSorted`: alive players by ascending relation value (stable).
    pub fn all_relations_sorted(&self) -> Vec<(SmallId, Relation)> {
        let mut v: Vec<(SmallId, f64)> = self
            .relations
            .iter()
            .filter(|(p, _)| self.game.player(**p).is_alive())
            .map(|(p, r)| (*p, *r))
            .collect();
        v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter()
            .map(|(p, r)| (p, Self::relation_from_value(r)))
            .collect()
    }

    /// `canTarget(other)`.
    pub fn can_target(&self, other: SmallId) -> bool {
        if self.id == other {
            return false;
        }
        if self.is_friendly(other, false) {
            return false;
        }
        for t in &self.targets {
            if (self.game.ticks() as f64 - t.tick as f64)
                < self.game.config().target_cooldown() as f64
            {
                return false;
            }
        }
        true
    }

    /// `targets()`: still-valid targets in order.
    pub fn targets(&self) -> Vec<SmallId> {
        self.targets
            .iter()
            .filter(|t| {
                (self.game.ticks() as f64 - t.tick as f64)
                    < self.game.config().target_duration() as f64
            })
            .map(|t| t.target)
            .collect()
    }

    /// `transitiveTargets()`: allies' targets then own, de-duplicated by first occurrence.
    pub fn transitive_targets(&self) -> Vec<SmallId> {
        let mut ts: Vec<SmallId> = Vec::new();
        for ally in self.allies() {
            ts.extend(self.game.player(ally).targets());
        }
        ts.extend(self.targets());
        let mut out: Vec<SmallId> = Vec::new();
        for t in ts {
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    /// `outgoingEmojis()`: emojis still on display, newest first.
    pub fn outgoing_emojis(&self) -> Vec<EmojiMessage> {
        let mut v: Vec<EmojiMessage> = self
            .outgoing_emojis
            .iter()
            .filter(|e| {
                (self.game.ticks() as f64 - e.created_at as f64)
                    < self.game.config().emoji_message_duration() as f64
            })
            .cloned()
            .collect();
        v.sort_by_key(|m| std::cmp::Reverse(m.created_at));
        v
    }

    /// `canSendEmoji(recipient)`.
    pub fn can_send_emoji(&self, recipient: EmojiRecipient) -> bool {
        if recipient == EmojiRecipient::Player(self.id) {
            return false;
        }
        if self.game.config().rules().has(Change::EmojiBatch5833) {
            let window = self.game.config().emoji_message_window() as f64;
            let ticks = self.game.ticks() as f64;
            let recent = self
                .outgoing_emojis
                .iter()
                .filter(|m| m.recipient == recipient && ticks - (m.created_at as f64) < window)
                .count();
            let limit = if self.info.player_type == PlayerType::Human {
                self.game.config().emoji_message_limit()
            } else {
                1
            };
            return recent < limit;
        }
        for msg in self
            .outgoing_emojis
            .iter()
            .filter(|m| m.recipient == recipient)
        {
            if (self.game.ticks() as f64 - msg.created_at as f64)
                < self.game.config().emoji_message_cooldown() as f64
            {
                return false;
            }
        }
        true
    }

    /// `canSendQuickChat(recipient)`.
    pub fn can_send_quick_chat(&self, recipient: SmallId) -> bool {
        if recipient == self.id {
            return false;
        }
        match self.outgoing_quick_chats.get(&recipient) {
            None => true,
            Some(&last) => {
                self.game.ticks() as f64 - last as f64
                    >= self.game.config().quick_chat_cooldown() as f64
            }
        }
    }

    fn donation_gate(&self, recipient: SmallId) -> bool {
        for d in &self.sent_donations {
            if d.recipient == recipient
                && (self.game.ticks() as f64 - d.tick as f64)
                    < self.game.config().donate_cooldown() as f64
            {
                return false;
            }
        }
        true
    }

    /// `canDonateGold(recipient)`.
    pub fn can_donate_gold(&self, recipient: SmallId) -> bool {
        if recipient == self.id && self.game.config().rules().has(Change::SelfActionGuards4014) {
            return false;
        }
        let r = self.game.player(recipient);
        if !self.is_alive() || !r.is_alive() || !self.is_friendly(recipient, false) {
            return false;
        }
        if r.player_type() == PlayerType::Human && !self.game.config().donate_gold() {
            return false;
        }
        self.donation_gate(recipient)
    }

    /// `canDonateTroops(recipient)`.
    pub fn can_donate_troops(&self, recipient: SmallId) -> bool {
        if recipient == self.id && self.game.config().rules().has(Change::SelfActionGuards4014) {
            return false;
        }
        let r = self.game.player(recipient);
        if !self.is_alive() || !r.is_alive() || !self.is_friendly(recipient, false) {
            return false;
        }
        if r.player_type() == PlayerType::Human && !self.game.config().donate_troops() {
            return false;
        }
        self.donation_gate(recipient)
    }

    /// `canDeleteUnit`.
    pub fn can_delete_unit(&self) -> bool {
        self.game.ticks() as f64 - self.last_delete_unit_tick
            >= self.game.config().delete_unit_cooldown() as f64
    }

    /// `canEmbargoAll`.
    pub fn can_embargo_all(&self) -> bool {
        if (self.game.ticks() as f64 - self.last_embargo_all_tick)
            < self.game.config().embargo_all_cooldown() as f64
        {
            return false;
        }
        for p in self.game.players() {
            if p == self.id {
                continue;
            }
            if self.game.player(p).player_type() == PlayerType::Bot {
                continue;
            }
            if self.is_on_same_team(p) {
                continue;
            }
            if self
                .game
                .config()
                .rules()
                .has(Change::EmbargoAllSkipsAllies5919)
                && self.is_allied_with(p)
            {
                continue;
            }
            return true;
        }
        false
    }

    /// `canTrade(other)`.
    pub fn can_trade(&self, other: SmallId) -> bool {
        let embargo =
            self.game.player(other).has_embargo_against(self.id) || self.has_embargo_against(other);
        !embargo && other != self.id
    }

    /// `tradingPartners()`.
    pub fn trading_partners(&self) -> Vec<SmallId> {
        self.game
            .players()
            .into_iter()
            .filter(|&o| o != self.id && self.can_trade(o))
            .collect()
    }

    /// `isOnSameTeam(other)`.
    pub fn is_on_same_team(&self, other: SmallId) -> bool {
        if other == self.id {
            return false;
        }
        let other_player = self.game.player_impl(other);
        let (Some(mine), Some(theirs)) = (self.team.as_ref(), other_player.team.as_ref()) else {
            return false;
        };
        let bot = COLORED_TEAMS[7];
        if mine == bot || theirs == bot {
            return false;
        }
        mine == theirs
    }

    /// `isFriendly(other, treatAFKFriendly)`.
    pub fn is_friendly(&self, other: SmallId, treat_afk_friendly: bool) -> bool {
        if other == self.id && self.game.config().rules().has(Change::SelfActionGuards4014) {
            return true;
        }
        if self.game.player(other).is_disconnected && !treat_afk_friendly {
            return false;
        }
        self.is_on_same_team(other) || self.is_allied_with(other)
    }

    /// `findUnitToUpgrade(type, targetTile)`.
    pub fn find_unit_to_upgrade(&self, t: UnitType, target_tile: TileRef) -> Option<UnitId> {
        let unit = self.find_existing_unit_to_upgrade(t, target_tile)?;
        if !self.can_upgrade_unit(unit) {
            return None;
        }
        Some(unit)
    }

    fn find_existing_unit_to_upgrade(&self, t: UnitType, target_tile: TileRef) -> Option<UnitId> {
        let nearby = self.game.nearby_units(
            target_tile,
            self.game.config().structure_min_dist(),
            &[t],
            None,
            true,
        );
        find_closest_by(&nearby, |e| e.1, |_| true).map(|e| e.0)
    }

    fn can_build_unit_type(&self, unit_type: UnitType, known_cost: Option<Gold>) -> bool {
        if self.game.config().is_unit_disabled(unit_type) {
            return false;
        }
        let cost = known_cost.unwrap_or_else(|| self.unit_cost(unit_type, 0));
        if self.gold < cost {
            return false;
        }
        let exempt = unit_type == UnitType::MIRVWarhead
            && self
                .game
                .config()
                .rules()
                .has(Change::MirvWarheadsWhenDead3134);
        if !exempt && !self.is_alive() {
            return false;
        }
        true
    }

    fn can_upgrade_unit_type(&self, unit_type: UnitType) -> bool {
        self.game
            .config()
            .unit_info(unit_type)
            .upgradable
            .unwrap_or(false)
    }

    fn is_unit_valid_to_upgrade(&self, unit: UnitId) -> bool {
        let u = self.game.unit_any(unit);
        !(u.under_construction || u.is_marked_for_deletion() || u.owner != self.id)
    }

    /// `canUpgradeUnit(unit)`.
    pub fn can_upgrade_unit(&self, unit: UnitId) -> bool {
        let ut = self.game.unit_any(unit).unit_type;
        self.can_upgrade_unit_type(ut)
            && self.can_build_unit_type(ut, None)
            && self.is_unit_valid_to_upgrade(unit)
    }

    /// `canBuild(type, targetTile, validTiles)`: the tile to build on, or `None`.
    pub fn can_build(
        &self,
        unit_type: UnitType,
        target_tile: TileRef,
        valid_tiles: Option<&[TileRef]>,
    ) -> Option<TileRef> {
        if !self.can_build_unit_type(unit_type, None) {
            return None;
        }
        self.can_spawn_unit_type(unit_type, target_tile, valid_tiles)
    }

    fn can_spawn_unit_type(
        &self,
        unit_type: UnitType,
        target_tile: TileRef,
        valid_tiles: Option<&[TileRef]>,
    ) -> Option<TileRef> {
        match unit_type {
            UnitType::MIRV => {
                if !self.game.map.has_owner(target_tile) {
                    return None;
                }
                self.nuke_spawn(target_tile, unit_type)
            }
            UnitType::AtomBomb | UnitType::HydrogenBomb => self.nuke_spawn(target_tile, unit_type),
            UnitType::MIRVWarhead | UnitType::Shell | UnitType::SAMMissile => Some(target_tile),
            UnitType::Port => self.port_spawn(target_tile, valid_tiles),
            UnitType::Warship => self.warship_spawn(target_tile),
            UnitType::TransportShip => can_build_transport_ship(self.game, self.id, target_tile),
            UnitType::TradeShip => self.trade_ship_spawn(target_tile),
            UnitType::Train => self.land_based_unit_spawn(target_tile),
            UnitType::MissileSilo
            | UnitType::DefensePost
            | UnitType::SAMLauncher
            | UnitType::City
            | UnitType::Factory => self.land_based_structure_spawn(target_tile, valid_tiles),
        }
    }

    /// `nukeSpawn(tile, nukeType)`: the nearest ready silo's tile.
    pub fn nuke_spawn(&self, tile: TileRef, nuke_type: UnitType) -> Option<TileRef> {
        let mg = self.game;
        if mg.is_spawn_immunity_active() {
            return None;
        }
        if mg.map.is_impassable(tile) {
            return None;
        }
        let owner = mg.owner_id(tile);
        let game_over = mg
            .config()
            .rules()
            .has(crate::Builds::Rules::Change::AftergameTeammateNukes3410)
            && mg.winner().is_some()
            && (!mg
                .config()
                .rules()
                .has(crate::Builds::Rules::Change::AftergameSingleplayer5332)
                || mg.config().game_config().game_type != GameType::Singleplayer);
        if owner != 0 && self.is_on_same_team(owner) && !game_over {
            return None;
        }
        let config = mg.config();
        if config.rules().has(Change::NukeTeammateStructureGuard3105)
            && config.game_config().game_mode == GameMode::Team
            && nuke_type != UnitType::MIRV
            && !game_over
        {
            let magnitude = config.nuke_magnitudes(nuke_type);
            let would_hit_teammate = mg.any_unit_nearby(
                tile,
                magnitude.outer,
                STRUCTURES,
                |unit| unit.owner != 0 && self.is_on_same_team(unit.owner),
                None,
                false,
            );
            if would_hit_teammate {
                return None;
            }
        }
        let mut ready: Vec<UnitId> = self
            .units_of(UnitType::MissileSilo)
            .into_iter()
            .filter(|&s| {
                let u = mg.unit_any(s);
                u.active && !u.is_in_cooldown() && !u.under_construction
            })
            .collect();
        ready.sort_by_key(|&s| mg.map.manhattan_dist(mg.unit_any(s).tile, tile));
        ready.first().map(|&s| mg.unit_any(s).tile)
    }

    /// `portSpawn(tile, validTiles)`.
    pub fn port_spawn(&self, tile: TileRef, valid_tiles: Option<&[TileRef]>) -> Option<TileRef> {
        let mg = self.game;
        if mg.config().rules().has(Change::PortSpawnFirstShore36) {
            let computed;
            let valid: &[TileRef] = match valid_tiles {
                Some(v) => v,
                None => {
                    computed = self.valid_structure_spawn_tiles(tile);
                    &computed
                }
            };
            return valid.iter().copied().find(|&t| mg.map.is_shore(t));
        }
        let filter = manhattan_dist_fn(tile, mg.config().radius_port_spawn(), false);
        let found = mg.map.bfs(tile, |gm, t| filter(gm, t));
        let mut spawns: Vec<TileRef> = found
            .iter()
            .copied()
            .filter(|&t| {
                mg.map.owner_id(t) == self.id
                    && if mg.config().rules().has(Change::TradeWaterComponents31) {
                        mg.map.is_shore(t)
                    } else {
                        mg.map.is_ocean_shore(t)
                    }
            })
            .collect();
        spawns.sort_by_key(|&t| mg.map.manhattan_dist(t, tile));
        let computed;
        let valid: &[TileRef] = match valid_tiles {
            Some(v) => v,
            None => {
                computed = self.valid_structure_spawn_tiles(tile);
                &computed
            }
        };
        spawns.into_iter().find(|t| valid.contains(t))
    }

    /// `warshipSpawn(tile)`: the closest ready port in the tile's water component.
    pub fn warship_spawn(&self, tile: TileRef) -> Option<TileRef> {
        let mg = self.game;
        if !mg.config().rules().has(Change::TradeWaterComponents31) {
            // Before v0.31: the closest port of any water body, as long as the tile is ocean.
            if !mg.map.is_ocean(tile) {
                return None;
            }
            let ports = self.units_of(UnitType::Port);
            return find_closest_by(
                &ports,
                |&p| mg.map.manhattan_dist(mg.unit_any(p).tile, tile) as f64,
                |_| true,
            )
            .map(|&p| mg.unit_any(p).tile);
        }
        if !mg.map.is_water(tile) {
            return None;
        }
        let tile_component = mg.water_manager.get_water_component(&mg.map, tile);
        let ports = self.units_of(UnitType::Port);
        find_closest_by(
            &ports,
            |&p| mg.map.manhattan_dist(mg.unit_any(p).tile, tile) as f64,
            |&p| {
                let u = mg.unit_any(p);
                u.active
                    && !u.under_construction
                    && tile_component
                        .is_some_and(|c| mg.water_manager.has_water_component(&mg.map, u.tile, c))
            },
        )
        .map(|&p| mg.unit_any(p).tile)
    }

    /// `landBasedUnitSpawn(tile)`.
    pub fn land_based_unit_spawn(&self, tile: TileRef) -> Option<TileRef> {
        (self.game.map.is_land(tile) && !self.game.map.is_impassable(tile)).then_some(tile)
    }

    /// `landBasedStructureSpawn(tile, validTiles)`.
    pub fn land_based_structure_spawn(
        &self,
        tile: TileRef,
        valid_tiles: Option<&[TileRef]>,
    ) -> Option<TileRef> {
        match valid_tiles {
            Some(v) => v.first().copied(),
            None => self.valid_structure_spawn_tiles(tile).first().copied(),
        }
    }

    /// `validStructureSpawnTiles(tile)`: own tiles near `tile` far enough from every structure,
    /// nearest first.
    pub fn valid_structure_spawn_tiles(&self, tile: TileRef) -> Vec<TileRef> {
        let mg = self.game;
        if mg.map.owner_id(tile) != self.id {
            return vec![];
        }
        let search_radius = 15.0f64;
        let search_radius_squared = search_radius * search_radius;
        let nearby_units = mg.nearby_units(tile, search_radius * 2.0, STRUCTURES, None, true);
        let map = &mg.map;
        let w = map.width() as i64;
        let cx = tile as i64 % w;
        let cy = tile as i64 / w;
        let small_id = self.small_id;
        let inside = |t: TileRef| -> bool {
            let dx = (t as i64 % w - cx) as f64;
            let dy = (t as i64 / w - cy) as f64;
            dx * dx + dy * dy < search_radius_squared && map.owner_id(t) == small_id
        };
        let mut nearby_tiles: Vec<TileRef> = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack: Vec<TileRef> = Vec::new();
        if inside(tile) {
            visited.insert(tile);
            nearby_tiles.push(tile);
            stack.push(tile);
        }
        while let Some(t) = stack.pop() {
            map.for_each_neighbor_nswe(t, |n| {
                if !visited.contains(&n) && inside(n) {
                    visited.insert(n);
                    nearby_tiles.push(n);
                    stack.push(n);
                }
            });
        }
        let min_dist = mg.config().structure_min_dist();
        let min_dist_squared = min_dist * min_dist;
        let mut valid: Vec<TileRef> = Vec::new();
        for &t in &nearby_tiles {
            let mut blocked = false;
            for &(u, _) in &nearby_units {
                if (map.euclidean_dist_squared(mg.unit_any(u).tile, t) as f64) < min_dist_squared {
                    blocked = true;
                    break;
                }
            }
            if !blocked {
                valid.push(t);
            }
        }
        valid.sort_by_key(|&t| map.euclidean_dist_squared(t, tile));
        valid
    }

    /// `tradeShipSpawn(targetTile)`.
    pub fn trade_ship_spawn(&self, target_tile: TileRef) -> Option<TileRef> {
        self.units_of(UnitType::Port)
            .iter()
            .any(|&p| self.game.unit_any(p).tile == target_tile)
            .then_some(target_tile)
    }

    /// `hash()`: `simpleHash(id) * (troops + tiles) + sum(unit hashes)` in f64.
    pub fn hash(&self) -> f64 {
        let units = self
            .units
            .iter()
            .fold(0.0, |acc, &u| acc + self.game.unit_any(u).hash());
        simple_hash(&self.info.id) as f64 * (self.troops as f64 + self.tiles.len() as f64) + units
    }

    /// `incomingAttacks()`: those whose attacker is alive.
    pub fn incoming_attacks(&self) -> Vec<AttackId> {
        self.incoming_attacks
            .iter()
            .copied()
            .filter(|&a| {
                !self
                    .game
                    .config()
                    .rules()
                    .has(Change::IncomingAttacksAliveOnly31)
                    || self
                        .game
                        .player(self.game.attacks[a as usize].attacker)
                        .is_alive()
            })
            .collect()
    }

    /// `isImmune()`.
    pub fn is_immune(&self) -> bool {
        match self.info.player_type {
            PlayerType::Human => self.game.is_spawn_immunity_active(),
            PlayerType::Nation
                if self
                    .game
                    .config()
                    .rules()
                    .has(Change::NationSpawnImmunity3222) =>
            {
                self.game.is_nation_spawn_immunity_active()
            }
            PlayerType::Nation => false,
            PlayerType::Bot => false,
        }
    }

    /// `canAttackPlayer(player, treatAFKFriendly)`.
    pub fn can_attack_player(&self, player: SmallId, treat_afk_friendly: bool) -> bool {
        let rules = self.game.config().rules();
        let ignores_immunity = if rules.has(Change::NationsIgnoreImmunity4282)
            || !rules.has(Change::NationSpawnImmunity3222)
        {
            self.info.player_type != PlayerType::Human
        } else {
            self.info.player_type == PlayerType::Bot
        };
        if ignores_immunity {
            return !self.is_friendly(player, treat_afk_friendly);
        }
        !self.game.player(player).is_immune() && !self.is_friendly(player, treat_afk_friendly)
    }

    /// `canAttack(tile)`.
    pub fn can_attack(&self, tile: TileRef) -> bool {
        let mg = self.game;
        let owner = mg.owner_id(tile);
        if owner == self.id {
            return false;
        }
        if owner != 0 && !self.can_attack_player(owner, false) {
            return false;
        }
        if !mg.map.is_land(tile) || mg.map.is_impassable(tile) {
            return false;
        }
        if mg.map.has_owner(tile) {
            return self.shares_border_with(owner);
        }
        let filter = manhattan_dist_fn(tile, 200.0, false);
        let found = mg.map.bfs(tile, |gm: &GameMapImpl, t| {
            !gm.has_owner(t) && gm.is_land(t) && !gm.is_impassable(t) && filter(gm, t)
        });
        for t in found.iter() {
            for n in mg.map.neighbors(*t) {
                if mg.map.owner_id(n) == self.id {
                    return true;
                }
            }
        }
        false
    }

    /// `bestTransportShipSpawn(targetTile)`.
    pub fn best_transport_ship_spawn(&self, target_tile: TileRef) -> Option<TileRef> {
        best_shore_deployment_source(self.game, self.id, target_tile)
    }
}

impl PlayerMut<'_> {
    /// Read view of the same player.
    pub fn as_ref(&self) -> PlayerRef<'_> {
        PlayerRef {
            game: self.game,
            id: self.id,
        }
    }

    fn p(&mut self) -> &mut PlayerImpl {
        self.game.player_impl_mut(self.id)
    }

    /// `setTroops`.
    pub fn set_troops(&mut self, troops: f64) {
        self.p().troops = to_int(troops);
    }

    /// `conquer(tile)`.
    pub fn conquer(&mut self, tile: TileRef) {
        self.game.conquer(self.id, tile);
    }

    /// `orderRetreat(id)`.
    pub fn order_retreat(&mut self, id: &str) {
        let found = self
            .outgoing_attacks
            .iter()
            .copied()
            .find(|&a| self.game.attacks[a as usize].id == id);
        if let Some(a) = found {
            self.game.attacks[a as usize].order_retreat();
        }
    }

    /// `executeRetreat(id)`.
    pub fn execute_retreat(&mut self, id: &str) {
        let found = self
            .outgoing_attacks
            .iter()
            .copied()
            .find(|&a| self.game.attacks[a as usize].id == id);
        if let Some(a) = found {
            self.game.attacks[a as usize].execute_retreat();
        }
    }

    /// `relinquish(tile)`.
    pub fn relinquish(&mut self, tile: TileRef) {
        assert!(
            self.game.owner_id(tile) == self.id,
            "Cannot relinquish tile not owned by this player"
        );
        self.game.relinquish(tile);
    }

    /// `markTraitor`.
    pub fn mark_traitor(&mut self) {
        let t = self.game.ticks() as f64;
        let p = self.p();
        p.marked_traitor_tick = t;
        p.betrayal_count += 1;
    }

    /// `enterDoomsdayClock`.
    pub fn enter_doomsday_clock(&mut self) {
        let t = self.game.ticks() as f64;
        let p = self.p();
        if p.marked_doomsday_clock_tick < 0.0 {
            p.marked_doomsday_clock_tick = t;
        }
    }

    /// `clearDoomsdayClock`.
    pub fn clear_doomsday_clock(&mut self) {
        let p = self.p();
        p.marked_doomsday_clock_tick = -1.0;
        p.rotted_at_tick = -1.0;
    }

    /// `markRotted`.
    pub fn mark_rotted(&mut self) {
        let t = self.game.ticks() as f64;
        self.p().rotted_at_tick = t;
    }

    /// `createAllianceRequest(recipient)`.
    pub fn create_alliance_request(&mut self, recipient: SmallId) -> Option<AllianceRequestId> {
        assert!(
            !self.as_ref().is_allied_with(recipient),
            "cannot create alliance request, already allies"
        );
        self.game.create_alliance_request(self.id, recipient)
    }

    /// `updateRelation(other, delta)`.
    pub fn update_relation(&mut self, other: SmallId, delta: f64) {
        assert!(other != self.id, "cannot update relation with self");
        let relation = self.relations.get(&other).copied().unwrap_or(0.0);
        let new_relation = within(relation + delta, -100.0, 100.0);
        self.p().relations.insert(other, new_relation);
    }

    /// `decayRelations`.
    pub fn decay_relations(&mut self) {
        for r in self.p().relations.values_mut() {
            let sign = -crate::Compat::JsNumber::sign(*r);
            let delta = 0.05;
            let mut v = *r;
            v += sign * delta;
            if v.abs() < delta * 2.0 {
                v = 0.0;
            }
            *r = v;
        }
    }

    /// `target(other)`.
    pub fn target(&mut self, other: SmallId) {
        let tick = self.game.ticks();
        self.p().targets.push(TargetEntry {
            tick,
            target: other,
        });
    }

    /// `sendEmoji(recipient, emoji)`; panics when sending to oneself.
    pub fn send_emoji(&mut self, recipient: EmojiRecipient, emoji: &str) {
        assert!(
            recipient != EmojiRecipient::Player(self.id),
            "Cannot send emoji to oneself"
        );
        let msg = EmojiMessage {
            message: emoji.to_string(),
            sender_id: self.id,
            recipient,
            created_at: self.game.ticks(),
        };
        self.p().outgoing_emojis.push(msg);
    }

    /// `recordQuickChat(recipient)`.
    pub fn record_quick_chat(&mut self, recipient: SmallId) {
        let t = self.game.ticks();
        self.p().outgoing_quick_chats.insert(recipient, t);
    }

    /// `donateTroops(recipient, troops)`.
    pub fn donate_troops(&mut self, recipient: SmallId, troops: f64) -> bool {
        if recipient == self.id && self.game.config().rules().has(Change::SelfActionGuards4014) {
            return false;
        }
        if troops <= 0.0 {
            return false;
        }
        let removed = self.remove_troops(troops);
        if removed == 0.0 {
            return false;
        }
        self.game.player_mut(recipient).add_troops(removed);
        let tick = self.game.ticks();
        self.p().sent_donations.push(Donation { recipient, tick });
        true
    }

    /// `donateGold(recipient, gold)`.
    pub fn donate_gold(&mut self, recipient: SmallId, gold: Gold) -> bool {
        if recipient == self.id && self.game.config().rules().has(Change::SelfActionGuards4014) {
            return false;
        }
        if gold <= 0 {
            return false;
        }
        let removed = self.remove_gold(gold);
        if removed == 0 {
            return false;
        }
        self.game.player_mut(recipient).add_gold(removed);
        let tick = self.game.ticks();
        self.p().sent_donations.push(Donation { recipient, tick });
        true
    }

    /// `recordDeleteUnit`.
    pub fn record_delete_unit(&mut self) {
        let t = self.game.ticks() as f64;
        self.p().last_delete_unit_tick = t;
    }

    /// `recordEmbargoAll`.
    pub fn record_embargo_all(&mut self) {
        let t = self.game.ticks() as f64;
        self.p().last_embargo_all_tick = t;
    }

    /// `addEmbargo(other, isTemporary)`.
    pub fn add_embargo(&mut self, other: SmallId, is_temporary: bool) {
        if let Some(e) = self.embargoes.get(&other)
            && !e.is_temporary
        {
            return;
        }
        let created_at = self.game.ticks();
        self.p().embargoes.insert(
            other,
            Embargo {
                created_at,
                is_temporary,
                target: other,
            },
        );
    }

    /// `stopEmbargo(other)`.
    pub fn stop_embargo(&mut self, other: SmallId) {
        self.p().embargoes.remove(&other);
    }

    /// `endTemporaryEmbargo(other)`.
    pub fn end_temporary_embargo(&mut self, other: SmallId) {
        if let Some(e) = self.embargoes.get(&other)
            && !e.is_temporary
        {
            return;
        }
        self.stop_embargo(other);
    }

    /// `addTradeGold`.
    pub fn add_trade_gold(&mut self, to_add: Gold) {
        self.p().trade_gold += to_add;
    }

    /// `addTrainGold`.
    pub fn add_train_gold(&mut self, to_add: Gold) {
        self.p().train_gold += to_add;
    }

    /// `addPiracyGold`.
    pub fn add_piracy_gold(&mut self, to_add: Gold) {
        self.p().piracy_gold += to_add;
    }

    /// `addGold(toAdd)`.
    pub fn add_gold(&mut self, to_add: Gold) {
        let p = self.p();
        p.gold += to_add;
        p.gold_earned += to_add;
    }

    /// `removeGold(toRemove)`: returns the amount actually removed.
    pub fn remove_gold(&mut self, to_remove: Gold) -> Gold {
        if to_remove <= 0 {
            return 0;
        }
        let p = self.p();
        let actual = p.gold.min(to_remove);
        p.gold -= actual;
        actual
    }

    /// `addTroops(troops)`.
    pub fn add_troops(&mut self, troops: f64) {
        if troops < 0.0 {
            self.remove_troops(-troops);
            return;
        }
        self.p().troops += to_int(troops);
    }

    /// `removeTroops(troops)`: returns the amount actually removed.
    pub fn remove_troops(&mut self, troops: f64) -> f64 {
        if troops <= 0.0 {
            return 0.0;
        }
        let p = self.p();
        let to_remove = p.troops.min(to_int(troops));
        p.troops -= to_remove;
        to_remove as f64
    }

    /// `captureUnit(unit)`.
    pub fn capture_unit(&mut self, unit: UnitId) {
        assert!(
            self.game.unit_any(unit).owner != self.id,
            "Cannot capture unit, already owned"
        );
        let id = self.id;
        self.game.unit_mut(unit).set_owner(id);
    }

    /// `buildUnit(type, spawnTile, params)`.
    pub fn build_unit(
        &mut self,
        unit_type: UnitType,
        spawn_tile: TileRef,
        params: UnitParams,
    ) -> UnitId {
        assert!(
            !self.game.config().is_unit_disabled(unit_type),
            "Attempted to build disabled unit {} at tile {spawn_tile} by player {}",
            unit_type.as_str(),
            self.name()
        );
        let cost = self.as_ref().unit_cost(unit_type, 0);
        let troops = params.troops.unwrap_or(0.0);
        let id = self
            .game
            .create_unit(unit_type, spawn_tile, self.id, params);
        let p = self.p();
        p.units.push(id);
        p.my_units_version += 1;
        p.num_units_constructed[unit_type.index()] += 1;
        self.remove_gold(cost);
        self.remove_troops(troops);
        self.game.add_unit(id);
        id
    }

    /// `upgradeUnit(unit)`.
    pub fn upgrade_unit(&mut self, unit: UnitId) {
        let t = self.game.unit_any(unit).unit_type;
        let cost = self.as_ref().unit_cost(t, 0);
        self.remove_gold(cost);
        self.game.unit_mut(unit).increase_level();
        self.p().num_units_constructed[t.index()] += 1;
    }

    /// `markDisconnected(isDisconnected, snapshot)`.
    pub fn mark_disconnected(
        &mut self,
        is_disconnected: bool,
        snapshot: Option<DisconnectSnapshot>,
    ) {
        self.p().is_disconnected = is_disconnected;
        if is_disconnected {
            if self.disconnect_snapshot.is_none() {
                let snap = snapshot.unwrap_or_else(|| {
                    let team_tiles = match self.team.clone() {
                        Some(t) => self.game.team_tiles_owned(&t),
                        None => 0.0,
                    };
                    DisconnectSnapshot {
                        current_tick: self.game.ticks(),
                        team_tiles,
                        total_land: self.game.total_land_tiles(),
                        was_alive: self.is_alive(),
                    }
                });
                self.p().disconnect_snapshot = Some(snap);
            }
        } else {
            self.p().disconnect_snapshot = None;
        }
    }

    /// `createAttack(target, troops, sourceTile, border)`.
    pub fn create_attack(
        &mut self,
        target: SmallId,
        troops: f64,
        source_tile: Option<TileRef>,
        border: OrderedSet<TileRef>,
    ) -> AttackId {
        let attack_id = self.p().pseudo_random.next_id();
        let attack = super::AttackImpl::AttackImpl::new(
            attack_id,
            target,
            self.id,
            troops,
            source_tile,
            border,
        );
        self.game.attacks.push(attack);
        let id = (self.game.attacks.len() - 1) as AttackId;
        self.p().outgoing_attacks.push(id);
        if target != 0 {
            self.game.player_impl_mut(target).incoming_attacks.push(id);
        }
        id
    }

    /// `breakAlliance(alliance)`.
    pub fn break_alliance(&mut self, alliance: AllianceId) {
        self.game.break_alliance(self.id, alliance);
    }

    /// `removeAllAlliances`.
    pub fn remove_all_alliances(&mut self) {
        self.game.remove_alliances_by_player_silently(self.id);
    }
}

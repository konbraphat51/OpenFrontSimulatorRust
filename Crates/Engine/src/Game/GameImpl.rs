// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/GameImpl.ts: the game state and the per-tick loop.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! `GameImpl` owns everything (map, players, units, attacks, alliances, executions); players,
//! units, attacks and alliances are addressed by small id / unit id / arena index. Skipped as
//! client plumbing: the update queues (`addUpdate`, `drainPacked*`, `recordTileUpdate`, motion
//! plans, `displayMessage`/`displayChat`/`displayIncomingUnit`, `makeWinner`, `sendEmojiUpdate`,
//! the nuke impact queue), per-tick `recordTickSample` stats, and the units-by-type memos (pure
//! caches). `isPaused` is stored but nothing in the simulation reads it.

use super::AllianceImpl::AllianceImpl;
use super::AllianceRequestImpl::{AllianceRequestImpl, RequestStatus};
use super::AttackImpl::AttackImpl;
use super::Game::{
    AllianceId, AllianceRequestId, AttackId, COLORED_TEAMS, Cell, GameMode, Gold, Nation,
    PlayerInfo, PlayerType, SmallId, Team, TeamCountConfig, TeamGameSpawnAreas, Tick, UnitId,
    UnitParams, UnitType,
};
use super::GameMap::{GameMapImpl, TileRef};
use super::PlayerImpl::{PlayerImpl, PlayerMut, PlayerRef};
use super::RailNetwork::RailNetwork;
use super::StatsImpl::StatsImpl;
use super::TeamAssignment::{assign_teams, resolve_teams_list};
use super::UnitGrid::{UnitGrid, UnitPredicate};
use super::UnitImpl::{UnitImpl, UnitMut, UnitRef};
use super::WaterManager::WaterManager;
use crate::Configuration::Config::Config;
use crate::Core::Util::simple_hash;
use crate::Execution::Execution::Execution;
use crate::Execution::Nation::SharedWaterCache::{SharedWaterCache, SharedWaterView};
use ofsim_map::SpawnArea;
use std::collections::{HashMap, HashSet};

/// Winner of a game: a player or a team.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Winner {
    /// A single player.
    Player(SmallId),
    /// A team.
    Team(Team),
}

/// Generation-stamped scratch buffers shared by tile traversals (`TileTraversalScratch`).
#[derive(Debug, Default)]
pub struct TileTraversalScratch {
    /// `visited[tile] == gen` means seen in the current pass.
    pub visited: Vec<u32>,
    /// Reusable DFS stack.
    pub stack: Vec<TileRef>,
    /// Cluster index plus one per tile (`PlayerExecution`).
    pub cluster_index_map: Vec<i32>,
    /// Current generation.
    pub gen_: u32,
}

impl TileTraversalScratch {
    /// `bumpTraversalGeneration`: starts a new pass and returns its stamp.
    pub fn bump_generation(&mut self) -> u32 {
        self.gen_ += 1;
        if self.gen_ == 0xffff_ffff {
            self.visited.fill(0);
            self.gen_ = 1;
        }
        self.gen_
    }
}

/// The whole game state.
pub struct GameImpl {
    ticks: Tick,
    start_tick: Option<Tick>,
    un_init_execs: Vec<Box<dyn Execution>>,
    execs: Vec<Box<dyn Execution>>,
    /// Players in insertion order; small id `n` is at index `n - 1`.
    pub(crate) players: Vec<PlayerImpl>,
    players_by_id: HashMap<String, SmallId>,
    players_by_client_id: HashMap<String, SmallId>,
    /// Unit arena: unit id `n` is at index `n - 1`; deleted units stay (inactive).
    pub(crate) units: Vec<UnitImpl>,
    /// Attack arena (`AttackId` is the index).
    pub(crate) attacks: Vec<AttackImpl>,
    /// Alliance arena (`AllianceId` is the index and the upstream id).
    pub(crate) alliances: Vec<AllianceImpl>,
    /// Alliance request arena.
    pub(crate) alliance_requests: Vec<AllianceRequestImpl>,
    /// Pending requests, upstream `allianceRequests`.
    pub(crate) pending_requests: Vec<AllianceRequestId>,
    next_player_id: u32,
    next_unit_id: u32,
    next_alliance_id: u32,
    unit_grid: UnitGrid,
    /// The game map.
    pub map: GameMapImpl,
    config: Config,
    stats: StatsImpl,
    /// Rail network (stub until the train phase).
    pub rail_network: RailNetwork,
    /// Water manager (stub until p2-path lands).
    pub water_manager: WaterManager,
    shared_water_cache: SharedWaterCache,
    humans: Vec<PlayerInfo>,
    nations: Vec<Nation>,
    player_teams: Vec<Team>,
    bot_team: Team,
    is_paused: bool,
    winner: Option<Winner>,
    team_game_spawn_areas: Option<TeamGameSpawnAreas>,
    units_version: u32,
    territory_version: u32,
    /// Shared tile traversal scratch.
    pub scratch: TileTraversalScratch,
}

impl GameImpl {
    /// `createGame` / the `GameImpl` constructor.
    pub fn new(
        humans: Vec<PlayerInfo>,
        nations: Vec<Nation>,
        map: GameMapImpl,
        mini_map: GameMapImpl,
        config: Config,
        team_game_spawn_areas: Option<TeamGameSpawnAreas>,
    ) -> Self {
        let unit_grid = UnitGrid::new(&map);
        let water_manager = WaterManager::new(mini_map, config.disable_nav_mesh());
        let tiles = map.width() as usize * map.height() as usize;
        let mut game = Self {
            ticks: 0,
            start_tick: None,
            un_init_execs: Vec::new(),
            execs: Vec::new(),
            players: Vec::new(),
            players_by_id: HashMap::new(),
            players_by_client_id: HashMap::new(),
            units: Vec::new(),
            attacks: Vec::new(),
            alliances: Vec::new(),
            alliance_requests: Vec::new(),
            pending_requests: Vec::new(),
            next_player_id: 1,
            next_unit_id: 1,
            next_alliance_id: 0,
            unit_grid,
            map,
            config,
            stats: StatsImpl::default(),
            rail_network: RailNetwork::new(),
            water_manager,
            shared_water_cache: SharedWaterCache::new(),
            humans,
            nations,
            player_teams: Vec::new(),
            bot_team: COLORED_TEAMS[7].to_string(),
            is_paused: false,
            winner: None,
            team_game_spawn_areas,
            units_version: 0,
            territory_version: 0,
            scratch: TileTraversalScratch {
                visited: vec![0; tiles],
                stack: Vec::new(),
                cluster_index_map: vec![0; tiles],
                gen_: 0,
            },
        };
        if game.config.game_config().game_mode == GameMode::Team {
            game.populate_teams();
        }
        game.add_players();
        game
    }

    fn populate_teams(&mut self) {
        let total = self.humans.len() + self.nations.len();
        self.player_teams = resolve_teams_list(self.config.player_teams(), total);
    }

    fn add_players(&mut self) {
        let humans = self.humans.clone();
        let nations = self.nations.clone();
        if self.config.game_config().game_mode == GameMode::FFA {
            for p in &humans {
                self.add_player(p.clone(), None);
            }
            for n in &nations {
                self.add_player(n.player_info.clone(), None);
            }
            return;
        }
        if self.config.player_teams() == TeamCountConfig::HumansVsNations {
            for p in &humans {
                self.add_player(p.clone(), Some(COLORED_TEAMS[8].to_string()));
            }
            for n in &nations {
                self.add_player(n.player_info.clone(), Some(COLORED_TEAMS[9].to_string()));
            }
            return;
        }
        let mut all_players = humans;
        all_players.extend(nations.iter().map(|n| n.player_info.clone()));
        let pt = self.config.player_teams();
        let is_duos_trios_quads = matches!(
            pt,
            TeamCountConfig::Duos | TeamCountConfig::Trios | TeamCountConfig::Quads
        );
        let assignment = assign_teams(
            &all_players,
            &self.player_teams.clone(),
            is_duos_trios_quads,
            None,
        );
        for (i, team) in assignment {
            match team {
                None => continue,
                Some(team) => {
                    self.add_player(all_players[i].clone(), Some(team));
                }
            }
        }
    }

    // ---- accessors ----------------------------------------------------------------------

    /// `config()`.
    pub fn config(&self) -> &Config {
        &self.config
    }
    /// `stats()`.
    pub fn stats(&self) -> &StatsImpl {
        &self.stats
    }
    /// `stats()` for mutation.
    pub fn stats_mut(&mut self) -> &mut StatsImpl {
        &mut self.stats
    }
    /// `ticks()`.
    pub fn ticks(&self) -> Tick {
        self.ticks
    }
    /// `isPaused`.
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }
    /// `setPaused`.
    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
    }
    /// `nations()`.
    pub fn nations(&self) -> &[Nation] {
        &self.nations
    }
    /// `inSpawnPhase`.
    pub fn in_spawn_phase(&self) -> bool {
        self.start_tick.is_none()
    }
    /// `endSpawnPhase`.
    pub fn end_spawn_phase(&mut self) {
        if self.start_tick.is_some() {
            return;
        }
        self.start_tick = Some(self.ticks);
    }
    /// `getWinner`.
    pub fn winner(&self) -> Option<&Winner> {
        self.winner.as_ref()
    }
    /// `setWinner` (the stats snapshot upstream attaches is client plumbing).
    pub fn set_winner(&mut self, winner: Option<Winner>) {
        self.winner = winner;
    }
    /// `unitsVersion` (bumped on any unit list change).
    pub fn units_version(&self) -> u32 {
        self.units_version
    }
    /// `bumpUnitsVersion`.
    pub fn bump_units_version(&mut self) {
        self.units_version += 1;
    }
    /// `territoryVersion`.
    pub fn territory_version(&self) -> u32 {
        self.territory_version
    }

    /// `owner(tile)` as a small id (0 = terra nullius).
    pub fn owner_id(&self, tile: TileRef) -> SmallId {
        self.map.owner_id(tile)
    }
    /// `hasOwner`.
    pub fn has_owner(&self, tile: TileRef) -> bool {
        self.map.has_owner(tile)
    }
    /// `isLand`.
    pub fn is_land(&self, tile: TileRef) -> bool {
        self.map.is_land(tile)
    }
    /// `isWater`.
    pub fn is_water(&self, tile: TileRef) -> bool {
        self.map.is_water(tile)
    }
    /// `isValidRef`.
    pub fn is_valid_ref(&self, tile: TileRef) -> bool {
        self.map.is_valid_ref(tile)
    }
    /// `isValidCoord`.
    pub fn is_valid_coord(&self, x: i32, y: i32) -> bool {
        self.map.is_valid_coord(x, y)
    }
    /// `ref(x, y)`.
    pub fn tile_ref(&self, x: i32, y: i32) -> TileRef {
        self.map.tile_ref(x, y)
    }
    /// `x(tile)`.
    pub fn x(&self, tile: TileRef) -> u32 {
        self.map.x(tile)
    }
    /// `y(tile)`.
    pub fn y(&self, tile: TileRef) -> u32 {
        self.map.y(tile)
    }
    /// `width()`.
    pub fn width(&self) -> u32 {
        self.map.width()
    }
    /// `height()`.
    pub fn height(&self) -> u32 {
        self.map.height()
    }
    /// `manhattanDist`.
    pub fn manhattan_dist(&self, a: TileRef, b: TileRef) -> u32 {
        self.map.manhattan_dist(a, b)
    }
    /// `isOnMap(cell)`.
    pub fn is_on_map(&self, cell: Cell) -> bool {
        cell.x >= 0
            && (cell.x as i64) < self.map.width() as i64
            && cell.y >= 0
            && (cell.y as i64) < self.map.height() as i64
    }
    /// `numLandTiles`.
    pub fn num_land_tiles(&self) -> f64 {
        self.map.num_land_tiles() as f64
    }
    /// `numTilesWithFallout`.
    pub fn num_tiles_with_fallout(&self) -> f64 {
        self.map.num_tiles_with_fallout() as f64
    }

    // ---- handles --------------------------------------------------------------------------

    /// The player data (panics on an unknown small id).
    pub fn player_impl(&self, id: SmallId) -> &PlayerImpl {
        &self.players[id as usize - 1]
    }
    /// The player data for mutation.
    pub fn player_impl_mut(&mut self, id: SmallId) -> &mut PlayerImpl {
        &mut self.players[id as usize - 1]
    }
    /// Read handle on a player.
    pub fn player(&self, id: SmallId) -> PlayerRef<'_> {
        PlayerRef { game: self, id }
    }
    /// Mutating handle on a player.
    pub fn player_mut(&mut self, id: SmallId) -> PlayerMut<'_> {
        PlayerMut { game: self, id }
    }
    /// `hasPlayer(id)`.
    pub fn has_player(&self, id: &str) -> bool {
        self.players_by_id.contains_key(id)
    }
    /// `player(id)`: small id of the player with that id string; panics when unknown.
    pub fn player_id(&self, id: &str) -> SmallId {
        match self.players_by_id.get(id) {
            Some(&s) => s,
            None => panic!("Player with id {id} not found"),
        }
    }
    /// Small id of the player with that id string, if any.
    pub fn find_player(&self, id: &str) -> Option<SmallId> {
        self.players_by_id.get(id).copied()
    }
    /// `playerByClientID(id)`.
    pub fn player_by_client_id(&self, id: &str) -> Option<SmallId> {
        self.players_by_client_id.get(id).copied()
    }
    /// `players()`: alive players in insertion order.
    pub fn players(&self) -> Vec<SmallId> {
        self.players
            .iter()
            .filter(|p| p.is_alive())
            .map(|p| p.small_id)
            .collect()
    }
    /// `allPlayers()`: every player in insertion order.
    pub fn all_players(&self) -> Vec<SmallId> {
        self.players.iter().map(|p| p.small_id).collect()
    }

    /// A unit by id, including deleted ones (executions that kept the id).
    pub fn unit_any(&self, id: UnitId) -> &UnitImpl {
        &self.units[id as usize - 1]
    }
    /// A unit by id for direct field mutation.
    pub fn unit_any_mut(&mut self, id: UnitId) -> &mut UnitImpl {
        &mut self.units[id as usize - 1]
    }
    /// `unit(id)`: `None` once the unit left the unit map.
    pub fn unit(&self, id: UnitId) -> Option<UnitRef<'_>> {
        let u = self.units.get((id as usize).checked_sub(1)?)?;
        u.in_unit_map.then_some(UnitRef { game: self, id })
    }
    /// Read handle on a unit regardless of activity.
    pub fn unit_ref(&self, id: UnitId) -> UnitRef<'_> {
        UnitRef { game: self, id }
    }
    /// Mutating handle on a unit.
    pub fn unit_mut(&mut self, id: UnitId) -> UnitMut<'_> {
        UnitMut { game: self, id }
    }
    /// An attack by handle (attacks are never freed, so deleted ones stay readable).
    pub fn attack(&self, id: AttackId) -> &AttackImpl {
        &self.attacks[id as usize]
    }
    /// An attack by handle for mutation.
    pub fn attack_mut(&mut self, id: AttackId) -> &mut AttackImpl {
        &mut self.attacks[id as usize]
    }
    /// An alliance by handle (the upstream alliance id).
    pub fn alliance(&self, id: AllianceId) -> &AllianceImpl {
        &self.alliances[id as usize]
    }
    /// An alliance by handle for mutation.
    pub fn alliance_mut(&mut self, id: AllianceId) -> &mut AllianceImpl {
        &mut self.alliances[id as usize]
    }
    /// An alliance request by handle.
    pub fn alliance_request(&self, id: AllianceRequestId) -> &AllianceRequestImpl {
        &self.alliance_requests[id as usize]
    }
    /// The pending alliance requests in order (upstream `allianceRequests`).
    pub fn pending_alliance_requests(&self) -> &[AllianceRequestId] {
        &self.pending_requests
    }
    /// `nextUnitID`.
    pub fn next_unit_id(&mut self) -> UnitId {
        let old = self.next_unit_id;
        self.next_unit_id += 1;
        old
    }

    /// Creates the unit data in the arena (not yet in the owner's list or the grid); used by
    /// `PlayerMut::build_unit`.
    pub(crate) fn create_unit(
        &mut self,
        t: UnitType,
        tile: TileRef,
        owner: SmallId,
        params: UnitParams,
    ) -> UnitId {
        let id = self.next_unit_id();
        let unit = UnitImpl::new(self, t, tile, id, owner, params);
        assert_eq!(
            self.units.len() + 1,
            id as usize,
            "unit ids must be sequential"
        );
        self.units.push(unit);
        id
    }

    /// `units(types)`: every player's units of those types, players in order.
    pub fn units_of_types(&self, types: &[UnitType]) -> Vec<UnitId> {
        let mut out = Vec::new();
        for p in &self.players {
            for &u in &p.units {
                if types.contains(&self.unit_any(u).unit_type) {
                    out.push(u);
                }
            }
        }
        out
    }
    /// `units(type)`.
    pub fn units_of(&self, t: UnitType) -> Vec<UnitId> {
        self.units_of_types(&[t])
    }
    /// `units()`.
    pub fn all_units(&self) -> Vec<UnitId> {
        self.players
            .iter()
            .flat_map(|p| p.units.iter().copied())
            .collect()
    }
    /// `unitCount(type)`: level-weighted count across all players.
    pub fn unit_count(&self, t: UnitType) -> i64 {
        let mut total = 0;
        for p in &self.players {
            total += self.player(p.small_id).unit_count(t);
        }
        total
    }

    // ---- unit grid ------------------------------------------------------------------------

    /// `addUnit`.
    pub fn add_unit(&mut self, id: UnitId) {
        self.units_version += 1;
        let (t, tile) = {
            let u = self.unit_any(id);
            (u.unit_type, u.tile)
        };
        self.unit_grid.add_unit(&self.map, id, t, tile);
        self.units[id as usize - 1].in_unit_map = true;
    }
    /// `removeUnit`.
    pub fn remove_unit(&mut self, id: UnitId) {
        self.units_version += 1;
        let (t, tile, has_station) = {
            let u = self.unit_any(id);
            (u.unit_type, u.tile, u.has_train_station)
        };
        self.unit_grid.remove_unit(&self.map, id, t, tile);
        self.units[id as usize - 1].in_unit_map = false;
        if has_station {
            self.rail_network.remove_station(id);
        }
    }
    /// `updateUnitTile`.
    pub fn update_unit_tile(&mut self, id: UnitId) {
        let (t, tile, last) = {
            let u = self.unit_any(id);
            (u.unit_type, u.tile, u.last_tile)
        };
        self.unit_grid
            .update_unit_cell(&self.map, id, t, tile, last);
    }
    /// `hasUnitNearby`.
    pub fn has_unit_nearby(
        &self,
        tile: TileRef,
        search_range: f64,
        t: UnitType,
        player: Option<SmallId>,
        include_under_construction: bool,
    ) -> bool {
        self.unit_grid.has_unit_nearby(
            &self.map,
            &self.units,
            tile,
            search_range,
            t,
            player,
            include_under_construction,
        )
    }
    /// `anyUnitNearby`.
    pub fn any_unit_nearby(
        &self,
        tile: TileRef,
        search_range: f64,
        types: &[UnitType],
        predicate: impl FnMut(&UnitImpl) -> bool,
        player: Option<SmallId>,
        include_under_construction: bool,
    ) -> bool {
        self.unit_grid.any_unit_nearby(
            &self.map,
            &self.units,
            tile,
            search_range,
            types,
            predicate,
            player,
            include_under_construction,
        )
    }
    /// `nearbyUnits`: (unit id, distSquared) pairs.
    pub fn nearby_units(
        &self,
        tile: TileRef,
        search_range: f64,
        types: &[UnitType],
        predicate: Option<UnitPredicate<'_>>,
        include_under_construction: bool,
    ) -> Vec<(UnitId, f64)> {
        self.unit_grid.nearby_units(
            &self.map,
            &self.units,
            tile,
            search_range,
            types,
            predicate,
            include_under_construction,
        )
    }

    /// `sharedWaterComponents(player)`: water components shared with a trade partner (see `SharedWaterCache`).
    pub fn shared_water_components(&mut self, player: SmallId) -> Option<HashSet<i64>> {
        let mut cache = std::mem::take(&mut self.shared_water_cache);
        let result = cache.get(&*self, self.ticks, player).cloned();
        self.shared_water_cache = cache;
        result
    }

    // ---- executions -----------------------------------------------------------------------

    /// `addExecution`.
    pub fn add_execution(&mut self, exec: Box<dyn Execution>) {
        self.un_init_execs.push(exec);
    }

    /// `executeNextTick`: the per-tick loop. Returns `GameImpl::hash()` when `ticks % 10 == 0`
    /// (the value upstream writes into the record), evaluated before the water manager and the
    /// tick increment.
    pub fn execute_next_tick(&mut self) -> Option<f64> {
        let ticks = self.ticks;
        let mut execs = std::mem::take(&mut self.execs);
        for e in execs.iter_mut() {
            if (!self.in_spawn_phase() || e.active_during_spawn_phase()) && e.is_active() {
                e.tick(self, ticks);
            }
        }
        // Executions added by the ticks above are in `un_init_execs`; init them. Executions added
        // by an `init` land in the (now empty) list and are dropped below, like upstream's
        // reassignment of `unInitExecs`.
        let un_init = std::mem::take(&mut self.un_init_execs);
        let mut inited: Vec<Box<dyn Execution>> = Vec::new();
        let mut un_inited: Vec<Box<dyn Execution>> = Vec::new();
        for mut e in un_init {
            if !self.in_spawn_phase() || e.active_during_spawn_phase() {
                e.init(self, ticks);
                inited.push(e);
            } else {
                un_inited.push(e);
            }
        }
        // removeInactiveExecutions
        let in_spawn_phase = self.in_spawn_phase();
        execs.retain(|e| {
            if in_spawn_phase {
                !e.active_during_spawn_phase() || e.is_active()
            } else {
                e.is_active()
            }
        });
        execs.extend(inited);
        self.execs = execs;
        self.un_init_execs = un_inited;
        let hash = self.ticks.is_multiple_of(10).then(|| self.hash());
        // Water manager flush (the changed-tile list only feeds the client).
        let ticks_now = self.ticks;
        self.water_manager.tick(&mut self.map, ticks_now);
        self.ticks += 1;
        hash
    }

    /// `hash()`: `1 + sum of player hashes` in f64, players in insertion order.
    pub fn hash(&self) -> f64 {
        let mut hash = 1.0;
        for p in &self.players {
            hash += self.player(p.small_id).hash();
        }
        hash
    }

    // ---- players --------------------------------------------------------------------------

    /// `addPlayer(info, team)`.
    pub fn add_player(&mut self, info: PlayerInfo, team: Option<Team>) -> SmallId {
        let team = match team {
            Some(t) => Some(t),
            None => self.maybe_assign_team(&info),
        };
        let start = self
            .config
            .start_manpower(info.player_type, info.is_lobby_creator);
        let small_id = self.next_player_id as SmallId;
        let player = PlayerImpl::new(self, small_id, info.clone(), start, team);
        self.players.push(player);
        self.next_player_id += 1;
        self.players_by_id.insert(info.id.clone(), small_id);
        if let Some(c) = info.client_id.clone() {
            self.players_by_client_id.entry(c).or_insert(small_id);
        }
        small_id
    }

    fn maybe_assign_team(&self, player: &PlayerInfo) -> Option<Team> {
        if self.config.game_config().game_mode != GameMode::Team {
            return None;
        }
        if player.player_type == PlayerType::Bot {
            return Some(self.bot_team.clone());
        }
        let rand = simple_hash(&player.id) as usize;
        Some(self.player_teams[rand % self.player_teams.len()].clone())
    }

    // ---- territory ------------------------------------------------------------------------

    /// `setFallout(tile, value)`.
    pub fn set_fallout(&mut self, tile: TileRef, value: bool) {
        assert!(
            !(value && self.has_owner(tile)),
            "cannot set fallout, tile {tile} has owner"
        );
        if self.map.has_fallout(tile) {
            return;
        }
        self.territory_version += 1;
        self.map.set_fallout(tile, value);
    }

    /// `setWater(tile)`.
    pub fn set_water(&mut self, tile: TileRef) {
        if !self.is_land(tile) {
            return;
        }
        assert!(
            !self.has_owner(tile),
            "cannot set water, tile {tile} has owner"
        );
        if self.map.has_fallout(tile) {
            self.map.set_fallout(tile, false);
        }
        self.territory_version += 1;
        self.map.set_water(tile);
    }

    /// `queueWaterConversion(tile)`.
    pub fn queue_water_conversion(&mut self, tile: TileRef) {
        if !self.is_land(tile) {
            return;
        }
        assert!(
            !self.has_owner(tile),
            "cannot queue water conversion, tile {tile} has owner"
        );
        if !self.config.water_nukes() {
            self.set_fallout(tile, true);
            return;
        }
        self.water_manager.queue_tile(tile);
    }

    /// `setOwnerID(tile, playerId)` (raw, bumps the territory version).
    pub fn set_owner_id(&mut self, tile: TileRef, player_id: SmallId) {
        self.territory_version += 1;
        self.map.set_owner_id(tile, player_id as u32);
    }

    /// `conquer(owner, tile)`.
    pub fn conquer(&mut self, owner: SmallId, tile: TileRef) {
        assert!(self.map.is_land(tile), "cannot conquer water");
        assert!(
            !self.map.is_impassable(tile),
            "cannot conquer impassable terrain"
        );
        let previous = self.map.owner_id(tile);
        let now = self.ticks as f64;
        if previous != 0 {
            let p = self.player_impl_mut(previous);
            p.last_tile_change = now;
            p.tile_change_version += 1;
            p.tiles.delete(tile);
            p.border_tiles.delete(tile);
        }
        self.territory_version += 1;
        self.map.set_owner_id(tile, owner as u32);
        let o = self.player_impl_mut(owner);
        o.tiles.add(tile);
        o.last_tile_change = now;
        o.tile_change_version += 1;
        self.update_borders(tile);
        self.map.set_fallout(tile, false);
    }

    /// `relinquish(tile)`.
    pub fn relinquish(&mut self, tile: TileRef) {
        assert!(
            self.has_owner(tile),
            "Cannot relinquish tile because it is unowned"
        );
        assert!(!self.is_water(tile), "Cannot relinquish water");
        let previous = self.map.owner_id(tile);
        let now = self.ticks as f64;
        let p = self.player_impl_mut(previous);
        p.last_tile_change = now;
        p.tile_change_version += 1;
        p.tiles.delete(tile);
        p.border_tiles.delete(tile);
        self.territory_version += 1;
        self.map.set_owner_id(tile, 0);
        self.update_borders(tile);
    }

    fn update_borders(&mut self, tile: TileRef) {
        self.update_border_status(tile);
        let mut nbuf = [0; 4];
        let n = self.map.neighbors4(tile, &mut nbuf);
        for &nb in &nbuf[..n] {
            self.update_border_status(nb);
        }
    }

    fn update_border_status(&mut self, t: TileRef) {
        if !self.map.has_owner(t) {
            return;
        }
        let owner = self.map.owner_id(t);
        let is_border = self.map.is_border(t);
        let p = self.player_impl_mut(owner);
        if is_border {
            p.border_tiles.add(t);
        } else {
            p.border_tiles.delete(t);
        }
    }

    // ---- diplomacy ------------------------------------------------------------------------

    /// `createAllianceRequest(requestor, recipient)`.
    pub fn create_alliance_request(
        &mut self,
        requestor: SmallId,
        recipient: SmallId,
    ) -> Option<AllianceRequestId> {
        if self.player(requestor).is_allied_with(recipient) {
            return None;
        }
        if self
            .player(recipient)
            .incoming_alliance_requests()
            .iter()
            .any(|&r| self.alliance_requests[r as usize].requestor == requestor)
        {
            return None;
        }
        let corresponding = self
            .player(requestor)
            .incoming_alliance_requests()
            .into_iter()
            .find(|&r| self.alliance_requests[r as usize].requestor == recipient);
        if let Some(r) = corresponding {
            self.accept_request(r);
            return None;
        }
        self.alliance_requests.push(AllianceRequestImpl {
            status: RequestStatus::Pending,
            requestor,
            recipient,
            tick_created: self.ticks,
        });
        let id = (self.alliance_requests.len() - 1) as AllianceRequestId;
        self.pending_requests.push(id);
        Some(id)
    }

    /// `acceptAllianceRequest(request)`.
    pub fn accept_alliance_request(&mut self, request: AllianceRequestId) {
        self.pending_requests.retain(|&r| r != request);
        let (requestor, recipient) = {
            let r = &self.alliance_requests[request as usize];
            (r.requestor, r.recipient)
        };
        assert!(
            self.player(requestor).alliance_with(recipient).is_none(),
            "cannot accept alliance request, already allied with {}",
            self.player(recipient).name()
        );
        let id = self.next_alliance_id;
        self.next_alliance_id += 1;
        let alliance = AllianceImpl::new(self, requestor, recipient, self.ticks, id);
        self.alliances.push(alliance);
        self.player_impl_mut(requestor).alliances.push(id);
        self.player_impl_mut(recipient).alliances.push(id);
        self.player_impl_mut(requestor)
            .past_outgoing_alliance_requests
            .push(request);
    }

    /// `rejectAllianceRequest(request)`.
    pub fn reject_alliance_request(&mut self, request: AllianceRequestId) {
        self.pending_requests.retain(|&r| r != request);
        let requestor = self.alliance_requests[request as usize].requestor;
        self.player_impl_mut(requestor)
            .past_outgoing_alliance_requests
            .push(request);
    }

    /// `breakAlliance(breaker, alliance)`.
    pub fn break_alliance(&mut self, breaker: SmallId, alliance: AllianceId) {
        let other = {
            let a = &self.alliances[alliance as usize];
            if a.requestor == breaker {
                a.recipient
            } else {
                a.requestor
            }
        };
        assert!(
            self.player(breaker).is_allied_with(other),
            "not allied, cannot break alliance"
        );
        let other_ref = self.player(other);
        if !other_ref.is_traitor() && !other_ref.is_disconnected() {
            self.player_mut(breaker).mark_traitor();
        }
        self.detach_alliance(alliance);
    }

    /// `expireAlliance(alliance)`.
    pub fn expire_alliance(&mut self, alliance: AllianceId) {
        let (requestor, recipient) = {
            let a = &self.alliances[alliance as usize];
            (a.requestor, a.recipient)
        };
        let p1_set = self.player(recipient).alliances().to_vec();
        let alliances: Vec<AllianceId> = self
            .player(requestor)
            .alliances()
            .iter()
            .copied()
            .filter(|a| p1_set.contains(a))
            .collect();
        assert!(
            alliances.len() == 1,
            "cannot expire alliance: must have exactly one alliance, have {}",
            alliances.len()
        );
        self.detach_alliance(alliances[0]);
    }

    /// `removeAlliancesByPlayerSilently(player)`.
    pub fn remove_alliances_by_player_silently(&mut self, player: SmallId) {
        let removed = self.player(player).alliances().to_vec();
        for a in removed {
            self.detach_alliance(a);
        }
    }

    fn detach_alliance(&mut self, alliance: AllianceId) {
        let (requestor, recipient) = {
            let a = &self.alliances[alliance as usize];
            (a.requestor, a.recipient)
        };
        self.player_impl_mut(requestor)
            .alliances
            .retain(|&a| a != alliance);
        self.player_impl_mut(recipient)
            .alliances
            .retain(|&a| a != alliance);
    }

    /// `conquerPlayer(conqueror, conquered)`.
    pub fn conquer_player(&mut self, conqueror: SmallId, conquered: SmallId) {
        if self.player(conquered).is_disconnected()
            && self.player(conqueror).is_on_same_team(conquered)
        {
            let ships = self
                .player(conquered)
                .units_of_types(&[UnitType::Warship, UnitType::TransportShip]);
            for ship in ships {
                self.player_mut(conqueror).capture_unit(ship);
            }
        }
        let attacks_sent = self.stats.attacks_sent(self.player(conquered).client_id());
        let skip_gold_transfer =
            attacks_sent == 0 && self.player(conquered).player_type() == PlayerType::Human;
        let gold: Gold = if skip_gold_transfer {
            0
        } else {
            self.player(conquered).gold()
        };
        let gold_captured: Gold = if skip_gold_transfer {
            0
        } else {
            self.config
                .conquer_gold_amount(self.player(conquered).player_type(), gold)
        };
        if !skip_gold_transfer {
            self.player_mut(conqueror).add_gold(gold_captured);
            self.player_mut(conquered).remove_gold(gold);
        }
    }

    // ---- spawn immunity, teams, win helpers ------------------------------------------------

    fn ticks_since_start(&self) -> f64 {
        match self.start_tick {
            None => 0.0,
            Some(s) => f64::max(0.0, self.ticks as f64 - s as f64),
        }
    }

    /// `isSpawnImmunityActive`.
    pub fn is_spawn_immunity_active(&self) -> bool {
        self.in_spawn_phase()
            || self.ticks_since_start() < self.config.spawn_immunity_duration() as f64
    }

    /// `elapsedGameSeconds`.
    pub fn elapsed_game_seconds(&self) -> f64 {
        self.ticks_since_start() / 10.0
    }

    /// `isNationSpawnImmunityActive`.
    pub fn is_nation_spawn_immunity_active(&self) -> bool {
        self.in_spawn_phase()
            || self.ticks_since_start() < self.config.nation_spawn_immunity_duration() as f64
    }

    /// `teams()`.
    pub fn teams(&self) -> Vec<Team> {
        if self.config.game_config().game_mode != GameMode::Team {
            return vec![];
        }
        let mut t = vec![self.bot_team.clone()];
        t.extend(self.player_teams.iter().cloned());
        t
    }

    /// `teamTilesOwned(team)`.
    pub fn team_tiles_owned(&self, team: &str) -> f64 {
        let mut tiles = 0.0;
        for p in &self.players {
            if p.team.as_deref() == Some(team) {
                tiles += p.tiles.len() as f64;
            }
        }
        tiles
    }

    /// `totalLandTiles()`.
    pub fn total_land_tiles(&self) -> f64 {
        f64::max(0.0, self.num_land_tiles() - self.num_tiles_with_fallout())
    }

    /// `teamSpawnArea(team)`.
    pub fn team_spawn_area(&self, team: &str) -> Option<SpawnArea> {
        let areas_map = self.team_game_spawn_areas.as_ref()?;
        let num_teams = self.player_teams.len();
        let areas = areas_map.get(&num_teams.to_string())?;
        let team_index = self.player_teams.iter().position(|t| t == team)?;
        areas.get(team_index).cloned()
    }
}

impl SharedWaterView for GameImpl {
    fn map(&self) -> &GameMapImpl {
        &self.map
    }
    fn players(&self) -> Vec<SmallId> {
        GameImpl::players(self)
    }
    fn is_bot(&self, player: SmallId) -> bool {
        self.player(player).player_type() == PlayerType::Bot
    }
    fn tile_change_version(&self, player: SmallId) -> u32 {
        self.player(player).tile_change_version()
    }
    fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef)) {
        self.player(player).border_tiles().for_each(f);
    }
    fn can_trade(&self, player: SmallId, other: SmallId) -> bool {
        self.player(player).can_trade(other)
    }
    fn water_component(&self, tile: TileRef) -> Option<u32> {
        self.water_manager.get_water_component(&self.map, tile)
    }
}

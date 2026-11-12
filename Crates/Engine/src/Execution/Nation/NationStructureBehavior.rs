// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationStructureBehavior.ts: what and where a nation builds or upgrades.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::NationBehaviorPorts::{Decision, DecisionLog};
use super::NationUtils::rand_territory_tile_array;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Configuration::Config::TrainRelation;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Util::{nearest_tile_dist, nearest_tile_dist_capped};
use crate::Game::Game::{
    Difficulty, GameMode, Gold, PlayerType, STRUCTURES, SmallId, Tick, UnitId, UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// SAM launcher ratio per city, by difficulty.
fn sam_ratio(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy => 0.15,
        Difficulty::Medium => 0.2,
        Difficulty::Hard => 0.25,
        Difficulty::Impossible => 0.3,
    }
}

/// How many structures of one type a nation builds per city, and how much each owned one inflates the perceived cost.
struct StructureRatioConfig {
    ratio_per_city: f64,
    perceived_cost_increase_per_owned: f64,
}

/// `getStructureRatios(difficulty)[type]`.
fn structure_ratio(difficulty: Difficulty, t: UnitType) -> Option<StructureRatioConfig> {
    match t {
        UnitType::Port | UnitType::Factory => Some(StructureRatioConfig {
            ratio_per_city: 0.75,
            perceived_cost_increase_per_owned: 1.0,
        }),
        UnitType::SAMLauncher => Some(StructureRatioConfig {
            ratio_per_city: sam_ratio(difficulty),
            perceived_cost_increase_per_owned: 0.3,
        }),
        UnitType::MissileSilo => Some(StructureRatioConfig {
            ratio_per_city: 0.2,
            perceived_cost_increase_per_owned: 1.0,
        }),
        _ => None,
    }
}

const CITY_PERCEIVED_COST_INCREASE_PER_OWNED: f64 = 1.0;
const FACTORY_COASTAL_RATIO_MULTIPLIER: f64 = 0.33;
const MAX_MISSILE_SILOS: i64 = 3;
const FIRST_MISSILE_SILO_RATIO: f64 = 0.4;
/// If there are more structures per tile than this, upgrading is preferred over building.
const UPGRADE_DENSITY_THRESHOLD: f64 = 1.0 / 1500.0;
/// Smallest water body (in full-map tiles) a port may be placed on.
const MIN_PORT_WATER_COMPONENT_SIZE: u32 = 3000;
const TILES_PER_CITY_EQUIVALENT: f64 = 2000.0;
const HIGH_NATION_DENSITY_THRESHOLD: f64 = 1.0 / 7500.0;
const HIGH_STARTING_GOLD_THRESHOLD: Gold = 3_000_000;
/// Tick gap a high-starting-gold nation must wait before placing its Nth structure.
const HIGH_GOLD_STRUCTURE_COOLDOWN_TICKS: [u32; 5] = [0, 0, 250, 150, 100];
const TEAM_POST_SAVE_UP_PHASE_TICKS: u32 = 150;
const UNDER_ATTACK_THREAT_RATIO: f64 = 0.35;
const DEFENSE_POST_RATIO_PER_POST: f64 = 0.4;

/// One reachable train station: its tile, rail cluster (`None` when isolated) and trade weight.
#[derive(Clone, Copy)]
struct StationEntry {
    tile: TileRef,
    cluster: Option<u32>,
    weight: f64,
}

/// `{ borderSpacing, structureSpacing }` derived from the atom bomb range.
struct Spacing {
    border: f64,
    structure: f64,
}

/// What a connectivity-scored value function (factory, city) needs of the train network.
struct ConnectivityScore {
    stations: Vec<StationEntry>,
    min_range_squared: f64,
    station_range_squared: f64,
}

/// The value function of one structure type, created once per placement search (as upstream's closures).
enum SpawnValue {
    MissileSilo {
        other_tiles: Vec<TileRef>,
        spacing: Spacing,
    },
    Port {
        other_tiles: Vec<TileRef>,
    },
    Factory {
        other_tiles: Vec<TileRef>,
        city_tiles: Vec<TileRef>,
        spacing: Spacing,
        station_range: f64,
        connectivity: Option<ConnectivityScore>,
    },
    City {
        other_tiles: Vec<TileRef>,
        factory_tiles: Vec<TileRef>,
        spacing: Spacing,
        connectivity: Option<ConnectivityScore>,
    },
    SamLauncher {
        other_tiles: Vec<TileRef>,
        spacing: Spacing,
        difficulty: Difficulty,
        protect_entries: Vec<(TileRef, f64)>,
        range_squared: f64,
        structure_coverage: Option<HashMap<TileRef, f64>>,
    },
}

/// Structure placement decisions of one nation.
pub struct NationStructureBehavior {
    random: Rc<RefCell<PseudoRandom>>,
    player: SmallId,
    reachable_stations_cache: Option<Vec<StationEntry>>,
    shared_water_components: Option<HashSet<i64>>,
    last_structure_tick: Option<Tick>,
    placements_count: usize,
    built_crowded_map_first_structure: bool,
    has_high_starting_gold: Option<bool>,
    post_save_up_start_tick: Option<Tick>,
    /// Test hook: records the placements instead of queueing them.
    pub log: DecisionLog,
}

impl NationStructureBehavior {
    /// `new NationStructureBehavior(random, game, player)`.
    pub fn new(random: Rc<RefCell<PseudoRandom>>, player: SmallId) -> Self {
        Self {
            random,
            player,
            reachable_stations_cache: None,
            shared_water_components: None,
            last_structure_tick: None,
            placements_count: 0,
            built_crowded_map_first_structure: false,
            has_high_starting_gold: None,
            post_save_up_start_tick: None,
            log: DecisionLog::default(),
        }
    }

    fn rnd(&self) -> std::cell::RefMut<'_, PseudoRandom> {
        self.random.borrow_mut()
    }

    fn difficulty(game: &GameImpl) -> Difficulty {
        game.config().game_config().difficulty
    }

    /// `handleStructures()`.
    pub fn handle_structures(&mut self, game: &mut GameImpl) -> bool {
        // Defense posts are outside the pacing/counter system and are never the very first structure.
        if self.placements_count > 0 && !game.config().is_unit_disabled(UnitType::DefensePost) {
            if self.try_build_defense_post(game) {
                return true;
            }
            // If the attack threshold is met, block other structures even when placement failed.
            if self.defense_post_needed(game) {
                return false;
            }
        }

        if self.is_on_structure_cooldown(game) {
            return false;
        }
        if self.is_in_post_save_up_blocked_phase(game) {
            return false;
        }
        let built = self.do_handle_structures(game);
        if built {
            self.last_structure_tick = Some(game.ticks());
            self.placements_count += 1;
        }
        built
    }

    /// Land attacks (no source tile) on the nation: (attacker, troops) pairs.
    fn incoming_land_attacks(&self, game: &GameImpl) -> Vec<(SmallId, f64)> {
        game.player(self.player)
            .incoming_attacks()
            .into_iter()
            .filter(|&a| game.attack(a).source_tile().is_none())
            .map(|a| (game.attack(a).attacker(), game.attack(a).troops()))
            .collect()
    }

    fn try_build_defense_post(&mut self, game: &mut GameImpl) -> bool {
        let difficulty = Self::difficulty(game);
        if difficulty == Difficulty::Easy {
            return false;
        }
        if difficulty == Difficulty::Medium && !self.rnd().chance(2.0) {
            return false;
        }

        let land_attacks = self.incoming_land_attacks(game);
        if land_attacks.is_empty() {
            return false;
        }

        let our_troops = game.player(self.player).troops();
        if our_troops <= 0.0 {
            return false;
        }

        let incoming_troops = land_attacks.iter().fold(0.0, |sum, a| sum + a.1);
        let ratio = incoming_troops / our_troops;
        if ratio < UNDER_ATTACK_THREAT_RATIO {
            return false;
        }

        let allowed = if difficulty == Difficulty::Medium {
            1.0
        } else {
            (ratio / DEFENSE_POST_RATIO_PER_POST).ceil()
        };

        let front_tiles = self.get_attack_front_tiles(game, &land_attacks);
        if self.count_defense_posts_near_front(game, &front_tiles, Some(allowed)) >= allowed {
            return false;
        }

        let cost = self.cost(game, UnitType::DefensePost);
        if game.player(self.player).gold() < cost {
            return false;
        }

        let tiles = self.sample_tiles_near_front(game, &front_tiles, 25, UnitType::DefensePost);
        for tile in tiles {
            if game
                .player(self.player)
                .can_build(UnitType::DefensePost, tile, None)
                .is_none()
            {
                continue;
            }
            self.log.queue(
                game,
                self.player,
                Decision::Construct {
                    unit_type: UnitType::DefensePost,
                    tile,
                },
            );
            return true;
        }
        false
    }

    fn defense_post_needed(&self, game: &GameImpl) -> bool {
        if Self::difficulty(game) == Difficulty::Easy {
            return false;
        }
        let land_attacks = self.incoming_land_attacks(game);
        if land_attacks.is_empty() {
            return false;
        }
        let our_troops = game.player(self.player).troops();
        if our_troops <= 0.0 {
            return false;
        }
        let incoming_troops = land_attacks.iter().fold(0.0, |sum, a| sum + a.1);
        incoming_troops / our_troops >= UNDER_ATTACK_THREAT_RATIO
    }

    /// Our border tiles adjacent to a tile owned by any of the attacking players.
    fn get_attack_front_tiles(
        &self,
        game: &GameImpl,
        land_attacks: &[(SmallId, f64)],
    ) -> Vec<TileRef> {
        let attackers: HashSet<SmallId> = land_attacks.iter().map(|a| a.0).collect();
        if attackers.is_empty() {
            return Vec::new();
        }
        let mut front_tiles = Vec::new();
        let mut nbuf = [0; 4];
        game.player(self.player)
            .border_tiles()
            .for_each(|border_tile| {
                let n = game.map.neighbors4(border_tile, &mut nbuf);
                for &neighbor in &nbuf[..n] {
                    if attackers.contains(&game.owner_id(neighbor)) {
                        front_tiles.push(border_tile);
                        return;
                    }
                }
            });
        front_tiles
    }

    /// Counts defense posts within 1.5 x borderSpacing of any front tile; `cap` stops the scan early.
    fn count_defense_posts_near_front(
        &self,
        game: &GameImpl,
        front_tiles: &[TileRef],
        cap: Option<f64>,
    ) -> f64 {
        if front_tiles.is_empty() {
            return 0.0;
        }
        let border_spacing = Self::spacing(game).border;
        let range_squared = (border_spacing * 1.5).powi(2);

        let mut count = 0.0;
        for dp in game.player(self.player).units_of(UnitType::DefensePost) {
            let dp_tile = game.unit_any(dp).tile;
            for &front_tile in front_tiles {
                if game.map.euclidean_dist_squared(dp_tile, front_tile) as f64 <= range_squared {
                    count += 1.0;
                    if let Some(cap) = cap
                        && count >= cap
                    {
                        return count;
                    }
                    break;
                }
            }
        }
        count
    }

    /// Samples territory tiles for defense-post placement around the attack front (see upstream doc).
    fn sample_tiles_near_front(
        &self,
        game: &GameImpl,
        front_tiles: &[TileRef],
        count: usize,
        unit_type: UnitType,
    ) -> Vec<TileRef> {
        if front_tiles.is_empty() {
            return Vec::new();
        }

        let border_spacing = Self::spacing(game).border;
        let search_radius = (border_spacing * 1.5).ceil() as i32;
        let min_border_dist = (border_spacing * 0.75).ceil();
        let max_border_dist = (border_spacing * 1.5).ceil();
        let player = game.player(self.player);

        // Spread: prefer front tiles far from existing defense posts.
        let spread_range_squared = (border_spacing * 1.5).powi(2);
        let existing_dp_tiles: Vec<TileRef> = player
            .units_of(UnitType::DefensePost)
            .iter()
            .map(|&u| game.unit_any(u).tile)
            .collect();

        let anchors: Vec<TileRef> = if !existing_dp_tiles.is_empty() {
            let filtered: Vec<TileRef> = front_tiles
                .iter()
                .copied()
                .filter(|&ft| {
                    !existing_dp_tiles.iter().any(|&dp| {
                        (game.map.euclidean_dist_squared(ft, dp) as f64) < spread_range_squared
                    })
                })
                .collect();
            if filtered.is_empty() {
                front_tiles.to_vec()
            } else {
                filtered
            }
        } else {
            front_tiles.to_vec()
        };

        let mut result: Vec<TileRef> = Vec::new();
        let mut attempt = 0;
        while attempt < count * 6 && result.len() < count {
            attempt += 1;
            let anchor = *self.rnd().rand_element(&anchors);
            let ax = game.x(anchor) as i32;
            let ay = game.y(anchor) as i32;
            let x = self
                .rnd()
                .next_int((ax - search_radius) as f64, (ax + search_radius + 1) as f64)
                as i32;
            let y = self
                .rnd()
                .next_int((ay - search_radius) as f64, (ay + search_radius + 1) as f64)
                as i32;
            if !game.is_valid_coord(x, y) {
                continue;
            }
            let t = game.tile_ref(x, y);
            if game.owner_id(t) != self.player {
                continue;
            }
            // Only "inside [min, max]" matters, so the search is capped at max.
            let border_dist = nearest_tile_dist_capped(
                &game.map,
                player.border_tiles(),
                t,
                max_border_dist as i64,
            );
            if border_dist < min_border_dist || border_dist > max_border_dist {
                continue;
            }
            if player.can_build(unit_type, t, None).is_none() {
                continue;
            }
            result.push(t);
        }

        if !result.is_empty() {
            return result;
        }

        // Fallback: relax the border-depth constraint (territory too small for the depth ring).
        let mut fallback: Vec<TileRef> = Vec::new();
        let mut attempt = 0;
        while attempt < count * 4 && fallback.len() < count {
            attempt += 1;
            let anchor = *self.rnd().rand_element(&anchors);
            let ax = game.x(anchor) as i32;
            let ay = game.y(anchor) as i32;
            let x = self
                .rnd()
                .next_int((ax - search_radius) as f64, (ax + search_radius + 1) as f64)
                as i32;
            let y = self
                .rnd()
                .next_int((ay - search_radius) as f64, (ay + search_radius + 1) as f64)
                as i32;
            if !game.is_valid_coord(x, y) {
                continue;
            }
            let t = game.tile_ref(x, y);
            if game.owner_id(t) != self.player {
                continue;
            }
            fallback.push(t);
        }
        fallback
    }

    fn is_on_structure_cooldown(&self, game: &GameImpl) -> bool {
        // Only high-starting-gold nations pause.
        let Some(last) = self.last_structure_tick else {
            return false;
        };
        if !self.has_high_starting_gold(game) {
            return false;
        }
        let required_gap = HIGH_GOLD_STRUCTURE_COOLDOWN_TICKS
            .get(self.placements_count)
            .copied()
            .unwrap_or(0);
        if required_gap == 0 {
            return false;
        }
        ((game.ticks() as f64) - (last as f64)) < required_gap as f64
    }

    /// Spreads placements after the save-up target is first reached: 15s on / 15s off.
    fn is_in_post_save_up_blocked_phase(&mut self, game: &GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::MissileSilo) {
            return false;
        }
        let save_up_target = self.get_save_up_target(game);
        if self.post_save_up_start_tick.is_none() {
            if game.player(self.player).gold() < save_up_target {
                return false;
            }
            self.post_save_up_start_tick = Some(game.ticks());
        }
        let elapsed = (game.ticks() as f64) - (self.post_save_up_start_tick.unwrap() as f64);
        elapsed % ((TEAM_POST_SAVE_UP_PHASE_TICKS * 2) as f64)
            >= TEAM_POST_SAVE_UP_PHASE_TICKS as f64
    }

    fn do_handle_structures(&mut self, game: &mut GameImpl) -> bool {
        self.reachable_stations_cache = None;
        let cities_disabled = game.config().is_unit_disabled(UnitType::City);
        let city_count = if cities_disabled {
            (game.player(self.player).num_tiles_owned() as f64 / TILES_PER_CITY_EQUIVALENT)
                .floor()
                .max(1.0)
        } else {
            game.player(self.player).units_owned(UnitType::City) as f64
        };
        self.shared_water_components = game.shared_water_components(self.player);
        let has_coastal_tiles = self.shared_water_components.is_some();

        let missile_silos_enabled = !game.config().is_unit_disabled(UnitType::MissileSilo);

        // High-starting-gold Hard/Impossible nations build a SAM first.
        let difficulty = Self::difficulty(game);
        if self.placements_count == 0
            && (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && !game.config().is_unit_disabled(UnitType::AtomBomb)
            && missile_silos_enabled
            && !game.config().is_unit_disabled(UnitType::SAMLauncher)
            && self.has_high_starting_gold(game)
            && self.maybe_spawn_structure(game, UnitType::SAMLauncher)
        {
            return true;
        }

        // On crowded maps the first structure is a port (or factory if landlocked).
        if !cities_disabled
            && !self.built_crowded_map_first_structure
            && self.is_high_nation_density(game)
        {
            let preferred_first =
                if has_coastal_tiles && !game.config().is_unit_disabled(UnitType::Port) {
                    UnitType::Port
                } else {
                    UnitType::Factory
                };
            if game.config().is_unit_disabled(preferred_first) {
                self.built_crowded_map_first_structure = true;
            } else if self.maybe_spawn_structure(game, preferred_first) {
                self.built_crowded_map_first_structure = true;
                return true;
            }
        }

        // Build order for non-city structures (priority order).
        let build_order = [
            UnitType::Port,
            UnitType::Factory,
            UnitType::SAMLauncher,
            UnitType::MissileSilo,
        ];

        let nukes_enabled = !game.config().is_unit_disabled(UnitType::AtomBomb)
            || !game.config().is_unit_disabled(UnitType::HydrogenBomb)
            || !game.config().is_unit_disabled(UnitType::MIRV);

        for structure_type in build_order {
            if game.config().is_unit_disabled(structure_type) {
                continue;
            }
            if structure_type == UnitType::Port && !has_coastal_tiles {
                continue;
            }
            if !nukes_enabled
                && (structure_type == UnitType::MissileSilo
                    || structure_type == UnitType::SAMLauncher)
            {
                continue;
            }
            if !missile_silos_enabled && structure_type == UnitType::SAMLauncher {
                continue;
            }

            if self.should_build_structure(game, structure_type, city_count, has_coastal_tiles)
                && self.maybe_spawn_structure(game, structure_type)
            {
                return true;
            }
        }

        if !cities_disabled && self.maybe_spawn_structure(game, UnitType::City) {
            return true;
        }

        false
    }

    fn has_high_starting_gold(&self, game: &GameImpl) -> bool {
        // Upstream memoises; the value never changes during a game.
        if let Some(v) = self.has_high_starting_gold {
            return v;
        }
        let info = game.player(self.player).info().clone();
        game.config()
            .starting_gold(info.player_type, info.is_lobby_creator)
            >= HIGH_STARTING_GOLD_THRESHOLD
    }

    fn is_high_nation_density(&self, game: &GameImpl) -> bool {
        let land_tiles = game.num_land_tiles();
        if land_tiles <= 0.0 {
            return false;
        }
        (game.nations().len() as f64) / land_tiles > HIGH_NATION_DENSITY_THRESHOLD
    }

    /// Whether to build more of this type given the city count and the configured ratio.
    fn should_build_structure(
        &self,
        game: &GameImpl,
        t: UnitType,
        city_count: f64,
        has_coastal_tiles: bool,
    ) -> bool {
        let difficulty = Self::difficulty(game);
        let Some(config) = structure_ratio(difficulty, t) else {
            return false;
        };

        let mut ratio = config.ratio_per_city;

        // Heavily reduce factory spawning if there are coastal tiles.
        if t == UnitType::Factory
            && has_coastal_tiles
            && !game.config().is_unit_disabled(UnitType::Port)
        {
            ratio *= FACTORY_COASTAL_RATIO_MULTIPLIER;
        }

        let owned = game.player(self.player).units_owned(t);

        if t == UnitType::MissileSilo && owned >= MAX_MISSILE_SILOS {
            return false;
        }

        // The first missile silo uses a higher ratio so nations start nuking earlier.
        if t == UnitType::MissileSilo && owned == 0 {
            ratio = FIRST_MISSILE_SILO_RATIO;
        }

        let target_count = (city_count * ratio).floor();

        (owned as f64) < target_count
    }

    fn cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        game.player(self.player).unit_cost(t, 0)
    }

    fn maybe_spawn_structure(&mut self, game: &mut GameImpl, t: UnitType) -> bool {
        let perceived_cost = self.get_perceived_cost(game, t);
        if game.player(self.player).gold() < perceived_cost {
            return false;
        }

        // Upgrade instead of building new when the density threshold is exceeded.
        let structures = game.player(self.player).units_of(t);
        if self.get_total_structure_density(game) > UPGRADE_DENSITY_THRESHOLD
            && game.config().unit_info(t).upgradable.unwrap_or(false)
        {
            if self.maybe_upgrade_structure(game, &structures) {
                return true;
            }
            // Density too high but nothing to upgrade (e.g. all under construction): wait.
            if !structures.is_empty() {
                return false;
            }
            // No structure of this type yet: fall through and build the first one.
        }

        let Some(tile) = self.structure_spawn_tile(game, t) else {
            return false;
        };
        if game.player(self.player).can_build(t, tile, None).is_none() {
            return false;
        }
        self.log.queue(
            game,
            self.player,
            Decision::Construct { unit_type: t, tile },
        );
        true
    }

    /// The cost the nation perceives: it grows with every owned structure until the save-up target is reached.
    fn get_perceived_cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        let real_cost = self.cost(game, t);

        let save_up_target = self.get_save_up_target(game);
        if save_up_target == 0 || game.player(self.player).gold() >= save_up_target {
            return real_cost;
        }

        let owned = game.player(self.player).units_owned(t) as f64;

        let increase_per_owned = if t == UnitType::City {
            CITY_PERCEIVED_COST_INCREASE_PER_OWNED
        } else {
            structure_ratio(Self::difficulty(game), t)
                .map_or(0.1, |c| c.perceived_cost_increase_per_owned)
        };

        // realCost * (1 + increasePerOwned * owned)
        let multiplier = 1.0 + increase_per_owned * owned;
        ((real_cost as f64) * multiplier).ceil() as Gold
    }

    /// The gold target to save up for, based on the enabled nukes; 0 when no saving is needed.
    fn get_save_up_target(&self, game: &GameImpl) -> Gold {
        let config = game.config();

        // Only save up for SAMs if missile silos are disabled.
        if config.is_unit_disabled(UnitType::MissileSilo) {
            return self.cost(game, UnitType::SAMLauncher);
        }

        // A limited amount in team games, synced with NationNukeBehavior.
        if config.game_config().game_mode == GameMode::Team {
            return self.cost(game, UnitType::HydrogenBomb);
        }

        let mirv_enabled = !config.is_unit_disabled(UnitType::MIRV);
        let hydro_enabled = !config.is_unit_disabled(UnitType::HydrogenBomb);
        let atom_enabled = !config.is_unit_disabled(UnitType::AtomBomb);

        if mirv_enabled {
            return self.cost(game, UnitType::MIRV) + self.cost(game, UnitType::HydrogenBomb);
        }
        if hydro_enabled {
            return self.cost(game, UnitType::HydrogenBomb) * 5;
        }
        if atom_enabled {
            return self.cost(game, UnitType::AtomBomb) * 20;
        }
        self.cost(game, UnitType::SAMLauncher)
    }

    /// Tries to upgrade an existing structure when the density threshold is exceeded.
    fn maybe_upgrade_structure(&mut self, game: &mut GameImpl, structures: &[UnitId]) -> bool {
        if self.get_total_structure_density(game) <= UPGRADE_DENSITY_THRESHOLD {
            return false;
        }
        if structures.is_empty() {
            return false;
        }
        if let Some(structure) = self.find_best_structure_to_upgrade(game, structures) {
            self.log
                .queue(game, self.player, Decision::Upgrade { unit: structure });
            return true;
        }
        false
    }

    /// Structure count (levels ignored) per owned tile.
    fn get_total_structure_density(&self, game: &GameImpl) -> f64 {
        let player = game.player(self.player);
        let tiles_owned = player.num_tiles_owned();
        if tiles_owned > 0 {
            player.units_of_types(STRUCTURES).len() as f64 / tiles_owned as f64
        } else {
            0.0
        }
    }

    /// The best structure to upgrade: preferably one protected by a SAM; in 50% of cases the 2nd or 3rd best.
    fn find_best_structure_to_upgrade(
        &self,
        game: &GameImpl,
        structures: &[UnitId],
    ) -> Option<UnitId> {
        if structures.is_empty() {
            return None;
        }
        let player = game.player(self.player);

        let upgradable: Vec<UnitId> = structures
            .iter()
            .copied()
            .filter(|&s| player.can_upgrade_unit(s))
            .collect();
        if upgradable.is_empty() {
            return None;
        }

        // Based on difficulty, a chance to just pick a random structure.
        let random_chance = match Self::difficulty(game) {
            Difficulty::Easy => 70.0,
            Difficulty::Medium => 40.0,
            Difficulty::Hard => 25.0,
            Difficulty::Impossible => 10.0,
        };

        if self.rnd().next_int(0.0, 100.0) < random_chance {
            return Some(*self.rnd().rand_element(&upgradable));
        }

        let sam_launchers = player.units_of(UnitType::SAMLauncher);

        // Score each structure by SAM protection.
        let mut scored: Vec<(UnitId, f64)> = Vec::new();
        for &structure in &upgradable {
            let mut score = 0.0;
            for &sam in &sam_launchers {
                let sam_unit = game.unit_any(sam);
                let sam_range = game.config().sam_range(sam_unit.level as f64);
                let sam_range_squared = sam_range * sam_range;
                let dist_squared = game
                    .map
                    .euclidean_dist_squared(game.unit_any(structure).tile, sam_unit.tile)
                    as f64;
                if dist_squared <= sam_range_squared {
                    score += 10.0;
                    if sam_unit.level > 1 {
                        score += (sam_unit.level as f64 - 1.0) * 7.5;
                    }
                }
            }
            // A small random factor breaks ties.
            score += self.rnd().next_int(0.0, 5.0);
            scored.push((structure, score));
        }

        // Descending by score (stable).
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        // 50% of the time, pick the second or third best for variety.
        if scored.len() >= 2 && self.rnd().chance(2.0) {
            let pick_index = if scored.len() >= 3 {
                self.rnd().next_int(1.0, 3.0) as usize
            } else {
                1
            };
            return Some(scored[pick_index].0);
        }

        Some(scored[0].0)
    }

    fn structure_spawn_tile(&mut self, game: &GameImpl, t: UnitType) -> Option<TileRef> {
        let tiles = if t == UnitType::Port {
            self.rand_coastal_tile_array(game, 25)
        } else {
            rand_territory_tile_array(&mut self.rnd(), game, self.player, 25)
        };
        if tiles.is_empty() {
            return None;
        }
        let value_function = self.structure_spawn_tile_value(game, t);
        let mut best_tile: Option<TileRef> = None;
        let mut best_value = 0.0;
        for tile in tiles {
            let v = self.eval_value(game, &value_function, tile);
            if v <= best_value && best_tile.is_some() {
                continue;
            }
            if game.player(self.player).can_build(t, tile, None).is_none() {
                continue;
            }
            // Found a better tile.
            best_tile = Some(tile);
            best_value = v;
        }
        best_tile
    }

    /// Samples shore tiles adjacent to water reachable by another player (so trading is possible).
    fn rand_coastal_tile_array(&self, game: &GameImpl, num_tiles: usize) -> Vec<TileRef> {
        let shared = self.shared_water_components.as_ref();
        let difficulty = Self::difficulty(game);
        let tiles: Vec<TileRef> = game
            .player(self.player)
            .border_tiles()
            .iter()
            .filter(|&t| {
                if !game.map.is_shore(t) {
                    return false;
                }
                let Some(shared) = shared else {
                    return false;
                };
                for neighbor in game.map.neighbors(t) {
                    if !game.map.is_water(neighbor) {
                        continue;
                    }
                    // Ocean is always shared: any ocean neighbour makes the tile a valid port site.
                    if game.map.is_ocean(neighbor) {
                        return true;
                    }
                    let Some(comp) = game.water_manager.get_water_component(&game.map, neighbor)
                    else {
                        continue;
                    };
                    if !shared.contains(&(comp as i64)) {
                        continue;
                    }
                    // Skip tiny lakes (not on Easy).
                    if difficulty != Difficulty::Easy {
                        let size = game
                            .water_manager
                            .get_water_component_size(&game.map, neighbor);
                        if size.is_some_and(|s| s < MIN_PORT_WATER_COMPONENT_SIZE) {
                            continue;
                        }
                    }
                    return true;
                }
                false
            })
            .collect();
        self.array_sampler(tiles, num_tiles)
    }

    /// `arraySampler`: all elements, or `sample_size` random ones drawn without replacement.
    fn array_sampler<T>(&self, mut a: Vec<T>, sample_size: usize) -> Vec<T> {
        if a.len() <= sample_size {
            return a;
        }
        let mut out = Vec::with_capacity(sample_size);
        for _ in 0..sample_size {
            // randFromSet picks the n-th element in insertion order; removing keeps that order.
            let index = self.rnd().rand_index(a.len());
            out.push(a.remove(index));
        }
        out
    }

    fn structure_spawn_tile_value(&mut self, game: &GameImpl, t: UnitType) -> SpawnValue {
        match t {
            UnitType::City => self.city_value(game),
            UnitType::MissileSilo => self.missile_silo_value(game),
            UnitType::Factory => self.factory_value(game),
            UnitType::Port => self.port_value(game),
            UnitType::SAMLauncher => self.sam_launcher_value(game),
            _ => panic!("Value function not implemented for {}", t.as_str()),
        }
    }

    fn unit_tiles(&self, game: &GameImpl, t: UnitType) -> Vec<TileRef> {
        game.player(self.player)
            .units_of(t)
            .iter()
            .map(|&u| game.unit_any(u).tile)
            .collect()
    }

    /// Prefers high elevation, distance from the border, and spacing from same-type structures.
    fn missile_silo_value(&self, game: &GameImpl) -> SpawnValue {
        SpawnValue::MissileSilo {
            other_tiles: self.unit_tiles(game, UnitType::MissileSilo),
            spacing: Self::spacing(game),
        }
    }

    /// Prefers spacing from other ports.
    fn port_value(&self, game: &GameImpl) -> SpawnValue {
        SpawnValue::Port {
            other_tiles: self.unit_tiles(game, UnitType::Port),
        }
    }

    /// Prefers elevation, spacing and (by difficulty) rail connectivity.
    fn factory_value(&mut self, game: &GameImpl) -> SpawnValue {
        let spacing = Self::spacing(game);
        let station_range = game.config().train_station_max_range();
        let use_connection_score = self.should_use_connectivity_score(Self::difficulty(game));
        let connectivity = self.connectivity(game, use_connection_score);
        SpawnValue::Factory {
            other_tiles: self.unit_tiles(game, UnitType::Factory),
            city_tiles: self.unit_tiles(game, UnitType::City),
            spacing,
            station_range,
            connectivity,
        }
    }

    /// Prefers interior positions, spacing from cities and factories, and rail connectivity.
    fn city_value(&mut self, game: &GameImpl) -> SpawnValue {
        let spacing = Self::spacing(game);
        let use_connection_score = self.should_use_connectivity_score(Self::difficulty(game));
        let connectivity = self.connectivity(game, use_connection_score);
        SpawnValue::City {
            other_tiles: self.unit_tiles(game, UnitType::City),
            factory_tiles: self.unit_tiles(game, UnitType::Factory),
            spacing,
            connectivity,
        }
    }

    fn connectivity(
        &mut self,
        game: &GameImpl,
        use_connection_score: bool,
    ) -> Option<ConnectivityScore> {
        if !use_connection_score {
            return None;
        }
        let station_range = game.config().train_station_max_range();
        let stations = self.get_or_build_reachable_stations(game).clone();
        Some(ConnectivityScore {
            stations,
            min_range_squared: game.config().train_station_min_range().powi(2),
            station_range_squared: station_range * station_range,
        })
    }

    /// Whether to use connectivity scoring for factories and cities (consumes a draw on every difficulty).
    fn should_use_connectivity_score(&self, difficulty: Difficulty) -> bool {
        let random_chance = match difficulty {
            Difficulty::Easy => 0.0,
            Difficulty::Medium => 60.0,
            Difficulty::Hard => 75.0,
            Difficulty::Impossible => 100.0,
        };
        self.rnd().next_int(0.0, 100.0) < random_chance
    }

    fn get_or_build_reachable_stations(&mut self, game: &GameImpl) -> &Vec<StationEntry> {
        if self.reachable_stations_cache.is_none() {
            self.reachable_stations_cache = Some(self.build_reachable_stations(game));
        }
        self.reachable_stations_cache.as_ref().unwrap()
    }

    /// Trade-weighted station entries for connectivity scoring: own and non-embargoed non-bot neighbour structures.
    fn build_reachable_stations(&self, game: &GameImpl) -> Vec<StationEntry> {
        let player = game.player(self.player);
        let is_lobby_creator = player.info().is_lobby_creator;

        // Unit to cluster lookup in one pass over all stations.
        let unit_to_cluster: HashMap<UnitId, Option<u32>> =
            game.rail_network.station_clusters().into_iter().collect();

        let train_gold = |rel: TrainRelation| -> f64 {
            game.config().train_gold(rel, 0.0, is_lobby_creator) as f64
        };
        let max_trade_gold = train_gold(TrainRelation::Ally).max(1.0);
        let mut result = Vec::new();

        // Own structures: weighted by "self" trade gold.
        let self_weight = train_gold(TrainRelation::Self_) / max_trade_gold;
        let types = [UnitType::City, UnitType::Port, UnitType::Factory];
        for unit in player.units_of_types(&types) {
            if let Some(&cluster) = unit_to_cluster.get(&unit) {
                result.push(StationEntry {
                    tile: game.unit_any(unit).tile,
                    cluster,
                    weight: self_weight,
                });
            }
        }

        // Neighbour structures: all non-embargoed non-bot neighbours.
        for neighbor in player.nearby() {
            if neighbor == 0 {
                continue;
            }
            if game.player(neighbor).player_type() == PlayerType::Bot {
                continue;
            }
            if !player.can_trade(neighbor) {
                continue;
            }
            let rel = if player.is_on_same_team(neighbor) {
                TrainRelation::Team
            } else if player.is_allied_with(neighbor) {
                TrainRelation::Ally
            } else {
                TrainRelation::Other
            };
            let weight = train_gold(rel) / max_trade_gold;
            for unit in game.player(neighbor).units_of_types(&types) {
                if let Some(&cluster) = unit_to_cluster.get(&unit) {
                    result.push(StationEntry {
                        tile: game.unit_any(unit).tile,
                        cluster,
                        weight,
                    });
                }
            }
        }

        result
    }

    /// Summed cluster-deduplicated connectivity weight of a candidate tile (best weight per cluster in range).
    fn compute_connectivity_score(game: &GameImpl, tile: TileRef, c: &ConnectivityScore) -> f64 {
        let mut clusters_in_range: OrderedMap<u32, f64> = OrderedMap::new();
        let mut isolated_weight = 0.0;
        for station in &c.stations {
            let dist = game.map.euclidean_dist_squared(tile, station.tile) as f64;
            if dist < c.min_range_squared || dist > c.station_range_squared {
                continue;
            }
            match station.cluster {
                Some(cluster) => {
                    let best = clusters_in_range.get(&cluster).copied().unwrap_or(0.0);
                    clusters_in_range.insert(cluster, best.max(station.weight));
                }
                None => isolated_weight += station.weight,
            }
        }
        let mut score = isolated_weight;
        for cw in clusters_in_range.values() {
            score += *cw;
        }
        score
    }

    /// Prefers elevation, border distance, spacing, and proximity to protectable structures.
    fn sam_launcher_value(&mut self, game: &GameImpl) -> SpawnValue {
        let player = game.player(self.player);
        let spacing = Self::spacing(game);
        let difficulty = Self::difficulty(game);
        let weight_by_level =
            difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible;

        let mut protect_entries: Vec<(TileRef, f64)> = Vec::new();
        for &unit in player.units() {
            let u = game.unit_any(unit);
            if matches!(
                u.unit_type,
                UnitType::City | UnitType::Factory | UnitType::MissileSilo | UnitType::Port
            ) {
                protect_entries.push((u.tile, if weight_by_level { u.level as f64 } else { 1.0 }));
            }
        }
        let range = game.config().default_sam_range();
        let range_squared = range * range;

        let use_coverage_weighting =
            difficulty != Difficulty::Easy && self.rnd().next_int(0.0, 100.0) < 25.0;

        // Existing SAM coverage of each protectable structure.
        let mut structure_coverage: Option<HashMap<TileRef, f64>> = None;
        if use_coverage_weighting {
            let mut coverage = HashMap::new();
            let existing_sams = player.units_of(UnitType::SAMLauncher);
            for &(tile, _) in &protect_entries {
                let mut coverage_score = 0.0;
                for &sam in &existing_sams {
                    let sam_unit = game.unit_any(sam);
                    let sam_range = game.config().sam_range(sam_unit.level as f64);
                    let dist = game.map.euclidean_dist_squared(tile, sam_unit.tile) as f64;
                    if dist <= sam_range * sam_range {
                        coverage_score += sam_unit.level as f64;
                    }
                }
                coverage.insert(tile, coverage_score);
            }
            structure_coverage = Some(coverage);
        }

        SpawnValue::SamLauncher {
            other_tiles: self.unit_tiles(game, UnitType::SAMLauncher),
            spacing,
            difficulty,
            protect_entries,
            range_squared,
            structure_coverage,
        }
    }

    /// Evaluates a value function at a tile.
    fn eval_value(&self, game: &GameImpl, value: &SpawnValue, tile: TileRef) -> f64 {
        let owner = game.player(self.player);
        let border_tiles = owner.border_tiles();
        let nearest_other = |tiles: &[TileRef]| {
            nearest_tile_dist(
                &game.map,
                tiles.iter().copied().filter(|&t| t != tile),
                tile,
            )
        };
        let border_term = |spacing: &Spacing| {
            // Clamped at borderSpacing, so the ring search may stop there.
            let closest_border_dist =
                nearest_tile_dist_capped(&game.map, border_tiles, tile, spacing.border as i64);
            closest_border_dist.min(spacing.border)
        };
        match value {
            SpawnValue::MissileSilo {
                other_tiles,
                spacing,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                w += border_term(spacing);
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(spacing.structure);
                }
                w
            }
            SpawnValue::Port { other_tiles } => {
                let mut w = 0.0;
                w += nearest_other(other_tiles);
                w
            }
            SpawnValue::Factory {
                other_tiles,
                city_tiles,
                spacing,
                station_range,
                connectivity,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                w += border_term(spacing);
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(*station_range);
                }
                // Cross-type spacing: prefer to be away from cities.
                let d2 = nearest_tile_dist(&game.map, city_tiles.iter().copied(), tile);
                if d2 != f64::INFINITY {
                    w += d2.min(spacing.structure);
                }
                if let Some(c) = connectivity {
                    w += Self::compute_connectivity_score(game, tile, c) * spacing.structure;
                }
                w
            }
            SpawnValue::City {
                other_tiles,
                factory_tiles,
                spacing,
                connectivity,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                w += border_term(spacing);
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(spacing.structure);
                }
                // Cross-type spacing: prefer to be away from factories.
                let d2 = nearest_tile_dist(&game.map, factory_tiles.iter().copied(), tile);
                if d2 != f64::INFINITY {
                    w += d2.min(spacing.structure);
                }
                if let Some(c) = connectivity {
                    w += Self::compute_connectivity_score(game, tile, c) * spacing.structure;
                }
                w
            }
            SpawnValue::SamLauncher {
                other_tiles,
                spacing,
                difficulty,
                protect_entries,
                range_squared,
                structure_coverage,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                // Infinity (farther than borderSpacing) still earns the clamped term; only an empty border earns nothing.
                if !border_tiles.is_empty() {
                    w += border_term(spacing);
                }
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(spacing.structure);
                }
                // Prefer to be in range of other structures (skipped on Easy).
                if *difficulty != Difficulty::Easy {
                    for &(entry_tile, entry_weight) in protect_entries {
                        let distance_squared =
                            game.map.euclidean_dist_squared(tile, entry_tile) as f64;
                        if distance_squared > *range_squared {
                            continue;
                        }
                        if let Some(coverage_map) = structure_coverage {
                            let coverage = coverage_map.get(&entry_tile).copied().unwrap_or(0.0);
                            let coverage_weight = 1.0 / (1.0 + coverage);
                            w += spacing.structure * entry_weight * coverage_weight;
                        } else {
                            w += spacing.structure * entry_weight;
                        }
                    }
                }
                w
            }
        }
    }

    /// Spacing constants derived from the atom bomb range.
    fn spacing(game: &GameImpl) -> Spacing {
        let border = game.config().nuke_magnitudes(UnitType::AtomBomb).outer;
        Spacing {
            border,
            structure: border * 2.0,
        }
    }
}

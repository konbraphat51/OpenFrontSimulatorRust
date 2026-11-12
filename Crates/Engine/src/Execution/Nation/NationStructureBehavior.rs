// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationStructureBehavior.ts (part: core): what and where a nation builds or upgrades.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::NationBehaviorPorts::{Decision, DecisionLog};
use super::NationMIRVBehavior::is_mirv_worth_saving_for;
use super::NationUtils::{
    has_high_starting_gold, is_save_up_within_reach, rand_territory_tile_array,
};
use super::SharedRandom;
use crate::Builds::Rules::Change;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Configuration::Config::TrainRelation;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Util::{nearest_tile_dist, nearest_tile_dist_capped};
use crate::Game::Game::{
    Difficulty, GameMode, Gold, PlayerType, Relation, STRUCTURES, SmallId, Tick, UnitId, UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::collections::{HashMap, HashSet};

mod DefensePosts;
mod Placement;
mod Upgrades;

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
fn structure_ratio(
    difficulty: Difficulty,
    game_mode: GameMode,
    v2: bool,
    legacy: bool,
    t: UnitType,
) -> Option<StructureRatioConfig> {
    match t {
        UnitType::DefensePost if legacy => Some(StructureRatioConfig {
            ratio_per_city: 0.25,
            perceived_cost_increase_per_owned: 1.0,
        }),
        UnitType::Port | UnitType::Factory => Some(StructureRatioConfig {
            ratio_per_city: if v2
                && t == UnitType::Port
                && game_mode == GameMode::Team
                && difficulty != Difficulty::Easy
            {
                TEAM_PORT_RATIO
            } else {
                0.75
            },
            perceived_cost_increase_per_owned: 1.0,
        }),
        UnitType::SAMLauncher => Some(StructureRatioConfig {
            ratio_per_city: sam_ratio(difficulty),
            perceived_cost_increase_per_owned: if legacy { 0.5 } else { 0.3 },
        }),
        UnitType::MissileSilo => Some(StructureRatioConfig {
            ratio_per_city: 0.2,
            perceived_cost_increase_per_owned: 1.0,
        }),
        _ => None,
    }
}

/// Port ratio per city in team games (not on Easy), where teammates are safe trade partners.
const TEAM_PORT_RATIO: f64 = 1.0;
const CITY_PERCEIVED_COST_INCREASE_PER_OWNED: f64 = 1.0;
const FACTORY_COASTAL_RATIO_MULTIPLIER: f64 = 0.33;

/// Cities owned before saving up for nukes inflates structure costs, by difficulty.
fn cities_before_saving(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy => 0.0,
        Difficulty::Medium => 3.0,
        Difficulty::Hard => 4.0,
        Difficulty::Impossible => 5.0,
    }
}

/// Scales the perceived cost increase of cities, ports and factories while saving up, by difficulty.
fn income_structure_saving_weight(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy => 1.0,
        Difficulty::Medium => 0.8,
        Difficulty::Hard => 0.6,
        Difficulty::Impossible => 0.4,
    }
}

/// Factory ratio multiplier when the nation has coastal tiles, by difficulty (v0.35.0 on).
fn factory_coastal_ratio_multiplier(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy => 0.33,
        Difficulty::Medium => 0.45,
        Difficulty::Hard => 0.55,
        Difficulty::Impossible => 0.6,
    }
}

/// Structures a nation wants under SAM cover.
const SAM_PROTECTED_TYPES: [UnitType; 4] = [
    UnitType::City,
    UnitType::Factory,
    UnitType::MissileSilo,
    UnitType::Port,
];

/// Percent chance to upgrade a SAM instead of building one when SAMs already cover every structure.
fn sam_upgrade_when_covered_chance(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy => 0.0,
        Difficulty::Medium => 50.0,
        Difficulty::Hard | Difficulty::Impossible => 100.0,
    }
}

/// Most levels bought in one upgrade, as far as the gold beyond the save-up target pays for them.
fn max_upgrades_at_once(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy => 3.0,
        Difficulty::Medium => 10.0,
        Difficulty::Hard | Difficulty::Impossible => f64::INFINITY,
    }
}

/// Starting gold below which high-starting-gold nations wait between their first structures.
const PACED_STARTING_GOLD_LIMIT: Gold = 10_000_000;
/// Closest a reactive defense post goes to the attack front, in post ranges, and per unit of incoming-to-own troop ratio.
const DEFENSE_POST_FRONT_MIN_DIST: f64 = 2.0 / 3.0;
const DEFENSE_POST_FRONT_DIST_PER_RATIO: f64 = 1.0 / 3.0;
const DEFENSE_POST_FRONT_RATIO_CAP: f64 = 2.0;
/// Depth of the band a reactive defense post may go in, in post ranges.
const DEFENSE_POST_FRONT_BAND: f64 = 1.0 / 2.0;
/// Distance band from a threatening neighbour's border for a defense post built before it attacks, in post ranges.
const PROACTIVE_DEFENSE_POST_MIN_DIST: f64 = 1.0 / 2.0;
const PROACTIVE_DEFENSE_POST_MAX_DIST: f64 = 1.0;
/// Cities owned before Hard and Impossible nations fortify borders that are not under attack yet.
const PROACTIVE_DEFENSE_POST_MIN_CITIES: f64 = 2.0;
/// Defense posts owned per city above which nations stop fortifying borders that are not under attack.
const PROACTIVE_DEFENSE_POSTS_PER_CITY: f64 = 0.5;
/// Random tiles tried when looking for a defense post spot.
const DEFENSE_POST_SAMPLE_ATTEMPTS: usize = 150;

/// Share of our troops a hostile neighbour needs before we fortify against it (Hard and Impossible only).
fn proactive_defense_post_hostile_troop_ratio(difficulty: Difficulty) -> Option<f64> {
    match difficulty {
        Difficulty::Hard => Some(1.0),
        Difficulty::Impossible => Some(0.5),
        _ => None,
    }
}
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

/// Structures counted for density; cheap defense posts would push nations into upgrading early.
const DENSITY_STRUCTURE_TYPES: [UnitType; 5] = [
    UnitType::City,
    UnitType::Factory,
    UnitType::MissileSilo,
    UnitType::Port,
    UnitType::SAMLauncher,
];

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
        /// Before v0.31 the spacing to other ports is capped at the structure spacing.
        cap: Option<f64>,
    },
    DefensePost {
        other_tiles: Vec<TileRef>,
        spacing: Spacing,
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
    random: SharedRandom,
    player: SmallId,
    reachable_stations_cache: Option<Vec<StationEntry>>,
    shared_water_components: Option<HashSet<i64>>,
    last_structure_tick: Option<Tick>,
    placements_count: usize,
    built_crowded_map_first_structure: bool,
    has_high_starting_gold: Option<bool>,
    post_save_up_start_tick: Option<Tick>,
    /// `isMirvWorthSavingFor` for the tick it was computed in.
    mirv_worth_saving_memo: std::cell::Cell<Option<(Tick, bool)>>,
    /// Test hook: records the placements instead of queueing them.
    pub log: DecisionLog,
}

impl NationStructureBehavior {
    /// `new NationStructureBehavior(random, game, player)`.
    pub fn new(random: SharedRandom, _game: &GameImpl, player: SmallId) -> Self {
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
            mirv_worth_saving_memo: std::cell::Cell::new(None),
            log: DecisionLog::default(),
        }
    }

    fn rnd(&self) -> std::cell::RefMut<'_, PseudoRandom> {
        self.random.borrow_mut()
    }

    fn difficulty(game: &GameImpl) -> Difficulty {
        game.config().game_config().difficulty
    }

    /// True before the v0.31 structure rework (plain build order, defense posts in it).
    fn legacy31(game: &GameImpl) -> bool {
        !game.has(Change::NationStructureRework31)
    }

    /// `handleStructures()`.
    pub fn handle_structures(&mut self, game: &mut GameImpl) -> bool {
        if Self::legacy31(game) {
            return self.do_handle_structures(game);
        }
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

    fn is_on_structure_cooldown(&self, game: &GameImpl) -> bool {
        // Only high-starting-gold nations pause.
        let Some(last) = self.last_structure_tick else {
            return false;
        };
        if !self.has_high_starting_gold(game) {
            return false;
        }
        if game.has(Change::NationsLikeHumans5899) {
            let p = game.player(self.player);
            let info = p.info();
            if game
                .config()
                .starting_gold(info.player_type, info.is_lobby_creator)
                >= PACED_STARTING_GOLD_LIMIT
            {
                return false;
            }
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
        // Not while the gold covers the target twice, as bulk upgrades leave the target for nukes.
        if game.has(Change::NationsLikeHumans5899)
            && game.player(self.player).gold() >= save_up_target * 2
        {
            return false;
        }
        let elapsed = (game.ticks() as f64) - (self.post_save_up_start_tick.unwrap() as f64);
        elapsed % ((TEAM_POST_SAVE_UP_PHASE_TICKS * 2) as f64)
            >= TEAM_POST_SAVE_UP_PHASE_TICKS as f64
    }

    fn do_handle_structures(&mut self, game: &mut GameImpl) -> bool {
        self.reachable_stations_cache = None;
        let cities_disabled = game.config().is_unit_disabled(UnitType::City);
        let city_count = self.city_count(game);
        let legacy = Self::legacy31(game);
        let has_coastal_tiles = if legacy {
            game.player(self.player)
                .border_tiles()
                .iter()
                .any(|t| game.map.is_ocean_shore(t))
        } else {
            self.shared_water_components = game.shared_water_components(self.player);
            self.shared_water_components.is_some()
        };

        let missile_silos_enabled = !game.config().is_unit_disabled(UnitType::MissileSilo);

        // High-starting-gold Hard/Impossible nations build a SAM first.
        let difficulty = Self::difficulty(game);
        if !legacy
            && self.placements_count == 0
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
        let own_flag = game.has(Change::NationCrowdedFirst5344);
        let first_pending = if own_flag {
            !self.built_crowded_map_first_structure
        } else {
            game.player(self.player).units_owned(UnitType::City) == 0
        };
        if !legacy
            && !cities_disabled
            && first_pending
            && (self.is_high_nation_density(game)
                || (game.has(Change::NationsLikeHumans5899)
                    && difficulty != Difficulty::Easy
                    && self.starts_in_team_spawn_area(game)))
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

        if game.has(Change::NationsLikeHumans5899) && self.maybe_build_proactive_defense_post(game)
        {
            return true;
        }

        // Build order for non-city structures (priority order).
        let build_order: &[UnitType] = if legacy {
            &[
                UnitType::DefensePost,
                UnitType::Port,
                UnitType::Factory,
                UnitType::SAMLauncher,
                UnitType::MissileSilo,
            ]
        } else {
            &[
                UnitType::Port,
                UnitType::Factory,
                UnitType::SAMLauncher,
                UnitType::MissileSilo,
            ]
        };

        let nukes_enabled = !game.config().is_unit_disabled(UnitType::AtomBomb)
            || !game.config().is_unit_disabled(UnitType::HydrogenBomb)
            || !game.config().is_unit_disabled(UnitType::MIRV);

        for &structure_type in build_order {
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
        if game.has(Change::NationsLikeHumans5899) {
            return has_high_starting_gold(game, self.player);
        }
        let info = game.player(self.player).info().clone();
        game.config()
            .starting_gold(info.player_type, info.is_lobby_creator)
            >= HIGH_STARTING_GOLD_THRESHOLD
    }

    /// Cities owned, or a territory-based equivalent when cities are disabled.
    fn city_count(&self, game: &GameImpl) -> f64 {
        if game.config().is_unit_disabled(UnitType::City) {
            return (game.player(self.player).num_tiles_owned() as f64 / TILES_PER_CITY_EQUIVALENT)
                .floor()
                .max(1.0);
        }
        game.player(self.player).units_owned(UnitType::City) as f64
    }

    fn starts_in_team_spawn_area(&self, game: &GameImpl) -> bool {
        game.player(self.player)
            .team()
            .is_some_and(|team| game.team_spawn_area(team).is_some())
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
        if game.has(Change::NationsLikeHumans5899) {
            return self.levels_due(game, t, city_count, has_coastal_tiles) > 0.0;
        }
        let Some(config) =
            structure_ratio(difficulty, GameMode::FFA, false, Self::legacy31(game), t)
        else {
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

        // Density cap on defense posts: they cannot be upgraded, so a new one would be built.
        if t == UnitType::DefensePost {
            let tiles_owned = game.player(self.player).num_tiles_owned() as f64;
            if tiles_owned > 0.0 && (owned as f64) / tiles_owned >= 1.0 / 5000.0 {
                return false;
            }
        }

        // The first missile silo uses a higher ratio so nations start nuking earlier.
        if t == UnitType::MissileSilo && owned == 0 && !Self::legacy31(game) {
            ratio = FIRST_MISSILE_SILO_RATIO;
        }

        let target_count = (city_count * ratio).floor();

        (owned as f64) < target_count
    }

    /// Levels of this structure type the configured ratio to the city count still asks for.
    fn levels_due(
        &self,
        game: &GameImpl,
        t: UnitType,
        city_count: f64,
        has_coastal_tiles: bool,
    ) -> f64 {
        let game_config = game.config().game_config();
        let (difficulty, game_mode) = (game_config.difficulty, game_config.game_mode);
        let Some(config) = structure_ratio(difficulty, game_mode, true, false, t) else {
            return 0.0;
        };
        let mut ratio = config.ratio_per_city;
        // Reduce factory spawning if there are coastal tiles.
        if t == UnitType::Factory
            && has_coastal_tiles
            && !game.config().is_unit_disabled(UnitType::Port)
        {
            ratio *= factory_coastal_ratio_multiplier(difficulty);
        }
        let owned = game.player(self.player).units_owned(t) as f64;
        let target_count = if t == UnitType::MissileSilo {
            // The first missile silo uses a higher ratio so nations start nuking earlier.
            if owned == 0.0 {
                ratio = FIRST_MISSILE_SILO_RATIO;
            }
            f64::min((city_count * ratio).floor(), MAX_MISSILE_SILOS as f64)
        } else {
            (city_count * ratio).floor()
        };
        target_count - owned
    }

    fn cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        self.cost_extra(game, t, 0)
    }

    fn cost_extra(&self, game: &GameImpl, t: UnitType, extra_units: i64) -> Gold {
        game.player(self.player).unit_cost(t, extra_units)
    }

    fn maybe_spawn_structure(&mut self, game: &mut GameImpl, t: UnitType) -> bool {
        let perceived_cost = self.get_perceived_cost(game, t);
        if game.player(self.player).gold() < perceived_cost {
            return false;
        }

        let v2 = game.has(Change::NationsLikeHumans5899);
        // Another SAM would cover nothing new, but a higher level stops more of a salvo.
        if v2 && t == UnitType::SAMLauncher && self.should_upgrade_sam_instead(game) {
            let best = self.sam_protecting_most(game);
            return self.upgrade_structure(game, best);
        }

        // Upgrade instead of building new when the density threshold is exceeded.
        let structures = game.player(self.player).units_of(t);
        let upgradable = game.config().unit_info(t).upgradable.unwrap_or(false);
        if self.get_total_structure_density(game) > UPGRADE_DENSITY_THRESHOLD && upgradable {
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
            // No room for a new one (e.g. a short coastline full of ports): upgrade one instead.
            return v2
                && upgradable
                && Self::difficulty(game) != Difficulty::Easy
                && self.maybe_upgrade_structure(game, &structures);
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

        let v2 = game.has(Change::NationsLikeHumans5899);
        let save_up_target = self.get_save_up_target(game);
        if save_up_target == 0
            || game.player(self.player).gold() >= save_up_target
            || (v2 && !is_save_up_within_reach(game, self.player, save_up_target))
        {
            return real_cost;
        }

        let game_config = game.config().game_config();
        let (difficulty, game_mode) = (game_config.difficulty, game_config.game_mode);
        // Like humans, nations do not save up before their first few cities stand.
        if v2 && self.city_count(game) < cities_before_saving(difficulty) {
            return real_cost;
        }

        let owned = game.player(self.player).units_owned(t) as f64;

        let mut increase_per_owned = if t == UnitType::City {
            CITY_PERCEIVED_COST_INCREASE_PER_OWNED
        } else {
            structure_ratio(difficulty, game_mode, v2, Self::legacy31(game), t)
                .map_or(0.1, |c| c.perceived_cost_increase_per_owned)
        };
        // Income structures pay for themselves, so smarter nations keep building them while saving.
        if v2 && matches!(t, UnitType::City | UnitType::Port | UnitType::Factory) {
            increase_per_owned *= income_structure_saving_weight(difficulty);
        }

        // realCost * (1 + increasePerOwned * owned)
        let multiplier = 1.0 + increase_per_owned * owned;
        ((real_cost as f64) * multiplier).ceil() as Gold
    }

    /// The gold target to save up for, based on the enabled nukes; 0 when no saving is needed.
    fn get_save_up_target(&self, game: &GameImpl) -> Gold {
        let config = game.config();

        let legacy = Self::legacy31(game);
        // Only save up for SAMs if missile silos are disabled (nothing to save before v0.31).
        if config.is_unit_disabled(UnitType::MissileSilo) {
            return if legacy {
                0
            } else {
                self.cost(game, UnitType::SAMLauncher)
            };
        }

        // A limited amount in team games, synced with NationNukeBehavior.
        if !legacy && config.game_config().game_mode == GameMode::Team {
            return self.cost(game, UnitType::HydrogenBomb);
        }

        let mirv_enabled = !config.is_unit_disabled(UnitType::MIRV);
        let hydro_enabled = !config.is_unit_disabled(UnitType::HydrogenBomb);
        let atom_enabled = !config.is_unit_disabled(UnitType::AtomBomb);

        // Not for a MIRV that SAMs would shoot down.
        if mirv_enabled
            && (!game.has(Change::NationsLikeHumans5899)
                || self.is_mirv_worth_saving_for_memo(game))
        {
            return self.cost(game, UnitType::MIRV) + self.cost(game, UnitType::HydrogenBomb);
        }
        if hydro_enabled {
            return self.cost(game, UnitType::HydrogenBomb) * 5;
        }
        if atom_enabled {
            return self.cost(game, UnitType::AtomBomb) * 20;
        }
        if legacy {
            return 0;
        }
        self.cost(game, UnitType::SAMLauncher)
    }
}

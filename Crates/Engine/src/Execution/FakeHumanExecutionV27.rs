// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/FakeHumanExecution.ts of v0.27.10: the AI player of a map nation on the stateful `BotBehavior`.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::SpawnExecution::SpawnExecution;
use crate::Builds::Rules::Change;
use crate::Builds::Rules::Rules;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::{bounding_box_tiles, simple_hash};
use crate::Execution::Alliance::AllianceRequestExecution::AllianceRequestExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::{calculate_territory_center, closest_two_tiles};
use crate::Execution::Utils::BotBehaviorShared::{BotBehaviorAlliances, BotBehaviorShared};
use crate::Execution::Utils::BotBehaviorV27::BotBehaviorV27;
use crate::Execution::Utils::FakeHumanMirv::{FakeHumanMirv, MIRV_HESITATION_ODDS};
use crate::Execution::Utils::FakeHumanShared::{
    FakeHumanEmoji, FakeHumanShared, FakeHumanStructures, truthy,
};
use crate::Game::Game::{Difficulty, Nation, PlayerType, SmallId, Tick, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::cell::RefCell;
use std::rc::Rc;

/// Drives one fake human: spawning, then periodic decisions.
pub struct FakeHumanExecution {
    active: bool,
    random: SharedRandom,
    behavior: Option<BotBehaviorV27>,
    player: Option<SmallId>,
    attack_rate: f64,
    attack_tick: f64,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    last_emoji_sent: OrderedMap<SmallId, Tick>,
    last_nuke_sent: Vec<(Tick, TileRef)>,
    last_mirv_sent: Vec<(Tick, TileRef)>,
    embargo_malus_applied: OrderedSet<SmallId>,
    tracked_transport_ships: OrderedSet<UnitId>,
    tracked_trade_ships: OrderedSet<UnitId>,
    mirv_targets_cache: Option<(Tick, Vec<SmallId>)>,
    game_id: String,
    nation: Nation,
    rules: Rules,
}

impl FakeHumanShared for FakeHumanExecution {
    fn random(&self) -> &SharedRandom {
        &self.random
    }

    fn player(&self) -> Option<SmallId> {
        self.player
    }

    fn nation(&self) -> &Nation {
        &self.nation
    }

    fn last_nuke_sent(&self) -> &[(Tick, TileRef)] {
        &self.last_nuke_sent
    }

    fn last_nuke_sent_mut(&mut self) -> &mut Vec<(Tick, TileRef)> {
        &mut self.last_nuke_sent
    }

    fn embargo_malus_applied_mut(&mut self) -> &mut OrderedSet<SmallId> {
        &mut self.embargo_malus_applied
    }
}

impl FakeHumanEmoji for FakeHumanExecution {
    fn last_emoji_sent_mut(&mut self) -> &mut OrderedMap<SmallId, Tick> {
        &mut self.last_emoji_sent
    }
}

impl FakeHumanMirv for FakeHumanExecution {
    fn last_mirv_sent_mut(&mut self) -> &mut Vec<(Tick, TileRef)> {
        &mut self.last_mirv_sent
    }

    fn mirv_targets_cache_mut(&mut self) -> &mut Option<(Tick, Vec<SmallId>)> {
        &mut self.mirv_targets_cache
    }

    fn tracked_transport_ships_mut(&mut self) -> &mut OrderedSet<UnitId> {
        &mut self.tracked_transport_ships
    }

    fn tracked_trade_ships_mut(&mut self) -> &mut OrderedSet<UnitId> {
        &mut self.tracked_trade_ships
    }
}

impl FakeHumanStructures for FakeHumanExecution {}

impl FakeHumanExecution {
    /// `new FakeHumanExecution(gameID, nation)`.
    pub fn new(game_id: &str, nation: Nation, rules: &Rules) -> Self {
        let seed = simple_hash(&nation.player_info.id) as f64 + simple_hash(game_id) as f64;
        let mut random = PseudoRandom::for_rules(rules, seed);
        let attack_rate = random.next_int(40.0, 80.0);
        let attack_tick = random.next_int(0.0, attack_rate);
        let trigger_ratio = random.next_int(50.0, 60.0) / 100.0;
        let reserve_ratio = random.next_int(30.0, 40.0) / 100.0;
        let expand_ratio = random.next_int(10.0, 20.0) / 100.0;
        Self {
            active: true,
            random: Rc::new(RefCell::new(random)),
            behavior: None,
            player: None,
            attack_rate,
            attack_tick,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            last_emoji_sent: OrderedMap::new(),
            last_nuke_sent: Vec::new(),
            last_mirv_sent: Vec::new(),
            embargo_malus_applied: OrderedSet::new(),
            tracked_transport_ships: OrderedSet::new(),
            tracked_trade_ships: OrderedSet::new(),
            mirv_targets_cache: None,
            game_id: game_id.to_string(),
            nation,
            rules: rules.clone(),
        }
    }

    fn maybe_attack(&mut self, game: &mut GameImpl) {
        let player = self.me();
        let map = &game.map;
        let enemy_border: Vec<TileRef> = game
            .player(player)
            .border_tiles()
            .iter()
            .flat_map(|t| map.neighbors(t))
            .filter(|&t| map.is_land(t) && map.owner_id(t) != player)
            .collect();
        let owners: Vec<SmallId> = enemy_border.iter().map(|&t| map.owner_id(t)).collect();
        let mut behavior = self.behavior.take().expect("not initialized");
        self.run_attack(game, &mut behavior, enemy_border.is_empty(), &owners);
        self.behavior = Some(behavior);
    }

    fn run_attack(
        &mut self,
        game: &mut GameImpl,
        behavior: &mut BotBehaviorV27,
        no_enemy_border: bool,
        owners: &[SmallId],
    ) {
        let player = self.me();
        let mut bordering_enemies: Vec<SmallId> = Vec::new();
        if no_enemy_border {
            if self.chance(5.0) {
                self.send_boat_randomly(game);
            }
        } else {
            if self.chance(10.0) {
                self.send_boat_randomly(game);
                return;
            }
            if owners.contains(&0) {
                behavior.send_attack(game, 0);
                return;
            }
            // Duplicates stay: the list is sorted by troops (stable) and sampled as is
            bordering_enemies = owners.to_vec();
            bordering_enemies.sort_by(|&a, &b| {
                (game.player(a).troops() - game.player(b).troops())
                    .partial_cmp(&0.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            // 5% chance to send a random alliance request
            if self.chance(20.0) {
                let to_ally = *self.random.borrow_mut().rand_element(&bordering_enemies);
                if game.player(player).can_send_alliance_request(to_ally) {
                    let id = game.player(to_ally).id().to_string();
                    if game.has(Change::NationAllianceRequestExecution2234) {
                        game.add_execution(Box::new(AllianceRequestExecution::new(player, id)));
                    } else {
                        game.player_mut(player).create_alliance_request(to_ally);
                    }
                }
            }
        }
        behavior.forget_old_enemies(game);
        behavior.assist_allies(game);
        let Some(enemy) = behavior.select_enemy(game, &bordering_enemies) else {
            return;
        };
        self.maybe_send_emoji(game, enemy);
        self.maybe_send_nuke(game, enemy);
        if game.player(player).shares_border_with(enemy) {
            behavior.send_attack(game, enemy);
        } else {
            self.maybe_send_boat_attack(game, enemy);
        }
    }

    fn maybe_send_boat_attack(&mut self, game: &mut GameImpl, other: SmallId) {
        let player = self.me();
        if game.player(player).is_friendly(other, false) {
            return;
        }
        let map = &game.map;
        let ours: Vec<TileRef> = game
            .player(player)
            .border_tiles()
            .iter()
            .filter(|&t| map.is_ocean_shore(t))
            .collect();
        let theirs: Vec<TileRef> = game
            .player(other)
            .border_tiles()
            .iter()
            .filter(|&t| map.is_ocean_shore(t))
            .collect();
        let Some((_, landing)) = closest_two_tiles(map, ours, theirs) else {
            return;
        };
        let troops = game.player(player).troops() / 5.0;
        let other_id = game.player(other).id().to_string();
        game.add_execution(Box::new(
            TransportShipExecution::new(player, landing, Some(troops))
                .with_intent_target(Some(other_id), None),
        ));
    }

    fn maybe_send_nuke(&mut self, game: &mut GameImpl, other: SmallId) {
        let player = self.me();
        let silos = game.player(player).units_of(UnitType::MissileSilo);
        if silos.is_empty()
            || game.player(player).gold() < self.cost(game, UnitType::AtomBomb)
            // Don't nuke bots (as opposed to fake humans and humans)
            || game.player(other).player_type() == PlayerType::Bot
            || game.player(player).is_on_same_team(other)
        {
            return;
        }
        let nuke_type = if game.player(player).gold() > self.cost(game, UnitType::HydrogenBomb) {
            UnitType::HydrogenBomb
        } else {
            UnitType::AtomBomb
        };
        let range: i32 = if nuke_type == UnitType::HydrogenBomb {
            60
        } else {
            15
        };
        let structures = game.player(other).units_of_types(&[
            UnitType::City,
            UnitType::DefensePost,
            UnitType::MissileSilo,
            UnitType::Port,
            UnitType::SAMLauncher,
        ]);
        let structure_tiles: Vec<TileRef> = structures
            .iter()
            .map(|&u| game.unit_any(u).tile())
            .collect();
        let random_tiles = self.rand_territory_tile_array(game, 10);
        let mut all_tiles: OrderedSet<TileRef> = OrderedSet::new();
        for t in random_tiles.into_iter().chain(structure_tiles) {
            all_tiles.insert(t);
        }

        let mut best_tile: Option<TileRef> = None;
        let mut best_value = 0.0;
        self.remove_old_nuke_events(game);
        'outer: for &tile in all_tiles.iter() {
            let mut bounding_box = bounding_box_tiles(&game.map, tile, range);
            // Add radius / 2 in case there is a piece of unwanted territory inside the outer radius
            bounding_box.extend(bounding_box_tiles(&game.map, tile, range / 2));
            for t in bounding_box {
                // Make sure we nuke away from the border
                if game.owner_id(t) != other {
                    continue 'outer;
                }
            }
            if !truthy(game.player(player).can_build(nuke_type, tile, None)) {
                continue;
            }
            let value = self.nuke_tile_score(game, tile, &silos, &structures);
            if value > best_value {
                best_tile = Some(tile);
                best_value = value;
            }
        }
        if let Some(tile) = best_tile {
            self.send_nuke(game, tile, nuke_type);
        }
    }

    fn send_nuke(&mut self, game: &mut GameImpl, tile: TileRef, nuke_type: UnitType) {
        let tick = game.ticks();
        self.last_nuke_sent.push((tick, tile));
        game.add_execution(Box::new(NukeExecution::new(
            nuke_type,
            self.me(),
            tile,
            None,
            -1.0,
            0.0,
            None,
        )));
    }

    fn send_boat_randomly(&mut self, game: &mut GameImpl) {
        let player = self.me();
        let ocean_shore: Vec<TileRef> = game
            .player(player)
            .border_tiles()
            .iter()
            .filter(|&t| game.map.is_ocean_shore(t))
            .collect();
        if ocean_shore.is_empty() {
            return;
        }
        let src = *self.random.borrow_mut().rand_element(&ocean_shore);

        // First look for high-interest targets (unowned or bot-owned), mainly relevant early on
        let mut dst = self.random_boat_target(game, src, true);
        if dst.is_none() {
            // None found? Then look for players
            dst = self.random_boat_target(game, src, false);
        }
        let Some(dst) = dst else { return };
        let troops = game.player(player).troops() / 5.0;
        let owner = game.owner_id(dst);
        let target_id = (owner != 0).then(|| game.player(owner).id().to_string());
        game.add_execution(Box::new(
            TransportShipExecution::new(player, dst, Some(troops))
                .with_intent_target(target_id, None),
        ));
    }

    fn random_boat_target(
        &self,
        game: &GameImpl,
        tile: TileRef,
        high_interest_only: bool,
    ) -> Option<TileRef> {
        let player = self.me();
        let (x, y) = (game.x(tile) as f64, game.y(tile) as f64);
        for _ in 0..500 {
            let rand_x = self.next_int(x - 150.0, x + 150.0);
            let rand_y = self.next_int(y - 150.0, y + 150.0);
            if !game.is_valid_coord(rand_x as i32, rand_y as i32) {
                continue;
            }
            let rand_tile = game.tile_ref(rand_x as i32, rand_y as i32);
            if !game.is_land(rand_tile) {
                continue;
            }
            let owner = game.owner_id(rand_tile);
            if owner == player {
                continue;
            }
            // Don't spam boats into players more than twice as large as us
            if owner != 0 && game.player(owner).troops() > game.player(player).troops() * 2.0 {
                continue;
            }
            if high_interest_only {
                // Prioritise unowned tiles or tiles owned by bots
                if owner == 0 || game.player(owner).player_type() == PlayerType::Bot {
                    return Some(rand_tile);
                }
            } else if owner == 0 || !game.player(owner).is_friendly(player, false) {
                // Unowned tiles or tiles of non-friendly players
                return Some(rand_tile);
            }
        }
        None
    }

    // ---- MIRV strategy ------------------------------------------------------------------------

    fn consider_mirv(&mut self, game: &mut GameImpl) -> bool {
        let player = self.me();
        if game
            .player(player)
            .units_of(UnitType::MissileSilo)
            .is_empty()
        {
            return false;
        }
        if game.player(player).gold() < self.cost(game, UnitType::MIRV) {
            return false;
        }
        self.remove_old_mirv_events(game);
        if !self.last_mirv_sent.is_empty() {
            return false;
        }
        if self.chance(MIRV_HESITATION_ODDS) {
            self.trigger_mirv_cooldown(game, None);
            return false;
        }
        if let Some(sender) = self.select_counter_mirv_target(game) {
            self.maybe_send_mirv(game, sender);
            return true;
        }
        if let Some(target) = self.select_victory_denial_target(game) {
            self.maybe_send_mirv(game, target);
            return true;
        }
        if let Some(target) = self.select_steamroll_stop_target(game) {
            self.maybe_send_mirv(game, target);
            return true;
        }
        false
    }

    fn maybe_send_mirv(&mut self, game: &mut GameImpl, enemy: SmallId) {
        self.maybe_send_emoji(game, enemy);
        let center_tile = calculate_territory_center(game, enemy);
        if let Some(center) = center_tile
            && center != 0
            && truthy(
                game.player(self.me())
                    .can_build(UnitType::MIRV, center, None),
            )
        {
            self.send_mirv(game, center);
        }
    }

    // ---- ship tracking ------------------------------------------------------------------------
}

impl Execution for FakeHumanExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        // Upstream draws once here for a never-used traitor role
        let _ = self.chance(10.0);
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        // Ship tracking
        if let Some(player) = self.player
            && game.player(player).is_alive()
            && game.config().game_config().difficulty != Difficulty::Easy
        {
            self.track_transport_ships_and_retaliate(game);
            self.track_trade_ships_and_retaliate(game);
        }

        if ticks as f64 % self.attack_rate != self.attack_tick {
            return;
        }

        if game.in_spawn_phase() {
            let Some(rl) = self.random_spawn_land(game) else {
                return;
            };
            game.add_execution(Box::new(SpawnExecution::new(
                &self.game_id,
                self.nation.player_info.clone(),
                Some(rl),
                false,
                &self.rules,
            )));
            return;
        }

        if self.player.is_none() {
            self.player = game
                .players()
                .into_iter()
                .find(|&p| game.player(p).id() == self.nation.player_info.id);
            if self.player.is_none() {
                return;
            }
        }
        let player = self.me();

        if !game.player(player).is_alive() {
            self.active = false;
            return;
        }

        if self.behavior.is_none() {
            // The player is unavailable during init
            let mut behavior = BotBehaviorV27::new(
                self.random.clone(),
                player,
                self.trigger_ratio,
                self.reserve_ratio,
                self.expand_ratio,
            );
            // Send an attack on the first tick
            behavior.force_send_attack(game, 0);
            self.behavior = Some(behavior);
            return;
        }

        self.update_relations_from_embargos(game);
        let mut behavior = self.behavior.take().expect("checked above");
        behavior.handle_alliance_requests(game);
        behavior.handle_alliance_extension_requests(game);
        self.behavior = Some(behavior);
        self.handle_units(game);
        self.handle_embargoes_to_hostile_nations(game);
        self.consider_mirv(game);
        self.maybe_attack(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}

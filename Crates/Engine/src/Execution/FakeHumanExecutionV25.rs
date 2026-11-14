// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/FakeHumanExecution.ts of v0.25.20 to v0.25.23: the AI player of a map nation (the v25 branch, before the structure value function and betrayal rework).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::SpawnExecution::SpawnExecution;
use crate::Builds::Rules::Rules;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::{calculate_bounding_box, simple_hash};
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::closest_two_tiles;
use crate::Execution::Utils::BotBehaviorShared::BotBehaviorAlliances;
use crate::Execution::Utils::BotBehaviorV25::BotBehaviorV25;
use crate::Execution::Utils::FakeHumanShared::{
    FakeHumanBoats, FakeHumanEmoji, FakeHumanShared, truthy,
};
use crate::Game::Game::{Difficulty, Nation, PlayerType, Relation, SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::{TileRef, manhattan_dist_fn};
use std::cell::RefCell;
use std::rc::Rc;

/// Drives one fake human: spawning, then periodic decisions.
pub struct FakeHumanExecutionV25 {
    active: bool,
    random: SharedRandom,
    behavior: Option<BotBehaviorV25>,
    player: Option<SmallId>,
    attack_rate: f64,
    attack_tick: f64,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    last_nuke_sent: Vec<(Tick, TileRef)>,
    last_emoji_sent: OrderedMap<SmallId, Tick>,
    embargo_malus_applied: OrderedSet<SmallId>,
    game_id: String,
    nation: Nation,
    rules: Rules,
}

impl FakeHumanShared for FakeHumanExecutionV25 {
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

impl FakeHumanEmoji for FakeHumanExecutionV25 {
    fn last_emoji_sent_mut(&mut self) -> &mut OrderedMap<SmallId, Tick> {
        &mut self.last_emoji_sent
    }
}

impl FakeHumanBoats for FakeHumanExecutionV25 {}

impl FakeHumanExecutionV25 {
    /// `new FakeHumanExecution(gameID, nation)`.
    pub fn new(game_id: &str, nation: Nation, rules: &Rules) -> Self {
        let seed = simple_hash(&nation.player_info.id) as f64 + simple_hash(game_id) as f64;
        let mut random = PseudoRandom::for_rules(rules, seed);
        let attack_rate = random.next_int(40.0, 80.0);
        let attack_tick = random.next_int(0.0, attack_rate);
        let trigger_ratio = random.next_int(60.0, 90.0) / 100.0;
        let reserve_ratio = random.next_int(30.0, 60.0) / 100.0;
        let expand_ratio = random.next_int(15.0, 25.0) / 100.0;
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
            last_nuke_sent: Vec::new(),
            last_emoji_sent: OrderedMap::new(),
            embargo_malus_applied: OrderedSet::new(),
            game_id: game_id.to_string(),
            nation,
            rules: rules.clone(),
        }
    }

    fn handle_embargoes_ignoring_teams(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player else { return };
        let others: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != player)
            .collect();
        for other in others {
            // When hostile, start an embargo and keep it until neutral again
            let relation = game.player(player).relation(other);
            if relation <= Relation::Hostile && !game.player(player).has_embargo_against(other) {
                game.player_mut(player).add_embargo(other, false);
            } else if relation >= Relation::Neutral
                && game.player(player).has_embargo_against(other)
            {
                game.player_mut(player).stop_embargo(other);
            }
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

        if enemy_border.is_empty() {
            if self.chance(10.0) {
                self.send_boat_randomly(game);
            }
            return;
        }
        if self.chance(20.0) {
            self.send_boat_randomly(game);
            return;
        }

        let border_players: Vec<SmallId> = enemy_border.iter().map(|&t| map.owner_id(t)).collect();
        let mut behavior = self.behavior.take().expect("not initialized");
        if border_players.contains(&0) {
            behavior.send_attack(game, 0);
            self.behavior = Some(behavior);
            return;
        }

        // Duplicates stay: the list is not de-duplicated upstream
        let mut enemies: Vec<SmallId> = border_players;
        enemies.sort_by(|&a, &b| {
            (game.player(a).troops() - game.player(b).troops())
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 5% chance to send a random alliance request
        if self.chance(20.0) {
            let to_ally = *self.random.borrow_mut().rand_element(&enemies);
            if game.player(player).can_send_alliance_request(to_ally) {
                game.player_mut(player).create_alliance_request(to_ally);
                self.behavior = Some(behavior);
                return;
            }
        }

        // 50-50 attack weakest player vs random player
        let to_attack = if self.chance(2.0) {
            enemies[0]
        } else {
            *self.random.borrow_mut().rand_element(&enemies)
        };
        if self.should_attack(game, to_attack) {
            behavior.send_attack(game, to_attack);
            self.behavior = Some(behavior);
            return;
        }

        behavior.forget_old_enemies(game);
        behavior.assist_allies(game);
        let enemy = behavior.select_enemy(game);
        if let Some(enemy) = enemy {
            self.maybe_send_emoji(game, enemy);
            self.maybe_send_nuke(game, enemy);
            if game.player(player).shares_border_with(enemy) {
                behavior.send_attack(game, enemy);
            } else {
                self.maybe_send_boat_attack(game, enemy);
            }
        }
        self.behavior = Some(behavior);
    }

    fn should_attack(&self, game: &GameImpl, other: SmallId) -> bool {
        let player = self.me();
        if game.player(player).is_on_same_team(other) {
            return false;
        }
        if game.player(player).is_friendly(other, false) {
            if self.should_discourage_attack(game, other) {
                self.chance(200.0)
            } else {
                self.chance(50.0)
            }
        } else if self.should_discourage_attack(game, other) {
            self.chance(4.0)
        } else {
            true
        }
    }

    fn should_discourage_attack(&self, game: &GameImpl, other: SmallId) -> bool {
        if game.player(other).is_traitor() {
            return false;
        }
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible {
            return false;
        }
        // Only discourage attacks on humans who are not traitors on easy or medium difficulty
        game.player(other).player_type() == PlayerType::Human
    }

    fn maybe_send_boat_attack(&mut self, game: &mut GameImpl, other: SmallId) {
        let player = self.me();
        if game.player(player).is_on_same_team(other) {
            return;
        }
        let map = &game.map;
        let ours: Vec<u32> = game
            .player(player)
            .border_tiles()
            .iter()
            .filter(|&t| map.is_ocean_shore(t))
            .collect();
        let theirs: Vec<u32> = game
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
        let bounding_box =
            calculate_bounding_box(&game.map, game.player(other).border_tiles().iter());
        let mut random_tiles: Vec<Option<TileRef>> = Vec::with_capacity(10);
        for _ in 0..10 {
            random_tiles.push(self.rand_territory_tile(game, other, bounding_box));
        }
        let mut all_tiles: OrderedSet<TileRef> = OrderedSet::new();
        for t in random_tiles.into_iter().flatten().chain(structure_tiles) {
            all_tiles.insert(t);
        }

        let mut best_tile: Option<TileRef> = None;
        let mut best_value = 0.0;
        self.remove_old_nuke_events(game);
        'outer: for &tile in all_tiles.iter() {
            let diamond = manhattan_dist_fn(tile, 15.0, false);
            let near = game.map.bfs(tile, |g, t| diamond(g, t));
            for &t in near.iter() {
                // Make sure we nuke at least 15 tiles in border
                if game.owner_id(t) != other {
                    continue 'outer;
                }
            }
            if !truthy(
                game.player(player)
                    .can_build(UnitType::AtomBomb, tile, None),
            ) {
                continue;
            }
            let value = self.nuke_tile_score(game, tile, &silos, &structures);
            if value > best_value {
                best_tile = Some(tile);
                best_value = value;
            }
        }
        if let Some(tile) = best_tile {
            self.send_nuke(game, tile);
        }
    }

    fn send_nuke(&mut self, game: &mut GameImpl, tile: TileRef) {
        let tick = game.ticks();
        self.last_nuke_sent.push((tick, tile));
        game.add_execution(Box::new(NukeExecution::new(
            UnitType::AtomBomb,
            self.me(),
            tile,
            None,
            -1.0,
            0.0,
            None,
        )));
    }

    fn handle_units(&mut self, game: &mut GameImpl) -> bool {
        self.maybe_spawn_structure(game, UnitType::Port, 1)
            || self.maybe_spawn_structure(game, UnitType::City, 2)
            || self.maybe_spawn_warship(game)
            || self.maybe_spawn_structure(game, UnitType::Factory, 1)
            || self.maybe_spawn_structure(game, UnitType::MissileSilo, 1)
    }

    fn maybe_spawn_structure(
        &mut self,
        game: &mut GameImpl,
        unit_type: UnitType,
        max_num: i64,
    ) -> bool {
        let player = self.me();
        if game.player(player).units_owned(unit_type) >= max_num {
            return false;
        }
        if game.player(player).gold() < self.cost(game, unit_type) {
            return false;
        }
        let Some(tile) = self.structure_spawn_tile(game, unit_type) else {
            return false;
        };
        if game
            .player(player)
            .can_build(unit_type, tile, None)
            .is_none()
        {
            return false;
        }
        game.add_execution(Box::new(ConstructionExecution::new(
            player, unit_type, tile, None, None,
        )));
        true
    }

    fn structure_spawn_tile(&mut self, game: &GameImpl, unit_type: UnitType) -> Option<TileRef> {
        let player = self.me();
        let tiles: Vec<TileRef> = if unit_type == UnitType::Port {
            game.player(player)
                .border_tiles()
                .iter()
                .filter(|&t| game.map.is_ocean_shore(t))
                .collect()
        } else {
            game.player(player).tiles().iter().collect()
        };
        if tiles.is_empty() {
            return None;
        }
        Some(*self.random.borrow_mut().rand_element(&tiles))
    }
}

impl Execution for FakeHumanExecutionV25 {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        // Upstream draws once here for a never-used traitor role
        let _ = self.chance(10.0);
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
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
            let mut behavior = BotBehaviorV25::new(
                self.random.clone(),
                player,
                self.trigger_ratio,
                self.reserve_ratio,
                self.expand_ratio,
            );
            // Send an attack on the first tick
            behavior.send_attack(game, 0);
            self.behavior = Some(behavior);
            return;
        }

        self.update_relations_from_embargos(game);
        let mut behavior = self.behavior.take().expect("checked above");
        behavior.handle_alliance_requests(game);
        behavior.handle_alliance_extension_requests(game);
        self.behavior = Some(behavior);
        self.handle_units(game);
        self.handle_embargoes_ignoring_teams(game);
        self.maybe_attack(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}

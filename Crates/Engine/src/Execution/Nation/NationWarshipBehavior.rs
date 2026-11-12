// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationWarshipBehavior.ts: nation warship decisions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::Nation::NationEmojiBehavior::{
    NationEmojiBehavior, emoji_warship_retaliation,
};
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Game::Game::{
    Difficulty, EmojiRecipient, PlayerType, SmallId, UnitId, UnitType, WarshipMode,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Why a warship is sent out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reason {
    Trade,
    Transport,
    /// An incoming enemy transport (v0.36.0 on): may dip into the gold kept for nukes.
    Invasion,
}

/// Decides warship spawning, retaliation and infestation counters of a nation.
pub struct NationWarshipBehavior {
    random: SharedRandom,
    player: SmallId,
    emoji_behavior: Shared<NationEmojiBehavior>,
    tracked_transport_ships: OrderedSet<UnitId>,
    tracked_trade_ships: OrderedSet<UnitId>,
    tracked_incoming_transport_ships: OrderedSet<UnitId>,
    dealt_with_transport_ship: OrderedSet<UnitId>,
}

impl NationWarshipBehavior {
    /// `new NationWarshipBehavior(random, game, player, emojiBehavior)`.
    pub fn new(
        random: SharedRandom,
        _game: &GameImpl,
        player: SmallId,
        emoji_behavior: Shared<NationEmojiBehavior>,
    ) -> Self {
        Self {
            random,
            player,
            emoji_behavior,
            tracked_transport_ships: OrderedSet::new(),
            tracked_trade_ships: OrderedSet::new(),
            tracked_incoming_transport_ships: OrderedSet::new(),
            dealt_with_transport_ship: OrderedSet::new(),
        }
    }

    fn next_int(&self, min: f64, max: f64) -> f64 {
        self.random.borrow_mut().next_int(min, max)
    }

    fn cost(&self, game: &GameImpl, t: UnitType) -> i64 {
        game.player(self.player).unit_cost(t, 0)
    }

    /// `spareGold()`: with atom bombs disabled, silo owners keep a hydrogen bomb's price for nukes (v0.36.0 on).
    fn spare_gold(&self, game: &GameImpl) -> i64 {
        let gold = game.player(self.player).gold();
        if !game.has(Change::NationWarshipSpareGold36) {
            return gold;
        }
        let config = game.config();
        if !config.is_unit_disabled(UnitType::AtomBomb)
            || config.is_unit_disabled(UnitType::HydrogenBomb)
            || game.player(self.player).unit_count(UnitType::MissileSilo) == 0
        {
            return gold;
        }
        gold - self.cost(game, UnitType::HydrogenBomb)
    }

    /// `maybeSpawnWarship()`.
    pub fn maybe_spawn_warship(&mut self, game: &mut GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::Warship) {
            return false;
        }
        if !self.random.borrow_mut().chance(50.0) {
            return false;
        }
        let ports = game.player(self.player).units_of(UnitType::Port);
        let ships = game.player(self.player).units_of(UnitType::Warship);
        if !ports.is_empty()
            && ships.is_empty()
            && self.spare_gold(game) > self.cost(game, UnitType::Warship)
        {
            let port = *self.random.borrow_mut().rand_element(&ports);
            let port_tile = game.unit_any(port).tile();
            let Some(target_tile) = self.warship_spawn_tile(game, port_tile, 250) else {
                return false;
            };
            if game
                .player(self.player)
                .can_build(UnitType::Warship, target_tile, None)
                .is_none()
            {
                return false;
            }
            game.add_execution(Box::new(ConstructionExecution::new(
                self.player,
                UnitType::Warship,
                target_tile,
                None,
                None,
            )));
            return true;
        }
        false
    }

    fn warship_spawn_tile(
        &self,
        game: &GameImpl,
        port_tile: TileRef,
        radius: i32,
    ) -> Option<TileRef> {
        for _ in 0..50 {
            let px = game.x(port_tile) as i32;
            let py = game.y(port_tile) as i32;
            let rand_x = self.next_int((px - radius) as f64, (px + radius) as f64) as i32;
            let rand_y = self.next_int((py - radius) as f64, (py + radius) as f64) as i32;
            if !game.is_valid_coord(rand_x, rand_y) {
                continue;
            }
            let tile = game.tile_ref(rand_x, rand_y);
            // Sanity check
            let ok = if game.has(Change::NationWarshipRework31) {
                game.is_water(tile)
            } else {
                game.map.is_ocean(tile)
            };
            if !ok {
                continue;
            }
            return Some(tile);
        }
        None
    }

    /// `trackShipsAndRetaliate()`.
    pub fn track_ships_and_retaliate(&mut self, game: &mut GameImpl) {
        self.track_transport_ships_and_retaliate(game);
        self.track_trade_ships_and_retaliate(game);
        if game.has(Change::NationWarshipRework31) {
            self.track_incoming_transports_and_retaliate(game);
        }
    }

    // Send out a warship if our transport ship got captured.
    fn track_transport_ships_and_retaliate(&mut self, game: &mut GameImpl) {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return;
        }
        for u in game.player(self.player).units_of(UnitType::TransportShip) {
            self.tracked_transport_ships.insert(u);
        }
        let tracked: Vec<UnitId> = self.tracked_transport_ships.iter().copied().collect();
        for ship in tracked {
            let unit = game.unit_any(ship);
            if !unit.is_active() {
                // Distinguish arrival/retreat from destruction by an enemy.
                if unit.was_destroyed_by_enemy()
                    && let Some(destroyer) = unit.destroyer()
                {
                    let tile = unit.tile();
                    self.maybe_retaliate_with_warship(game, tile, destroyer, Reason::Transport);
                }
                self.tracked_transport_ships.remove(&ship);
            }
        }
    }

    // Send out a warship if our trade ship got captured.
    fn track_trade_ships_and_retaliate(&mut self, game: &mut GameImpl) {
        for u in game.player(self.player).units_of(UnitType::TradeShip) {
            self.tracked_trade_ships.insert(u);
        }
        let tracked: Vec<UnitId> = self.tracked_trade_ships.iter().copied().collect();
        for ship in tracked {
            let unit = game.unit_any(ship);
            if !unit.is_active() {
                self.tracked_trade_ships.remove(&ship);
                continue;
            }
            if unit.owner_id() != self.player {
                // Ship was ours and is now owned by someone else: captured.
                let (tile, owner) = (unit.tile(), unit.owner_id());
                self.maybe_retaliate_with_warship(game, tile, owner, Reason::Trade);
                self.tracked_trade_ships.remove(&ship);
            }
        }
    }

    fn track_incoming_transports_and_retaliate(&mut self, game: &mut GameImpl) {
        for p in game.units_of(UnitType::TransportShip) {
            let unit = game.unit_any(p);
            // `target &&` is falsy for tile 0 upstream.
            if let Some(target) = unit.target_tile()
                && target != 0
                && unit.is_active()
                && !unit.transport_ship_state().is_retreating
                && game.owner_id(target) == self.player
                && unit.owner_id() != self.player
            {
                self.tracked_incoming_transport_ships.insert(p);
            }
        }

        let tracked: Vec<UnitId> = self
            .tracked_incoming_transport_ships
            .iter()
            .copied()
            .collect();
        for transport in tracked {
            let unit = game.unit_any(transport);
            let target = unit.target_tile();
            if !unit.is_active() || target.is_none() || unit.transport_ship_state().is_retreating {
                self.tracked_incoming_transport_ships.remove(&transport);
                self.dealt_with_transport_ship.remove(&transport);
                continue;
            }
            let target = target.expect("checked above");
            // Transport has already been dealt with.
            if self.dealt_with_transport_ship.contains(&transport) {
                continue;
            }

            let distance_to_target = game.manhattan_dist(unit.tile(), target);
            // Too close to deal with.
            if distance_to_target < 20 {
                self.dealt_with_transport_ship.insert(transport);
                continue;
            }

            let owner = unit.owner_id();
            if !game.player(owner).is_allied_with(self.player) {
                let near_warship =
                    game.has_unit_nearby(target, 90.0, UnitType::Warship, Some(self.player), true)
                        || game
                            .player(self.player)
                            .units_of(UnitType::Warship)
                            .into_iter()
                            .any(|w| {
                                game.unit_any(w)
                                    .warship_state()
                                    .patrol_tile
                                    .is_some_and(|patrol| game.manhattan_dist(target, patrol) < 90)
                            });
                if near_warship {
                    self.dealt_with_transport_ship.insert(transport);
                    continue;
                }
                let Some(ocean_tile) = self.warship_spawn_tile(game, target, 30) else {
                    continue;
                };
                let reason = if game.has(Change::NationWarshipSpareGold36) {
                    Reason::Invasion
                } else {
                    Reason::Transport
                };
                self.maybe_retaliate_with_warship(game, ocean_tile, owner, reason);
                self.dealt_with_transport_ship.insert(transport);
                break;
            }
        }
    }

    fn maybe_retaliate_with_warship(
        &mut self,
        game: &mut GameImpl,
        tile: TileRef,
        enemy: SmallId,
        reason: Reason,
    ) {
        // Do not retaliate against ourselves (e.g. an own nuke destroyed an own ship).
        if enemy == self.player && game.has(Change::NationBehaviorUpdate3246) {
            return;
        }
        // One of ours already patrols there (sent by any trigger): do not pile up more.
        if game.has(Change::NationsLikeHumans5899) && !self.warships_covering(game, tile).is_empty()
        {
            return;
        }
        // Do not send too many warships.
        if game.player(self.player).units_of(UnitType::Warship).len() >= 10 {
            self.maybe_move_warship(game, tile);
            return;
        }
        let difficulty = game.config().game_config().difficulty;
        // Never on Easy; Medium 15%, Hard 50%, Impossible 80%.
        let retaliate = (difficulty == Difficulty::Medium && self.next_int(0.0, 100.0) < 15.0)
            || (difficulty == Difficulty::Hard && self.next_int(0.0, 100.0) < 50.0)
            || (difficulty == Difficulty::Impossible && self.next_int(0.0, 100.0) < 80.0);
        if retaliate {
            // Stopping an invasion may dip into the gold kept for nukes.
            let affordable = !game.has(Change::NationWarshipSpareGold36)
                || reason == Reason::Invasion
                || self.spare_gold(game) >= self.cost(game, UnitType::Warship);
            if game
                .player(self.player)
                .can_build(UnitType::Warship, tile, None)
                .is_none()
                || !affordable
            {
                self.maybe_move_warship(game, tile);
                return;
            }
            game.add_execution(Box::new(ConstructionExecution::new(
                self.player,
                UnitType::Warship,
                tile,
                None,
                None,
            )));
            self.emoji_behavior.borrow_mut().maybe_send_emoji(
                game,
                EmojiRecipient::Player(enemy),
                &emoji_warship_retaliation(),
            );
            let delta = if reason == Reason::Trade { -7.5 } else { -15.0 };
            if game.has(Change::NationBehaviorUpdate3246) {
                game.player_mut(self.player).update_relation(enemy, delta);
            }
        }
    }

    /// `clearSeaLane(blocker)`: sends a squad at the fleet blocking our boats, all at once or not at all.
    pub fn clear_sea_lane(&mut self, game: &mut GameImpl, blocker: UnitId) {
        if game.config().is_unit_disabled(UnitType::Warship) {
            return;
        }
        let target = game.unit_any(blocker).tile();
        let fleet = self.hostile_warships_near(game, target);
        let wanted = match game.config().game_config().difficulty {
            Difficulty::Easy | Difficulty::Medium => return,
            Difficulty::Hard => fleet,
            Difficulty::Impossible => fleet + 1,
        } as i64;

        let covering = self.warships_covering(game, target);
        let component = game.water_manager.get_water_component(&game.map, target);
        // Ships with enemies around their patrol tile are busy; pulling them away would thrash.
        let mut spare: Vec<UnitId> = game
            .player(self.player)
            .units_of(UnitType::Warship)
            .into_iter()
            .filter(|&w| {
                let unit = game.unit_any(w);
                let state = unit.warship_state();
                !covering.contains(&w)
                    && state.state == WarshipMode::Patrolling
                    && game
                        .water_manager
                        .get_water_component(&game.map, unit.tile())
                        == component
                    && state
                        .patrol_tile
                        .is_none_or(|p| self.hostile_warships_near(game, p) == 0)
            })
            .collect();
        spare.sort_by_key(|&w| game.manhattan_dist(game.unit_any(w).tile(), target));
        let missing = i64::max(0, wanted - covering.len() as i64) as usize;
        let recruits: Vec<UnitId> = spare.into_iter().take(missing).collect();
        let builds = wanted - covering.len() as i64 - recruits.len() as i64;
        // Ships sent in one by one get sunk one by one.
        if builds > 0 {
            if game.player(self.player).units_of(UnitType::Warship).len() as i64 + builds > 10 {
                return;
            }
            // Each ship we own raises the price of the next.
            let mut total = 0i64;
            for i in 0..builds {
                total += game.player(self.player).unit_cost(UnitType::Warship, i);
            }
            if self.spare_gold(game) < total {
                return;
            }
            if game
                .player(self.player)
                .can_build(UnitType::Warship, target, None)
                .is_none()
            {
                return;
            }
        }
        for w in recruits {
            game.unit_mut(w)
                .update_warship_state(false, None, Some(Some(target)), None);
        }
        for _ in 0..builds {
            game.add_execution(Box::new(ConstructionExecution::new(
                self.player,
                UnitType::Warship,
                target,
                None,
                None,
            )));
        }
    }

    fn hostile_warships_near(&self, game: &GameImpl, tile: TileRef) -> usize {
        game.nearby_units(
            tile,
            game.config().warship_targetting_range(),
            &[UnitType::Warship],
            None,
            false,
        )
        .into_iter()
        .filter(|&(unit, _)| {
            game.player(game.unit_any(unit).owner_id())
                .can_attack_player(self.player, true)
        })
        .count()
    }

    /// Our warships on the same water whose patrol area includes `tile`.
    fn warships_covering(&self, game: &GameImpl, tile: TileRef) -> Vec<UnitId> {
        let component = game.water_manager.get_water_component(&game.map, tile);
        let patrol_range = game.config().warship_patrol_range();
        game.player(self.player)
            .units_of(UnitType::Warship)
            .into_iter()
            .filter(|&w| {
                let unit = game.unit_any(w);
                unit.warship_state().patrol_tile.is_some_and(|patrol| {
                    game.water_manager
                        .get_water_component(&game.map, unit.tile())
                        == component
                        && game.manhattan_dist(patrol, tile) as f64 <= patrol_range
                })
            })
            .collect()
    }

    fn maybe_move_warship(&mut self, game: &mut GameImpl, tile: TileRef) {
        if !game.has(Change::NationWarshipRework31) {
            return;
        }
        // Make sure we are targeting water.
        if !game.is_water(tile) {
            return;
        }
        // Closest first; ships already travelling (patrol tile far away) are skipped.
        let mut best: Option<(UnitId, u32)> = None;
        for p in game.player(self.player).units_of(UnitType::Warship) {
            let unit = game.unit_any(p);
            let eligible = unit
                .warship_state()
                .patrol_tile
                .is_some_and(|patrol| game.manhattan_dist(unit.tile(), patrol) < 130);
            if !eligible {
                continue;
            }
            let dist = game.manhattan_dist(unit.tile(), tile);
            if best.is_none_or(|(_, d)| dist < d) {
                best = Some((p, dist));
            }
        }
        if let Some((warship, _)) = best {
            game.unit_mut(warship)
                .update_warship_state(false, None, Some(Some(tile)), None);
        }
    }

    /// `counterWarshipInfestation()`.
    pub fn counter_warship_infestation(&mut self, game: &mut GameImpl) {
        if !self.should_counter_warship_infestation(game) {
            return;
        }
        let is_team_game = game.player(self.player).team().is_some();
        if !self.is_rich_player(game, is_team_game) {
            return;
        }
        let target = if is_team_game {
            self.find_team_game_warship_target(game)
        } else {
            self.find_free_for_all_warship_target(game)
        };
        if let Some(warship) = target {
            self.build_counter_warship(game, warship);
        }
    }

    fn should_counter_warship_infestation(&self, game: &GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::Warship) {
            return false;
        }
        // Only the smart nations do this.
        let difficulty = game.config().game_config().difficulty;
        if difficulty != Difficulty::Hard && difficulty != Difficulty::Impossible {
            return false;
        }
        if game.unit_count(UnitType::Warship) <= 10 {
            return false;
        }
        if self.cost(game, UnitType::Warship) > self.spare_gold(game) {
            return false;
        }
        if game.player(self.player).units_of(UnitType::Port).is_empty() {
            return false;
        }
        if game.player(self.player).units_of(UnitType::Warship).len() >= 10 {
            return false;
        }
        true
    }

    // One of the 3 richest non-human players (of our team in team games).
    fn is_rich_player(&self, game: &GameImpl, is_team_game: bool) -> bool {
        let my_team = game.player(self.player).team().cloned();
        let mut players: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                if game.player(p).player_type() == PlayerType::Human {
                    return false;
                }
                !is_team_game || game.player(p).team().cloned() == my_team
            })
            .collect();
        players.sort_by_key(|&p| std::cmp::Reverse(game.player(p).gold()));
        players.iter().take(3).any(|&p| p == self.player)
    }

    fn find_team_game_warship_target(&self, game: &GameImpl) -> Option<UnitId> {
        // (team, warship count, players) in first-seen order.
        let mut enemy_teams: Vec<(String, usize, Vec<SmallId>)> = Vec::new();
        let mut own_fleet = 0usize;
        let count_own = game.has(Change::NationWarshipSpareGold36);
        for p in game.players() {
            // Skip friendly players (our team and allies), but count their warships (v0.36.0 on).
            if game.player(self.player).is_friendly(p, false) || p == self.player {
                if count_own {
                    own_fleet += game.player(p).units_of(UnitType::Warship).len();
                }
                continue;
            }
            let Some(team) = game.player(p).team().cloned() else {
                continue;
            };
            let warship_count = game.player(p).units_of(UnitType::Warship).len();
            let idx = match enemy_teams.iter().position(|(t, _, _)| *t == team) {
                Some(i) => i,
                None => {
                    enemy_teams.push((team, 0, Vec::new()));
                    enemy_teams.len() - 1
                }
            };
            enemy_teams[idx].1 += warship_count;
            enemy_teams[idx].2.push(p);
        }

        for (_, count, players) in &enemy_teams {
            if *count > 15 && (!count_own || *count > own_fleet) {
                // The player of that team with the most warships.
                let mut most: Option<SmallId> = None;
                for &p in players {
                    let c = game.player(p).units_of(UnitType::Warship).len();
                    let max_count =
                        most.map_or(0, |m| game.player(m).units_of(UnitType::Warship).len());
                    if c > max_count {
                        most = Some(p);
                    }
                }
                if let Some(most) = most {
                    let warships = game.player(most).units_of(UnitType::Warship);
                    if warships.len() > 3 {
                        return Some(*self.random.borrow_mut().rand_element(&warships));
                    }
                }
            }
        }
        None
    }

    fn find_free_for_all_warship_target(&self, game: &GameImpl) -> Option<UnitId> {
        let enemies: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| !game.player(self.player).is_friendly(p, false) && p != self.player)
            .collect();
        for enemy in enemies {
            let enemy_warships = game.player(enemy).units_of(UnitType::Warship);
            if enemy_warships.len() > 10 {
                return Some(*self.random.borrow_mut().rand_element(&enemy_warships));
            }
        }
        None
    }

    fn build_counter_warship(&mut self, game: &mut GameImpl, warship: UnitId) {
        let tile = game.unit_any(warship).tile();
        if game
            .player(self.player)
            .can_build(UnitType::Warship, tile, None)
            .is_none()
        {
            self.maybe_move_warship(game, tile);
            return;
        }
        game.add_execution(Box::new(ConstructionExecution::new(
            self.player,
            UnitType::Warship,
            tile,
            None,
            None,
        )));
        self.emoji_behavior.borrow_mut().send_emoji(
            game,
            EmojiRecipient::All,
            &emoji_warship_retaliation(),
        );
    }
}

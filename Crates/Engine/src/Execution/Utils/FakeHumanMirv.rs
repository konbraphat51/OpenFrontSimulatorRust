// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/FakeHumanExecution.ts (MIRV part): the retaliation and victory-denial helpers of the fake human in v0.27 and v0.28.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Compat::OrderedSet::OrderedSet;
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::MIRVExecution::MirvExecution;
use crate::Execution::Utils::FakeHumanShared::FakeHumanShared;
use crate::Game::Game::{Difficulty, PlayerType, SmallId, Tick, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Odds of aborting a MIRV attempt.
pub const MIRV_HESITATION_ODDS: f64 = 7.0;
/// Ticks until a MIRV can be attempted again.
const MIRV_COOLDOWN_TICKS: u32 = 20;
/// Threshold for team victory denial.
const VICTORY_DENIAL_TEAM_THRESHOLD: f64 = 0.8;
/// Threshold for individual victory denial.
const VICTORY_DENIAL_INDIVIDUAL_THRESHOLD: f64 = 0.65;
/// Multiplier for the steamroll city gap threshold.
const STEAMROLL_CITY_GAP_MULTIPLIER: f64 = 1.3;
/// Minimum city count for the leader to trigger steamroll detection.
const STEAMROLL_MIN_LEADER_CITIES: i64 = 10;
/// How long the valid MIRV target list is kept, in ticks.
const MIRV_TARGETS_CACHE_TICKS: u32 = 2 * 10;

/// MIRV bookkeeping and the helpers that read it.
pub trait FakeHumanMirv: FakeHumanShared {
    /// Recent MIRV targets with their tick.
    fn last_mirv_sent_mut(&mut self) -> &mut Vec<(Tick, TileRef)>;
    /// The valid MIRV target players and the tick they were computed at.
    fn mirv_targets_cache_mut(&mut self) -> &mut Option<(Tick, Vec<SmallId>)>;
    /// Transport ships of other players already answered.
    fn tracked_transport_ships_mut(&mut self) -> &mut OrderedSet<UnitId>;
    /// Trade ships of other players already answered.
    fn tracked_trade_ships_mut(&mut self) -> &mut OrderedSet<UnitId>;

    fn get_valid_mirv_target_players(&mut self, game: &GameImpl) -> Vec<SmallId> {
        let player = self.me();
        if let Some((tick, players)) = &self.mirv_targets_cache_mut()
            && game.ticks() - tick < MIRV_TARGETS_CACHE_TICKS
        {
            return players.clone();
        }
        let players: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                p != player
                    && game.player(p).player_type() != PlayerType::Bot
                    && !game.player(player).is_on_same_team(p)
            })
            .collect();
        *self.mirv_targets_cache_mut() = Some((game.ticks(), players.clone()));
        players
    }

    fn is_inbound_mirv_from(&self, game: &GameImpl, attacker: SmallId) -> bool {
        for mirv in game.player(attacker).units_of(UnitType::MIRV) {
            let Some(dst) = game.unit_any(mirv).target_tile() else {
                continue;
            };
            // A tile 0 would be falsy
            if dst == 0 {
                continue;
            }
            if !game.has_owner(dst) {
                continue;
            }
            if game.owner_id(dst) == self.me() {
                return true;
            }
        }
        false
    }

    fn maybe_retaliate_with_warship(&mut self, game: &mut GameImpl, tile: TileRef) {
        let Some(player) = self.player() else { return };
        let difficulty = game.config().game_config().difficulty;
        // Never on Easy; Medium retaliates 15 percent of the time, Hard 50, Impossible 80
        let retaliate = (difficulty == Difficulty::Medium && self.next_int(0.0, 100.0) < 15.0)
            || (difficulty == Difficulty::Hard && self.next_int(0.0, 100.0) < 50.0)
            || (difficulty == Difficulty::Impossible && self.next_int(0.0, 100.0) < 80.0);
        if retaliate {
            if game
                .player(player)
                .can_build(UnitType::Warship, tile, None)
                .is_none()
            {
                return;
            }
            game.add_execution(Box::new(ConstructionExecution::new(
                player,
                UnitType::Warship,
                tile,
                None,
                None,
            )));
        }
    }

    fn remove_old_mirv_events(&mut self, game: &GameImpl) {
        let tick = game.ticks();
        while let Some(&(sent, _)) = self.last_mirv_sent_mut().first() {
            if sent + MIRV_COOLDOWN_TICKS <= tick {
                self.last_mirv_sent_mut().remove(0);
            } else {
                break;
            }
        }
    }

    fn select_counter_mirv_target(&mut self, game: &GameImpl) -> Option<SmallId> {
        let mut attackers: Vec<SmallId> = self
            .get_valid_mirv_target_players(game)
            .into_iter()
            .filter(|&p| self.is_inbound_mirv_from(game, p))
            .collect();
        if attackers.is_empty() {
            return None;
        }
        attackers.sort_by(|&a, &b| {
            game.player(b)
                .num_tiles_owned()
                .cmp(&game.player(a).num_tiles_owned())
        });
        Some(attackers[0])
    }

    fn select_steamroll_stop_target(&mut self, game: &GameImpl) -> Option<SmallId> {
        let valid_targets = self.get_valid_mirv_target_players(game);
        if valid_targets.is_empty() {
            return None;
        }
        let mut all_players: Vec<(SmallId, i64)> = game
            .players()
            .into_iter()
            .map(|p| (p, game.player(p).unit_count(UnitType::City)))
            .collect();
        all_players.sort_by_key(|b| std::cmp::Reverse(b.1));
        if all_players.len() < 2 {
            return None;
        }
        let top = all_players[0];
        if top.1 <= STEAMROLL_MIN_LEADER_CITIES {
            return None;
        }
        let second_highest = all_players[1].1;
        let threshold = second_highest as f64 * STEAMROLL_CITY_GAP_MULTIPLIER;
        if top.1 as f64 >= threshold {
            return valid_targets.contains(&top.0).then_some(top.0);
        }
        None
    }

    fn select_victory_denial_target(&mut self, game: &GameImpl) -> Option<SmallId> {
        let total_land = game.num_land_tiles();
        if total_land == 0.0 {
            return None;
        }
        let mut best: Option<(SmallId, f64)> = None;
        for p in self.get_valid_mirv_target_players(game) {
            let mut severity = 0.0;
            let team = game.player(p).team().cloned();
            if let Some(team) = team {
                let team_members: Vec<SmallId> = game
                    .players()
                    .into_iter()
                    .filter(|&x| game.player(x).team() == Some(&team))
                    .collect();
                let team_territory: f64 = team_members
                    .iter()
                    .map(|&x| game.player(x).num_tiles_owned() as f64)
                    .fold(0.0, |a, b| a + b);
                let team_share = team_territory / total_land;
                if team_share >= VICTORY_DENIAL_TEAM_THRESHOLD {
                    // Only the largest team member is a target once the team exceeds the threshold
                    let mut largest_member: Option<SmallId> = None;
                    let mut largest_tiles: i64 = -1;
                    for &member in &team_members {
                        let tiles = game.player(member).num_tiles_owned() as i64;
                        if tiles > largest_tiles {
                            largest_tiles = tiles;
                            largest_member = Some(member);
                        }
                    }
                    severity = if largest_member == Some(p) {
                        team_share
                    } else {
                        // Skip non-largest members
                        0.0
                    };
                }
            } else {
                let share = game.player(p).num_tiles_owned() as f64 / total_land;
                if share >= VICTORY_DENIAL_INDIVIDUAL_THRESHOLD {
                    severity = share;
                }
            }
            if severity > 0.0 && best.is_none_or(|(_, s)| severity > s) {
                best = Some((p, severity));
            }
        }
        best.map(|(p, _)| p)
    }

    fn send_mirv(&mut self, game: &mut GameImpl, tile: TileRef) {
        self.trigger_mirv_cooldown(game, Some(tile));
        game.add_execution(Box::new(MirvExecution::new(self.me(), tile)));
    }

    /// Sends out a warship if our trade ship got captured.
    fn track_trade_ships_and_retaliate(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player() else { return };
        for u in game.player(player).units_of(UnitType::TradeShip) {
            self.tracked_trade_ships_mut().insert(u);
        }
        let tracked: Vec<UnitId> = self.tracked_trade_ships_mut().iter().copied().collect();
        for ship in tracked {
            if !game.unit_any(ship).is_active() {
                self.tracked_trade_ships_mut().remove(&ship);
                continue;
            }
            if game.unit_any(ship).owner_id() != player {
                // The ship was ours and is now owned by someone else: captured
                let tile = game.unit_any(ship).tile();
                self.maybe_retaliate_with_warship(game, tile);
                self.tracked_trade_ships_mut().remove(&ship);
            }
        }
    }

    /// Sends out a warship if our transport ship got destroyed by an enemy.
    fn track_transport_ships_and_retaliate(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player() else { return };
        for u in game.player(player).units_of(UnitType::TransportShip) {
            self.tracked_transport_ships_mut().insert(u);
        }
        let tracked: Vec<UnitId> = self.tracked_transport_ships_mut().iter().copied().collect();
        for ship in tracked {
            if !game.unit_any(ship).is_active() {
                // Arrival and retreat differ from destruction by an enemy
                if game.unit_any(ship).was_destroyed_by_enemy() {
                    let tile = game.unit_any(ship).tile();
                    self.maybe_retaliate_with_warship(game, tile);
                }
                self.tracked_transport_ships_mut().remove(&ship);
            }
        }
    }

    fn trigger_mirv_cooldown(&mut self, game: &GameImpl, tile: Option<TileRef>) {
        self.remove_old_mirv_events(game);
        let tick = game.ticks();
        // Any tile of the territory serves for cooldown tracking when none is given
        let cooldown_tile = tile.unwrap_or_else(|| {
            game.player(self.me())
                .tiles()
                .iter()
                .next()
                .unwrap_or_else(|| game.tile_ref(0, 0))
        });
        self.last_mirv_sent_mut().push((tick, cooldown_tile));
    }
}

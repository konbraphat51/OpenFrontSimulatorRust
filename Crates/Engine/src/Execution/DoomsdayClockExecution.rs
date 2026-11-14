// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/DoomsdayClockExecution.ts: anti-stall clock that doomes sides below a rising territory bar.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Execution::Execution::Execution;
use crate::Game::DoomsdayClock::{
    DoomsdayClockDrainConfig, DoomsdayClockFloorConfig, DoomsdayClockProfile, ROT_NOISE_SCALE,
    doomsday_clock_drain, doomsday_clock_required_tiles, doomsday_clock_rot_quota,
    doomsday_clock_troop_floor, rot_front_noise, rot_speckle_noise,
};
use crate::Game::Game::{GameMode, PlayerType, SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::collections::HashMap;

/// Keeps the `n` lowest-keyed tiles seen, in key order.
struct LowestN {
    n: usize,
    tiles: Vec<TileRef>,
    keys: Vec<f64>,
}

impl LowestN {
    fn new(n: usize) -> Self {
        Self {
            n,
            tiles: Vec::new(),
            keys: Vec::new(),
        }
    }

    fn size(&self) -> usize {
        self.tiles.len()
    }

    fn offer(&mut self, tile: TileRef, key: f64) {
        if self.tiles.len() == self.n && key >= self.keys[self.tiles.len() - 1] {
            return;
        }
        let mut i = self.keys.len();
        while i > 0 && self.keys[i - 1] > key {
            i -= 1;
        }
        self.tiles.insert(i, tile);
        self.keys.insert(i, key);
        if self.tiles.len() > self.n {
            self.tiles.pop();
            self.keys.pop();
        }
    }
}

/// Rot progress of one doomed player.
struct RotState {
    since: Tick,
    held: f64,
    front: OrderedMap<TileRef, f64>,
}

/// Doomsday Clock: runs once per second and dooms sides below the rising bar.
pub struct DoomsdayClockExecution {
    active: bool,
    rot_state: HashMap<SmallId, RotState>,
}

impl Default for DoomsdayClockExecution {
    fn default() -> Self {
        Self::new()
    }
}

impl DoomsdayClockExecution {
    /// Creates the execution.
    pub fn new() -> Self {
        Self {
            active: true,
            rot_state: HashMap::new(),
        }
    }

    /// Group contenders into sides: singletons in FFA, by team otherwise.
    fn sides(game: &GameImpl, contenders: &[SmallId], ffa: bool) -> Vec<Vec<SmallId>> {
        if ffa {
            return contenders.iter().map(|&p| vec![p]).collect();
        }
        let mut by_team: Vec<(String, Vec<SmallId>)> = Vec::new();
        for &p in contenders {
            let Some(team) = game.player(p).team().cloned() else {
                continue;
            };
            match by_team.iter_mut().find(|(t, _)| *t == team) {
                Some((_, members)) => members.push(p),
                None => by_team.push((team, vec![p])),
            }
        }
        by_team.into_iter().map(|(_, m)| m).collect()
    }

    /// Takes territory so the player holds none by `rotDeathSeconds` after the skull appeared.
    fn rot(
        &mut self,
        game: &mut GameImpl,
        player: SmallId,
        seconds_under: f64,
        rot_death_seconds: f64,
        rot_grain_seconds: f64,
        rot_speckle_percent: f64,
    ) {
        let owned = game.player(player).num_tiles_owned() as f64;
        if owned <= 0.0 {
            return;
        }
        let state = self.rot_state.remove(&player).unwrap_or(RotState {
            since: game.ticks(),
            held: owned,
            front: OrderedMap::new(),
        });

        let even_quota = doomsday_clock_rot_quota(owned, seconds_under, rot_death_seconds);

        let grainy = (game.ticks() as f64) - (state.since as f64) < rot_grain_seconds * 10.0;
        let specks = if grainy {
            f64::max(
                1.0,
                ((state.held * rot_speckle_percent) / 100.0 / rot_grain_seconds).ceil(),
            )
        } else {
            0.0
        };
        let mut budget = f64::min(owned, f64::max(even_quota, specks));

        let mut front: OrderedMap<TileRef, f64> = OrderedMap::new();
        for (&tile, &rotted) in state.front.iter() {
            if game.player(player).tiles().has(tile) {
                front.insert(tile, rotted);
            }
        }

        budget -= Self::speckle(game, player, f64::min(specks, budget), &mut front);
        budget -= Self::spread(game, player, budget, &mut front);
        if budget > 0.0 {
            Self::speckle(game, player, budget, &mut front);
        }

        self.rot_state.insert(
            player,
            RotState {
                since: state.since,
                held: state.held,
                front,
            },
        );
    }

    /// Opens `count` holes, preferring the interior.
    fn speckle(
        game: &mut GameImpl,
        player: SmallId,
        count: f64,
        front: &mut OrderedMap<TileRef, f64>,
    ) -> f64 {
        if count <= 0.0 {
            return 0.0;
        }
        let n = count as usize;
        let salt = player as u32;
        let mut interior = LowestN::new(n);
        let mut edge = LowestN::new(n);
        {
            let p = game.player(player);
            let border = p.border_tiles();
            p.tiles().for_each(|tile| {
                let key = rot_speckle_noise(game.x(tile), game.y(tile), salt);
                if border.has(tile) {
                    edge.offer(tile, key);
                } else {
                    interior.offer(tile, key);
                }
            });
        }
        let picked: Vec<TileRef> = if interior.size() >= n {
            interior.tiles
        } else {
            interior.tiles.into_iter().chain(edge.tiles).collect()
        };
        let mut opened = 0.0;
        for tile in picked {
            if opened == count {
                break;
            }
            if Self::consume(game, player, tile, front) {
                opened += 1.0;
            }
        }
        opened
    }

    /// Grows the holes outward by up to `count` tiles, favouring tips.
    fn spread(
        game: &mut GameImpl,
        player: SmallId,
        count: f64,
        front: &mut OrderedMap<TileRef, f64>,
    ) -> f64 {
        if count <= 0.0 || front.is_empty() {
            return 0.0;
        }
        let salt = player as u32;
        let mut pick = LowestN::new(count as usize);
        for (&tile, &rotted) in front.iter() {
            let key = rotted * ROT_NOISE_SCALE + rot_front_noise(tile, salt);
            pick.offer(tile, key);
        }
        let mut taken = 0.0;
        for tile in pick.tiles {
            if taken == count {
                break;
            }
            if Self::consume(game, player, tile, front) {
                taken += 1.0;
            }
        }
        taken
    }

    /// Rots one tile: hands the land to nobody and queues its neighbours.
    fn consume(
        game: &mut GameImpl,
        player: SmallId,
        tile: TileRef,
        front: &mut OrderedMap<TileRef, f64>,
    ) -> bool {
        if !game.player(player).tiles().has(tile) {
            return false;
        }
        game.player_mut(player).relinquish(tile);
        if game.has(Change::DoomsdayRotWasteland5052) {
            game.set_fallout(tile, true);
        }
        game.player_mut(player).mark_rotted();
        front.remove(&tile);
        for neighbour in game.map.neighbors(tile) {
            if game.player(player).tiles().has(neighbour) {
                let prev = front.get(&neighbour).copied().unwrap_or(0.0);
                front.insert(neighbour, prev + 1.0);
            }
        }
        true
    }
}

impl Execution for DoomsdayClockExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        if !ticks.is_multiple_of(10) {
            return;
        }
        let cfg = game.config().doomsday_clock_config();
        if !cfg.enabled {
            return;
        }
        let rot_era = game.has(Change::DoomsdayRotLadder4893);
        assert!(
            !matches!(game.config().rules().class_id(), "C025" | "C026" | "C027"),
            "not ported: Doomsday Clock of class {}",
            game.config().rules().class_id()
        );
        let drain_cfg = DoomsdayClockDrainConfig {
            drain_start_percent: cfg.drain_start_percent,
            drain_max_percent: cfg.drain_max_percent,
            drain_ramp_seconds: cfg.drain_ramp_seconds,
        };
        let warship_drain_cfg = DoomsdayClockDrainConfig {
            drain_start_percent: cfg.warship_drain_start_percent,
            drain_max_percent: cfg.warship_drain_max_percent,
            drain_ramp_seconds: cfg.drain_ramp_seconds,
        };
        let floor_cfg = DoomsdayClockFloorConfig {
            drain_floor_percent: cfg.drain_floor_percent,
            floor_start_percent: cfg.floor_start_percent,
            floor_decay_seconds: cfg.floor_decay_seconds,
        };

        let elapsed = game.elapsed_game_seconds();
        let contenders: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| game.player(p).player_type() != PlayerType::Bot)
            .collect();

        let ffa = game.config().game_config().game_mode == GameMode::FFA;
        let sides = Self::sides(game, &contenders, ffa);

        let winner_idle = game.has(Change::DoomsdayIdleOnWinner4953);
        if (winner_idle && game.winner().is_some()) || sides.len() < 2 {
            for &p in &contenders {
                game.player_mut(p).clear_doomsday_clock();
            }
            if winner_idle {
                self.active = false;
            }
            return;
        }

        let land = game.num_land_tiles() - game.num_tiles_with_fallout();
        let required = doomsday_clock_required_tiles(
            &DoomsdayClockProfile {
                six_waves: !rot_era,
                speed: cfg.speed,
                team_game: !ffa && game.has(Change::DoomsdayTeamLevels5030),
            },
            land,
            elapsed,
        );

        let side_tiles: Vec<f64> = sides
            .iter()
            .map(|members| {
                members
                    .iter()
                    .fold(0.0, |sum, &m| sum + game.player(m).num_tiles_owned() as f64)
            })
            .collect();
        let mut leader_idx = 0;
        for i in 1..side_tiles.len() {
            if side_tiles[i] > side_tiles[leader_idx] {
                leader_idx = i;
            }
        }

        for (i, members) in sides.iter().enumerate() {
            if i != leader_idx && side_tiles[i] < required {
                for &m in members {
                    game.player_mut(m).enter_doomsday_clock();
                    let seconds_under = (game.player(m).doomsday_clock_ticks() / 10.0).floor();
                    if seconds_under >= cfg.warn_seconds {
                        let seconds_past_warn = seconds_under - cfg.warn_seconds;
                        let max_troops = game.player(m).max_troops();
                        let troop_floor = if rot_era {
                            doomsday_clock_troop_floor(max_troops, seconds_past_warn, &floor_cfg)
                        } else {
                            ((max_troops * cfg.drain_floor_percent) / 100.0).floor()
                        };
                        let chunk =
                            doomsday_clock_drain(max_troops, seconds_past_warn, &drain_cfg, 1.0);
                        let troops = game.player(m).troops();
                        game.player_mut(m)
                            .remove_troops(f64::min(chunk, f64::max(0.0, troops - troop_floor)));
                        if rot_era
                            && cfg.rot_death_seconds > 0.0
                            && seconds_past_warn >= cfg.floor_decay_seconds
                            && game.player(m).troops() <= troop_floor
                        {
                            self.rot(
                                game,
                                m,
                                seconds_under,
                                cfg.rot_death_seconds,
                                cfg.rot_grain_seconds,
                                cfg.rot_speckle_percent,
                            );
                        }
                        for ws in game.player(m).units_of(UnitType::Warship) {
                            let (max_health, health) = {
                                let u = game.unit_ref(ws);
                                (u.max_health(), u.health())
                            };
                            let ship_floor =
                                ((max_health * cfg.drain_floor_percent) / 100.0).floor();
                            let removable = f64::max(0.0, health - ship_floor);
                            if removable <= 0.0 {
                                continue;
                            }
                            let dmg = doomsday_clock_drain(
                                max_health,
                                seconds_past_warn,
                                &warship_drain_cfg,
                                cfg.warship_drain_curve_exponent,
                            );
                            game.unit_mut(ws)
                                .modify_health(-f64::min(dmg, removable), None);
                        }
                    }
                }
            } else {
                for &m in members {
                    game.player_mut(m).clear_doomsday_clock();
                    self.rot_state.remove(&m);
                }
            }
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}

// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/NukeExecution.ts: flight and detonation of atom bombs, hydrogen bombs and MIRV warheads.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Client plumbing (`displayIncomingUnit`, `displayMessage`, `recordMotionPlan`, `queueNukeImpact`,
//! `touch`) and record-keeping stats are skipped; `recordMotionPlan` replays a separate pathfinder
//! and has no effect on the state.

use crate::Compat::OrderedMap::OrderedMap;
use crate::Core::DetMath;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Execution::Execution;
use crate::Execution::NukeAlliance::list_nuke_break_alliance;
use crate::Game::Game::{SmallId, Tick, TrajectoryTile, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::PathFinderParabola::{ParabolaOptions, ParabolaUniversalPathFinder};
use crate::Pathfinding::Types::PathResult;

/// Flight and detonation of one nuke.
pub struct NukeExecution {
    active: bool,
    nuke_type: UnitType,
    player: SmallId,
    dst: TileRef,
    src: Option<TileRef>,
    speed: f64,
    wait_ticks: f64,
    rocket_direction_up: bool,
    nuke: Option<UnitId>,
    path_finder: ParabolaUniversalPathFinder,
}

impl NukeExecution {
    /// `new NukeExecution(nukeType, player, dst, src, speed = -1, waitTicks = 0, rocketDirectionUp = true)`.
    pub fn new(
        nuke_type: UnitType,
        player: SmallId,
        dst: TileRef,
        src: Option<TileRef>,
        speed: Option<f64>,
        wait_ticks: Option<f64>,
        rocket_direction_up: Option<bool>,
    ) -> Self {
        Self {
            active: true,
            nuke_type,
            player,
            dst,
            src,
            speed: speed.unwrap_or(-1.0),
            wait_ticks: wait_ticks.unwrap_or(0.0),
            rocket_direction_up: rocket_direction_up.unwrap_or(true),
            nuke: None,
            path_finder: ParabolaUniversalPathFinder::default(),
        }
    }

    /// `getNuke()`: the launched unit, once launched.
    pub fn get_nuke(&self) -> Option<UnitId> {
        self.nuke
    }

    /// `owner()`.
    pub fn owner(&self) -> SmallId {
        self.player
    }

    /// `cancel()`: stops the execution and retires a still-flying nuke.
    pub fn cancel(&mut self, game: &mut GameImpl) {
        self.active = false;
        if let Some(nuke) = self.nuke
            && game.unit_any(nuke).is_active()
        {
            game.unit_mut(nuke).delete(Some(false), None);
        }
    }

    fn tiles_to_destroy(&self, game: &GameImpl) -> Vec<TileRef> {
        let magnitude = game.config().nuke_magnitudes(self.nuke_type);
        let mut rand = PseudoRandom::new(game.ticks() as f64);
        let inner2 = magnitude.inner * magnitude.inner;
        let outer2 = magnitude.outer * magnitude.outer;
        let map = &game.map;

        if game.config().water_nukes() {
            const NUM_SAMPLES: usize = 16;
            let mut radii_sq = [0.0f64; NUM_SAMPLES];
            for r in radii_sq.iter_mut() {
                *r = rand.next_float(inner2, outer2);
            }
            let prev = radii_sq;
            for i in 0..NUM_SAMPLES {
                let l = (i + NUM_SAMPLES - 1) % NUM_SAMPLES;
                let r = (i + 1) % NUM_SAMPLES;
                radii_sq[i] = prev[i] * 0.6 + prev[l] * 0.2 + prev[r] * 0.2;
            }

            let cx = map.x(self.dst) as f64;
            let cy = map.y(self.dst) as f64;
            let outer = magnitude.outer;
            let use_det_math = game
                .config()
                .rules()
                .has(crate::Builds::Rules::Change::DetMath5191);

            let mut result = Vec::new();
            let x0 = f64::max(0.0, cx - outer) as i32;
            let y0 = f64::max(0.0, cy - outer) as i32;
            let x1 = f64::min(map.width() as f64 - 1.0, cx + outer) as i32;
            let y1 = f64::min(map.height() as f64 - 1.0, cy + outer) as i32;
            for py in y0..=y1 {
                for px in x0..=x1 {
                    let dx = px as f64 - cx;
                    let dy = py as f64 - cy;
                    let d2 = dx * dx + dy * dy;
                    if d2 > outer2 {
                        continue;
                    }
                    if d2 > inner2 {
                        let a = if use_det_math {
                            DetMath::atan2(dy, dx)
                        } else {
                            crate::Compat::V8Math::atan2(dy, dx)
                        };
                        let angle = a + std::f64::consts::PI;
                        let t = (angle / (2.0 * std::f64::consts::PI)) * NUM_SAMPLES as f64;
                        let i0 = (t.floor() as usize) % NUM_SAMPLES;
                        let i1 = (i0 + 1) % NUM_SAMPLES;
                        let frac = t - t.floor();
                        let threshold = radii_sq[i0] * (1.0 - frac) + radii_sq[i1] * frac;
                        if d2 > threshold {
                            continue;
                        }
                    }
                    let tile = map.tile_ref(px, py);
                    if map.is_impassable(tile) {
                        continue;
                    }
                    result.push(tile);
                }
            }
            result
        } else {
            let dst = self.dst;
            map.bfs(dst, |m, n| {
                let d2 = m.euclidean_dist_squared(dst, n) as f64;
                d2 <= outer2 && (d2 <= inner2 || rand.chance(2.0)) && !m.is_impassable(n)
            })
            .iter()
            .copied()
            .collect()
        }
    }

    fn maybe_break_alliances(&self, game: &mut GameImpl) {
        let nuke_type = game.unit_any(self.nuke.expect("not initialized")).unit_type;
        if nuke_type == UnitType::MIRVWarhead {
            return;
        }
        let magnitude = game.config().nuke_magnitudes(nuke_type);
        let to_break = list_nuke_break_alliance(
            game,
            self.dst,
            magnitude,
            game.config().nuke_alliance_break_threshold(),
        );

        for incoming in game.player(self.player).incoming_alliance_requests() {
            if to_break.contains(&game.alliance_request(incoming).requestor()) {
                game.reject_alliance_request(incoming);
            }
        }

        for attacked in to_break {
            if attacked == 0 {
                continue;
            }
            let pending = game
                .player(attacked)
                .incoming_alliance_requests()
                .into_iter()
                .find(|&ar| game.alliance_request(ar).requestor() == self.player);
            if let Some(request) = pending {
                game.reject_alliance_request(request);
                continue;
            }
            if let Some(alliance) = game.player(self.player).alliance_with(attacked) {
                game.player_mut(self.player).break_alliance(alliance);
            }
            if attacked != self.player {
                game.player_mut(attacked)
                    .update_relation(self.player, -100.0);
            }
        }
    }

    fn is_targetable(
        &self,
        game: &GameImpl,
        target_tile: TileRef,
        nuke_tile: TileRef,
        target_range_squared: f64,
    ) -> bool {
        (game.map.euclidean_dist_squared(nuke_tile, target_tile) as f64) < target_range_squared
            || self.src.is_some_and(|src| {
                (game.map.euclidean_dist_squared(src, nuke_tile) as f64) < target_range_squared
            })
    }

    fn get_trajectory(&self, game: &GameImpl, target: TileRef) -> Vec<TrajectoryTile> {
        let range = game.config().default_nuke_targetable_range();
        let target_range_squared = range * range;
        let src = self.src.expect("src set before trajectory");
        self.path_finder
            .find_path(&game.map, src, target)
            .into_iter()
            .map(|tile| TrajectoryTile {
                tile,
                targetable: self.is_targetable(game, target, tile, target_range_squared),
            })
            .collect()
    }

    fn update_nuke_targetable(&self, game: &mut GameImpl, nuke: UnitId) {
        let Some(target_tile) = game.unit_any(nuke).target_tile() else {
            return;
        };
        let range = game.config().default_nuke_targetable_range();
        let tile = game.unit_any(nuke).tile();
        let targetable = self.is_targetable(game, target_tile, tile, range * range);
        game.unit_mut(nuke).set_targetable(targetable);
    }

    fn detonate(&mut self, game: &mut GameImpl, nuke: UnitId) {
        let magnitude = game.config().nuke_magnitudes(game.unit_any(nuke).unit_type);
        let to_destroy = self.tiles_to_destroy(game);

        let mut tiles_per_player: OrderedMap<SmallId, f64> = OrderedMap::new();
        for &tile in &to_destroy {
            let owner = game.owner_id(tile);
            if owner != 0 {
                game.player_mut(owner).relinquish(tile);
                let prev = tiles_per_player.get(&owner).copied().unwrap_or(0.0);
                tiles_per_player.insert(owner, prev + 1.0);
            }
            if game.is_land(tile) {
                game.queue_water_conversion(tile);
            }
        }

        for (&player, &impacted) in tiles_per_player.iter() {
            let tiles_before_nuke = game.player(player).num_tiles_owned() as f64 + impacted;
            let transport_ships = game.player(player).units_of(UnitType::TransportShip);
            let mut transport_troops: Vec<(UnitId, f64)> = transport_ships
                .iter()
                .map(|&u| (u, game.unit_any(u).troops()))
                .collect();
            let outgoing_attacks: Vec<_> = game.player(player).outgoing_attacks().to_vec();
            let max_troops = game.player(player).max_troops();
            let mut i = 0.0;
            while i < impacted {
                let num_tiles_left = tiles_before_nuke - i;
                let troops = game.player(player).troops();
                let factor = game.config().nuke_death_factor(
                    self.nuke_type,
                    troops,
                    num_tiles_left,
                    max_troops,
                );
                game.player_mut(player).remove_troops(factor);
                for &attack in &outgoing_attacks {
                    let attack_troops = game.attack(attack).troops();
                    let deaths = game.config().nuke_death_factor(
                        self.nuke_type,
                        attack_troops,
                        num_tiles_left,
                        max_troops,
                    );
                    game.attack_mut(attack).set_troops(attack_troops - deaths);
                }
                for entry in transport_troops.iter_mut() {
                    let deaths = game.config().nuke_death_factor(
                        self.nuke_type,
                        entry.1,
                        num_tiles_left,
                        max_troops,
                    );
                    entry.1 = f64::max(0.0, entry.1 - deaths);
                }
                i += 1.0;
            }
            for (unit, troops) in transport_troops {
                game.unit_mut(unit).set_troops(troops);
            }
        }

        let outer2 = magnitude.outer * magnitude.outer;
        let dst = self.dst;
        for unit in game.all_units() {
            let (t, tile, owner) = {
                let u = game.unit_any(unit);
                (u.unit_type, u.tile(), u.owner_id())
            };
            if matches!(
                t,
                UnitType::AtomBomb
                    | UnitType::HydrogenBomb
                    | UnitType::MIRVWarhead
                    | UnitType::MIRV
                    | UnitType::SAMMissile
            ) {
                continue;
            }
            if (game.map.euclidean_dist_squared(dst, tile) as f64) < outer2 {
                let friendly = game.player(self.player).is_friendly(owner, true);
                game.unit_mut(unit)
                    .delete(Some(true), if friendly { None } else { Some(self.player) });
            }
        }

        self.active = false;
        game.unit_mut(nuke).set_reached_target();
        game.unit_mut(nuke).delete(Some(false), None);
    }
}

impl Execution for NukeExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if self.speed == -1.0 {
            self.speed = game.config().nuke_speed(self.nuke_type);
        }
        self.path_finder = ParabolaUniversalPathFinder::new(ParabolaOptions {
            increment: Some(self.speed),
            direction_up: Some(self.rocket_direction_up),
            ..Default::default()
        });
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let Some(nuke) = self.nuke else {
            let Some(spawn) = game
                .player(self.player)
                .can_build(self.nuke_type, self.dst, None)
            else {
                self.active = false;
                return;
            };
            let src = *self.src.get_or_insert(spawn);
            let silo = game
                .player(self.player)
                .units_of(UnitType::MissileSilo)
                .into_iter()
                .find(|&s| game.unit_any(s).tile() == spawn);
            if let Some(silo) = silo {
                let mut last_dep = 0.0;
                for &launch_tick in game.unit_any(silo).missile_timer_queue() {
                    last_dep = f64::max(launch_tick + 1.0, last_dep + 1.0);
                }
                if last_dep > game.ticks() as f64 {
                    self.wait_ticks += last_dep - game.ticks() as f64;
                }
            }
            let trajectory = self.get_trajectory(game, self.dst);
            let id = game.player_mut(self.player).build_unit(
                self.nuke_type,
                src,
                UnitParams {
                    target_tile: Some(self.dst),
                    trajectory: Some(trajectory),
                    ..Default::default()
                },
            );
            self.nuke = Some(id);
            game.unit_mut(id)
                .update_nuke_state(None, None, Some(self.wait_ticks));
            if self.nuke_type != UnitType::MIRVWarhead {
                self.maybe_break_alliances(game);
            }
            if let Some(silo) = silo {
                game.unit_mut(silo).launch();
            }
            return;
        };

        if !game.unit_any(nuke).is_active() {
            self.active = false;
            return;
        }

        if self.wait_ticks > 0.0 {
            self.wait_ticks -= 1.0;
            let w = self.wait_ticks;
            game.unit_mut(nuke).update_nuke_state(None, None, Some(w));
            return;
        }

        let src = self.src.expect("src set at launch");
        match self
            .path_finder
            .next(&game.map, src, self.dst, Some(self.speed))
        {
            PathResult::Complete(node) => {
                game.unit_mut(nuke).move_to(node);
                let owner = game.unit_any(nuke).owner_id();
                let should_be_destroyed = !game
                    .nearby_units(
                        self.dst,
                        game.config().default_sam_missile_speed(),
                        &[UnitType::SAMMissile],
                        Some(&mut |unit, _| {
                            unit.owner_id() != owner && unit.target_unit() == Some(nuke)
                        }),
                        false,
                    )
                    .is_empty();
                if !should_be_destroyed {
                    self.detonate(game, nuke);
                }
            }
            PathResult::Next(node) => {
                self.update_nuke_targetable(game, nuke);
                game.unit_mut(nuke).move_to(node);
                let index = self.path_finder.current_index() as i64;
                game.unit_mut(nuke).set_trajectory_index(index);
            }
            PathResult::NotFound => {}
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}

// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/MIRVExecution.ts: a MIRV carrier flight that separates into warheads.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Compat::JsNumber;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Execution::Execution::Execution;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::Legacy28::PathFinding::ParabolaPathFinder;
use crate::Pathfinding::Line::DistanceBasedBezierCurve;
use crate::Pathfinding::PathFinderParabola::{
    ParabolaOptions, ParabolaUniversalPathFinder, get_parabola_control_points,
};
use crate::Pathfinding::Types::PathResult;
use std::cell::RefCell;
use std::rc::Rc;

const MATH_SCALE: f64 = 100.0;
const LONG_FLIGHT_MULT: f64 = 14.0;
const LONG_FLIGHT_LINEAR_PERCENT: f64 = 10.0;
const SHORT_FLIGHT_MULT: f64 = 10.0;

const RANGE: f64 = 1500.0;
const RANGE_SQUARED: f64 = RANGE * RANGE;
const MINIMUM_SPREAD: f64 = 55.0;
const WARHEAD_COUNT: usize = 350;
/// Overlap failures in a row after which target generation gives up (v0.36.0 on).
const SATURATION_THRESHOLD: u32 = 1500;
/// Cells of the overlap grid per side (v0.36.0 on).
const GRID_WIDTH: i32 = (RANGE * 2.0 / MINIMUM_SPREAD).ceil() as i32 + 2;

/// A warhead execution shared between the game's execution list and the MIRV that may cancel it.
struct SharedNuke(Rc<RefCell<NukeExecution>>);

impl Execution for SharedNuke {
    fn init(&mut self, game: &mut GameImpl, ticks: Tick) {
        self.0.borrow_mut().init(game, ticks);
    }
    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        self.0.borrow_mut().tick(game, ticks);
    }
    fn is_active(&self) -> bool {
        self.0.borrow().is_active()
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}

/// The MIRV carrier: flies to the separation point, then releases up to 350 warheads.
pub struct MirvExecution {
    active: bool,
    nuke: Option<UnitId>,
    base_x: f64,
    base_y: f64,
    random: PseudoRandom,
    full_path: Vec<TileRef>,
    path_index: usize,
    target_player: SmallId,
    separate_dst: TileRef,
    spawn_tile: TileRef,
    staged_targets: Vec<TileRef>,
    warheads_spawned: bool,
    warhead_executions: Vec<Rc<RefCell<NukeExecution>>>,
    player: SmallId,
    dst: TileRef,
    /// Pre-PR-5188 flight: betrayal at init, float Bezier path, warheads released on arrival.
    legacy: bool,
    legacy_speed: f64,
    legacy_path_finder: ParabolaUniversalPathFinder,
    /// v0.28.4 flight: one-sided betrayal at init, control-point Bezier, warheads scattered by `nextInt`.
    v28: bool,
    v28_path: ParabolaPathFinder,
    /// Box of the target range clipped to the map (v0.36.0 on): min x, max x, min y, max y.
    bounds: [i32; 4],
    consecutive_overlap_failures: u32,
    /// Two target slots per cell of 55 tiles, -1 when empty (v0.36.0 on).
    spatial_grid: Vec<i32>,
}

impl MirvExecution {
    /// `new MirvExecution(player, dst)`.
    pub fn new(player: SmallId, dst: TileRef) -> Self {
        Self {
            active: true,
            nuke: None,
            base_x: 0.0,
            base_y: 0.0,
            random: PseudoRandom::new(0.0),
            full_path: Vec::new(),
            path_index: 0,
            target_player: 0,
            separate_dst: 0,
            spawn_tile: 0,
            staged_targets: Vec::new(),
            warheads_spawned: false,
            warhead_executions: Vec::new(),
            player,
            dst,
            legacy: false,
            legacy_speed: -1.0,
            legacy_path_finder: ParabolaUniversalPathFinder::default(),
            v28: false,
            v28_path: ParabolaPathFinder::new(),
            bounds: [0; 4],
            consecutive_overlap_failures: 0,
            spatial_grid: Vec::new(),
        }
    }

    fn grid_rules(game: &GameImpl) -> bool {
        game.config().rules().has(Change::MirvTargetGrid36)
    }

    /// Grid cell offset of a tile, or None when outside the grid.
    fn grid_cell(&self, game: &GameImpl, x: i32, y: i32) -> Option<(i32, i32)> {
        let _ = game;
        let rel_x = x as f64 - self.base_x + RANGE;
        let rel_y = y as f64 - self.base_y + RANGE;
        let cx = (rel_x / MINIMUM_SPREAD).floor() as i32;
        let cy = (rel_y / MINIMUM_SPREAD).floor() as i32;
        if !(0..GRID_WIDTH).contains(&cx) || !(0..GRID_WIDTH).contains(&cy) {
            return None;
        }
        Some((cx, cy))
    }

    fn add_to_grid(&mut self, game: &GameImpl, tile: TileRef) {
        let Some((cx, cy)) = self.grid_cell(game, game.x(tile) as i32, game.y(tile) as i32) else {
            return;
        };
        let base = ((cy * GRID_WIDTH + cx) * 2) as usize;
        for i in 0..2 {
            if self.spatial_grid[base + i] == -1 {
                self.spatial_grid[base + i] = tile as i32;
                return;
            }
        }
    }

    fn remove_from_grid(&mut self, game: &GameImpl, tile: TileRef) {
        if self.spatial_grid.is_empty() {
            return;
        }
        let Some((cx, cy)) = self.grid_cell(game, game.x(tile) as i32, game.y(tile) as i32) else {
            return;
        };
        let base = ((cy * GRID_WIDTH + cx) * 2) as usize;
        if self.spatial_grid[base] == tile as i32 {
            self.spatial_grid[base] = self.spatial_grid[base + 1];
            self.spatial_grid[base + 1] = -1;
        } else if self.spatial_grid[base + 1] == tile as i32 {
            self.spatial_grid[base + 1] = -1;
        }
    }

    fn build_overlap_grid(&mut self, game: &GameImpl) {
        self.spatial_grid = vec![-1; (GRID_WIDTH * GRID_WIDTH * 2) as usize];
        for t in self.staged_targets.clone() {
            self.add_to_grid(game, t);
        }
    }

    fn is_overlapping_grid(&self, game: &GameImpl, x: i32, y: i32) -> bool {
        let Some((cx, cy)) = self.grid_cell(game, x, y) else {
            return true;
        };
        // A full target cell rejects, so a target is never silently dropped from the grid.
        let target_off = ((cy * GRID_WIDTH + cx) * 2) as usize;
        if self.spatial_grid[target_off + 1] != -1 {
            return true;
        }
        for gy in (cy - 1)..=(cy + 1) {
            if !(0..GRID_WIDTH).contains(&gy) {
                continue;
            }
            for gx in (cx - 1)..=(cx + 1) {
                if !(0..GRID_WIDTH).contains(&gx) {
                    continue;
                }
                let base = ((gy * GRID_WIDTH + gx) * 2) as usize;
                for i in 0..2 {
                    let t = self.spatial_grid[base + i];
                    if t == -1 {
                        break;
                    }
                    let dx = (x - game.x(t as TileRef) as i32).abs() as f64;
                    if dx < MINIMUM_SPREAD {
                        let dy = (y - game.y(t as TileRef) as i32).abs() as f64;
                        if dx + dy < MINIMUM_SPREAD {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `tryGenerateTarget()` of v0.36.0: samples inside the clipped box and uses the overlap grid.
    fn try_generate_target_grid(&mut self, game: &GameImpl) -> Option<TileRef> {
        if self.consecutive_overlap_failures >= SATURATION_THRESHOLD {
            return None;
        }
        let [min_x, max_x, min_y, max_y] = self.bounds;
        for _ in 0..100 {
            let r1 = self.random.next();
            let r2 = (r1 * 15485863.0) % 1.0;
            let x = min_x + (r1 * (max_x - min_x + 1) as f64).floor() as i32;
            let y = min_y + (r2 * (max_y - min_y + 1) as f64).floor() as i32;
            let (dx, dy) = (x as f64 - self.base_x, y as f64 - self.base_y);
            if dx * dx + dy * dy > RANGE_SQUARED {
                continue;
            }
            let tile = game.tile_ref(x, y);
            if game.owner_id(tile) != self.target_player {
                continue;
            }
            if !game.is_land(tile) || game.map.is_impassable(tile) {
                continue;
            }
            if self.is_overlapping_grid(game, x, y) {
                self.consecutive_overlap_failures += 1;
                if self.consecutive_overlap_failures >= SATURATION_THRESHOLD {
                    return None;
                }
                continue;
            }
            self.consecutive_overlap_failures = 0;
            return Some(tile);
        }
        None
    }

    /// Stagger of target finding on ticks 20 to 11 (v0.36.0).
    fn stage_targets_grid(&mut self, game: &GameImpl) {
        if self.consecutive_overlap_failures >= SATURATION_THRESHOLD
            || self.staged_targets.len() >= WARHEAD_COUNT
        {
            return;
        }
        for _ in 0..100 {
            if self.staged_targets.len() >= WARHEAD_COUNT {
                break;
            }
            if let Some(target) = self.try_generate_target_grid(game) {
                self.staged_targets.push(target);
                self.add_to_grid(game, target);
            }
            if self.consecutive_overlap_failures >= SATURATION_THRESHOLD {
                break;
            }
        }
    }

    /// `finalizeDestinations` of v0.36.0.
    fn finalize_destinations_grid(&mut self, game: &GameImpl, additional_attempts: usize) {
        let dst = self.dst;
        let target_player = self.target_player;
        let count_before = self.staged_targets.len();
        let staged = std::mem::take(&mut self.staged_targets);
        let mut kept = Vec::with_capacity(staged.len());
        for tile in staged {
            if tile == dst || game.owner_id(tile) == target_player {
                kept.push(tile);
            } else {
                self.remove_from_grid(game, tile);
            }
        }
        self.staged_targets = kept;
        if self.staged_targets.len() < count_before {
            self.consecutive_overlap_failures = 0;
        }
        if self.consecutive_overlap_failures < SATURATION_THRESHOLD {
            for _ in 0..additional_attempts {
                if self.staged_targets.len() >= WARHEAD_COUNT {
                    break;
                }
                if let Some(target) = self.try_generate_target_grid(game) {
                    self.staged_targets.push(target);
                    self.add_to_grid(game, target);
                }
                if self.consecutive_overlap_failures >= SATURATION_THRESHOLD {
                    break;
                }
            }
        }
        self.staged_targets.sort_by(|&a, &b| {
            game.manhattan_dist(b, dst)
                .cmp(&game.manhattan_dist(a, dst))
        });
        self.spatial_grid = Vec::new();
    }

    /// Pre-PR-5188 `init`: betrayal on construction and the fixed-speed path finder.
    fn init_legacy(&mut self, game: &mut GameImpl) {
        self.legacy_speed = game.config().nuke_speed(UnitType::MIRV);
        self.legacy_path_finder = ParabolaUniversalPathFinder::new(ParabolaOptions {
            increment: Some(self.legacy_speed),
            legacy_curve: true,
            legacy_reset: !game.config().rules().has(Change::SamTargetingFixes4770),
            ..Default::default()
        });
        if self.target_player != 0 {
            if let Some(alliance) = game.player(self.player).alliance_with(self.target_player) {
                game.player_mut(self.player).break_alliance(alliance);
            }
            if self.target_player != self.player {
                game.player_mut(self.target_player)
                    .update_relation(self.player, -100.0);
                game.player_mut(self.player)
                    .update_relation(self.target_player, -100.0);
            }
        }
    }

    /// `init` of v0.28.4: betrayal on launch, the target loses relation towards the sender.
    fn init_v28(&mut self, game: &mut GameImpl) {
        if self.target_player != 0 {
            if let Some(alliance) = game.player(self.player).alliance_with(self.target_player) {
                game.player_mut(self.player).break_alliance(alliance);
            }
            if self.target_player != self.player {
                game.player_mut(self.target_player)
                    .update_relation(self.player, -100.0);
            }
        }
    }

    /// `tick` of v0.28.4.
    fn tick_v28(&mut self, game: &mut GameImpl) {
        if self.nuke.is_none() {
            let Some(spawn) = game
                .player(self.player)
                .can_build(UnitType::MIRV, self.dst, None)
            else {
                self.active = false;
                return;
            };
            self.spawn_tile = spawn;
            let nuke = game.player_mut(self.player).build_unit(
                UnitType::MIRV,
                spawn,
                UnitParams {
                    target_tile: Some(self.dst),
                    ..Default::default()
                },
            );
            self.nuke = Some(nuke);
            game.stats_mut().mirv_launched();
            let x = ((game.x(self.dst) as f64 + game.x(game.unit_any(nuke).tile()) as f64) / 2.0)
                .floor();
            let y = f64::max(0.0, game.y(self.dst) as f64 - 500.0) + 50.0;
            self.separate_dst = game.tile_ref(x as i32, y as i32);
            self.v28_path
                .compute_control_points(&game.map, spawn, self.separate_dst, 3.0, true);
        }
        let nuke = self.nuke.expect("launched above");
        let speed = game.config().nuke_speed(UnitType::MIRV);
        match self.v28_path.next_tile(&game.map, speed) {
            None => {
                self.separate_v28(game, nuke);
                self.active = false;
            }
            Some(tile) => game.unit_mut(nuke).move_to(tile),
        }
    }

    fn separate_v28(&mut self, game: &mut GameImpl, nuke: UnitId) {
        let dst = self.dst;
        let mut dsts: Vec<TileRef> = vec![dst];
        let mut attempts = 1000;
        while attempts > 0 && dsts.len() < WARHEAD_COUNT {
            attempts -= 1;
            if let Some(potential) = self.random_land_v28(game, dst, &dsts) {
                dsts.push(potential);
            }
        }
        dsts.sort_by(|&a, &b| {
            game.manhattan_dist(b, dst)
                .cmp(&game.manhattan_dist(a, dst))
        });
        let src = game.unit_any(nuke).tile();
        for (i, &d) in dsts.iter().enumerate() {
            let speed = 15.0 + ((i as f64 / WARHEAD_COUNT as f64) * 5.0).floor();
            let wait = self.random.next_int(0.0, 15.0);
            game.add_execution(Box::new(NukeExecution::new(
                UnitType::MIRVWarhead,
                self.player,
                d,
                Some(src),
                speed,
                wait,
                None,
            )));
        }
        game.unit_mut(nuke).delete(Some(false), None);
    }

    fn random_land_v28(
        &mut self,
        game: &GameImpl,
        reference: TileRef,
        taken: &[TileRef],
    ) -> Option<TileRef> {
        let range2 = RANGE_SQUARED as u64;
        let (rx, ry) = (game.x(reference) as f64, game.y(reference) as f64);
        for _ in 0..100 {
            let x = self.random.next_int(rx - RANGE, rx + RANGE);
            let y = self.random.next_int(ry - RANGE, ry + RANGE);
            if !game.is_valid_coord(x as i32, y as i32) {
                continue;
            }
            let tile = game.tile_ref(x as i32, y as i32);
            if !game.is_land(tile) {
                continue;
            }
            if game.map.euclidean_dist_squared(tile, reference) > range2 {
                continue;
            }
            if game.owner_id(tile) != self.target_player {
                continue;
            }
            if taken
                .iter()
                .any(|&t| (game.manhattan_dist(tile, t) as f64) < MINIMUM_SPREAD)
            {
                continue;
            }
            return Some(tile);
        }
        None
    }

    /// Pre-PR-5188 `tick`.
    fn tick_legacy(&mut self, game: &mut GameImpl) {
        if self.nuke.is_none() {
            let Some(spawn) = game
                .player(self.player)
                .can_build(UnitType::MIRV, self.dst, None)
            else {
                self.active = false;
                return;
            };
            self.spawn_tile = spawn;
            let nuke = game.player_mut(self.player).build_unit(
                UnitType::MIRV,
                spawn,
                UnitParams {
                    target_tile: Some(self.dst),
                    ..Default::default()
                },
            );
            self.nuke = Some(nuke);
            game.stats_mut().mirv_launched();
            let x = ((game.x(self.dst) as f64 + game.x(game.unit_any(nuke).tile()) as f64) / 2.0)
                .floor();
            let y = f64::max(0.0, game.y(self.dst) as f64 - 500.0) + 50.0;
            self.separate_dst = game.tile_ref(x as i32, y as i32);

            let silo = game
                .player(self.player)
                .units_of(UnitType::MissileSilo)
                .into_iter()
                .find(|&s| game.unit_any(s).tile() == spawn);
            if let Some(silo) = silo
                && game.config().rules().has(Change::MirvSiloCooldown4707)
            {
                game.unit_mut(silo).launch();
            }
        }
        let nuke = self.nuke.expect("launched above");
        match self.legacy_path_finder.next(
            &game.map,
            self.spawn_tile,
            self.separate_dst,
            Some(self.legacy_speed),
        ) {
            PathResult::Complete(_) => {
                self.separate_legacy(game, nuke);
                self.active = false;
            }
            PathResult::Next(node) => game.unit_mut(nuke).move_to(node),
            _ => {}
        }
    }

    fn separate_legacy(&mut self, game: &mut GameImpl, nuke: UnitId) {
        self.base_x = game.x(self.dst) as f64;
        self.base_y = game.y(self.dst) as f64;
        self.staged_targets = vec![self.dst];
        for _ in 0..1000 {
            if let Some(target) = self.try_generate_target(game) {
                self.staged_targets.push(target);
            }
            if self.staged_targets.len() >= WARHEAD_COUNT {
                break;
            }
        }
        let dst = self.dst;
        let mut destinations = std::mem::take(&mut self.staged_targets);
        destinations.sort_by(|&a, &b| {
            game.manhattan_dist(b, dst)
                .cmp(&game.manhattan_dist(a, dst))
        });
        let warhead_speed = game.config().nuke_speed(UnitType::MIRVWarhead);
        let src = game.unit_any(nuke).tile();
        for (i, &dst) in destinations.iter().enumerate() {
            game.add_execution(Box::new(NukeExecution::new(
                UnitType::MIRVWarhead,
                self.player,
                dst,
                Some(src),
                warhead_speed + ((i as f64 / WARHEAD_COUNT as f64) * 5.0).floor(),
                self.random.next_int(0.0, 15.0),
                None,
            )));
        }
        game.unit_mut(nuke).delete(Some(false), None);
    }

    /// `owner()`.
    pub fn owner(&self) -> SmallId {
        self.player
    }

    fn finalize_destinations(&mut self, game: &GameImpl, additional_attempts: usize) {
        let dst = self.dst;
        let target_player = self.target_player;
        self.staged_targets
            .retain(|&tile| tile == dst || game.owner_id(tile) == target_player);

        for _ in 0..additional_attempts {
            if self.staged_targets.len() >= WARHEAD_COUNT {
                break;
            }
            if let Some(target) = self.try_generate_target(game) {
                self.staged_targets.push(target);
            }
        }

        self.staged_targets.sort_by(|&a, &b| {
            game.manhattan_dist(b, dst)
                .cmp(&game.manhattan_dist(a, dst))
        });
    }

    fn spawn_warheads_with_wait(&mut self, game: &mut GameImpl, remaining_ticks: f64) {
        if self.nuke.is_none() {
            return;
        }
        let wait_base = f64::max(0.0, remaining_ticks);
        let warhead_speed = game.config().nuke_speed(UnitType::MIRVWarhead);
        for (i, &dst) in self.staged_targets.clone().iter().enumerate() {
            let speed_offset = if i < 70 {
                0.0
            } else if i < 140 {
                1.0
            } else if i < 210 {
                2.0
            } else if i < 280 {
                3.0
            } else {
                4.0
            };
            let execution = Rc::new(RefCell::new(NukeExecution::new(
                UnitType::MIRVWarhead,
                self.player,
                dst,
                Some(self.separate_dst),
                warhead_speed + speed_offset,
                wait_base + self.random.next_int(0.0, 15.0),
                None,
            )));
            self.warhead_executions.push(execution.clone());
            game.add_execution(Box::new(SharedNuke(execution)));
        }
    }

    fn try_generate_target(&mut self, game: &GameImpl) -> Option<TileRef> {
        for _ in 0..100 {
            let r1 = self.random.next();
            let r2 = (r1 * 15485863.0) % 1.0;

            let x = JsNumber::round(r1 * RANGE * 2.0 - RANGE + self.base_x);
            let y = JsNumber::round(r2 * RANGE * 2.0 - RANGE + self.base_y);

            if !game.is_valid_coord(x as i32, y as i32) {
                continue;
            }

            let tile = game.tile_ref(x as i32, y as i32);

            if !game.is_land(tile) {
                continue;
            }

            let (dx, dy) = (x - self.base_x, y - self.base_y);
            if dx * dx + dy * dy > RANGE_SQUARED {
                continue;
            }

            if game.owner_id(tile) != self.target_player {
                continue;
            }

            if self.is_overlapping(game, x, y) {
                continue;
            }

            return Some(tile);
        }
        None
    }

    fn is_overlapping(&self, game: &GameImpl, x: f64, y: f64) -> bool {
        for &existing in &self.staged_targets {
            let ex = game.x(existing) as f64;
            let ey = game.y(existing) as f64;
            let manhattan = (x - ex).abs() + (y - ey).abs();
            if manhattan < MINIMUM_SPREAD {
                return true;
            }
        }
        false
    }

    fn calculate_deterministic_speed(
        &self,
        game: &GameImpl,
        spawn_tile: TileRef,
        separate_dst: TileRef,
    ) -> f64 {
        let ideal_options = ParabolaOptions {
            distance_based_height: Some(true),
            direction_up: Some(true),
            ignore_map_bounds: Some(true),
            ..Default::default()
        };
        let [ip0, ip1, ip2, ip3] =
            get_parabola_control_points(&game.map, spawn_tile, separate_dst, &ideal_options);
        let ideal_mirv_length = DistanceBasedBezierCurve::get_length(ip0, ip1, ip2, ip3);
        let base_speed = game.config().nuke_speed(UnitType::MIRV);

        let ideal_mirv_ticks = ideal_mirv_length / base_speed;

        let base_ticks = game.config().mirv_normalize_target_ticks();
        let base_ticks_scaled = base_ticks * MATH_SCALE;

        let ideal_mirv_ticks_int = (ideal_mirv_ticks * MATH_SCALE).floor();
        let mut target_mirv_ticks_int = base_ticks_scaled;

        if ideal_mirv_ticks_int > base_ticks_scaled {
            let diff = ideal_mirv_ticks_int - base_ticks_scaled;
            target_mirv_ticks_int = base_ticks_scaled
                + diff.sqrt().floor() * LONG_FLIGHT_MULT
                + ((diff * LONG_FLIGHT_LINEAR_PERCENT) / 100.0).floor();
        } else if ideal_mirv_ticks_int < base_ticks_scaled {
            let diff = base_ticks_scaled - ideal_mirv_ticks_int;
            target_mirv_ticks_int = base_ticks_scaled - diff.sqrt().floor() * SHORT_FLIGHT_MULT;
        }
        target_mirv_ticks_int = f64::max(MATH_SCALE, target_mirv_ticks_int);

        let [ap0, ap1, ap2, ap3] = get_parabola_control_points(
            &game.map,
            spawn_tile,
            separate_dst,
            &ParabolaOptions::default(),
        );
        let actual_mirv_length = DistanceBasedBezierCurve::get_length(ap0, ap1, ap2, ap3);

        let actual_mirv_length_int = JsNumber::round(actual_mirv_length * 256.0);

        let num = actual_mirv_length_int * MATH_SCALE;
        let den = target_mirv_ticks_int;
        let pure_integer_speed_scaled = ((num + (den / 2.0).floor()) / den).floor();
        f64::max(1.0, pure_integer_speed_scaled / 256.0)
    }
}

impl Execution for MirvExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.random = PseudoRandom::for_rules(
            game.config().rules(),
            game.ticks() as f64 + simple_hash(game.player(self.player).id()) as f64,
        );
        self.target_player = game.owner_id(self.dst);
        self.base_x = game.x(self.dst) as f64;
        self.base_y = game.y(self.dst) as f64;
        self.staged_targets = vec![self.dst];
        if Self::grid_rules(game) {
            self.bounds = [
                f64::max(0.0, self.base_x - RANGE) as i32,
                f64::min(game.width() as f64 - 1.0, self.base_x + RANGE) as i32,
                f64::max(0.0, self.base_y - RANGE) as i32,
                f64::min(game.height() as f64 - 1.0, self.base_y + RANGE) as i32,
            ];
            self.build_overlap_grid(game);
        }
        self.v28 = game.config().rules().has(Change::LegacyProjectiles28);
        if self.v28 {
            self.init_v28(game);
            return;
        }
        self.legacy = !game.config().rules().has(Change::MirvNormalize5188);
        if self.legacy {
            self.init_legacy(game);
        }
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if self.v28 {
            self.tick_v28(game);
            return;
        }
        if self.legacy {
            self.tick_legacy(game);
            return;
        }
        if self.nuke.is_none() {
            let Some(spawn) = game
                .player(self.player)
                .can_build(UnitType::MIRV, self.dst, None)
            else {
                self.active = false;
                return;
            };
            self.spawn_tile = spawn;
            let nuke = game.player_mut(self.player).build_unit(
                UnitType::MIRV,
                spawn,
                UnitParams {
                    target_tile: Some(self.dst),
                    target_player: Some(self.target_player),
                    ..Default::default()
                },
            );
            self.nuke = Some(nuke);
            game.stats_mut().mirv_launched();

            // Betrayal on launch, only once the missile has actually spawned.
            if self.target_player != 0 {
                if let Some(alliance) = game.player(self.player).alliance_with(self.target_player) {
                    game.player_mut(self.player).break_alliance(alliance);
                }
                if self.target_player != self.player {
                    game.player_mut(self.target_player)
                        .update_relation(self.player, -100.0);
                    game.player_mut(self.player)
                        .update_relation(self.target_player, -100.0);
                }
            }
            let x = ((self.base_x + game.x(game.unit_any(nuke).tile()) as f64) / 2.0).floor();
            let y = f64::max(0.0, self.base_y - 500.0) + 50.0;
            self.separate_dst = game.tile_ref(x as i32, y as i32);

            let speed =
                self.calculate_deterministic_speed(game, self.spawn_tile, self.separate_dst);
            let mut path_finder = ParabolaUniversalPathFinder::new(ParabolaOptions {
                increment: Some(speed),
                ..Default::default()
            });

            while let PathResult::Next(node) =
                path_finder.next(&game.map, self.spawn_tile, self.separate_dst, Some(speed))
            {
                self.full_path.push(node);
            }

            let silo = game
                .player(self.player)
                .units_of(UnitType::MissileSilo)
                .into_iter()
                .find(|&s| game.unit_any(s).tile() == spawn);
            if let Some(silo) = silo {
                game.unit_mut(silo).launch();
            }
        }
        let nuke = self.nuke.expect("launched above");

        if !game.unit_any(nuke).is_active() {
            self.active = false;
            for warhead in std::mem::take(&mut self.warhead_executions) {
                warhead.borrow_mut().cancel(game);
            }
            return;
        }

        let remaining_ticks = self.full_path.len() as f64 - self.path_index as f64;

        if remaining_ticks <= 20.0 && remaining_ticks > 10.0 && Self::grid_rules(game) {
            self.stage_targets_grid(game);
        } else if remaining_ticks <= 20.0 && remaining_ticks > 10.0 {
            for _ in 0..100 {
                if self.staged_targets.len() >= WARHEAD_COUNT {
                    break;
                }
                if let Some(target) = self.try_generate_target(game) {
                    self.staged_targets.push(target);
                }
            }
        }

        if remaining_ticks <= 10.0 && !self.warheads_spawned {
            self.warheads_spawned = true;
            let extra_attempts = ((10.0 - remaining_ticks) * 50.0) as usize;
            if Self::grid_rules(game) {
                self.finalize_destinations_grid(game, 500 + extra_attempts);
            } else {
                self.finalize_destinations(game, 500 + extra_attempts);
            }
            self.spawn_warheads_with_wait(game, remaining_ticks);
        }

        if self.path_index < self.full_path.len() {
            let tile = self.full_path[self.path_index];
            self.path_index += 1;
            game.unit_mut(nuke).move_to(tile);
        } else {
            game.unit_mut(nuke).delete(Some(false), None);
            self.active = false;
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}

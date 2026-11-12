// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/PathFinding.ts: the stepping `PathFinder`, `AirPathFinder` and `ParabolaPathFinder`.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::MiniAStar::MiniAStar;
use super::SerialAStar::PathFindResultType;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Line::Point;
use crate::Pathfinding::LineV33::LegacyBezierCurve;
use crate::Pathfinding::PathFinderParabola::{ParabolaOptions, get_parabola_control_points};

const PARABOLA_MIN_HEIGHT: f64 = 50.0;

/// Upstream `AStarResult<TileRef>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepResult {
    /// Step to this tile next.
    NextTile(TileRef),
    /// The search needs more ticks.
    Pending,
    /// Arrived (the tile is the current one).
    Completed(TileRef),
    /// No path exists.
    PathNotFound,
}

impl StepResult {
    /// The result type tag of upstream.
    pub fn result_type(&self) -> PathFindResultType {
        match self {
            Self::NextTile(_) => PathFindResultType::NextTile,
            Self::Pending => PathFindResultType::Pending,
            Self::Completed(_) => PathFindResultType::Completed,
            Self::PathNotFound => PathFindResultType::PathNotFound,
        }
    }
}

/// The nuke and MIRV arc finder (`ParabolaPathFinder`): control points are computed once per flight.
#[derive(Clone, Debug, Default)]
pub struct ParabolaPathFinder {
    curve: Option<LegacyBezierCurve>,
}

impl ParabolaPathFinder {
    /// `new ParabolaPathFinder(mg)`.
    pub fn new() -> Self {
        Self { curve: None }
    }

    /// `computeControlPoints(orig, dst, increment, distanceBasedHeight)`.
    pub fn compute_control_points(
        &mut self,
        map: &GameMapImpl,
        orig: TileRef,
        dst: TileRef,
        increment: f64,
        distance_based_height: bool,
    ) {
        let options = ParabolaOptions {
            increment: Some(increment),
            distance_based_height: Some(distance_based_height),
            min_height: Some(PARABOLA_MIN_HEIGHT),
            ..ParabolaOptions::default()
        };
        // Always pointing up; the lower clamp of the control points is the only one that can bind
        let [p0, p1, p2, p3] = get_parabola_control_points(map, orig, dst, &options);
        self.curve = Some(LegacyBezierCurve::new(p0, p1, p2, p3, increment, true));
    }

    /// `nextTile(speed)`: `None` once the curve is exhausted (upstream's `true`).
    pub fn next_tile(&mut self, map: &GameMapImpl, speed: f64) -> Option<TileRef> {
        let curve = self
            .curve
            .as_mut()
            .expect("ParabolaPathFinder not initialized");
        let p = curve.increment(speed)?;
        Some(map.tile_ref(p.x.floor() as i32, p.y.floor() as i32))
    }

    /// `currentIndex()`.
    pub fn current_index(&self) -> usize {
        self.curve.as_ref().map_or(0, |c| c.get_current_index())
    }

    /// `allTiles()`: every sampled tile of the arc.
    pub fn all_tiles(&self, map: &GameMapImpl) -> Vec<TileRef> {
        let Some(curve) = &self.curve else {
            return Vec::new();
        };
        curve
            .get_all_points()
            .iter()
            .map(|p: &Point| map.tile_ref(p.x.floor() as i32, p.y.floor() as i32))
            .collect()
    }
}

/// `AirPathFinder`: the randomised staircase of shells and SAM missiles; one PRNG per finder.
#[derive(Clone, Debug)]
pub struct AirPathFinder {
    random: PseudoRandom,
}

impl AirPathFinder {
    /// `new AirPathFinder(mg, random)`.
    pub fn new(random: PseudoRandom) -> Self {
        Self { random }
    }

    /// `nextTile(tile, dst)`: `None` when the destination is reached (upstream's `true`).
    pub fn next_tile(&mut self, map: &GameMapImpl, tile: TileRef, dst: TileRef) -> Option<TileRef> {
        let x = map.x(tile) as i64;
        let y = map.y(tile) as i64;
        let dst_x = map.x(dst) as i64;
        let dst_y = map.y(dst) as i64;
        if x == dst_x && y == dst_y {
            return None;
        }
        let mut next_x = x;
        let mut next_y = y;
        let ratio = (1.0 + (dst_y - y).abs() as f64 / ((dst_x - x).abs() + 1) as f64).floor();
        if self.random.chance(ratio) && x != dst_x {
            if x < dst_x {
                next_x += 1;
            } else if x > dst_x {
                next_x -= 1;
            }
        } else if y < dst_y {
            next_y += 1;
        } else if y > dst_y {
            next_y -= 1;
        }
        if next_x == x && next_y == y {
            return None;
        }
        Some(map.tile_ref(next_x as i32, next_y as i32))
    }
}

/// `PathFinder.Mini`: steps along a `MiniAStar` path and recomputes when the destination moves.
#[derive(Clone, Debug)]
pub struct PathFinder {
    curr: Option<TileRef>,
    dst: Option<TileRef>,
    path: Option<Vec<TileRef>>,
    path_idx: usize,
    a_star: Option<MiniAStar>,
    compute_finished: bool,
    iterations: f64,
    water_path: bool,
    max_tries: f64,
}

impl PathFinder {
    /// `PathFinder.Mini(game, iterations, waterPath, maxTries)`.
    pub fn mini(iterations: f64, water_path: bool, max_tries: f64) -> Self {
        Self {
            curr: None,
            dst: None,
            path: None,
            path_idx: 0,
            a_star: None,
            compute_finished: true,
            iterations,
            water_path,
            max_tries,
        }
    }

    /// `nextTile(curr, dst, dist)` with upstream's recursion (the recursive calls use the default `dist` of 1).
    pub fn next_tile(
        &mut self,
        map: &GameMapImpl,
        mini_map: &GameMapImpl,
        curr: TileRef,
        dst: TileRef,
        dist: u32,
    ) -> StepResult {
        if map.manhattan_dist(curr, dst) < dist {
            self.path = None;
            return StepResult::Completed(curr);
        }
        if self.compute_finished {
            if self.should_recompute(map, curr, dst) {
                self.curr = Some(curr);
                self.dst = Some(dst);
                self.path = None;
                self.path_idx = 0;
                self.a_star = Some(MiniAStar::new(
                    map,
                    mini_map,
                    &[curr],
                    false,
                    dst,
                    self.iterations,
                    self.max_tries,
                    self.water_path,
                    0.0,
                ));
                self.compute_finished = false;
                return self.next_tile(map, mini_map, curr, dst, 1);
            }
            let tile = self
                .path
                .as_ref()
                .and_then(|p| p.get(self.path_idx))
                .copied()
                .expect("missing tile");
            self.path_idx += 1;
            return StepResult::NextTile(tile);
        }
        let a_star = self.a_star.as_mut().expect("search in progress");
        match a_star.compute(mini_map) {
            PathFindResultType::Completed => {
                self.compute_finished = true;
                self.path = Some(a_star.reconstruct_path(map, mini_map));
                // exclude first tile
                self.path_idx = 1;
                self.next_tile(map, mini_map, curr, dst, 1)
            }
            PathFindResultType::Pending => StepResult::Pending,
            PathFindResultType::PathNotFound => StepResult::PathNotFound,
            PathFindResultType::NextTile => panic!("unexpected compute result"),
        }
    }

    fn should_recompute(&self, map: &GameMapImpl, curr: TileRef, dst: TileRef) -> bool {
        let (Some(_), Some(_), Some(old_dst)) = (&self.path, self.curr, self.dst) else {
            return true;
        };
        let dist = map.manhattan_dist(curr, dst);
        let tolerance = if dist > 50 {
            10
        } else if dist > 25 {
            5
        } else {
            0
        };
        map.manhattan_dist(old_dst, dst) > tolerance
    }
}

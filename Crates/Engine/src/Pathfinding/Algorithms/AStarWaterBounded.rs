// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/AStar.WaterBounded.ts: water A* confined to a bounding box with local node indices.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::PriorityQueue::{BucketQueue, MinHeap, OpenQueue, PriorityQueue};
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};

const LAND_MASK: u8 = 1 << 7;
const MAGNITUDE_MASK: u8 = 0x1f;
const COST_SCALE: f64 = 100.0;
const COST_SCALE_INT: u32 = 100;
const BASE_COST: u32 = COST_SCALE_INT;

/// Prefer magnitude 3-10 (3-10 tiles from shore).
fn magnitude_penalty(magnitude: u8) -> u32 {
    if magnitude < 3 {
        3 * COST_SCALE_INT // too close to shore
    } else if magnitude <= 10 {
        0 // sweet spot
    } else {
        COST_SCALE_INT // deep water, slight penalty
    }
}

/// Inclusive search rectangle in map coordinates (upstream `SearchBounds`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchBounds {
    /// Left column.
    pub min_x: i64,
    /// Right column.
    pub max_x: i64,
    /// Top row.
    pub min_y: i64,
    /// Bottom row.
    pub max_y: i64,
}

/// Water A* limited to a bounding box of at most `max_search_area` tiles.
#[derive(Debug)]
pub struct AStarWaterBounded {
    stamp: u32,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    g_score: Vec<u32>,
    came_from: Vec<i32>,
    queue: OpenQueue,
    max_search_area: usize,
    map_width: i64,
    heuristic_weight: f64,
    max_iterations: i64,
}

impl AStarWaterBounded {
    /// Creates a finder; defaults are weight 3 and 100,000 iterations.
    pub fn new(
        map: &GameMapImpl,
        max_search_area: usize,
        heuristic_weight: Option<f64>,
        max_iterations: Option<i64>,
    ) -> Self {
        Self {
            stamp: 1,
            closed_stamp: vec![0; max_search_area],
            g_score_stamp: vec![0; max_search_area],
            g_score: vec![0; max_search_area],
            came_from: vec![0; max_search_area],
            queue: OpenQueue::Heap(MinHeap::new(max_search_area * 4)),
            max_search_area,
            map_width: map.width() as i64,
            heuristic_weight: heuristic_weight.unwrap_or(3.0),
            max_iterations: max_iterations.unwrap_or(100_000),
        }
    }

    /// Builds before PR 2932 queue with a bucket queue sized for the scaled costs.
    pub fn use_bucket_queue(&mut self) {
        let max_dim = (self.max_search_area as f64).sqrt().ceil();
        let max_f =
            (self.heuristic_weight + 1.0) * BASE_COST as f64 * max_dim * 2.0 + COST_SCALE * max_dim;
        self.queue = OpenQueue::Bucket(BucketQueue::new(max_f as usize));
    }

    /// Path over the box spanned by the sources and the goal (upstream `findPath`).
    pub fn find_path_on(
        &mut self,
        map: &GameMapImpl,
        starts: &[TileRef],
        goal: TileRef,
    ) -> Option<Vec<TileRef>> {
        let w = self.map_width;
        let goal_x = goal as i64 % w;
        let goal_y = goal as i64 / w;
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (goal_x, goal_x, goal_y, goal_y);
        for &s in starts {
            let sx = s as i64 % w;
            let sy = s as i64 / w;
            min_x = min_x.min(sx);
            max_x = max_x.max(sx);
            min_y = min_y.min(sy);
            max_y = max_y.max(sy);
        }
        self.search_bounded(
            map,
            starts,
            goal,
            SearchBounds {
                min_x,
                max_x,
                min_y,
                max_y,
            },
        )
    }

    /// A* from `starts` to `goal` inside `bounds`; `None` if unreachable, the box is too large or the iteration cap hits.
    pub fn search_bounded(
        &mut self,
        map: &GameMapImpl,
        starts: &[TileRef],
        goal: TileRef,
        bounds: SearchBounds,
    ) -> Option<Vec<TileRef>> {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.closed_stamp.fill(0);
            self.g_score_stamp.fill(0);
            self.stamp = 1;
        }

        let stamp = self.stamp;
        let map_width = self.map_width;
        let weight = self.heuristic_weight;
        let base = BASE_COST as f64;

        let SearchBounds {
            min_x,
            max_x,
            min_y,
            max_y,
        } = bounds;
        let bounds_width = max_x - min_x + 1;
        let goal_x = goal as i64 % map_width;
        let goal_y = goal as i64 / map_width;
        let bounds_height = max_y - min_y + 1;
        let num_local_nodes = bounds_width * bounds_height;

        if num_local_nodes > self.closed_stamp.len() as i64 {
            return None;
        }

        let to_local = |tile: TileRef, clamp: bool| -> i64 {
            let mut x = tile as i64 % map_width;
            let mut y = tile as i64 / map_width;
            if clamp {
                x = min_x.max(max_x.min(x));
                y = min_y.max(max_y.min(y));
            }
            (y - min_y) * bounds_width + (x - min_x)
        };
        let to_global = |local: i64| -> TileRef {
            let local_x = local % bounds_width;
            let local_y = local / bounds_width;
            ((local_y + min_y) * map_width + (local_x + min_x)) as TileRef
        };

        let goal_local = to_local(goal, true);
        if goal_local < 0 || goal_local >= num_local_nodes {
            return None;
        }

        self.queue.clear();

        let s0 = starts[0];
        let start_x = (s0 as i64 % map_width) as f64;
        let start_y = (s0 as i64 / map_width) as f64;
        let (goal_xf, goal_yf) = (goal_x as f64, goal_y as f64);
        let dx_goal = goal_xf - start_x;
        let dy_goal = goal_yf - start_y;
        let cross_norm = f64::max(1.0, dx_goal.abs() + dy_goal.abs());
        let cross_tie_breaker = |nx: f64, ny: f64| -> f64 {
            let dx_n = nx - goal_xf;
            let dy_n = ny - goal_yf;
            let cross = (dx_goal * dy_n - dy_goal * dx_n).abs();
            ((cross * (COST_SCALE - 1.0)) / cross_norm / cross_norm).floor()
        };

        for &s in starts {
            let start_local = to_local(s, true);
            if start_local < 0 || start_local >= num_local_nodes {
                continue;
            }
            let sl = start_local as usize;
            self.g_score[sl] = 0;
            self.g_score_stamp[sl] = stamp;
            self.came_from[sl] = -1;
            let sx = (s as i64 % map_width) as f64;
            let sy = (s as i64 / map_width) as f64;
            let h = weight * base * ((sx - goal_xf).abs() + (sy - goal_yf).abs());
            self.queue.push(start_local as u32, h);
        }

        let mut iterations = self.max_iterations;

        while !self.queue.is_empty() {
            iterations -= 1;
            if iterations <= 0 {
                return None;
            }

            let current_local = self.queue.pop() as i64;
            let cl = current_local as usize;
            if self.closed_stamp[cl] == stamp {
                continue;
            }
            self.closed_stamp[cl] = stamp;

            if current_local == goal_local {
                return Some(self.build_path(goal_local, &to_global, num_local_nodes));
            }

            let current_g = self.g_score[cl];
            let current = to_global(current_local);
            let current_x = current as i64 % map_width;
            let current_y = current as i64 / map_width;

            // North, south, west, east
            for dir in 0..4 {
                let (neighbor, neighbor_local, nx, ny) = match dir {
                    0 if current_y > min_y => (
                        current - map_width as u32,
                        current_local - bounds_width,
                        current_x,
                        current_y - 1,
                    ),
                    1 if current_y < max_y => (
                        current + map_width as u32,
                        current_local + bounds_width,
                        current_x,
                        current_y + 1,
                    ),
                    2 if current_x > min_x => {
                        (current - 1, current_local - 1, current_x - 1, current_y)
                    }
                    3 if current_x < max_x => {
                        (current + 1, current_local + 1, current_x + 1, current_y)
                    }
                    _ => continue,
                };
                let nl = neighbor_local as usize;
                let neighbor_terrain = map.terrain_byte(neighbor);
                if self.closed_stamp[nl] != stamp
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let (nxf, nyf) = (nx as f64, ny as f64);
                    let dist_to_goal = (nxf - goal_xf).abs() + (nyf - goal_yf).abs();
                    let magnitude = neighbor_terrain & MAGNITUDE_MASK;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g.wrapping_add(cost);
                    if self.g_score_stamp[nl] != stamp || tentative_g < self.g_score[nl] {
                        self.came_from[nl] = current_local as i32;
                        self.g_score[nl] = tentative_g;
                        self.g_score_stamp[nl] = stamp;
                        let h = weight * base * dist_to_goal;
                        let f = tentative_g as f64 + h + cross_tie_breaker(nxf, nyf);
                        self.queue.push(neighbor_local as u32, f);
                    }
                }
            }
        }

        None
    }

    fn build_path(
        &self,
        goal_local: i64,
        to_global: &impl Fn(i64) -> TileRef,
        max_path_length: i64,
    ) -> Vec<TileRef> {
        let mut path = Vec::new();
        let mut current = goal_local;
        let mut iterations = 0;
        while current != -1 && iterations < max_path_length {
            path.push(to_global(current));
            current = self.came_from[current as usize] as i64;
            iterations += 1;
        }
        path.reverse();
        path
    }
}

impl PathFinder for AStarWaterBounded {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        self.find_path_on(ctx.mini_map, from.as_slice(), to)
    }
}

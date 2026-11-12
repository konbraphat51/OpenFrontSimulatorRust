// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/AStar.Water.ts: whole-map water A* with a shore-distance cost and a cross-product tie-breaker.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::PriorityQueue::{MinHeap, PriorityQueue};
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
        10 * COST_SCALE_INT // too close to shore
    } else if magnitude <= 10 {
        0 // sweet spot
    } else {
        COST_SCALE_INT // deep water, slight penalty
    }
}

/// Whole-map water A*; paths run over water tiles (the goal may be land).
#[derive(Debug)]
pub struct AStarWater {
    stamp: u32,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    g_score: Vec<u32>,
    came_from: Vec<i32>,
    queue: MinHeap,
    width: u32,
    num_nodes: u32,
    heuristic_weight: f64,
    max_iterations: i64,
}

impl AStarWater {
    /// Creates a finder sized for `map`; defaults are weight 5 and 1,000,000 iterations.
    pub fn new(
        map: &GameMapImpl,
        heuristic_weight: Option<f64>,
        max_iterations: Option<i64>,
    ) -> Self {
        let num_nodes = map.width() * map.height();
        let n = num_nodes as usize;
        Self {
            stamp: 1,
            closed_stamp: vec![0; n],
            g_score_stamp: vec![0; n],
            g_score: vec![0; n],
            came_from: vec![0; n],
            queue: MinHeap::new(n),
            width: map.width(),
            num_nodes,
            heuristic_weight: heuristic_weight.unwrap_or(5.0),
            max_iterations: max_iterations.unwrap_or(1_000_000),
        }
    }

    /// Path from the first reachable of `starts` to `goal` over `map`'s current terrain.
    pub fn find_path_on(
        &mut self,
        map: &GameMapImpl,
        starts: &[TileRef],
        goal: TileRef,
    ) -> Option<Vec<TileRef>> {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.closed_stamp.fill(0);
            self.g_score_stamp.fill(0);
            self.stamp = 1;
        }

        let stamp = self.stamp;
        let width = self.width;
        let num_nodes = self.num_nodes;
        let weight = self.heuristic_weight;
        let base = BASE_COST as f64;

        let goal_x = (goal % width) as f64;
        let goal_y = (goal / width) as f64;

        self.queue.clear();

        // Cross-product tie-breaker (prefer diagonal paths)
        let s0 = starts[0];
        let start_x = (s0 % width) as f64;
        let start_y = (s0 / width) as f64;
        let dx_goal = goal_x - start_x;
        let dy_goal = goal_y - start_y;
        let cross_norm = f64::max(1.0, dx_goal.abs() + dy_goal.abs());

        let cross_tie_breaker = |nx: f64, ny: f64| -> f64 {
            let dx_n = nx - goal_x;
            let dy_n = ny - goal_y;
            let cross = (dx_goal * dy_n - dy_goal * dx_n).abs();
            ((cross * (COST_SCALE - 1.0)) / cross_norm / cross_norm).floor()
        };

        for &s in starts {
            self.g_score[s as usize] = 0;
            self.g_score_stamp[s as usize] = stamp;
            self.came_from[s as usize] = -1;
            let sx = (s % width) as f64;
            let sy = (s / width) as f64;
            let h = weight * base * ((sx - goal_x).abs() + (sy - goal_y).abs());
            self.queue.push(s, h);
        }

        let mut iterations = self.max_iterations;

        while !self.queue.is_empty() {
            iterations -= 1;
            if iterations <= 0 {
                return None;
            }

            let current = self.queue.pop();
            if self.closed_stamp[current as usize] == stamp {
                continue;
            }
            self.closed_stamp[current as usize] = stamp;

            if current == goal {
                return Some(self.build_path(goal));
            }

            let current_g = self.g_score[current as usize];
            let current_x = current % width;
            let current_y = current / width;

            // North, south, west, east
            for dir in 0..4 {
                let (neighbor, nx, ny) = match dir {
                    0 if current >= width => {
                        (current - width, current_x, current_y.wrapping_sub(1))
                    }
                    1 if current < num_nodes - width => (current + width, current_x, current_y + 1),
                    2 if current_x != 0 => (current - 1, current_x - 1, current_y),
                    3 if current_x != width - 1 => (current + 1, current_x + 1, current_y),
                    _ => continue,
                };
                let neighbor_terrain = map.terrain_byte(neighbor);
                if self.closed_stamp[neighbor as usize] != stamp
                    && (neighbor == goal || (neighbor_terrain & LAND_MASK) == 0)
                {
                    let magnitude = neighbor_terrain & MAGNITUDE_MASK;
                    let cost = BASE_COST + magnitude_penalty(magnitude);
                    let tentative_g = current_g.wrapping_add(cost);
                    if self.g_score_stamp[neighbor as usize] != stamp
                        || tentative_g < self.g_score[neighbor as usize]
                    {
                        self.came_from[neighbor as usize] = current as i32;
                        self.g_score[neighbor as usize] = tentative_g;
                        self.g_score_stamp[neighbor as usize] = stamp;
                        let (nxf, nyf) = (nx as f64, ny as f64);
                        let h = weight * base * ((nxf - goal_x).abs() + (nyf - goal_y).abs());
                        let f = tentative_g as f64 + h + cross_tie_breaker(nxf, nyf);
                        self.queue.push(neighbor, f);
                    }
                }
            }
        }

        None
    }

    fn build_path(&self, goal: TileRef) -> Vec<TileRef> {
        let mut path = Vec::new();
        let mut current = goal as i32;
        while current != -1 {
            path.push(current as TileRef);
            current = self.came_from[current as usize];
        }
        path.reverse();
        path
    }
}

impl PathFinder for AStarWater {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        self.find_path_on(ctx.mini_map, from.as_slice(), to)
    }
}

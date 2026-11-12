// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/AStar.Rail.ts: 4-connected A* for railroads on the mini map.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::AStar::{AStar, AStarAdapter, AStarScratch};
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};
use std::cell::RefCell;

thread_local! {
    static SCRATCH: RefCell<Option<AStarScratch>> = const { RefCell::new(None) };
}

const WATER_PENALTY: f64 = 5.0;
const HEURISTIC_WEIGHT: f64 = 2.0;
const DIRECTION_CHANGE_PENALTY: f64 = 3.0;

/// Rail path finder; it keeps no state, so every query builds its search on the current mini map.
#[derive(Clone, Copy, Debug, Default)]
pub struct AStarRail {
    /// Rails may start on and cross impassable terrain (before PR 4903).
    pub cross_impassable: bool,
}

impl PathFinder for AStarRail {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        let scratch = SCRATCH.with(|s| s.borrow_mut().take());
        let mut a_star = AStar::with_scratch(RailAdapter::new(ctx.mini_map, self.cross_impassable), None, scratch);
        let path = a_star.find_path(from.as_slice(), to);
        SCRATCH.with(|s| *s.borrow_mut() = Some(a_star.into_scratch()));
        path
    }
}

/// Upstream `RailAdapter`.
struct RailAdapter<'a> {
    map: &'a GameMapImpl,
    width: u32,
    num_nodes: u32,
    cross_impassable: bool,
}

impl<'a> RailAdapter<'a> {
    fn new(map: &'a GameMapImpl, cross_impassable: bool) -> Self {
        Self {
            map,
            cross_impassable,
            width: map.width(),
            num_nodes: map.width() * map.height(),
        }
    }

    fn is_traversable(&self, to: TileRef, from_shoreline: bool, is_impassable: bool) -> bool {
        if is_impassable && !self.cross_impassable {
            return false;
        }
        if !self.map.is_water(to) {
            return true;
        }
        from_shoreline || self.map.is_shoreline(to)
    }
}

impl AStarAdapter for RailAdapter<'_> {
    fn num_nodes(&self) -> usize {
        self.num_nodes as usize
    }

    fn max_neighbors(&self) -> usize {
        4
    }

    fn max_priority(&self) -> usize {
        let max_cost = 1.0 + WATER_PENALTY + DIRECTION_CHANGE_PENALTY;
        (HEURISTIC_WEIGHT * (self.width + self.map.height()) as f64 * max_cost) as usize
    }

    fn neighbors(&self, node: u32, buffer: &mut [u32]) -> usize {
        let mut count = 0;
        let x = node % self.width;
        let from_shoreline = self.map.is_shoreline(node);
        let is_impassable = self.map.is_impassable(node);

        if node >= self.width {
            let n = node - self.width;
            if self.is_traversable(n, from_shoreline, is_impassable) {
                buffer[count] = n;
                count += 1;
            }
        }
        if node < self.num_nodes - self.width {
            let n = node + self.width;
            if self.is_traversable(n, from_shoreline, is_impassable) {
                buffer[count] = n;
                count += 1;
            }
        }
        if x != 0 {
            let n = node - 1;
            if self.is_traversable(n, from_shoreline, is_impassable) {
                buffer[count] = n;
                count += 1;
            }
        }
        if x != self.width - 1 {
            let n = node + 1;
            if self.is_traversable(n, from_shoreline, is_impassable) {
                buffer[count] = n;
                count += 1;
            }
        }
        count
    }

    fn cost(&self, from: u32, to: u32, prev: Option<u32>) -> f64 {
        let penalized = self.map.is_water(to) || self.map.is_shoreline(to);
        let mut c = if penalized { 1.0 + WATER_PENALTY } else { 1.0 };

        if let Some(prev) = prev {
            let d1 = from as i64 - prev as i64;
            let d2 = to as i64 - from as i64;
            if d1 != d2 {
                c += DIRECTION_CHANGE_PENALTY;
            }
        }
        c
    }

    fn heuristic(&self, node: u32, goal: u32) -> f64 {
        let nx = (node % self.width) as f64;
        let ny = (node / self.width) as f64;
        let gx = (goal % self.width) as f64;
        let gy = (goal / self.width) as f64;
        HEURISTIC_WEIGHT * ((nx - gx).abs() + (ny - gy).abs())
    }
}

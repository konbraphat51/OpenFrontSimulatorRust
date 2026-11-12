// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/spatial/SpatialQuery.ts: closest shore queries (land BFS, reachable water, water path).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Keep one `SpatialQuery` per game: it owns the traversal scratch, the bounded A* scratch and the
//! per-player reachable-water cache (upstream keeps the cache in a module-level `WeakMap`).

use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Algorithms::AStarWaterBounded::{AStarWaterBounded, SearchBounds};
use crate::Pathfinding::PathGameView::{PathGameView, SmallId};
use crate::Pathfinding::Types::Source;
use std::collections::{HashMap, HashSet};

const REFINE_MAX_SEARCH_AREA: i64 = 100 * 100;

/// Default `maxDist` of the shore queries.
pub const DEFAULT_MAX_DIST: f64 = 50.0;

/// Water components touching a player's shoreline, valid while its tiles and the map's water are unchanged.
struct ReachableCache {
    tile_version: u32,
    water_version: u32,
    components: HashSet<u32>,
}

/// Closest shore queries for transport ships and landings.
#[derive(Default)]
pub struct SpatialQuery {
    bounded_a_star: Option<AStarWaterBounded>,
    visited: Vec<u32>,
    gen_counter: u32,
    stack: Vec<TileRef>,
    reachable: HashMap<SmallId, ReachableCache>,
    simple_start: bool,
}

impl SpatialQuery {
    /// Builds before PR 2932 take the first tile of the shore path without refining it.
    pub fn set_simple_start(&mut self, simple_start: bool) {
        self.simple_start = simple_start;
    }

    /// Creates an empty query helper.
    pub fn new() -> Self {
        Self::default()
    }

    /// Nearest tile matching `predicate` by LIFO traversal (N, S, W, E) within Manhattan distance `max_dist`.
    ///
    /// Strict `<` keeps the first candidate at the minimum distance, so the winner depends only on the traversal order.
    fn bfs_nearest(
        &mut self,
        map: &GameMapImpl,
        from: TileRef,
        max_dist: f64,
        mut predicate: impl FnMut(TileRef) -> bool,
    ) -> Option<TileRef> {
        if !map.is_valid_ref(from) {
            return None;
        }
        let total = map.width() as usize * map.height() as usize;
        if self.visited.len() < total {
            self.visited = vec![0; total];
            self.gen_counter = 0;
        }
        self.gen_counter += 1;
        if self.gen_counter == 0xffff_ffff {
            self.visited.fill(0);
            self.gen_counter = 1;
        }
        let gen_stamp = self.gen_counter;
        let visited = &mut self.visited;
        let stack = &mut self.stack;
        stack.clear();

        let mut best: Option<TileRef> = None;
        let mut best_dist = f64::INFINITY;

        let mut mark = |t: TileRef, visited: &mut Vec<u32>, stack: &mut Vec<TileRef>| {
            visited[t as usize] = gen_stamp;
            stack.push(t);
            if predicate(t) {
                let dist = map.manhattan_dist(from, t) as f64;
                if dist < best_dist {
                    best = Some(t);
                    best_dist = dist;
                }
            }
        };

        if max_dist >= 0.0 {
            mark(from, visited, stack);
        }
        while let Some(curr) = stack.pop() {
            map.for_each_neighbor_nswe(curr, |n| {
                if visited[n as usize] != gen_stamp
                    && map.manhattan_dist(from, n) as f64 <= max_dist
                {
                    mark(n, visited, stack);
                }
            });
        }

        best
    }

    /// Closest shore tile owned by `owner` (a player or terra nullius, id 0) by land BFS.
    pub fn closest_shore(
        &mut self,
        game: &impl PathGameView,
        owner: SmallId,
        tile: TileRef,
        max_dist: f64,
    ) -> Option<TileRef> {
        let map = game.map();
        let owner_id = owner;
        self.bfs_nearest(map, tile, max_dist, |t| {
            if !map.is_shore(t) || !map.is_land(t) {
                return false;
            }
            map.owner_id(t) == owner_id
        })
    }

    /// Closest shore owned by `target_owner` near `tile` on a water component reachable from `attacker`'s shoreline.
    ///
    /// Skips shores that only border a disconnected water body (an inland lake).
    pub fn closest_reachable_shore(
        &mut self,
        game: &impl PathGameView,
        target_owner: SmallId,
        attacker: SmallId,
        tile: TileRef,
        max_dist: f64,
    ) -> Option<TileRef> {
        let map = game.map();
        let target_id = target_owner;

        // Water components adjacent to the attacker's own shoreline
        self.refresh_reachable_components(game, attacker);
        let reachable = &self.reachable[&attacker].components;
        if reachable.is_empty() {
            return None;
        }

        let is_valid_tile = |t: TileRef| -> bool {
            if !map.is_shore(t) || !map.is_land(t) {
                return false;
            }
            if map.owner_id(t) != target_id {
                return false;
            }
            matches!(game.water_component(t), Some(component) if reachable.contains(&component))
        };

        // The start tile is at distance 0: when it qualifies no BFS can beat it
        if max_dist >= 0.0 && map.is_valid_ref(tile) && is_valid_tile(tile) {
            return Some(tile);
        }

        // The BFS needs `&mut self` scratch while `reachable` borrows the cache: copy the set out
        let reachable = reachable.clone();
        self.bfs_nearest(map, tile, max_dist, |t| {
            if !map.is_shore(t) || !map.is_land(t) || map.owner_id(t) != target_id {
                return false;
            }
            matches!(game.water_component(t), Some(component) if reachable.contains(&component))
        })
    }

    fn refresh_reachable_components(&mut self, game: &impl PathGameView, attacker: SmallId) {
        let map = game.map();
        let tile_version = game.tile_change_version(attacker);
        let water_version = map.water_version();
        if let Some(cached) = self.reachable.get(&attacker)
            && cached.tile_version == tile_version
            && cached.water_version == water_version
        {
            return;
        }
        let mut components: HashSet<u32> = HashSet::new();
        game.for_each_border_tile(attacker, &mut |t| {
            if !map.is_shore(t) || !map.is_land(t) {
                return;
            }
            if let Some(component) = game.water_component(t) {
                components.insert(component);
            }
        });
        self.reachable.insert(
            attacker,
            ReachableCache {
                tile_version,
                water_version,
                components,
            },
        );
    }

    /// Closest shore tile of `owner` by water pathfinding to `target`; `None` for terra nullius.
    pub fn closest_shore_by_water(
        &mut self,
        game: &mut impl PathGameView,
        owner: SmallId,
        target: TileRef,
    ) -> Option<TileRef> {
        if owner == 0 {
            return None;
        }

        // Target must be water or shore (land adjacent to water)
        {
            let map = game.map();
            if !map.is_water(target) && !map.is_shore(target) {
                return None;
            }
        }

        let target_component = game.water_component(target)?;

        // Single pass over the border set
        let mut shores: Vec<TileRef> = Vec::new();
        {
            let view = &*game;
            view.for_each_border_tile(owner, &mut |t| {
                let map = view.map();
                if !map.is_shore(t) || !map.is_land(t) {
                    return;
                }
                if view.water_component(t) == Some(target_component) {
                    shores.push(t);
                }
            });
        }
        if shores.is_empty() {
            return None;
        }

        let path = game.find_water_path(Source::Multi(&shores), target)?;
        if path.is_empty() {
            return None;
        }

        if self.simple_start {
            return path.first().copied();
        }
        Some(self.refine_start_tile(game.map(), &path, &shores))
    }

    fn refine_start_tile(
        &mut self,
        map: &GameMapImpl,
        path: &[TileRef],
        shores: &[TileRef],
    ) -> TileRef {
        const CANDIDATE_RADIUS: u32 = 20;
        const MIN_WAYPOINT_DIST: usize = 50;
        const MAX_WAYPOINT_DIST: usize = 200;
        const PADDING: i64 = 10;

        if path.len() <= MIN_WAYPOINT_DIST {
            return path[0];
        }

        let best_tile = path[0];
        let candidates: Vec<TileRef> = shores
            .iter()
            .copied()
            .filter(|&s| map.manhattan_dist(s, best_tile) <= CANDIDATE_RADIUS)
            .collect();

        if candidates.len() <= 1 {
            return best_tile;
        }

        // Precompute candidate bounds
        let mut cand_min_x = map.x(candidates[0]) as i64;
        let mut cand_max_x = cand_min_x;
        let mut cand_min_y = map.y(candidates[0]) as i64;
        let mut cand_max_y = cand_min_y;
        for &c in &candidates[1..] {
            let sx = map.x(c) as i64;
            let sy = map.y(c) as i64;
            cand_min_x = cand_min_x.min(sx);
            cand_max_x = cand_max_x.max(sx);
            cand_min_y = cand_min_y.min(sy);
            cand_max_y = cand_max_y.max(sy);
        }

        // Binary search for the furthest waypoint that keeps the bounds within the limit
        let mut lo = MIN_WAYPOINT_DIST as i64;
        let mut hi = (MAX_WAYPOINT_DIST as i64).min(path.len() as i64 - 1);
        let mut best_waypoint_idx = lo;

        let mut i = 0;
        while i < 5 && lo <= hi {
            let mid = (lo + hi) >> 1;
            let wp = path[mid as usize];
            let wp_x = map.x(wp) as i64;
            let wp_y = map.y(wp) as i64;

            let min_x = cand_min_x.min(wp_x) - PADDING;
            let max_x = cand_max_x.max(wp_x) + PADDING;
            let min_y = cand_min_y.min(wp_y) - PADDING;
            let max_y = cand_max_y.max(wp_y) + PADDING;

            let area = (max_x - min_x + 1) * (max_y - min_y + 1);
            if area <= REFINE_MAX_SEARCH_AREA {
                best_waypoint_idx = mid;
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
            i += 1;
        }

        let waypoint = path[best_waypoint_idx as usize];
        let wp_x = map.x(waypoint) as i64;
        let wp_y = map.y(waypoint) as i64;

        let bounds = SearchBounds {
            min_x: 0.max(cand_min_x.min(wp_x) - PADDING),
            max_x: (map.width() as i64 - 1).min(cand_max_x.max(wp_x) + PADDING),
            min_y: 0.max(cand_min_y.min(wp_y) - PADDING),
            max_y: (map.height() as i64 - 1).min(cand_max_y.max(wp_y) + PADDING),
        };

        let bounds_area = (bounds.max_x - bounds.min_x + 1) * (bounds.max_y - bounds.min_y + 1);
        if bounds_area > REFINE_MAX_SEARCH_AREA {
            return best_tile;
        }

        let a_star = self.bounded_a_star.get_or_insert_with(|| {
            AStarWaterBounded::new(map, REFINE_MAX_SEARCH_AREA as usize, None, None)
        });
        let refined = a_star.search_bounded(map, &candidates, waypoint, bounds);
        refined
            .and_then(|p| p.first().copied())
            .unwrap_or(best_tile)
    }
}

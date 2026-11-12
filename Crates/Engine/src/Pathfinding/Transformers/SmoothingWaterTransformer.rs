// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/transformers/SmoothingWaterTransformer.ts: line-of-sight smoothing and local A* refinement of water paths.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Algorithms::AStarWaterBounded::{AStarWaterBounded, SearchBounds};
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};

const ENDPOINT_REFINEMENT_TILES: i64 = 50;
const LOCAL_ASTAR_MAX_AREA: usize = 100 * 100;
const LOS_MIN_MAGNITUDE_PASS1: u8 = 2;
const LOS_MIN_MAGNITUDE_PASS2: u8 = 3;
const MAGNITUDE_MASK: u8 = 0x1f;
const MAX_TILES: i64 = 100_000;

/// Water path smoother: LOS smoothing, endpoint refinement by local A*, LOS smoothing again (on `ctx.mini_map`).
pub struct SmoothingWaterTransformer<P> {
    inner: P,
    local_a_star: AStarWaterBounded,
    single_magnitude: bool,
    final_pass: bool,
}

impl<P> SmoothingWaterTransformer<P> {
    /// Wraps `inner`; `map` only sizes the scratch (the mini map).
    pub fn new(inner: P, map: &GameMapImpl) -> Self {
        Self {
            inner,
            local_a_star: AStarWaterBounded::new(map, LOCAL_ASTAR_MAX_AREA, None, None),
            single_magnitude: false,
            final_pass: true,
        }
    }

    /// Builds before PR 2932 stop after the endpoint refinement; `bucket_queue` also queues its local search that way.
    pub fn with_legacy_passes(mut self, legacy: bool) -> Self {
        if legacy {
            self.final_pass = false;
            self.local_a_star.use_bucket_queue();
        }
        self
    }

    /// Builds before PR 2959 smooth both passes with the same minimum magnitude (3).
    pub fn with_single_magnitude(mut self, single_magnitude: bool) -> Self {
        self.single_magnitude = single_magnitude;
        self
    }

    /// The wrapped finder.
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }

    fn smooth(&mut self, map: &GameMapImpl, path: Vec<TileRef>) -> Vec<TileRef> {
        if path.len() <= 2 {
            return path;
        }

        // Pass 1: LOS smoothing with binary search
        let pass1 = if self.single_magnitude {
            LOS_MIN_MAGNITUDE_PASS2
        } else {
            LOS_MIN_MAGNITUDE_PASS1
        };
        let mut smoothed = los_smooth(map, &path, pass1);
        // Pass 2: local A* refinement on the endpoints
        smoothed = self.refine_endpoints(map, smoothed);
        // Pass 3: LOS smoothing again, farther from the shore
        if !self.final_pass {
            return smoothed;
        }
        los_smooth(map, &smoothed, LOS_MIN_MAGNITUDE_PASS2)
    }

    fn refine_endpoints(&mut self, map: &GameMapImpl, path: Vec<TileRef>) -> Vec<TileRef> {
        if path.len() <= 2 {
            return path;
        }

        let refine_dist = ENDPOINT_REFINEMENT_TILES;
        let mut result = path.clone();

        // Index where the cumulative distance from the start reaches `refine_dist`
        let start_end_idx = find_tile_at_distance(map, &path, 0, refine_dist, true);

        if start_end_idx > 1
            && let Some(start_segment) = self.refine_segment(map, path[0], path[start_end_idx])
            && !start_segment.is_empty()
        {
            let mut joined: Vec<TileRef> = start_segment[..start_segment.len() - 1].to_vec();
            joined.extend_from_slice(&result[start_end_idx..]);
            result = joined;
        }

        let end_start_idx =
            find_tile_at_distance(map, &result, result.len() - 1, refine_dist, false);

        // Search in reverse (from the destination) so the path approaches the target naturally
        if (end_start_idx as i64) < result.len() as i64 - 2
            && let Some(mut end_segment) =
                self.refine_segment(map, result[result.len() - 1], result[end_start_idx])
            && !end_segment.is_empty()
        {
            end_segment.reverse();
            let mut joined: Vec<TileRef> = result[..end_start_idx].to_vec();
            joined.extend_from_slice(&end_segment);
            result = joined;
        }

        result
    }

    fn refine_segment(
        &mut self,
        map: &GameMapImpl,
        from: TileRef,
        to: TileRef,
    ) -> Option<Vec<TileRef>> {
        let x0 = map.x(from) as i64;
        let y0 = map.y(from) as i64;
        let x1 = map.x(to) as i64;
        let y1 = map.y(to) as i64;

        let padding = 10;
        let bounds = SearchBounds {
            min_x: 0.max(x0.min(x1) - padding),
            max_x: (map.width() as i64 - 1).min(x0.max(x1) + padding),
            min_y: 0.max(y0.min(y1) - padding),
            max_y: (map.height() as i64 - 1).min(y0.max(y1) + padding),
        };
        self.local_a_star.search_bounded(map, &[from], to, bounds)
    }
}

impl<P: PathFinder> PathFinder for SmoothingWaterTransformer<P> {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        let path = self.inner.find_path(ctx, from, to)?;
        Some(self.smooth(ctx.mini_map, path))
    }
}

fn los_smooth(map: &GameMapImpl, path: &[TileRef], min_magnitude: u8) -> Vec<TileRef> {
    let mut result = vec![path[0]];
    let mut current = 0usize;

    while current < path.len() - 1 {
        // Binary search for the farthest visible waypoint
        let mut lo = current + 1;
        let mut hi = path.len() - 1;
        let mut farthest = lo;

        while lo <= hi {
            let mid = (lo + hi) >> 1;
            if can_see(map, path[current], path[mid], min_magnitude) {
                farthest = mid;
                lo = mid + 1;
            } else {
                hi = mid - 1; // mid >= 1, so no underflow
            }
        }

        // Trace the line to the farthest visible point
        if farthest > current + 1
            && let Some(trace) = trace_path(map, path[current], path[farthest])
        {
            // Intermediate tiles only; the endpoint is added by the next round or at the end
            for &tile in trace.iter().take(trace.len().saturating_sub(1)).skip(1) {
                result.push(tile);
            }
        }

        current = farthest;
        if current < path.len() - 1 {
            result.push(path[current]);
        }
    }

    result.push(path[path.len() - 1]);
    result
}

fn find_tile_at_distance(
    map: &GameMapImpl,
    path: &[TileRef],
    start_idx: usize,
    distance: i64,
    forward: bool,
) -> usize {
    let mut cum_dist: i64 = 0;
    let mut idx = start_idx;

    if forward {
        while idx < path.len() - 1 && cum_dist < distance {
            cum_dist += map.manhattan_dist(path[idx], path[idx + 1]) as i64;
            idx += 1;
        }
    } else {
        while idx > 0 && cum_dist < distance {
            cum_dist += map.manhattan_dist(path[idx], path[idx - 1]) as i64;
            idx -= 1;
        }
    }
    idx
}

/// True when the Bresenham line between two tiles stays on water at or above `min_magnitude`.
fn can_see(map: &GameMapImpl, from: TileRef, to: TileRef, min_magnitude: u8) -> bool {
    let w = map.width() as i64;
    let x0 = from as i64 % w;
    let y0 = from as i64 / w;
    let x1 = to as i64 % w;
    let y1 = to as i64 / w;

    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    let (mut x, mut y) = (x0, y0);
    let mut iterations = 0;

    // Traversable and deep enough
    let ok = |tile: i64| -> bool {
        let tile = tile as TileRef;
        map.is_water(tile) && (map.terrain_byte(tile) & MAGNITUDE_MASK) >= min_magnitude
    };

    loop {
        let over = iterations > MAX_TILES;
        iterations += 1;
        if over {
            return false;
        }

        let tile = y * w + x;
        if !map.is_water(tile as TileRef) {
            return false;
        }
        // Avoid shallow water
        if (map.terrain_byte(tile as TileRef) & MAGNITUDE_MASK) < min_magnitude {
            return false;
        }

        if x == x1 && y == y1 {
            return true;
        }

        let e2 = 2 * err;
        let should_move_x = e2 > -dy;
        let should_move_y = e2 < dx;

        if should_move_x && should_move_y {
            // Diagonal move: check the intermediate tile
            x += sx;
            err -= dy;

            let intermediate = y * w + x;
            if !ok(intermediate) {
                // Try the alternative route
                x -= sx;
                err += dy;
                y += sy;
                err += dx;

                let alt = y * w + x;
                if !ok(alt) {
                    return false;
                }

                x += sx;
                err -= dy;
            } else {
                y += sy;
                err += dx;
            }
        } else {
            if should_move_x {
                x += sx;
                err -= dy;
            }
            if should_move_y {
                y += sy;
                err += dx;
            }
        }
    }
}

/// The tiles of the Bresenham line between two tiles, or `None` if it leaves the water.
fn trace_path(map: &GameMapImpl, from: TileRef, to: TileRef) -> Option<Vec<TileRef>> {
    let w = map.width() as i64;
    let x0 = from as i64 % w;
    let y0 = from as i64 / w;
    let x1 = to as i64 % w;
    let y1 = to as i64 / w;

    let mut tiles: Vec<TileRef> = Vec::new();

    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    let (mut x, mut y) = (x0, y0);
    let mut iterations = 0;

    loop {
        let over = iterations > MAX_TILES;
        iterations += 1;
        if over {
            return None;
        }

        let tile = (y * w + x) as TileRef;
        if !map.is_water(tile) {
            return None;
        }
        tiles.push(tile);

        if x == x1 && y == y1 {
            break;
        }

        let e2 = 2 * err;
        let should_move_x = e2 > -dy;
        let should_move_y = e2 < dx;

        if should_move_x && should_move_y {
            x += sx;
            err -= dy;

            let intermediate = (y * w + x) as TileRef;
            if !map.is_water(intermediate) {
                x -= sx;
                err += dy;
                y += sy;
                err += dx;

                let alt = (y * w + x) as TileRef;
                if !map.is_water(alt) {
                    return None;
                }
                tiles.push(alt);

                x += sx;
                err -= dy;
            } else {
                tiles.push(intermediate);
                y += sy;
                err += dx;
            }
        } else {
            if should_move_x {
                x += sx;
                err -= dy;
            }
            if should_move_y {
                y += sy;
                err += dx;
            }
        }
    }

    Some(tiles)
}

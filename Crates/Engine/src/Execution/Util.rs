// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/Util.ts: spawn tiles and tile-set distance helpers.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! `isTileSetLike`'s plain-iterable fallback of `nearestTileDistCapped` is test-only upstream and
//! not ported (callers pass a [`TileSet`]).

use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::{GameMapImpl, TileRef, eucl_dist_fn};
use crate::Game::TileSet::TileSet;

/// `getSpawnTiles`: tiles of the radius-4 disc around `tile`; with `require_all_valid` the result
/// is `None` when any of them is owned, water or impassable, else only the valid ones are kept.
pub fn get_spawn_tiles(
    gm: &GameMapImpl,
    tile: TileRef,
    require_all_valid: bool,
) -> Option<Vec<TileRef>> {
    let filter = eucl_dist_fn(tile, 4.0, true);
    let spawn_tiles: Vec<TileRef> = gm.bfs(tile, |g, t| filter(g, t)).iter().copied().collect();
    let is_invalid = |t: TileRef| gm.has_owner(t) || !gm.is_land(t) || gm.is_impassable(t);
    if !require_all_valid {
        return Some(
            spawn_tiles
                .into_iter()
                .filter(|&t| !is_invalid(t))
                .collect(),
        );
    }
    if spawn_tiles.iter().any(|&t| is_invalid(t)) {
        return None;
    }
    Some(spawn_tiles)
}

/// `closestTile`: nearest member of `refs` by Manhattan distance (first of equals) and its distance.
pub fn closest_tile(
    gm: &GameMapImpl,
    refs: impl IntoIterator<Item = TileRef>,
    tile: TileRef,
) -> (Option<TileRef>, f64) {
    let mut min_distance = f64::INFINITY;
    let mut min_ref = None;
    for r in refs {
        let distance = gm.manhattan_dist(r, tile) as f64;
        if distance < min_distance {
            min_distance = distance;
            min_ref = Some(r);
        }
    }
    (min_ref, min_distance)
}

/// `nearestTileDist`: Manhattan distance to the nearest tile, infinity when empty.
pub fn nearest_tile_dist(
    gm: &GameMapImpl,
    tiles: impl IntoIterator<Item = TileRef>,
    tile: TileRef,
) -> f64 {
    let mut best = f64::INFINITY;
    for t in tiles {
        let d = gm.manhattan_dist(t, tile) as f64;
        if d < best {
            best = d;
        }
    }
    best
}

/// `nearestTileDistCapped`: nearest distance when at most `cap`, otherwise infinity.
pub fn nearest_tile_dist_capped(gm: &GameMapImpl, tiles: &TileSet, tile: TileRef, cap: i64) -> f64 {
    if tiles.is_empty() {
        return f64::INFINITY;
    }
    if tiles.has(tile) {
        return 0.0;
    }
    let w = gm.width() as i64;
    let h = gm.height() as i64;
    let cx = gm.x(tile) as i64;
    let cy = gm.y(tile) as i64;
    for d in 1..=cap {
        for dx in -d..=d {
            let x = cx + dx;
            if x < 0 || x >= w {
                continue;
            }
            let dy = d - dx.abs();
            let y1 = cy - dy;
            if y1 >= 0 && tiles.has((y1 * w + x) as TileRef) {
                return d as f64;
            }
            if dy != 0 {
                let y2 = cy + dy;
                if y2 < h && tiles.has((y2 * w + x) as TileRef) {
                    return d as f64;
                }
            }
        }
    }
    f64::INFINITY
}

/// `closestTwoTiles`: the closest pair between two tile collections by a sweep over x.
#[allow(clippy::if_same_then_else)]
pub fn closest_two_tiles(
    gm: &GameMapImpl,
    x: impl IntoIterator<Item = TileRef>,
    y: impl IntoIterator<Item = TileRef>,
) -> Option<(TileRef, TileRef)> {
    let w = gm.width();
    let mut x_sorted: Vec<TileRef> = x.into_iter().collect();
    let mut y_sorted: Vec<TileRef> = y.into_iter().collect();
    x_sorted.sort_by_key(|&a| a % w);
    y_sorted.sort_by_key(|&a| a % w);
    if x_sorted.is_empty() || y_sorted.is_empty() {
        return None;
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut min_distance = f64::INFINITY;
    let mut result = (x_sorted[0], y_sorted[0]);
    while i < x_sorted.len() && j < y_sorted.len() {
        let current_x = x_sorted[i];
        let current_y = y_sorted[j];
        let cx_x = (current_x % w) as i64;
        let cy_x = (current_y % w) as i64;
        let distance =
            ((cx_x - cy_x).abs() + ((current_x / w) as i64 - (current_y / w) as i64).abs()) as f64;
        if distance < min_distance {
            min_distance = distance;
            result = (current_x, current_y);
        }
        if i == x_sorted.len() - 1 {
            j += 1;
        } else if j == y_sorted.len() - 1 {
            i += 1;
        } else if cx_x < cy_x {
            i += 1;
        } else {
            j += 1;
        }
    }
    Some(result)
}

/// `calculateTerritoryCenter`: bounding-box center of the border, or the nearest border tile when
/// the center is not owned by `target`.
pub fn calculate_territory_center(game: &GameImpl, target: SmallId) -> Option<TileRef> {
    let border = game.player(target);
    let border_tiles = border.border_tiles();
    if border_tiles.is_empty() {
        return None;
    }
    let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
    for tile in border_tiles.iter() {
        let x = game.x(tile) as f64;
        let y = game.y(tile) as f64;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let center_x = ((min_x + max_x) / 2.0).floor();
    let center_y = ((min_y + max_y) / 2.0).floor();
    let center_tile = game.tile_ref(center_x as i32, center_y as i32);
    if game.owner_id(center_tile) == target {
        return Some(center_tile);
    }
    let mut closest: Option<TileRef> = None;
    let mut closest_d = f64::INFINITY;
    for tile in border_tiles.iter() {
        let dx = game.x(tile) as f64 - center_x;
        let dy = game.y(tile) as f64 - center_y;
        let d = dx * dx + dy * dy;
        if d < closest_d {
            closest_d = d;
            closest = Some(tile);
        }
    }
    closest
}

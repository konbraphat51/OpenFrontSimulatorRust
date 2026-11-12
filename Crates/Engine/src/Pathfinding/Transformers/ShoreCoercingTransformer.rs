// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/transformers/ShoreCoercingTransformer.ts: coerces shore endpoints to adjacent water and restores them afterwards.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};
use std::collections::HashMap;

/// Wraps a finder so land (shore) endpoints path through their best water neighbour (on `ctx.mini_map`).
pub struct ShoreCoercingTransformer<P> {
    inner: P,
}

impl<P> ShoreCoercingTransformer<P> {
    /// Wraps `inner`.
    pub fn new(inner: P) -> Self {
        Self { inner }
    }

    /// The wrapped finder.
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }
}

impl<P: PathFinder> PathFinder for ShoreCoercingTransformer<P> {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        let map = ctx.mini_map;
        let mut water_to_original: HashMap<TileRef, TileRef> = HashMap::new();
        let mut water_from: Vec<TileRef> = Vec::new();

        for &f in from.as_slice() {
            if map.is_water(f) {
                water_from.push(f);
                // A raw water source overrides any earlier shore tile coerced to the same water tile
                water_to_original.remove(&f);
            } else if let Some(water) = best_water_neighbor(map, f) {
                water_from.push(water);
                water_to_original.insert(water, f);
            }
        }

        if water_from.is_empty() {
            return None;
        }

        // Coerce the destination; the original is appended afterwards
        let mut water_to = to;
        let mut original_to: Option<TileRef> = None;
        if !map.is_water(to) {
            water_to = best_water_neighbor(map, to)?;
            original_to = Some(to);
        }

        let mut path = if water_from.len() == 1 {
            self.inner
                .find_path(ctx, Source::Single(water_from[0]), water_to)
        } else {
            self.inner
                .find_path(ctx, Source::Multi(&water_from), water_to)
        }?;
        if path.is_empty() {
            return None;
        }

        // Restore the original start shore tile
        if let Some(&original_shore) = water_to_original.get(&path[0]) {
            path.insert(0, original_shore);
        }

        // Append the original destination if different
        if let Some(original) = original_to
            && path[path.len() - 1] != original
        {
            path.push(original);
        }

        Some(path)
    }
}

/// Adjacent water neighbour with the most water neighbours (first wins ties).
fn best_water_neighbor(map: &GameMapImpl, tile: TileRef) -> Option<TileRef> {
    let mut best = None;
    let mut max_score: i32 = -1;

    let mut nbuf = [0; 4];
    let n = map.neighbors4_nswe(tile, &mut nbuf);
    for &neighbor in &nbuf[..n] {
        if !map.is_water(neighbor) {
            continue;
        }
        let score = count_water_neighbors(map, neighbor);
        if score > max_score {
            max_score = score;
            best = Some(neighbor);
        }
    }
    best
}

fn count_water_neighbors(map: &GameMapImpl, tile: TileRef) -> i32 {
    let mut nbuf = [0; 4];
    let n = map.neighbors4_nswe(tile, &mut nbuf);
    nbuf[..n].iter().filter(|&&t| map.is_water(t)).count() as i32
}

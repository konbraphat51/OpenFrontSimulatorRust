// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/PathFinder.Air.ts: the randomised staircase path of shells and SAM missiles.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Game::GameMap::{GameMapImpl, TileRef};

/// Straight-ish air path; every query reseeds from the tick the finder was created on.
#[derive(Clone, Copy, Debug)]
pub struct AirPathFinder {
    seed: f64,
}

impl AirPathFinder {
    /// Creates a finder seeded with the current tick (`game.ticks()`).
    pub fn new(ticks: u32) -> Self {
        Self { seed: ticks as f64 }
    }

    /// Path from `from` to `to`, both included.
    pub fn find_path(&self, map: &GameMapImpl, from: TileRef, to: TileRef) -> Vec<TileRef> {
        let mut random = PseudoRandom::new(self.seed);
        let mut path = vec![from];
        let mut current = from;

        while current != to {
            let next = compute_next(map, current, to, &mut random);
            if next == current {
                break; // prevent an infinite loop if something breaks
            }
            current = next;
            path.push(current);
        }
        path
    }
}

fn compute_next(
    map: &GameMapImpl,
    from: TileRef,
    to: TileRef,
    random: &mut PseudoRandom,
) -> TileRef {
    let x = map.x(from) as i64;
    let y = map.y(from) as i64;
    let dst_x = map.x(to) as i64;
    let dst_y = map.y(to) as i64;

    if x == dst_x && y == dst_y {
        return to;
    }

    let mut next_x = x;
    let mut next_y = y;
    let ratio = (1.0 + (dst_y - y).abs() as f64 / ((dst_x - x).abs() + 1) as f64).floor();

    if x == dst_x {
        // Can only move in Y
        next_y += if y < dst_y { 1 } else { -1 };
    } else if y == dst_y {
        // Can only move in X
        next_x += if x < dst_x { 1 } else { -1 };
    } else if random.chance(ratio) {
        next_x += if x < dst_x { 1 } else { -1 };
    } else {
        next_y += if y < dst_y { 1 } else { -1 };
    }

    map.tile_ref(next_x as i32, next_y as i32)
}

// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/Railroad.ts: a railroad between two stations and its oriented view.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Stations and railroads are arena entries of the `RailNetwork`; they refer to each other by id.
//! `Railroad.delete` only emits a client update besides unlinking, so the unlinking is done by the network.

use super::GameMap::{GameMapImpl, TileRef};

/// Index of a station in the rail network (upstream `TrainStation.id`, starts at 1).
pub type StationId = u32;
/// Id of a railroad (upstream `Railroad.id`, sequential per network from 0; also the arena index).
pub type RailId = u32;

/// A railroad between two stations; `tiles` run from `from` to `to`.
#[derive(Clone, Debug)]
pub struct Railroad {
    /// First station.
    pub from: StationId,
    /// Second station.
    pub to: StationId,
    /// Tiles from `from` to `to`.
    pub tiles: Vec<TileRef>,
    /// Network-wide id.
    pub id: RailId,
}

impl Railroad {
    /// `getClosestTileIndex`: index of the tile nearest to `to` (first one on ties), or -1 when empty.
    pub fn get_closest_tile_index(&self, map: &GameMapImpl, to: TileRef) -> i64 {
        if self.tiles.is_empty() {
            return -1;
        }
        let to_x = map.x(to) as f64;
        let to_y = map.y(to) as f64;
        let mut closest_index = 0;
        let mut min_dist_squared = f64::INFINITY;
        for (i, &tile) in self.tiles.iter().enumerate() {
            let dx = map.x(tile) as f64 - to_x;
            let dy = map.y(tile) as f64 - to_y;
            let dist_squared = dx * dx + dy * dy;
            if dist_squared < min_dist_squared {
                min_dist_squared = dist_squared;
                closest_index = i;
            }
        }
        closest_index as i64
    }
}

/// `OrientedRailroad`: a railroad walked in one direction, so it always starts at `tiles[0]`.
#[derive(Clone, Debug)]
pub struct OrientedRailroad {
    tiles: Vec<TileRef>,
    start: StationId,
    end: StationId,
}

impl OrientedRailroad {
    /// Wraps `railroad`; `forward` walks it from `from` to `to`.
    pub fn new(railroad: &Railroad, forward: bool) -> Self {
        if forward {
            Self {
                tiles: railroad.tiles.clone(),
                start: railroad.from,
                end: railroad.to,
            }
        } else {
            Self {
                tiles: railroad.tiles.iter().rev().copied().collect(),
                start: railroad.to,
                end: railroad.from,
            }
        }
    }

    /// `getTiles`.
    pub fn tiles(&self) -> &[TileRef] {
        &self.tiles
    }

    /// `getStart`.
    pub fn start(&self) -> StationId {
        self.start
    }

    /// `getEnd`.
    pub fn end(&self) -> StationId {
        self.end
    }
}

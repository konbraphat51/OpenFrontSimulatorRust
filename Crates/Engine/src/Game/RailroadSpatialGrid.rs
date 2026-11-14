// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/RailroadSpatialGrid.ts: a coarse grid to find railroads near a tile.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::GameMap::{GameMapImpl, TileRef};
use super::Railroad::{RailId, Railroad};
use crate::Compat::OrderedSet::OrderedSet;
use std::collections::HashMap;

type Cell = (i64, i64);

/// Grid of `cell_size` x `cell_size` tiles mapping cells to the railroads crossing them.
#[derive(Debug, Default)]
pub struct RailSpatialGrid {
    cells: HashMap<Cell, OrderedSet<RailId>>,
    rail_to_cells: HashMap<RailId, Vec<Cell>>,
    cell_size: f64,
}

impl RailSpatialGrid {
    /// Creates a grid with the given cell size (must be positive).
    pub fn new(cell_size: f64) -> Self {
        assert!(cell_size > 0.0, "cellSize must be > 0");
        Self {
            cells: HashMap::new(),
            rail_to_cells: HashMap::new(),
            cell_size,
        }
    }

    /// `register`: indexes every cell the railroad's tiles touch.
    pub fn register(&mut self, map: &GameMapImpl, rail: &Railroad) {
        // Defensive: avoid double-registration but it should never happen
        self.unregister(rail.id);

        let mut rail_cells: Vec<Cell> = Vec::new();
        for &tile in &rail.tiles {
            let k = self.cell_of(map.x(tile) as f64, map.y(tile) as f64);
            if rail_cells.contains(&k) {
                continue;
            }
            self.cells.entry(k).or_default().insert(rail.id);
            rail_cells.push(k);
        }
        if !rail_cells.is_empty() {
            self.rail_to_cells.insert(rail.id, rail_cells);
        }
    }

    /// `unregister`.
    pub fn unregister(&mut self, rail: RailId) {
        let Some(keys) = self.rail_to_cells.remove(&rail) else {
            return;
        };
        for k in keys {
            let Some(set) = self.cells.get_mut(&k) else {
                continue;
            };
            set.remove(&rail);
            if set.is_empty() {
                self.cells.remove(&k);
            }
        }
    }

    /// `query`: railroads in the cells overlapping the square of `radius` around `tile`, in discovery order.
    pub fn query(&self, map: &GameMapImpl, tile: TileRef, radius: f64) -> Vec<RailId> {
        let x = map.x(tile) as f64;
        let y = map.y(tile) as f64;
        let c0 = self.cell_of(x - radius, y - radius);
        let c1 = self.cell_of(x + radius, y + radius);

        let mut seen: OrderedSet<RailId> = OrderedSet::new();
        for cx in c0.0..=c1.0 {
            for cy in c0.1..=c1.1 {
                let Some(set) = self.cells.get(&(cx, cy)) else {
                    continue;
                };
                for &rail in set.iter() {
                    seen.insert(rail);
                }
            }
        }
        seen.iter().copied().collect()
    }

    fn cell_of(&self, x: f64, y: f64) -> Cell {
        (
            (x / self.cell_size).floor() as i64,
            (y / self.cell_size).floor() as i64,
        )
    }
}

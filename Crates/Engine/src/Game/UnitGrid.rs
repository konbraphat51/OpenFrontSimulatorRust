// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/UnitGrid.ts: a coarse spatial grid of units for range queries.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Units live in the game's arena; the grid only stores ids per cell and unit type, in insertion
//! order (JS `Set`). Queries take the arena so predicates can read unit state.

use super::Game::{NUM_UNIT_TYPES, UnitId, UnitType};
use super::GameMap::{GameMapImpl, TileRef};
use super::UnitImpl::UnitImpl;
use crate::Compat::JsNumber::ceil;
use crate::Compat::OrderedSet::OrderedSet;

const CELL_SIZE: i64 = 100;

/// Optional filter of `nearby_units`: unit and squared distance.
pub type UnitPredicate<'a> = &'a mut dyn FnMut(&UnitImpl, f64) -> bool;

type TypeSets = Vec<OrderedSet<UnitId>>;

/// Spatial index of units by 100x100 cell and unit type.
#[derive(Debug)]
pub struct UnitGrid {
    grid: Vec<TypeSets>,
    cols: usize,
    rows: usize,
    /// Units per type over the whole grid, so queries for an absent type return without scanning cells.
    counts: [u32; NUM_UNIT_TYPES],
}

/// Inclusive cell range of a query.
struct CellRange {
    start_x: usize,
    end_x: i64,
    start_y: usize,
    end_y: i64,
}

fn ceil_div(range: f64, cell: f64) -> f64 {
    ceil(range / cell)
}

impl UnitGrid {
    /// Creates an empty grid covering the map.
    pub fn new(map: &GameMapImpl) -> Self {
        let rows = (map.height() as f64 / CELL_SIZE as f64).ceil() as usize;
        let cols = (map.width() as f64 / CELL_SIZE as f64).ceil() as usize;
        let grid = (0..rows * cols)
            .map(|_| (0..NUM_UNIT_TYPES).map(|_| OrderedSet::new()).collect())
            .collect();
        Self {
            grid,
            cols,
            rows,
            counts: [0; NUM_UNIT_TYPES],
        }
    }

    fn coords(map: &GameMapImpl, tile: TileRef) -> (i64, i64) {
        (
            map.x(tile) as i64 / CELL_SIZE,
            map.y(tile) as i64 / CELL_SIZE,
        )
    }

    fn is_valid_cell(&self, gx: i64, gy: i64) -> bool {
        gx >= 0 && (gx as usize) < self.cols && gy >= 0 && (gy as usize) < self.rows
    }

    fn cell_mut(&mut self, gx: i64, gy: i64, t: UnitType) -> &mut OrderedSet<UnitId> {
        &mut self.grid[gy as usize * self.cols + gx as usize][t.index()]
    }

    /// `addUnit`.
    pub fn add_unit(&mut self, map: &GameMapImpl, id: UnitId, t: UnitType, tile: TileRef) {
        let (gx, gy) = Self::coords(map, tile);
        if self.is_valid_cell(gx, gy) && self.cell_mut(gx, gy, t).insert(id) {
            self.counts[t.index()] += 1;
        }
    }

    /// `removeUnit` (at the unit's current tile).
    pub fn remove_unit(&mut self, map: &GameMapImpl, id: UnitId, t: UnitType, tile: TileRef) {
        self.remove_unit_by_tile(map, id, t, tile);
    }

    /// `removeUnitByTile`.
    pub fn remove_unit_by_tile(
        &mut self,
        map: &GameMapImpl,
        id: UnitId,
        t: UnitType,
        tile: TileRef,
    ) {
        let (gx, gy) = Self::coords(map, tile);
        if self.is_valid_cell(gx, gy) && self.cell_mut(gx, gy, t).remove(&id) {
            self.counts[t.index()] -= 1;
        }
    }

    /// `updateUnitCell`: moves the unit to its new cell if it changed.
    pub fn update_unit_cell(
        &mut self,
        map: &GameMapImpl,
        id: UnitId,
        t: UnitType,
        new_tile: TileRef,
        old_tile: TileRef,
    ) {
        if new_tile == old_tile {
            return;
        }
        let (gx, gy) = Self::coords(map, old_tile);
        let (ngx, ngy) = Self::coords(map, new_tile);
        if gx != ngx || gy != ngy {
            self.remove_unit_by_tile(map, id, t, old_tile);
            self.add_unit(map, id, t, new_tile);
        }
    }

    fn cells_in_range(&self, map: &GameMapImpl, tile: TileRef, range: f64) -> CellRange {
        let cs = CELL_SIZE as f64;
        // Tile coordinates are non-negative integers, so the JS `%` is an exact integer remainder.
        let x_in_cell = (map.x(tile) % CELL_SIZE as u32) as f64;
        let y_in_cell = (map.y(tile) % CELL_SIZE as u32) as f64;
        let (gx, gy) = Self::coords(map, tile);
        let start_x = (gx as f64 - ceil_div(range - x_in_cell, cs)).max(0.0);
        let end_x =
            (self.cols as f64 - 1.0).min(gx as f64 + ceil_div(range - (cs - x_in_cell), cs));
        let start_y = (gy as f64 - ceil_div(range - y_in_cell, cs)).max(0.0);
        let end_y =
            (self.rows as f64 - 1.0).min(gy as f64 + ceil_div(range - (cs - y_in_cell), cs));
        CellRange {
            start_x: start_x as usize,
            end_x: end_x as i64,
            start_y: start_y as usize,
            end_y: end_y as i64,
        }
    }

    fn unit_is_in_range(
        map: &GameMapImpl,
        unit: &UnitImpl,
        tile: TileRef,
        range_squared: f64,
        player: Option<super::Game::SmallId>,
        include_under_construction: bool,
    ) -> bool {
        if !unit.is_active() {
            return false;
        }
        if !include_under_construction && unit.is_under_construction() {
            return false;
        }
        if let Some(p) = player
            && unit.owner_id() != p
        {
            return false;
        }
        let dx = map.x(unit.tile()) as f64 - map.x(tile) as f64;
        let dy = map.y(unit.tile()) as f64 - map.y(tile) as f64;
        dx * dx + dy * dy <= range_squared
    }

    /// `nearbyUnits`: (unit, distSquared) pairs in cell order, then type order, then insertion order.
    #[allow(clippy::too_many_arguments)]
    pub fn nearby_units(
        &self,
        map: &GameMapImpl,
        units: &[UnitImpl],
        tile: TileRef,
        search_range: f64,
        types: &[UnitType],
        mut predicate: Option<UnitPredicate<'_>>,
        include_under_construction: bool,
    ) -> Vec<(UnitId, f64)> {
        let mut nearby = Vec::new();
        let x = map.x(tile) as f64;
        let y = map.y(tile) as f64;
        let r = self.cells_in_range(map, tile, search_range);
        let range_squared = search_range * search_range;
        let mut cy = r.start_y as i64;
        while cy <= r.end_y {
            let mut cx = r.start_x as i64;
            while cx <= r.end_x {
                let cell = &self.grid[cy as usize * self.cols + cx as usize];
                for t in types {
                    if self.counts[t.index()] == 0 {
                        continue;
                    }
                    for &id in cell[t.index()].iter() {
                        let unit = &units[id as usize - 1];
                        if !unit.is_active() {
                            continue;
                        }
                        if !include_under_construction && unit.is_under_construction() {
                            continue;
                        }
                        let dx = map.x(unit.tile()) as f64 - x;
                        let dy = map.y(unit.tile()) as f64 - y;
                        let dist_squared = dx * dx + dy * dy;
                        if dist_squared > range_squared {
                            continue;
                        }
                        if let Some(p) = predicate.as_mut()
                            && !p(unit, dist_squared)
                        {
                            continue;
                        }
                        nearby.push((id, dist_squared));
                    }
                }
                cx += 1;
            }
            cy += 1;
        }
        nearby
    }

    /// `hasUnitNearby`.
    #[allow(clippy::too_many_arguments)]
    pub fn has_unit_nearby(
        &self,
        map: &GameMapImpl,
        units: &[UnitImpl],
        tile: TileRef,
        search_range: f64,
        t: UnitType,
        player: Option<super::Game::SmallId>,
        include_under_construction: bool,
    ) -> bool {
        if self.counts[t.index()] == 0 {
            return false;
        }
        let r = self.cells_in_range(map, tile, search_range);
        let range_squared = search_range * search_range;
        let mut cy = r.start_y as i64;
        while cy <= r.end_y {
            let mut cx = r.start_x as i64;
            while cx <= r.end_x {
                let cell = &self.grid[cy as usize * self.cols + cx as usize];
                for &id in cell[t.index()].iter() {
                    if Self::unit_is_in_range(
                        map,
                        &units[id as usize - 1],
                        tile,
                        range_squared,
                        player,
                        include_under_construction,
                    ) {
                        return true;
                    }
                }
                cx += 1;
            }
            cy += 1;
        }
        false
    }

    /// `anyUnitNearby`.
    #[allow(clippy::too_many_arguments)]
    pub fn any_unit_nearby(
        &self,
        map: &GameMapImpl,
        units: &[UnitImpl],
        tile: TileRef,
        search_range: f64,
        types: &[UnitType],
        mut predicate: impl FnMut(&UnitImpl) -> bool,
        player: Option<super::Game::SmallId>,
        include_under_construction: bool,
    ) -> bool {
        let r = self.cells_in_range(map, tile, search_range);
        let range_squared = search_range * search_range;
        let mut cy = r.start_y as i64;
        while cy <= r.end_y {
            let mut cx = r.start_x as i64;
            while cx <= r.end_x {
                let cell = &self.grid[cy as usize * self.cols + cx as usize];
                for t in types {
                    if self.counts[t.index()] == 0 {
                        continue;
                    }
                    for &id in cell[t.index()].iter() {
                        let unit = &units[id as usize - 1];
                        if !Self::unit_is_in_range(
                            map,
                            unit,
                            tile,
                            range_squared,
                            player,
                            include_under_construction,
                        ) {
                            continue;
                        }
                        if predicate(unit) {
                            return true;
                        }
                    }
                }
                cx += 1;
            }
            cy += 1;
        }
        false
    }
}

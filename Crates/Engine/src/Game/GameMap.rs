// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/GameMap.ts: terrain bits, per-tile owner/fallout state, neighbours, searches.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Tile refs are `y * width + x`. Out-of-range refs panic (JS would read `undefined`); the engine
//! only passes refs it derived from the map, and every neighbour helper guards the borders.

use super::TileSet::TileSet;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Game::Game::{Cell, TerrainType};

/// Index of a tile in the map arrays.
pub type TileRef = u32;

const IS_LAND_BIT: u8 = 7;
const SHORELINE_BIT: u8 = 6;
const OCEAN_BIT: u8 = 5;
const MAGNITUDE_MASK: u8 = 0x1f;
/// Land with this magnitude is impassable terrain.
const IMPASSABLE_MAGNITUDE: u8 = 31;

const PLAYER_ID_MASK: u16 = 0xfff;
const FALLOUT_BIT: u16 = 13;
const DEFENSE_BONUS_BIT: u16 = 14;

/// Terrain plus mutable owner/fallout/defense state of one map.
#[derive(Clone, Debug)]
pub struct GameMapImpl {
    num_tiles_with_fallout: u32,
    terrain: Vec<u8>,
    state: Vec<u16>,
    width: u32,
    height: u32,
    num_land_tiles: i64,
    water_version: u32,
    /// False before PR 4340: magnitude 31 is ordinary mountain, nothing is impassable.
    impassable_enabled: bool,
    /// True before PR 4495: `neighbors4` and `for_each_neighbor` visit W, E, N, S.
    legacy_neighbor_order: bool,
}

impl GameMapImpl {
    /// Builds a map from terrain bytes (`width * height` of them) and the land tile count.
    pub fn new(
        width: u32,
        height: u32,
        terrain: Vec<u8>,
        num_land_tiles: i64,
    ) -> Result<Self, String> {
        if terrain.len() as u64 != width as u64 * height as u64 {
            return Err(format!(
                "Terrain data length {} doesn't match dimensions {width}x{height}",
                terrain.len()
            ));
        }
        let n = terrain.len();
        Ok(Self {
            num_tiles_with_fallout: 0,
            terrain,
            state: vec![0; n],
            width,
            height,
            num_land_tiles,
            water_version: 0,
            impassable_enabled: true,
            legacy_neighbor_order: false,
        })
    }

    /// Selects the build-dependent terrain and neighbour-order behaviour of this map.
    pub fn set_build_behavior(&mut self, impassable_enabled: bool, legacy_neighbor_order: bool) {
        self.impassable_enabled = impassable_enabled;
        self.legacy_neighbor_order = legacy_neighbor_order;
    }

    /// Tiles currently carrying fallout.
    pub fn num_tiles_with_fallout(&self) -> u32 {
        self.num_tiles_with_fallout
    }

    /// Tile ref of (x, y); panics on invalid coordinates like upstream throws.
    pub fn tile_ref(&self, x: i32, y: i32) -> TileRef {
        assert!(self.is_valid_coord(x, y), "Invalid coordinates: {x},{y}");
        y as u32 * self.width + x as u32
    }

    /// True when the ref indexes a tile.
    pub fn is_valid_ref(&self, tile: TileRef) -> bool {
        (tile as u64) < self.width as u64 * self.height as u64
    }

    /// Column of a tile.
    pub fn x(&self, tile: TileRef) -> u32 {
        tile % self.width
    }

    /// Row of a tile.
    pub fn y(&self, tile: TileRef) -> u32 {
        tile / self.width
    }

    /// Cell of a tile.
    pub fn cell(&self, tile: TileRef) -> Cell {
        Cell::new(self.x(tile) as i32, self.y(tile) as i32)
    }

    /// Map width in tiles.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Map height in tiles.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Number of land tiles (drops as land turns to water).
    pub fn num_land_tiles(&self) -> i64 {
        self.num_land_tiles
    }

    /// True when (x, y) lies on the map.
    pub fn is_valid_coord(&self, x: i32, y: i32) -> bool {
        x >= 0 && (x as u32) < self.width && y >= 0 && (y as u32) < self.height
    }

    /// Land bit.
    pub fn is_land(&self, tile: TileRef) -> bool {
        self.terrain[tile as usize] & (1 << IS_LAND_BIT) != 0
    }

    /// Land with the maximum magnitude: cannot be owned, attacked or nuked.
    pub fn is_impassable(&self, tile: TileRef) -> bool {
        self.impassable_enabled
            && self.is_land(tile)
            && (self.terrain[tile as usize] & MAGNITUDE_MASK) == IMPASSABLE_MAGNITUDE
    }

    /// Land tile with a cardinal ocean neighbour.
    pub fn is_ocean_shore(&self, tile: TileRef) -> bool {
        if !self.is_land(tile) {
            return false;
        }
        let w = self.width;
        let x = tile % w;
        if x != 0 && self.is_ocean(tile - 1) {
            return true;
        }
        if x != w - 1 && self.is_ocean(tile + 1) {
            return true;
        }
        if tile >= w && self.is_ocean(tile - w) {
            return true;
        }
        if tile < (self.height - 1) * w && self.is_ocean(tile + w) {
            return true;
        }
        false
    }

    /// Ocean bit (water connected to the map edge).
    pub fn is_ocean(&self, tile: TileRef) -> bool {
        self.terrain[tile as usize] & (1 << OCEAN_BIT) != 0
    }

    /// Shoreline bit (land or water next to the other kind).
    pub fn is_shoreline(&self, tile: TileRef) -> bool {
        self.terrain[tile as usize] & (1 << SHORELINE_BIT) != 0
    }

    /// Terrain magnitude, 0 to 31.
    pub fn magnitude(&self, tile: TileRef) -> u8 {
        self.terrain[tile as usize] & MAGNITUDE_MASK
    }

    /// Raw terrain byte.
    pub fn terrain_byte(&self, tile: TileRef) -> u8 {
        self.terrain[tile as usize]
    }

    /// Bumped every time a land tile turns to water.
    pub fn water_version(&self) -> u32 {
        self.water_version
    }

    /// Turns a land tile into lake water; no-op on water and impassable land.
    pub fn set_water(&mut self, tile: TileRef) {
        if !self.is_land(tile) || self.is_impassable(tile) {
            return;
        }
        self.water_version += 1;
        self.terrain[tile as usize] = 0;
        self.num_land_tiles -= 1;
    }

    /// Sets the shoreline bit.
    pub fn set_shoreline_bit(&mut self, tile: TileRef) {
        self.terrain[tile as usize] |= 1 << SHORELINE_BIT;
    }

    /// Clears the shoreline bit.
    pub fn clear_shoreline_bit(&mut self, tile: TileRef) {
        self.terrain[tile as usize] &= !(1 << SHORELINE_BIT);
    }

    /// Sets the ocean bit.
    pub fn set_ocean(&mut self, tile: TileRef) {
        self.terrain[tile as usize] |= 1 << OCEAN_BIT;
    }

    /// Replaces the magnitude bits (value masked to 5 bits).
    pub fn set_magnitude(&mut self, tile: TileRef, value: u8) {
        let t = &mut self.terrain[tile as usize];
        *t = (*t & !MAGNITUDE_MASK) | (value & MAGNITUDE_MASK);
    }

    /// Owner small id of a tile (0 = terra nullius).
    pub fn owner_id(&self, tile: TileRef) -> u16 {
        self.state[tile as usize] & PLAYER_ID_MASK
    }

    /// True when some player owns the tile.
    pub fn has_owner(&self, tile: TileRef) -> bool {
        self.owner_id(tile) != 0
    }

    /// Sets the owner; panics above 4095 like upstream throws.
    pub fn set_owner_id(&mut self, tile: TileRef, player_id: u32) {
        assert!(
            player_id <= PLAYER_ID_MASK as u32,
            "Player ID {player_id} exceeds maximum value {PLAYER_ID_MASK}"
        );
        let s = &mut self.state[tile as usize];
        *s = (*s & !PLAYER_ID_MASK) | player_id as u16;
    }

    /// Fallout bit.
    pub fn has_fallout(&self, tile: TileRef) -> bool {
        self.state[tile as usize] & (1 << FALLOUT_BIT) != 0
    }

    /// Sets or clears fallout, keeping the global counter in step.
    pub fn set_fallout(&mut self, tile: TileRef, value: bool) {
        let existing = self.has_fallout(tile);
        if value {
            if !existing {
                self.num_tiles_with_fallout += 1;
                self.state[tile as usize] |= 1 << FALLOUT_BIT;
            }
        } else if existing {
            self.num_tiles_with_fallout -= 1;
            self.state[tile as usize] &= !(1 << FALLOUT_BIT);
        }
    }

    /// True when the tile touches the map boundary or an impassable tile.
    pub fn is_on_edge_of_map(&self, tile: TileRef) -> bool {
        let w = self.width;
        let x = tile % w;
        if x == 0 || x == w - 1 || tile < w || tile >= (self.height - 1) * w {
            return true;
        }
        self.is_impassable(tile - 1)
            || self.is_impassable(tile + 1)
            || self.is_impassable(tile - w)
            || self.is_impassable(tile + w)
    }

    /// True when the tile lies on the map boundary (`isOnEdgeOfMap` before impassable tiles counted).
    pub fn is_on_map_boundary(&self, tile: TileRef) -> bool {
        let w = self.width;
        let x = tile % w;
        x == 0 || x == w - 1 || tile < w || tile >= (self.height - 1) * w
    }

    /// True when a cardinal neighbour has a different owner.
    pub fn is_border(&self, tile: TileRef) -> bool {
        let w = self.width;
        let x = tile % w;
        let owner = self.owner_id(tile);
        if x != 0 && self.owner_id(tile - 1) != owner {
            return true;
        }
        if x != w - 1 && self.owner_id(tile + 1) != owner {
            return true;
        }
        if tile >= w && self.owner_id(tile - w) != owner {
            return true;
        }
        if tile < (self.height - 1) * w && self.owner_id(tile + w) != owner {
            return true;
        }
        false
    }

    /// Defense-bonus bit (state bit 14).
    pub fn has_defense_bonus(&self, tile: TileRef) -> bool {
        self.state[tile as usize] & (1 << DEFENSE_BONUS_BIT) != 0
    }

    /// Sets or clears the defense-bonus bit.
    pub fn set_defense_bonus(&mut self, tile: TileRef, value: bool) {
        if value {
            self.state[tile as usize] |= 1 << DEFENSE_BONUS_BIT;
        } else {
            self.state[tile as usize] &= !(1 << DEFENSE_BONUS_BIT);
        }
    }

    /// Not land.
    pub fn is_water(&self, tile: TileRef) -> bool {
        !self.is_land(tile)
    }

    /// Land with the shoreline bit.
    pub fn is_shore(&self, tile: TileRef) -> bool {
        self.is_land(tile) && self.is_shoreline(tile)
    }

    /// Movement cost: 2 below magnitude 10, else 1.
    pub fn cost(&self, tile: TileRef) -> u32 {
        if self.magnitude(tile) < 10 { 2 } else { 1 }
    }

    /// Terrain class of a tile.
    pub fn terrain_type(&self, tile: TileRef) -> TerrainType {
        if self.is_land(tile) {
            let m = self.magnitude(tile);
            if self.impassable_enabled && m >= IMPASSABLE_MAGNITUDE {
                return TerrainType::Impassable;
            }
            if m < 10 {
                return TerrainType::Plains;
            }
            if m < 20 {
                return TerrainType::Highland;
            }
            return TerrainType::Mountain;
        }
        TerrainType::Ocean
    }

    /// Cardinal neighbours in N, S, W, E order (the same in every build).
    pub fn neighbors(&self, tile: TileRef) -> Vec<TileRef> {
        let mut out = Vec::with_capacity(4);
        self.visit_nswe(tile, |t| out.push(t));
        out
    }

    fn visit_nswe(&self, tile: TileRef, mut callback: impl FnMut(TileRef)) {
        let w = self.width;
        let x = tile % w;
        if tile >= w {
            callback(tile - w);
        }
        if tile < (self.height - 1) * w {
            callback(tile + w);
        }
        if x != 0 {
            callback(tile - 1);
        }
        if x != w - 1 {
            callback(tile + 1);
        }
    }

    fn visit_wens(&self, tile: TileRef, mut callback: impl FnMut(TileRef)) {
        let w = self.width;
        let x = tile % w;
        if x != 0 {
            callback(tile - 1);
        }
        if x != w - 1 {
            callback(tile + 1);
        }
        if tile >= w {
            callback(tile - w);
        }
        if tile < (self.height - 1) * w {
            callback(tile + w);
        }
    }

    /// Visits cardinal neighbours in N, S, W, E order (W, E, N, S before PR 4495).
    pub fn for_each_neighbor(&self, tile: TileRef, callback: impl FnMut(TileRef)) {
        if self.legacy_neighbor_order {
            self.visit_wens(tile, callback);
        } else {
            self.visit_nswe(tile, callback);
        }
    }

    /// Visits cardinal neighbours in N, S, W, E order in every build (upstream `neighbors()` order).
    pub fn for_each_neighbor_nswe(&self, tile: TileRef, callback: impl FnMut(TileRef)) {
        self.visit_nswe(tile, callback);
    }

    /// Like `neighbors4` but N, S, W, E in every build (upstream `neighbors()` order).
    pub fn neighbors4_nswe(&self, tile: TileRef, out: &mut [TileRef; 4]) -> usize {
        let mut n = 0;
        self.visit_nswe(tile, |t| {
            out[n] = t;
            n += 1;
        });
        n
    }

    /// Writes cardinal neighbours (N, S, W, E; W, E, N, S before PR 4495) into `out` and returns the count.
    pub fn neighbors4(&self, tile: TileRef, out: &mut [TileRef; 4]) -> usize {
        let mut n = 0;
        self.for_each_neighbor(tile, |t| {
            out[n] = t;
            n += 1;
        });
        n
    }

    /// Writes the 8 neighbours in dx-major order and returns the count.
    pub fn neighbors8(&self, tile: TileRef, out: &mut [TileRef; 8]) -> usize {
        let mut n = 0;
        self.for_each_neighbor_with_diag(tile, |t| {
            out[n] = t;
            n += 1;
        });
        n
    }

    /// Visits the 8 neighbours: (-1,-1),(-1,0),(-1,1),(0,-1),(0,1),(1,-1),(1,0),(1,1).
    pub fn for_each_neighbor_with_diag(&self, tile: TileRef, mut callback: impl FnMut(TileRef)) {
        let w = self.width;
        let x = tile % w;
        let has_n = tile >= w;
        let has_s = tile < (self.height - 1) * w;
        if x != 0 {
            if has_n {
                callback(tile - 1 - w);
            }
            callback(tile - 1);
            if has_s {
                callback(tile - 1 + w);
            }
        }
        if has_n {
            callback(tile - w);
        }
        if has_s {
            callback(tile + w);
        }
        if x != w - 1 {
            if has_n {
                callback(tile + 1 - w);
            }
            callback(tile + 1);
            if has_s {
                callback(tile + 1 + w);
            }
        }
    }

    /// Visits every tile in ref order.
    pub fn for_each_tile(&self, mut f: impl FnMut(TileRef)) {
        for t in 0..self.width * self.height {
            f(t);
        }
    }

    /// |dx| + |dy|.
    pub fn manhattan_dist(&self, c1: TileRef, c2: TileRef) -> u32 {
        self.x(c1).abs_diff(self.x(c2)) + self.y(c1).abs_diff(self.y(c2))
    }

    /// dx*dx + dy*dy, exact (can exceed `u32`).
    pub fn euclidean_dist_squared(&self, c1: TileRef, c2: TileRef) -> u64 {
        let x = self.x(c1).abs_diff(self.x(c2)) as u64;
        let y = self.y(c1).abs_diff(self.y(c2)) as u64;
        x * x + y * y
    }

    /// Visits tiles within `radius` (column-major: x outer, y inner) that pass `filter`, in
    /// upstream insertion order. Radius is an integer, as every upstream caller passes.
    pub fn circle_search_visit(
        &self,
        tile: TileRef,
        radius: i32,
        mut filter: impl FnMut(TileRef, u64) -> bool,
        mut visit: impl FnMut(TileRef),
    ) {
        let cx = self.x(tile) as i32;
        let cy = self.y(tile) as i32;
        let min_x = 0.max(cx - radius);
        let max_x = (self.width as i32 - 1).min(cx + radius);
        let min_y = 0.max(cy - radius);
        let max_y = (self.height as i32 - 1).min(cy + radius);
        let r2 = radius as i64 * radius as i64;
        for i in min_x..=max_x {
            for j in min_y..=max_y {
                let t = j as u32 * self.width + i as u32;
                let d2 = self.euclidean_dist_squared(tile, t);
                if d2 as i64 > r2 {
                    continue;
                }
                if filter(t, d2) {
                    visit(t);
                }
            }
        }
    }

    /// `circleSearch`: set of tiles within `radius` accepted by `filter`.
    pub fn circle_search(
        &self,
        tile: TileRef,
        radius: i32,
        filter: impl FnMut(TileRef, u64) -> bool,
    ) -> OrderedSet<TileRef> {
        let mut tiles = OrderedSet::new();
        self.circle_search_visit(tile, radius, filter, |t| {
            tiles.insert(t);
        });
        tiles
    }

    /// `bfs`: flood fill over cardinal neighbours using upstream's LIFO queue; the returned set
    /// is in discovery order.
    pub fn bfs(
        &self,
        tile: TileRef,
        mut filter: impl FnMut(&GameMapImpl, TileRef) -> bool,
    ) -> OrderedSet<TileRef> {
        let mut seen = OrderedSet::new();
        let mut q: Vec<TileRef> = Vec::new();
        if filter(self, tile) {
            seen.insert(tile);
            q.push(tile);
        }
        let w = self.width;
        let south_limit = (self.height - 1) * w;
        while let Some(curr) = q.pop() {
            let x = curr % w;
            let mut cand = [0; 4];
            let mut n = 0;
            if curr >= w {
                cand[n] = curr - w;
                n += 1;
            }
            if curr < south_limit {
                cand[n] = curr + w;
                n += 1;
            }
            if x != 0 {
                cand[n] = curr - 1;
                n += 1;
            }
            if x != w - 1 {
                cand[n] = curr + 1;
                n += 1;
            }
            for &nb in &cand[..n] {
                if !seen.contains(&nb) && filter(self, nb) {
                    seen.insert(nb);
                    q.push(nb);
                }
            }
        }
        seen
    }

    /// `bfs` of v0.36.0 on: FIFO queue, so the set comes out in breadth-first order and the
    /// filter runs in that order too.
    pub fn bfs_fifo(
        &self,
        tile: TileRef,
        mut filter: impl FnMut(&GameMapImpl, TileRef) -> bool,
    ) -> OrderedSet<TileRef> {
        let mut seen = OrderedSet::new();
        if !filter(self, tile) {
            return seen;
        }
        let mut q: Vec<TileRef> = vec![tile];
        let mut marked: std::collections::HashSet<TileRef> = std::collections::HashSet::new();
        marked.insert(tile);
        let w = self.width;
        let south_limit = (self.height - 1) * w;
        let mut head = 0;
        while head < q.len() {
            let curr = q[head];
            head += 1;
            seen.insert(curr);
            let x = curr % w;
            let mut cand = [0; 4];
            let mut n = 0;
            if curr >= w {
                cand[n] = curr - w;
                n += 1;
            }
            if curr < south_limit {
                cand[n] = curr + w;
                n += 1;
            }
            if x != 0 {
                cand[n] = curr - 1;
                n += 1;
            }
            if x != w - 1 {
                cand[n] = curr + 1;
                n += 1;
            }
            for &nb in &cand[..n] {
                if !marked.contains(&nb) && filter(self, nb) {
                    marked.insert(nb);
                    q.push(nb);
                }
            }
        }
        seen
    }

    /// `bfs` returning a [`TileSet`].
    pub fn bfs_tiles(
        &self,
        tile: TileRef,
        filter: impl FnMut(&GameMapImpl, TileRef) -> bool,
    ) -> TileSet {
        TileSet::from_ordered(self.bfs(tile, filter))
    }

    /// Packed per-tile state (owner bits 0-11, fallout bit 13, defense bit 14).
    pub fn tile_state(&self, tile: TileRef) -> u16 {
        self.state[tile as usize]
    }

    /// The whole state buffer.
    pub fn tile_state_buffer(&self) -> &[u16] {
        &self.state
    }

    /// Applies a packed update (`state | terrain << 16`); true when the terrain byte changed.
    pub fn update_tile(&mut self, tile: TileRef, packed: u32) -> bool {
        let state = (packed & 0xffff) as u16;
        let terrain_byte = ((packed >> 16) & 0xff) as u8;
        let existing_fallout = self.has_fallout(tile);
        self.state[tile as usize] = state;
        let new_fallout = self.has_fallout(tile);
        if existing_fallout && !new_fallout {
            self.num_tiles_with_fallout -= 1;
        }
        if !existing_fallout && new_fallout {
            self.num_tiles_with_fallout += 1;
        }
        let changed = self.terrain[tile as usize] != terrain_byte;
        if changed {
            let was_land = self.is_land(tile);
            self.terrain[tile as usize] = terrain_byte;
            let is_now_land = terrain_byte & (1 << IS_LAND_BIT) != 0;
            if was_land != is_now_land {
                self.water_version += 1;
            }
            if was_land && !is_now_land {
                self.num_land_tiles -= 1;
            } else if !was_land && is_now_land {
                self.num_land_tiles += 1;
            }
        }
        changed
    }
}

/// A tile predicate over a map, as produced by the `*DistFN` helpers.
pub type TileFilter = Box<dyn Fn(&GameMapImpl, TileRef) -> bool>;

/// `euclDistFN`.
pub fn eucl_dist_fn(root: TileRef, dist: f64, center: bool) -> TileFilter {
    let dist2 = dist * dist;
    if !center {
        Box::new(move |gm, n| gm.euclidean_dist_squared(root, n) as f64 <= dist2)
    } else {
        Box::new(move |gm, n| {
            let root_x = gm.x(root) as f64 - 0.5;
            let root_y = gm.y(root) as f64 - 0.5;
            let dx = gm.x(n) as f64 - root_x;
            let dy = gm.y(n) as f64 - root_y;
            dx * dx + dy * dy <= dist2
        })
    }
}

/// `manhattanDistFN`.
pub fn manhattan_dist_fn(root: TileRef, dist: f64, center: bool) -> TileFilter {
    if !center {
        Box::new(move |gm, n| gm.manhattan_dist(root, n) as f64 <= dist)
    } else {
        Box::new(move |gm, n| {
            let root_x = gm.x(root) as f64 - 0.5;
            let root_y = gm.y(root) as f64 - 0.5;
            let dx = (gm.x(n) as f64 - root_x).abs();
            let dy = (gm.y(n) as f64 - root_y).abs();
            dx + dy <= dist
        })
    }
}

/// `rectDistFN`.
pub fn rect_dist_fn(root: TileRef, dist: f64, center: bool) -> TileFilter {
    if !center {
        Box::new(move |gm, n| {
            let dx = gm.x(n).abs_diff(gm.x(root)) as f64;
            let dy = gm.y(n).abs_diff(gm.y(root)) as f64;
            dx <= dist && dy <= dist
        })
    } else {
        Box::new(move |gm, n| {
            let root_x = gm.x(root) as f64 - 0.5;
            let root_y = gm.y(root) as f64 - 0.5;
            let dx = (gm.x(n) as f64 - root_x).abs();
            let dy = (gm.y(n) as f64 - root_y).abs();
            dx <= dist && dy <= dist
        })
    }
}

/// `isometricDistFN`.
pub fn isometric_dist_fn(root: TileRef, dist: f64, center: bool) -> TileFilter {
    if !center {
        Box::new(move |gm, n| gm.manhattan_dist(root, n) as f64 <= dist)
    } else {
        Box::new(move |gm, n| {
            let root_x = gm.x(root) as f64 - 0.5;
            let root_y = gm.y(root) as f64 - 0.5;
            let dx = (gm.x(n) as f64 - root_x).abs();
            let dy = (gm.y(n) as f64 - (root_y + 0.0)).abs();
            dx + dy * 2.0 <= dist + 1.0
        })
    }
}

/// `hexDistFN`.
pub fn hex_dist_fn(root: TileRef, dist: f64, center: bool) -> TileFilter {
    if !center {
        Box::new(move |gm, n| {
            let dx = gm.x(n).abs_diff(gm.x(root)) as f64;
            let dy = gm.y(n).abs_diff(gm.y(root)) as f64;
            dx <= dist && dy <= dist && dx + dy <= dist * 1.5
        })
    } else {
        Box::new(move |gm, n| {
            let root_x = gm.x(root) as f64 - 0.5;
            let root_y = gm.y(root) as f64 - 0.5;
            let dx = (gm.x(n) as f64 - root_x).abs();
            let dy = (gm.y(n) as f64 - root_y).abs();
            dx <= dist && dy <= dist && dx + dy <= dist * 1.5
        })
    }
}

/// `andFN`.
pub fn and_fn(x: TileFilter, y: TileFilter) -> TileFilter {
    Box::new(move |gm, tile| x(gm, tile) && y(gm, tile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn u32s(v: &Value) -> Vec<u32> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as u32)
            .collect()
    }

    fn ordered(set: &OrderedSet<TileRef>) -> Vec<u32> {
        set.iter().copied().collect()
    }

    #[test]
    fn matches_upstream_goldens() {
        let g: Value =
            serde_json::from_str(include_str!("../../Tests/Golden/GameMap.json")).unwrap();
        for case in g.as_array().unwrap() {
            let (w, h) = (
                case["w"].as_u64().unwrap() as u32,
                case["h"].as_u64().unwrap() as u32,
            );
            let terrain: Vec<u8> = u32s(&case["terrain"])
                .into_iter()
                .map(|b| b as u8)
                .collect();
            let mut gm = GameMapImpl::new(w, h, terrain, case["land"].as_i64().unwrap()).unwrap();
            let tiles = w * h;
            for o in case["owners"].as_array().unwrap() {
                gm.set_owner_id(o[0].as_u64().unwrap() as u32, o[1].as_u64().unwrap() as u32);
            }
            for t in u32s(&case["fallout"]) {
                gm.set_fallout(t, true);
            }
            // Defense bits were set pseudo-randomly upstream; take them from the recorded state.
            for (t, p) in case["per"].as_array().unwrap().iter().enumerate() {
                let t = t as u32;
                if p["defense"].as_bool().unwrap() {
                    gm.set_defense_bonus(t, true);
                }
                let ctx = format!("{w}x{h} tile {t}");
                assert_eq!(gm.x(t) as u64, p["x"].as_u64().unwrap(), "{ctx}");
                assert_eq!(gm.y(t) as u64, p["y"].as_u64().unwrap(), "{ctx}");
                assert_eq!(gm.is_land(t), p["land"].as_bool().unwrap(), "{ctx}");
                assert_eq!(gm.is_impassable(t), p["imp"].as_bool().unwrap(), "{ctx}");
                assert_eq!(
                    gm.is_ocean_shore(t),
                    p["oceanShore"].as_bool().unwrap(),
                    "{ctx}"
                );
                assert_eq!(gm.is_ocean(t), p["ocean"].as_bool().unwrap(), "{ctx}");
                assert_eq!(
                    gm.is_shoreline(t),
                    p["shoreline"].as_bool().unwrap(),
                    "{ctx}"
                );
                assert_eq!(gm.magnitude(t) as u64, p["mag"].as_u64().unwrap(), "{ctx}");
                assert_eq!(gm.is_shore(t), p["shore"].as_bool().unwrap(), "{ctx}");
                assert_eq!(gm.cost(t) as u64, p["cost"].as_u64().unwrap(), "{ctx}");
                assert_eq!(
                    gm.terrain_type(t) as u64,
                    p["tt"].as_u64().unwrap(),
                    "{ctx}"
                );
                assert_eq!(gm.is_water(t), p["water"].as_bool().unwrap(), "{ctx}");
                assert_eq!(gm.neighbors(t), u32s(&p["n"]), "{ctx}");
                let mut n8 = vec![];
                gm.for_each_neighbor_with_diag(t, |x| n8.push(x));
                assert_eq!(n8, u32s(&p["n8"]), "{ctx}");
                let mut out = [0; 8];
                let k = gm.neighbors8(t, &mut out);
                assert_eq!(out[..k].to_vec(), u32s(&p["n8b"]), "{ctx}");
                assert_eq!(
                    gm.is_on_edge_of_map(t),
                    p["edge"].as_bool().unwrap(),
                    "{ctx}"
                );
                assert_eq!(gm.is_border(t), p["border"].as_bool().unwrap(), "{ctx}");
                assert_eq!(gm.owner_id(t) as u64, p["owner"].as_u64().unwrap(), "{ctx}");
                assert_eq!(gm.has_owner(t), p["hasOwner"].as_bool().unwrap(), "{ctx}");
                assert_eq!(gm.has_fallout(t), p["fallout"].as_bool().unwrap(), "{ctx}");
                assert_eq!(
                    gm.has_defense_bonus(t),
                    p["defense"].as_bool().unwrap(),
                    "{ctx}"
                );
                assert_eq!(
                    gm.tile_state(t) as u64,
                    p["state"].as_u64().unwrap(),
                    "{ctx}"
                );
            }
            for p in case["pairs"].as_array().unwrap() {
                let (a, b) = (p[0].as_u64().unwrap() as u32, p[1].as_u64().unwrap() as u32);
                assert_eq!(gm.manhattan_dist(a, b) as u64, p[2].as_u64().unwrap());
                assert_eq!(gm.euclidean_dist_squared(a, b), p[3].as_u64().unwrap());
            }
            for c in case["circles"].as_array().unwrap() {
                let (t, r) = (c[0].as_u64().unwrap() as u32, c[1].as_i64().unwrap() as i32);
                assert_eq!(
                    ordered(&gm.circle_search(t, r, |_, _| true)),
                    u32s(&c[2]),
                    "circle {t} {r}"
                );
                assert_eq!(
                    ordered(&gm.circle_search(t, r, |_, d2| d2 % 2 == 1)),
                    u32s(&c[3]),
                    "circle odd {t} {r}"
                );
            }
            for b in case["bfss"].as_array().unwrap() {
                let t = b[0].as_u64().unwrap() as u32;
                assert_eq!(
                    ordered(&gm.bfs(t, |g, x| g.is_land(x))),
                    u32s(&b[1]),
                    "bfs land {t}"
                );
                assert_eq!(
                    ordered(&gm.bfs(t, |g, x| g.owner_id(x) == g.owner_id(t))),
                    u32s(&b[2]),
                    "bfs owner {t}"
                );
            }
            for f in case["fns"].as_array().unwrap() {
                let (t, r, center) = (
                    f["t"].as_u64().unwrap() as u32,
                    f["r"].as_f64().unwrap(),
                    f["center"].as_bool().unwrap(),
                );
                type Maker = fn(TileRef, f64, bool) -> TileFilter;
                let makers: [(&str, Maker); 5] = [
                    ("eucl", eucl_dist_fn),
                    ("manh", manhattan_dist_fn),
                    ("rect", rect_dist_fn),
                    ("iso", isometric_dist_fn),
                    ("hex", hex_dist_fn),
                ];
                for (name, mk) in makers {
                    let filt = mk(t, r, center);
                    let got: Vec<u32> = (0..tiles).filter(|&x| filt(&gm, x)).collect();
                    assert_eq!(got, u32s(&f[name]), "{name} {t} {r} {center}");
                }
                let both = and_fn(eucl_dist_fn(t, r, center), rect_dist_fn(t, r + 1.0, center));
                let got: Vec<u32> = (0..tiles).filter(|&x| both(&gm, x)).collect();
                assert_eq!(got, u32s(&f["and"]));
            }
            for m in case["mut"].as_array().unwrap() {
                let a = m["args"].as_array().unwrap();
                let a0 = a[0].as_u64().unwrap() as u32;
                match m["op"].as_str().unwrap() {
                    "setWater" => gm.set_water(a0),
                    "magShoreOcean" => {
                        gm.set_magnitude(a0, a[1].as_u64().unwrap() as u8);
                        gm.set_shoreline_bit(a0);
                        gm.set_ocean(a0);
                    }
                    "clearShore" => gm.clear_shoreline_bit(a0),
                    "updateTile" => {
                        let changed = gm.update_tile(a0, a[1].as_u64().unwrap() as u32);
                        assert_eq!(changed, m["changed"].as_bool().unwrap());
                    }
                    "fallOwner" => {
                        gm.set_fallout(a0, a[1].as_u64().unwrap() == 1);
                        gm.set_owner_id(a0, a[2].as_u64().unwrap() as u32);
                    }
                    other => panic!("unknown op {other}"),
                }
                let s = &m["snap"];
                assert_eq!(gm.num_land_tiles(), s["land"].as_i64().unwrap());
                assert_eq!(gm.water_version() as u64, s["wv"].as_u64().unwrap());
                assert_eq!(
                    gm.num_tiles_with_fallout() as u64,
                    s["nf"].as_u64().unwrap()
                );
                let terrain: Vec<u32> = (0..tiles).map(|t| gm.terrain_byte(t) as u32).collect();
                assert_eq!(terrain, u32s(&s["terrain"]), "terrain after {}", m["op"]);
                let state: Vec<u32> = gm.tile_state_buffer().iter().map(|&x| x as u32).collect();
                assert_eq!(state, u32s(&s["state"]), "state after {}", m["op"]);
            }
        }
    }

    #[test]
    fn rejects_bad_buffer_length() {
        assert!(GameMapImpl::new(3, 3, vec![0; 8], 0).is_err());
    }
}

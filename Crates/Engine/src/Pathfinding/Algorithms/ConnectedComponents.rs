// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/ConnectedComponents.ts: water component labeling by scan-line flood fill.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream starts with a `Uint8Array` of ids (land marker 0xFF) and promotes it to `Uint16Array`
//! (land marker 0xFFFF) before component 253 is assigned; ids are stored as `u16` here with the
//! same marker switch, so labels are identical.

use crate::Game::GameMap::{GameMapImpl, TileRef};

/// Land marker while ids fit a byte.
pub const LAND_MARKER: u16 = 0xff;
const LAND_MARKER_WIDE: u16 = 0xffff;

/// Connected component labeling for grid maps, with incremental growth for new water tiles.
#[derive(Clone, Debug)]
pub struct ConnectedComponents {
    width: u32,
    num_tiles: usize,
    last_row_start: u32,
    component_ids: Option<Vec<u16>>,
    component_sizes: Vec<u32>,
    parents: Vec<u32>,
    max_id: u32,
    land_marker: u16,
    wide: bool,
}

impl ConnectedComponents {
    /// Creates an uninitialised labeler for `map` (call [`Self::initialize`] before use).
    pub fn new(map: &GameMapImpl) -> Self {
        let width = map.width();
        let height = map.height();
        Self {
            width,
            num_tiles: width as usize * height as usize,
            last_row_start: (height - 1) * width,
            component_ids: None,
            component_sizes: Vec::new(),
            parents: Vec::new(),
            max_id: 0,
            land_marker: LAND_MARKER,
            wide: false,
        }
    }

    /// Labels every water component of `map` (ids from 1 in scan order).
    pub fn initialize(&mut self, map: &GameMapImpl) {
        let mut ids: Vec<u16> = (0..self.num_tiles)
            .map(|i| if map.is_land(i as TileRef) { 0xff } else { 0 })
            .collect();
        let mut queue: Vec<u32> = vec![0; self.num_tiles];
        self.component_sizes = Vec::new();
        self.wide = false;
        let mut next_id: u32 = 0;

        for start in 0..self.num_tiles {
            let value = ids[start];
            if value == LAND_MARKER || value > 0 {
                continue;
            }
            next_id += 1;

            // Promote before assigning component 253 (0xFF would collide with the land marker).
            if next_id == 253 && !self.wide {
                for id in ids.iter_mut() {
                    if *id == LAND_MARKER {
                        *id = LAND_MARKER_WIDE;
                    }
                }
                self.wide = true;
            }
            if next_id == 0xffff {
                break;
            }
            self.flood_fill_component(&mut ids, &mut queue, start as u32, next_id as u16);
        }

        self.component_ids = Some(ids);
        self.land_marker = if self.wide {
            LAND_MARKER_WIDE
        } else {
            LAND_MARKER
        };
        self.max_id = next_id;
        self.parents = (0..=next_id).collect();
    }

    /// Folds newly converted water tiles (already water in the map) into the labeling.
    ///
    /// Tiles are processed in the given order; no-op before [`Self::initialize`].
    pub fn add_water_tiles(&mut self, tiles: &[TileRef]) {
        if self.component_ids.is_none() {
            return;
        }
        for &tile in tiles {
            if self.component_ids.as_ref().unwrap()[tile as usize] != self.land_marker {
                continue; // already labeled water
            }

            let x = tile % self.width;
            let (mut r0, mut r1, mut r2, mut r3) = (0u32, 0u32, 0u32, 0u32);
            if tile >= self.width {
                r0 = self.root_at(tile - self.width);
            }
            if tile < self.last_row_start {
                r1 = self.root_at(tile + self.width);
            }
            if x > 0 {
                r2 = self.root_at(tile - 1);
            }
            if x < self.width - 1 {
                r3 = self.root_at(tile + 1);
            }

            // Canonical id: smallest non-zero root
            let mut canon = 0u32;
            for r in [r0, r1, r2, r3] {
                if r != 0 && (canon == 0 || r < canon) {
                    canon = r;
                }
            }

            if canon == 0 {
                let id = self.alloc_component_id();
                self.component_ids.as_mut().unwrap()[tile as usize] = id as u16;
                self.set_size(id, 1);
                continue;
            }

            self.component_ids.as_mut().unwrap()[tile as usize] = canon as u16;
            let grown = self.size_of(canon) + 1;
            self.set_size(canon, grown);

            for r in [r0, r1, r2, r3] {
                if r != 0 && r != canon && self.parents[r as usize] != canon {
                    self.parents[r as usize] = canon;
                    let merged = self.size_of(canon) + self.size_of(r);
                    self.set_size(canon, merged);
                    self.set_size(r, 0);
                }
            }
        }
    }

    fn size_of(&self, id: u32) -> u32 {
        self.component_sizes.get(id as usize).copied().unwrap_or(0)
    }

    fn set_size(&mut self, id: u32, value: u32) {
        let index = id as usize;
        if self.component_sizes.len() <= index {
            self.component_sizes.resize(index + 1, 0);
        }
        self.component_sizes[index] = value;
    }

    /// Component root of the water tile at `index`, or 0 if land or unlabeled.
    fn root_at(&mut self, index: TileRef) -> u32 {
        let id = self.component_ids.as_ref().unwrap()[index as usize];
        if id == 0 || id == self.land_marker {
            return 0;
        }
        self.find(id as u32)
    }

    fn find(&mut self, id: u32) -> u32 {
        let parents = &mut self.parents;
        let mut root = id;
        while parents[root as usize] != root {
            root = parents[root as usize];
        }
        // Path compression (roots are identical without it)
        let mut cur = id;
        while parents[cur as usize] != root {
            let next = parents[cur as usize];
            parents[cur as usize] = root;
            cur = next;
        }
        root
    }

    fn alloc_component_id(&mut self) -> u32 {
        // Saturate at the last usable id; unreachable in practice.
        if self.max_id >= 0xfffe {
            return self.max_id;
        }
        self.max_id += 1;
        let id = self.max_id;
        if id >= 253 && !self.wide {
            for v in self.component_ids.as_mut().unwrap().iter_mut() {
                if *v == LAND_MARKER {
                    *v = LAND_MARKER_WIDE;
                }
            }
            self.wide = true;
            self.land_marker = LAND_MARKER_WIDE;
        }
        // `this.parents[id] = id`
        if self.parents.len() <= id as usize {
            self.parents.resize(id as usize + 1, 0);
        }
        self.parents[id as usize] = id;
        id
    }

    fn flood_fill_component(
        &mut self,
        ids: &mut [u16],
        queue: &mut [u32],
        start: u32,
        component_id: u16,
    ) {
        let width = self.width;
        let mut head = 0usize;
        let mut tail = 0usize;
        queue[tail] = start;
        tail += 1;

        while head < tail {
            let seed = queue[head];
            head += 1;
            if ids[seed as usize] != 0 {
                continue;
            }

            let row_start = seed - (seed % width);
            let mut left = seed;
            while left > row_start && ids[left as usize - 1] == 0 {
                left -= 1;
            }
            let row_end = row_start + width - 1;
            let mut right = seed;
            while right < row_end && ids[right as usize + 1] == 0 {
                right += 1;
            }

            let span_size = right - left + 1;
            let size = self.size_of(component_id as u32) + span_size;
            self.set_size(component_id as u32, size);
            for x in left..=right {
                ids[x as usize] = component_id;
                if x >= width {
                    let above = x - width;
                    if ids[above as usize] == 0 {
                        queue[tail] = above;
                        tail += 1;
                    }
                }
                if x < self.last_row_start {
                    let below = x + width;
                    if ids[below as usize] == 0 {
                        queue[tail] = below;
                        tail += 1;
                    }
                }
            }
        }
    }

    /// Component id of a tile: 0 for unlabeled, the land marker for land, else the union-find root.
    pub fn get_component_id(&mut self, tile: TileRef) -> u32 {
        let Some(ids) = self.component_ids.as_ref() else {
            return 0;
        };
        let id = ids.get(tile as usize).copied().unwrap_or(0);
        if id == 0 || id == self.land_marker {
            return id as u32;
        }
        self.find(id as u32)
    }

    /// Number of water tiles in the component, or 0 if unknown.
    pub fn get_component_size(&mut self, component_id: u32) -> u32 {
        if component_id == 0 || component_id > self.max_id {
            return 0;
        }
        let root = self.find(component_id);
        self.size_of(root)
    }
}

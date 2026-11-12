// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/WaterManager.ts: water conversions (nukes), terrain fix-up and the throttled water graph rebuild.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Ownership differs from upstream: the manager owns the mini map (nobody else mutates it) and the
//! caller passes the game map to every call that reads or writes it, because `GameImpl` owns that map.

use crate::Compat::OrderedSet::OrderedSet;
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Algorithms::AStarWater::AStarWater;
use crate::Pathfinding::Algorithms::AStarWaterHierarchical::AStarWaterHierarchical;
use crate::Pathfinding::Algorithms::AbstractGraph::{AbstractGraph, AbstractGraphBuilder};
use crate::Pathfinding::Algorithms::BfsGrid::BfsGrid;
use crate::Pathfinding::Algorithms::ConnectedComponents::ConnectedComponents;
use crate::Pathfinding::PathFinder::{
    ChainRef, SharedWaterChain, SimpleWaterChain, build_hierarchical_chain, build_simple_chain,
};
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const WATER_GRAPH_REBUILD_INTERVAL: u32 = 20;

/// Max BFS hops from a coastline that can affect a magnitude: `ceil(dist / 2)` capped at 31, so 62 hops.
const MAX_MAG_DIST: i64 = 62;

const TERRAIN_LAND_MASK: u8 = 0x80;
const TERRAIN_MAG_MASK: u8 = 0x1f;
const IMPASSABLE_MAG: u8 = 31;

/// Bounding box of one crater (inclusive).
#[derive(Clone, Copy, Debug)]
struct CraterBounds {
    min_x: i64,
    max_x: i64,
    min_y: i64,
    max_y: i64,
}

/// Stamp-based distance scratch of one magnitude BFS (map-sized).
#[derive(Clone, Debug, Default)]
struct MagnitudeScratch {
    dist: Vec<u16>,
    stamp_arr: Vec<u16>,
    stamp: u32,
}

impl MagnitudeScratch {
    fn ensure(&mut self, total: usize) {
        if self.dist.len() != total || self.stamp_arr.is_empty() {
            self.dist = vec![0; total];
            self.stamp_arr = vec![0; total];
            self.stamp = 0;
        }
    }

    fn bump(&mut self) -> u16 {
        self.stamp += 1;
        if self.stamp >= 0xffff {
            self.stamp_arr.fill(0);
            self.stamp = 1;
        }
        self.stamp as u16
    }
}

/// Water conversions plus the mini map's water graph and shared water path chain.
pub struct WaterManager {
    mini_map: GameMapImpl,
    disable_nav_mesh: bool,
    sort_edges: bool,
    /// v0.35.3 on: a water nuke converts a mini tile when any of its four tiles is water.
    pub mini_tile_any_water: bool,
    mini_water_graph: Option<Rc<AbstractGraph>>,
    // Present exactly when the nav mesh is enabled (upstream `_miniWaterHPA`)
    // Chains keep search scratch only; RefCell lets `&GameImpl` queries (canBuild) run them
    hierarchical_chain: Option<ChainRef>,
    simple_chain: RefCell<Option<SimpleWaterChain>>,
    mini_water_cc: Option<Rc<RefCell<ConnectedComponents>>>,
    legacy: bool,
    old_markers: bool,
    /// Before PR 4149 the abstract graph A* queues with a bucket queue.
    bucket_queue: bool,
    builder_bfs: Option<BfsGrid>,
    water_graph_version: u32,
    water_graph_dirty: bool,
    water_graph_last_rebuild_tick: u32,
    pending_water_tiles: OrderedSet<TileRef>,
    dirty_mini_tiles: OrderedSet<TileRef>,
    full_scratch: MagnitudeScratch,
    mini_scratch: MagnitudeScratch,
}

impl WaterManager {
    /// Creates the manager; with the nav mesh enabled it builds the initial graph and HPA* finder.
    pub fn new(mini_map: GameMapImpl, disable_nav_mesh: bool) -> Self {
        Self::new_with(mini_map, disable_nav_mesh, false, false, false)
    }

    /// Like [`Self::new`]; `sort_edges` renumbers graph edges by node pair (v0.35.0 on), `old_markers`
    /// selects the component labeling of builds before PR 4479.
    pub fn new_with(
        mini_map: GameMapImpl,
        disable_nav_mesh: bool,
        sort_edges: bool,
        old_markers: bool,
        bucket_queue: bool,
    ) -> Self {
        let mut manager = Self {
            sort_edges,
            mini_tile_any_water: false,
            mini_map,
            disable_nav_mesh,
            mini_water_graph: None,
            hierarchical_chain: None,
            simple_chain: RefCell::new(None),
            mini_water_cc: None,
            legacy: false,
            old_markers,
            bucket_queue,
            builder_bfs: None,
            water_graph_version: 0,
            water_graph_dirty: false,
            water_graph_last_rebuild_tick: 0,
            pending_water_tiles: OrderedSet::new(),
            dirty_mini_tiles: OrderedSet::new(),
            full_scratch: MagnitudeScratch::default(),
            mini_scratch: MagnitudeScratch::default(),
        };
        if !disable_nav_mesh {
            let mini = &manager.mini_map;
            let mut cc = ConnectedComponents::new(mini);
            cc.set_old_markers(old_markers);
            cc.initialize(mini);
            let cc = Rc::new(RefCell::new(cc));
            let mut bfs = BfsGrid::new(mini.width() as usize * mini.height() as usize);
            let graph = Rc::new(
                AbstractGraphBuilder::new(
                    mini,
                    AbstractGraphBuilder::CLUSTER_SIZE,
                    None,
                    None,
                    Some(Rc::clone(&cc)),
                    &mut bfs,
                )
                .with_sorted_edges(sort_edges)
                .build(),
            );
            let mut hpa = AStarWaterHierarchical::new(mini, Rc::clone(&graph), true);
            hpa.set_component_filter(old_markers);
            hpa.set_bucket_queue(bucket_queue);
            manager.hierarchical_chain = Some(Rc::new(SharedWaterChain {
                use_hierarchical: graph.node_count() >= 100,
                hierarchical: RefCell::new(build_hierarchical_chain(mini, hpa, Rc::clone(&graph))),
            }));
            manager.mini_water_cc = Some(cc);
            manager.builder_bfs = Some(bfs);
            manager.mini_water_graph = Some(graph);
        }
        manager
    }

    /// Switches to the pre-PR-4975 behaviour: fresh components per graph rebuild, no mini map
    /// ocean/magnitude fix, and a rebuild may share the tick of a conversion.
    pub fn set_legacy(&mut self) {
        self.legacy = true;
        self.mini_water_cc = None;
    }

    /// The mini map (2x down-scaled game map) used for pathfinding.
    pub fn mini_map(&self) -> &GameMapImpl {
        &self.mini_map
    }

    /// Queues a land tile for conversion to water at the next [`Self::tick`].
    pub fn queue_tile(&mut self, tile: TileRef) {
        self.pending_water_tiles.insert(tile);
    }

    /// Flushes pending conversions (terrain fix-up on both maps) and runs the throttled graph rebuild.
    ///
    /// Returns the tiles whose terrain changed, in upstream's order (for recording).
    pub fn tick(&mut self, map: &mut GameMapImpl, current_tick: u32) -> Vec<TileRef> {
        let mut changed_tiles: Vec<TileRef> = Vec::new();
        let mut converted_this_tick = false;

        if !self.pending_water_tiles.is_empty() {
            let mut converted: Vec<TileRef> = Vec::new();
            for &tile in self.pending_water_tiles.iter() {
                // The tile may have been conquered between queueing and flushing
                if map.is_land(tile) && !map.has_owner(tile) && !map.is_impassable(tile) {
                    if map.has_fallout(tile) {
                        map.set_fallout(tile, false);
                    }
                    map.set_water(tile);
                    converted.push(tile);
                }
            }
            self.pending_water_tiles.clear();
            if !converted.is_empty() {
                converted_this_tick = true;
                self.finalize_water_changes(map, &converted, &mut changed_tiles);
            }
        }

        // Throttled rebuild: at most once every 20 ticks, never in the tick of a conversion
        if self.water_graph_dirty
            && !self.disable_nav_mesh
            && (self.legacy || !converted_this_tick)
            && current_tick.wrapping_sub(self.water_graph_last_rebuild_tick)
                >= WATER_GRAPH_REBUILD_INTERVAL
        {
            self.water_graph_dirty = false;
            self.water_graph_last_rebuild_tick = current_tick;
            let dirty: Vec<TileRef> = self.dirty_mini_tiles.iter().copied().collect();
            let graph = {
                let old = self.mini_water_graph.as_deref();
                let builder = AbstractGraphBuilder::new(
                    &self.mini_map,
                    AbstractGraphBuilder::CLUSTER_SIZE,
                    old,
                    if dirty.is_empty() { None } else { Some(&dirty) },
                    self.mini_water_cc.clone(),
                    self.builder_bfs.as_mut().expect("nav mesh enabled"),
                )
                .with_sorted_edges(self.sort_edges)
                .with_old_markers(self.old_markers);
                Rc::new(builder.build())
            };
            self.dirty_mini_tiles.clear();
            if self.hierarchical_chain.is_some() {
                // A new finder and chain per version; finders created earlier keep theirs
                let mut hpa = AStarWaterHierarchical::new(&self.mini_map, Rc::clone(&graph), true);
                hpa.set_component_filter(self.old_markers);
                hpa.set_bucket_queue(self.bucket_queue);
                self.hierarchical_chain = Some(Rc::new(SharedWaterChain {
                    use_hierarchical: graph.node_count() >= 100,
                    hierarchical: RefCell::new(build_hierarchical_chain(
                        &self.mini_map,
                        hpa,
                        Rc::clone(&graph),
                    )),
                }));
            }
            self.mini_water_graph = Some(graph);
            self.water_graph_version += 1;
        }

        changed_tiles
    }

    /// Bumped on every graph rebuild.
    pub fn water_graph_version(&self) -> u32 {
        self.water_graph_version
    }

    /// True when the HPA* finder exists (nav mesh enabled).
    pub fn has_mini_water_hpa(&self) -> bool {
        self.hierarchical_chain.is_some()
    }

    /// The current abstract water graph.
    pub fn mini_water_graph(&self) -> Option<&Rc<AbstractGraph>> {
        self.mini_water_graph.as_ref()
    }

    /// The chain of the current graph version, `None` without a nav mesh.
    pub fn current_chain(&self) -> Option<ChainRef> {
        self.hierarchical_chain.clone()
    }

    /// Water path through the current shared chain (upstream `sharedWaterChain(game)`); `from` keeps its single/array form.
    pub fn find_water_path(
        &self,
        map: &GameMapImpl,
        from: Source,
        to: TileRef,
    ) -> Option<Vec<TileRef>> {
        self.find_water_path_with(None, map, from, to)
    }

    /// Like [`Self::find_water_path`] through the chain of an older graph version (`None` is the current one).
    ///
    /// Picks the HPA* chain when its graph had at least 100 nodes, else the plain A* chain on the mini map.
    pub fn find_water_path_with(
        &self,
        chain: Option<&ChainRef>,
        map: &GameMapImpl,
        from: Source,
        to: TileRef,
    ) -> Option<Vec<TileRef>> {
        let chain = chain.or(self.hierarchical_chain.as_ref());
        match chain {
            Some(chain) if chain.use_hierarchical => {
                let ctx = PathCtx {
                    map,
                    mini_map: &self.mini_map,
                };
                chain.hierarchical.borrow_mut().find_path(&ctx, from, to)
            }
            _ => self.find_water_path_simple(map, from, to),
        }
    }

    /// Water path through the simple chain (plain A* on the mini map), as used without a nav mesh.
    pub fn find_water_path_simple(
        &self,
        map: &GameMapImpl,
        from: Source,
        to: TileRef,
    ) -> Option<Vec<TileRef>> {
        let ctx = PathCtx {
            map,
            mini_map: &self.mini_map,
        };
        let mini = &self.mini_map;
        self.simple_chain
            .borrow_mut()
            .get_or_insert_with(|| build_simple_chain(AStarWater::new(mini, None, None)))
            .find_path(&ctx, from, to)
    }

    /// Mini map tile under a game map tile.
    fn mini_tile_of(&self, map: &GameMapImpl, tile: TileRef) -> TileRef {
        self.mini_map
            .tile_ref((map.x(tile) / 2) as i32, (map.y(tile) / 2) as i32)
    }

    /// Water component of a game map tile (upstream `getWaterComponent`); `Some(0)` without a nav mesh.
    pub fn get_water_component(&self, map: &GameMapImpl, tile: TileRef) -> Option<u32> {
        // Permissive fallback for tests with disableNavMesh
        let Some(graph) = self.mini_water_graph.as_ref() else {
            return Some(0);
        };
        let mini = &self.mini_map;
        let mini_tile = self.mini_tile_of(map, tile);

        if mini.is_water(mini_tile) {
            return Some(graph.get_component_id(mini_tile));
        }

        // Shore tile: a water neighbour (the mini map loses resolution)
        let neighbors = mini.neighbors(mini_tile);
        for &n in &neighbors {
            if mini.is_water(n) {
                return Some(graph.get_component_id(n));
            }
        }

        // Extended search: 2-hop neighbours for narrow straits
        for &n in &neighbors {
            for n2 in mini.neighbors(n) {
                if mini.is_water(n2) {
                    return Some(graph.get_component_id(n2));
                }
            }
        }
        None
    }

    /// True when the tile touches the given water component (upstream `hasWaterComponent`).
    pub fn has_water_component(&self, map: &GameMapImpl, tile: TileRef, component: u32) -> bool {
        // Permissive fallback for tests with disableNavMesh
        let Some(graph) = self.mini_water_graph.as_ref() else {
            return true;
        };
        let mini = &self.mini_map;
        let mini_tile = self.mini_tile_of(map, tile);

        // The shore tile of the game map may be water in the mini map
        if mini.is_water(mini_tile) && graph.get_component_id(mini_tile) == component {
            return true;
        }

        let neighbors = mini.neighbors(mini_tile);
        for &n in &neighbors {
            if mini.is_water(n) && graph.get_component_id(n) == component {
                return true;
            }
        }

        for &n in &neighbors {
            for n2 in mini.neighbors(n) {
                if mini.is_water(n2) && graph.get_component_id(n2) == component {
                    return true;
                }
            }
        }
        false
    }

    /// Approximate water tiles of the tile's component in game map units (mini tiles times 4); `Some(0)` without a nav mesh.
    pub fn get_water_component_size(&self, map: &GameMapImpl, tile: TileRef) -> Option<u32> {
        let component = self.get_water_component(map, tile)?;
        let Some(graph) = self.mini_water_graph.as_ref() else {
            return Some(0);
        };
        Some(graph.get_component_size(component) * 4)
    }

    fn finalize_water_changes(
        &mut self,
        map: &mut GameMapImpl,
        converted_tiles: &[TileRef],
        changed_tiles: &mut Vec<TileRef>,
    ) {
        let mut converted: OrderedSet<TileRef> = OrderedSet::new();
        for &t in converted_tiles {
            converted.insert(t);
        }
        if converted.is_empty() {
            return;
        }

        let w = map.width();
        let total_tiles = w * map.height();

        // Changed tiles dedupe in insertion order and drain into the output at the end
        let mut changed: OrderedSet<TileRef> = OrderedSet::new();
        for &tile in converted.iter() {
            changed.insert(tile);
        }

        // Cardinal neighbours written into `out` from `start`; returns the new end
        let push_neighbors = |tile: TileRef, out: &mut [TileRef; 8], mut start: usize| -> usize {
            if tile >= w {
                out[start] = tile - w;
                start += 1;
            }
            if tile < total_tiles - w {
                out[start] = tile + w;
                start += 1;
            }
            let x = tile % w;
            if x > 0 {
                out[start] = tile - 1;
                start += 1;
            }
            if x < w - 1 {
                out[start] = tile + 1;
                start += 1;
            }
            start
        };

        let mut nb: [TileRef; 8] = [0; 8];

        // 1. Propagate the ocean bit
        let mut ocean_queue: Vec<TileRef> = Vec::new();
        for &tile in converted.iter() {
            let end = push_neighbors(tile, &mut nb, 0);
            for &n in &nb[..end] {
                if !converted.contains(&n) && map.is_ocean(n) {
                    map.set_ocean(tile);
                    ocean_queue.push(tile);
                    break;
                }
            }
        }
        let mut o_head = 0;
        while o_head < ocean_queue.len() {
            let tile = ocean_queue[o_head];
            o_head += 1;
            let end = push_neighbors(tile, &mut nb, 0);
            for &n in &nb[..end] {
                if map.is_water(n) && !map.is_ocean(n) {
                    map.set_ocean(n);
                    changed.insert(n);
                    ocean_queue.push(n);
                }
            }
        }

        // 2. Recompute magnitudes by BFS from the remaining land outward, per crater group
        self.full_scratch.ensure(total_tiles as usize);
        let groups = if self.legacy {
            vec![bounding_box(&converted, w)]
        } else {
            compute_crater_groups(&converted, w, MAX_MAG_DIST)
        };
        for g in &groups {
            let stamp = self.full_scratch.bump();
            recompute_magnitudes_in_box(
                map,
                g,
                &mut self.full_scratch,
                stamp,
                true,
                Some(&mut changed),
            );
        }

        // 3. Fix shoreline bits: only converted tiles and their 2-ring can change
        let shore_stamp = self.full_scratch.bump();
        let mut tiles_to_check: Vec<TileRef> = Vec::new();
        {
            let stamp_arr = &mut self.full_scratch.stamp_arr;
            let mut push_to_check = |tile: TileRef| {
                if stamp_arr[tile as usize] != shore_stamp {
                    stamp_arr[tile as usize] = shore_stamp;
                    tiles_to_check.push(tile);
                }
            };
            for &tile in converted.iter() {
                push_to_check(tile);
                let end = push_neighbors(tile, &mut nb, 0);
                for i in 0..end {
                    push_to_check(nb[i]);
                    let end2 = push_neighbors(nb[i], &mut nb, end);
                    for &n in &nb[end..end2] {
                        push_to_check(n);
                    }
                }
            }
        }
        for &tile in &tiles_to_check {
            // Impassable tiles never get a shoreline
            if map.is_impassable(tile) {
                if map.is_shoreline(tile) {
                    map.clear_shoreline_bit(tile);
                    changed.insert(tile);
                }
                continue;
            }
            let tile_is_land = map.is_land(tile);
            let mut has_opposite = false;
            let end = push_neighbors(tile, &mut nb, 0);
            for &n in &nb[..end] {
                // Impassable neighbours are void, not coast
                if map.is_impassable(n) {
                    continue;
                }
                if map.is_land(n) != tile_is_land {
                    has_opposite = true;
                    break;
                }
            }
            let old_shoreline = map.is_shoreline(tile);
            if has_opposite {
                if !old_shoreline {
                    map.set_shoreline_bit(tile);
                    changed.insert(tile);
                }
            } else if old_shoreline {
                map.clear_shoreline_bit(tile);
                changed.insert(tile);
            }
        }

        // 4. Update the mini map terrain
        let mut mini_tiles_to_check: OrderedSet<TileRef> = OrderedSet::new();
        let mut converted_mini_tiles: OrderedSet<TileRef> = OrderedSet::new();
        for &tile in converted.iter() {
            let mini_x = (map.x(tile) / 2) as i32;
            let mini_y = (map.y(tile) / 2) as i32;
            if self.mini_map.is_valid_coord(mini_x, mini_y) {
                mini_tiles_to_check.insert(self.mini_map.tile_ref(mini_x, mini_y));
            }
        }
        for &mini_tile in mini_tiles_to_check.iter() {
            if !self.mini_map.is_land(mini_tile) {
                continue;
            }
            let fx = (self.mini_map.x(mini_tile) * 2) as i32;
            let fy = (self.mini_map.y(mini_tile) * 2) as i32;
            let mut water_count = 0;
            let mut total_count = 0;
            for dy in 0..2 {
                for dx in 0..2 {
                    if map.is_valid_coord(fx + dx, fy + dy) {
                        total_count += 1;
                        if map.is_water(map.tile_ref(fx + dx, fy + dy)) {
                            water_count += 1;
                        }
                    }
                }
            }
            let convert = if self.mini_tile_any_water {
                water_count > 0
            } else {
                water_count >= 3.min(total_count)
            };
            if convert {
                self.mini_map.set_water(mini_tile);
                converted_mini_tiles.insert(mini_tile);
            }
        }

        // 4b. Fix the mini map ocean bit and magnitude of converted tiles (setWater zeroes both)
        if !self.legacy && !converted_mini_tiles.is_empty() {
            let mini_w = self.mini_map.width();
            let mini_h = self.mini_map.height();

            let push_mini_neighbors = |tile: TileRef, out: &mut [TileRef; 4]| -> usize {
                let x = tile % mini_w;
                let y = (tile - x) / mini_w;
                let mut count = 0;
                if y > 0 {
                    out[count] = tile - mini_w;
                    count += 1;
                }
                if y < mini_h - 1 {
                    out[count] = tile + mini_w;
                    count += 1;
                }
                if x > 0 {
                    out[count] = tile - 1;
                    count += 1;
                }
                if x < mini_w - 1 {
                    out[count] = tile + 1;
                    count += 1;
                }
                count
            };
            let mut mini_nb: [TileRef; 4] = [0; 4];

            // 4b-i. Ocean bit by BFS, so chains of converted tiles that reach the ocean are all marked
            let mut mini_ocean_queue: Vec<TileRef> = Vec::new();
            for &mt in converted_mini_tiles.iter() {
                let nc = push_mini_neighbors(mt, &mut mini_nb);
                let near_ocean = mini_nb[..nc].iter().any(|&n| self.mini_map.is_ocean(n));
                if near_ocean {
                    self.mini_map.set_ocean(mt);
                    mini_ocean_queue.push(mt);
                }
            }
            let mut mo_head = 0;
            while mo_head < mini_ocean_queue.len() {
                let tile = mini_ocean_queue[mo_head];
                mo_head += 1;
                let nc = push_mini_neighbors(tile, &mut mini_nb);
                for &n in &mini_nb[..nc] {
                    if self.mini_map.is_water(n) && !self.mini_map.is_ocean(n) {
                        self.mini_map.set_ocean(n);
                        mini_ocean_queue.push(n);
                    }
                }
            }

            // 4b-ii. Magnitude by BFS from coastlines on the mini map (any land is coast there)
            let mini_total = (mini_w * mini_h) as usize;
            self.mini_scratch.ensure(mini_total);
            let mini_groups = compute_crater_groups(&converted_mini_tiles, mini_w, MAX_MAG_DIST);
            for g in &mini_groups {
                let stamp = self.mini_scratch.bump();
                recompute_magnitudes_in_box(
                    &mut self.mini_map,
                    g,
                    &mut self.mini_scratch,
                    stamp,
                    false,
                    None,
                );
            }
        }

        // 5. Mark the water graph dirty (rebuilt lazily, throttled)
        if !converted_mini_tiles.is_empty() {
            let mini_tiles: Vec<TileRef> = converted_mini_tiles.iter().copied().collect();
            if let Some(cc) = &self.mini_water_cc {
                cc.borrow_mut().add_water_tiles(&mini_tiles);
            }
            self.water_graph_dirty = true;
            for mt in mini_tiles {
                self.dirty_mini_tiles.insert(mt);
            }
        }

        changed_tiles.extend(changed.iter().copied());
    }
}

/// Bounding box of all converted tiles (the pre-PR-4979 single dirty box).
fn bounding_box(converted: &OrderedSet<TileRef>, w: u32) -> CraterBounds {
    let mut b = CraterBounds {
        min_x: w as i64,
        max_x: 0,
        min_y: i64::MAX,
        max_y: 0,
    };
    for &tile in converted.iter() {
        let x = (tile % w) as i64;
        let y = (tile / w) as i64;
        b.min_x = b.min_x.min(x);
        b.max_x = b.max_x.max(x);
        b.min_y = b.min_y.min(y);
        b.max_y = b.max_y.max(y);
    }
    b
}

/// Groups converted tiles into spatially separate craters (cells of `max_mag_dist`, 8-adjacent cells
/// unioned, then groups merged while the union's seed box is cheaper than the separate boxes).
fn compute_crater_groups(
    converted: &OrderedSet<TileRef>,
    w: u32,
    max_mag_dist: i64,
) -> Vec<CraterBounds> {
    let w = w as i64;
    let cell_size = max_mag_dist;
    let cells_x = (w + cell_size - 1) / cell_size;

    // Bucket tiles into coarse cells, tracking per-cell bounds (insertion order of cells matters)
    let mut cell_keys: Vec<i64> = Vec::new();
    let mut cells: HashMap<i64, CraterBounds> = HashMap::new();
    for &tile in converted.iter() {
        let tx = tile as i64 % w;
        let ty = (tile as i64 - tx) / w;
        let key = (ty / cell_size) * cells_x + tx / cell_size;
        match cells.get_mut(&key) {
            None => {
                cell_keys.push(key);
                cells.insert(
                    key,
                    CraterBounds {
                        min_x: tx,
                        max_x: tx,
                        min_y: ty,
                        max_y: ty,
                    },
                );
            }
            Some(b) => {
                b.min_x = b.min_x.min(tx);
                b.max_x = b.max_x.max(tx);
                b.min_y = b.min_y.min(ty);
                b.max_y = b.max_y.max(ty);
            }
        }
    }

    // Union-find over occupied cells; 8-adjacent cells belong to the same crater cluster
    let mut parent: HashMap<i64, i64> = cell_keys.iter().map(|&k| (k, k)).collect();
    fn find(parent: &mut HashMap<i64, i64>, k: i64) -> i64 {
        let mut root = k;
        while parent[&root] != root {
            root = parent[&root];
        }
        let mut cur = k;
        while parent[&cur] != root {
            let next = parent[&cur];
            parent.insert(cur, root);
            cur = next;
        }
        root
    }
    for &key in &cell_keys {
        let cx = key % cells_x;
        let cy = (key - cx) / cells_x;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || nx >= cells_x || ny < 0 {
                    continue;
                }
                let nk = ny * cells_x + nx;
                if !cells.contains_key(&nk) {
                    continue;
                }
                let ra = find(&mut parent, key);
                let rb = find(&mut parent, nk);
                if ra != rb {
                    parent.insert(ra.max(rb), ra.min(rb));
                }
            }
        }
    }

    // Merge cell bounds per root (first-appearance order)
    let mut root_order: Vec<i64> = Vec::new();
    let mut by_root: HashMap<i64, CraterBounds> = HashMap::new();
    for &key in &cell_keys {
        let b = cells[&key];
        let root = find(&mut parent, key);
        match by_root.get_mut(&root) {
            None => {
                root_order.push(root);
                by_root.insert(root, b);
            }
            Some(g) => {
                g.min_x = g.min_x.min(b.min_x);
                g.max_x = g.max_x.max(b.max_x);
                g.min_y = g.min_y.min(b.min_y);
                g.max_y = g.max_y.max(b.max_y);
            }
        }
    }
    let mut groups: Vec<CraterBounds> = root_order.iter().map(|r| by_root[r]).collect();

    // Greedily merge groups while the merged seed box is cheaper than the two separate ones
    let pad = 4 * max_mag_dist; // the seed box adds 2 * max_mag_dist on each side
    let box_area = |g: &CraterBounds| (g.max_x - g.min_x + pad) * (g.max_y - g.min_y + pad);
    let mut merged = true;
    while merged {
        merged = false;
        let mut i = 0;
        while i < groups.len() && !merged {
            for j in i + 1..groups.len() {
                let a = groups[i];
                let b = groups[j];
                let union = CraterBounds {
                    min_x: a.min_x.min(b.min_x),
                    max_x: a.max_x.max(b.max_x),
                    min_y: a.min_y.min(b.min_y),
                    max_y: a.max_y.max(b.max_y),
                };
                if box_area(&union) < box_area(&a) + box_area(&b) {
                    groups[i] = union;
                    groups.remove(j);
                    merged = true;
                    break;
                }
            }
            i += 1;
        }
    }
    groups
}

/// Recomputes water magnitudes (`ceil(dist_to_coast / 2)`, capped at 31) within `MAX_MAG_DIST` of the crater.
///
/// Coastlines inside the seed box (2x that distance) are BFS seeds; only dirty-box tiles are updated.
/// With `passable_coast_only`, impassable terrain is void rather than coastline (full map only).
fn recompute_magnitudes_in_box(
    map: &mut GameMapImpl,
    bounds: &CraterBounds,
    scratch: &mut MagnitudeScratch,
    stamp: u16,
    passable_coast_only: bool,
    mut changed: Option<&mut OrderedSet<TileRef>>,
) {
    let w = map.width() as i64;
    let h = map.height() as i64;

    // Dirty box: tiles whose magnitude may change
    let d_min_x = 0.max(bounds.min_x - MAX_MAG_DIST);
    let d_max_x = (w - 1).min(bounds.max_x + MAX_MAG_DIST);
    let d_min_y = 0.max(bounds.min_y - MAX_MAG_DIST);
    let d_max_y = (h - 1).min(bounds.max_y + MAX_MAG_DIST);
    // Seed box: coastlines seeded here; the BFS is clipped here
    let s_min_x = 0.max(bounds.min_x - MAX_MAG_DIST * 2);
    let s_max_x = (w - 1).min(bounds.max_x + MAX_MAG_DIST * 2);
    let s_min_y = 0.max(bounds.min_y - MAX_MAG_DIST * 2);
    let s_max_y = (h - 1).min(bounds.max_y + MAX_MAG_DIST * 2);

    let is_coast_byte = |b: u8| -> bool {
        (b & TERRAIN_LAND_MASK) != 0
            && (!passable_coast_only || (b & TERRAIN_MAG_MASK) != IMPASSABLE_MAG)
    };

    let stamp_arr = &mut scratch.stamp_arr;
    let dist_arr = &mut scratch.dist;

    // Seed from coastline water tiles inside the seed box
    let mut queue: Vec<TileRef> = Vec::new();
    for y in s_min_y..=s_max_y {
        let row_start = y * w;
        for x in s_min_x..=s_max_x {
            let tile = (row_start + x) as TileRef;
            if (map.terrain_byte(tile) & TERRAIN_LAND_MASK) != 0 {
                continue;
            }
            if stamp_arr[tile as usize] == stamp {
                continue;
            }
            let is_coast = (y > 0 && is_coast_byte(map.terrain_byte(tile - w as u32)))
                || (y < h - 1 && is_coast_byte(map.terrain_byte(tile + w as u32)))
                || (x > 0 && is_coast_byte(map.terrain_byte(tile - 1)))
                || (x < w - 1 && is_coast_byte(map.terrain_byte(tile + 1)));
            if is_coast {
                stamp_arr[tile as usize] = stamp;
                dist_arr[tile as usize] = 0;
                queue.push(tile);
            }
        }
    }

    // BFS outward through water, clipped to the seed box
    let mut head = 0;
    while head < queue.len() {
        let tile = queue[head];
        head += 1;
        let next_dist = dist_arr[tile as usize].wrapping_add(1);
        let x = tile as i64 % w;
        let y = (tile as i64 - x) / w;
        let mut visit = |n: TileRef, queue: &mut Vec<TileRef>| {
            if (map.terrain_byte(n) & TERRAIN_LAND_MASK) == 0 && stamp_arr[n as usize] != stamp {
                stamp_arr[n as usize] = stamp;
                dist_arr[n as usize] = next_dist;
                queue.push(n);
            }
        };
        if y > s_min_y {
            visit(tile - w as u32, &mut queue);
        }
        if y < s_max_y {
            visit(tile + w as u32, &mut queue);
        }
        if x > s_min_x {
            visit(tile - 1, &mut queue);
        }
        if x < s_max_x {
            visit(tile + 1, &mut queue);
        }
    }

    // Update magnitudes of dirty-box tiles only
    for y in d_min_y..=d_max_y {
        let row_start = y * w;
        for x in d_min_x..=d_max_x {
            let tile = (row_start + x) as TileRef;
            let b = map.terrain_byte(tile);
            if (b & TERRAIN_LAND_MASK) != 0 {
                continue;
            }
            // Unreached: the nearest coast is farther than MAX_MAG_DIST, so deep water (31)
            let new_mag: u8 = if stamp_arr[tile as usize] == stamp {
                (dist_arr[tile as usize] as u32).div_ceil(2).min(31) as u8
            } else {
                31
            };
            if (b & TERRAIN_MAG_MASK) != new_mag {
                map.set_magnitude(tile, new_mag);
                if let Some(c) = changed.as_deref_mut() {
                    c.insert(tile);
                }
            }
        }
    }
}

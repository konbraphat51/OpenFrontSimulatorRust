// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/PathFinder.ts: the shared water chain, `WaterPathFinder` and the steppers executions use.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream builds one shared transformer chain per water graph version and caches it on the game;
//! a finder keeps the chain of the version it was created or refreshed under (staggered ships keep
//! querying the old graph), so chains are `Rc` handles that [`WaterManager`] hands out, and the
//! stepping wrappers take a [`WaterStepperEnv`] per call. `PathFinding.Rail`/`Stations`
//! (`AStarRail`, `StationPathFinder`) belong to the rail phase and are not ported yet.
//! `WaterPathMemo` is a pure LRU cache of `findPath` answers and is not ported: it cannot change results.

use super::Algorithms::AStarWater::AStarWater;
use super::Algorithms::AStarWaterHierarchical::AStarWaterHierarchical;
use super::Algorithms::AbstractGraph::AbstractGraph;
use super::PathFinderAir::AirPathFinder;
use super::PathFinderStepper::{PathFinderStepper, StepperEnv};
use super::Transformers::ComponentCheckTransformer::ComponentCheckTransformer;
use super::Transformers::MiniMapTransformer::MiniMapTransformer;
use super::Transformers::ShoreCoercingTransformer::ShoreCoercingTransformer;
use super::Transformers::SmoothingWaterTransformer::SmoothingWaterTransformer;
use super::Types::{PathResult, Source};
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Game::WaterManager::WaterManager;
use std::cell::RefCell;
use std::rc::Rc;

/// HPA* chain: minimap, shore coercion, smoothing, component check, hierarchical A*.
pub type HierarchicalWaterChain = MiniMapTransformer<
    ShoreCoercingTransformer<
        SmoothingWaterTransformer<ComponentCheckTransformer<AStarWaterHierarchical>>,
    >,
>;

/// Fallback chain for tiny graphs (under 100 nodes) or without a nav mesh: minimap, shore coercion, plain water A*.
pub type SimpleWaterChain = MiniMapTransformer<ShoreCoercingTransformer<AStarWater>>;

/// Water pathfinding parameters that differ in builds before PRs 2937 and 2959.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaterTweaks {
    /// Shore endpoints coerce to the first water neighbour (older) instead of the best connected one.
    pub first_shore_neighbor: bool,
    /// Both smoothing passes use the same minimum magnitude (older).
    pub single_los_magnitude: bool,
    /// Edge length of the short path search area (260 now, 140 older).
    pub short_path_size: usize,
}

impl Default for WaterTweaks {
    fn default() -> Self {
        Self {
            first_shore_neighbor: false,
            single_los_magnitude: false,
            short_path_size: 260,
        }
    }
}

/// Builds the HPA* chain around `hpa` (upstream `buildWaterChain`, hierarchical branch).
pub fn build_hierarchical_chain(
    mini_map: &GameMapImpl,
    mut hpa: AStarWaterHierarchical,
    graph: Rc<AbstractGraph>,
    tweaks: WaterTweaks,
) -> HierarchicalWaterChain {
    if tweaks.short_path_size != 260 {
        hpa.set_short_path_size(mini_map, tweaks.short_path_size);
    }
    MiniMapTransformer::new(
        ShoreCoercingTransformer::new(
            SmoothingWaterTransformer::new(ComponentCheckTransformer::new(hpa, graph), mini_map)
                .with_single_magnitude(tweaks.single_los_magnitude),
        )
        .with_first_neighbor(tweaks.first_shore_neighbor),
    )
}

/// Builds the simple chain around `core` (upstream `buildWaterChain`, fallback branch).
pub fn build_simple_chain(core: AStarWater, tweaks: WaterTweaks) -> SimpleWaterChain {
    MiniMapTransformer::new(
        ShoreCoercingTransformer::new(core).with_first_neighbor(tweaks.first_shore_neighbor),
    )
}

/// Swaps the graph inside a hierarchical chain (`AStarWaterHierarchical.setGraph` plus the component check's graph).
pub fn set_chain_graph(chain: &mut HierarchicalWaterChain, graph: Rc<AbstractGraph>) {
    let component_check = chain.inner_mut().inner_mut().inner_mut();
    component_check.set_graph(Rc::clone(&graph));
    component_check.inner_mut().set_graph(graph);
}

/// The chain of one water graph version (upstream's `_waterChainCache` entry).
///
/// Upstream's HPA finder is one object owned by the water manager whose graph is swapped on rebuild,
/// so every version's chain searches the current graph; only the simple-or-HPA choice is per version.
pub struct SharedWaterChain {
    /// The HPA* chain shared by all versions.
    pub hierarchical: Rc<RefCell<HierarchicalWaterChain>>,
    /// False when the graph had under 100 nodes at build time: queries then use the simple chain.
    pub use_hierarchical: bool,
}

impl std::fmt::Debug for SharedWaterChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SharedWaterChain(hierarchical: {})",
            self.use_hierarchical
        )
    }
}

/// Handle to the chain of one graph version.
pub type ChainRef = Rc<SharedWaterChain>;

/// Everything a water stepper needs per call: the water manager (chains, graph version) and the game map.
pub struct WaterStepperEnv<'a> {
    /// Owner of the shared water chains.
    pub water: &'a mut WaterManager,
    /// The game map.
    pub map: &'a GameMapImpl,
    /// Chain of the calling finder's graph version; `None` means the current chain.
    pub chain: Option<ChainRef>,
}

impl StepperEnv<TileRef> for WaterStepperEnv<'_> {
    fn distance(&self, a: TileRef, b: TileRef) -> Option<f64> {
        Some(self.map.manhattan_dist(a, b) as f64)
    }

    fn has_pre_check(&self) -> bool {
        true
    }

    fn pre_check(&self, from: TileRef, to: TileRef) -> Option<PathResult<TileRef>> {
        if !self.map.is_valid_ref(from) || !self.map.is_valid_ref(to) {
            Some(PathResult::NotFound)
        } else {
            None
        }
    }

    fn find_path(&mut self, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        self.water
            .find_water_path_with(self.chain.as_ref(), self.map, from, to)
    }
}

/// Upstream `PathFinding.Water(game).findPath(from, to)`: stepper pre-check, then the shared chain.
pub fn find_water_path(
    water: &mut WaterManager,
    map: &GameMapImpl,
    from: Source,
    to: TileRef,
) -> Option<Vec<TileRef>> {
    let stepper: PathFinderStepper<TileRef> = PathFinderStepper::new();
    stepper.find_path(
        &mut WaterStepperEnv {
            water,
            map,
            chain: None,
        },
        from,
        to,
    )
}

/// Env of `PathFinding.Air`: plain equality, no pre-check, the staircase finder.
pub struct AirStepperEnv<'a> {
    /// The air path finder (seeded at creation).
    pub finder: &'a AirPathFinder,
    /// The game map.
    pub map: &'a GameMapImpl,
}

impl StepperEnv<TileRef> for AirStepperEnv<'_> {
    fn find_path(&mut self, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        match from {
            Source::Multi(_) => panic!("AirPathFinder does not support multiple start points"),
            Source::Single(f) => Some(self.finder.find_path(self.map, f, to)),
        }
    }
}

/// Water path finder that refreshes when the water graph changes (upstream `WaterPathFinder`).
///
/// Wraps a per-ship stepper; the graph version is staggered over `stagger` ticks so ships do not all
/// re-run A* in the same tick after a water nuke.
#[derive(Clone, Debug)]
pub struct WaterPathFinder {
    stepper: PathFinderStepper<TileRef>,
    water_graph_version: u32,
    rebuilt: bool,
    stagger_countdown: u32,
    pending_version: i64,
    stagger: u32,
    chain: Option<ChainRef>,
}

impl WaterPathFinder {
    /// Spread to use when auto-staggering ship path finders.
    pub const STAGGER_SPREAD: u32 = 50;

    /// Creates a finder for the manager's current graph version; `stagger` is the rebuild delay in ticks (0 = immediate).
    pub fn new(water: &WaterManager, stagger: u32) -> Self {
        Self {
            stepper: PathFinderStepper::new(),
            water_graph_version: water.water_graph_version(),
            rebuilt: false,
            stagger_countdown: 0,
            pending_version: -1,
            stagger,
            chain: water.current_chain(),
        }
    }

    /// True if the finder was rebuilt since the last call; resets on read.
    pub fn rebuilt(&mut self, water: &WaterManager) -> bool {
        self.ensure_fresh(water);
        std::mem::take(&mut self.rebuilt)
    }

    /// Runs the freshness check without a query (the call upstream's discarded motion-plan queries make).
    pub fn touch(&mut self, water: &WaterManager) {
        self.ensure_fresh(water);
    }

    fn ensure_fresh(&mut self, water: &WaterManager) {
        let v = water.water_graph_version();
        if v == self.water_graph_version {
            return;
        }

        // New graph version: start or continue the stagger countdown
        if self.pending_version != v as i64 {
            self.pending_version = v as i64;
            self.stagger_countdown = self.stagger;
        }

        if self.stagger_countdown > 0 {
            self.stagger_countdown -= 1;
            return; // keep the old stepper (and its cached path) for now
        }

        // Swapping in a fresh stepper drops the cached path, forcing an A* run against the new graph
        self.water_graph_version = v;
        self.chain = water.current_chain();
        self.stepper = PathFinderStepper::new();
        self.rebuilt = true;
    }

    /// Next step from `from` towards `to` (`dist` > 0 ends the walk within that Manhattan distance).
    pub fn next(
        &mut self,
        env: &mut WaterStepperEnv,
        from: TileRef,
        to: TileRef,
        dist: Option<f64>,
    ) -> PathResult<TileRef> {
        self.ensure_fresh(env.water);
        env.chain = self.chain.clone();
        self.stepper.next(env, from, to, dist)
    }

    /// One-shot query that leaves the path used by `next` alone.
    pub fn find_path(
        &mut self,
        env: &mut WaterStepperEnv,
        from: Source,
        to: TileRef,
    ) -> Option<Vec<TileRef>> {
        self.ensure_fresh(env.water);
        env.chain = self.chain.clone();
        self.stepper.find_path(env, from, to)
    }

    /// Route after a successful `next`, starting at `from`; falls back to a one-shot query when the stepper was replaced.
    pub fn path_for_traversal(
        &mut self,
        env: &mut WaterStepperEnv,
        from: TileRef,
        to: TileRef,
    ) -> Vec<TileRef> {
        self.ensure_fresh(env.water);
        env.chain = self.chain.clone();
        let path = match self.stepper.path_after_next() {
            Some(path) => Some(path),
            None => self.stepper.find_path(env, Source::Single(from), to),
        };
        match path {
            Some(path) if !path.is_empty() => {
                if path[0] == from {
                    path
                } else {
                    let mut with_from = Vec::with_capacity(path.len() + 1);
                    with_from.push(from);
                    with_from.extend(path);
                    with_from
                }
            }
            _ => vec![from],
        }
    }

    /// Drops the cached path.
    pub fn invalidate(&mut self) {
        self.stepper.invalidate();
    }
}

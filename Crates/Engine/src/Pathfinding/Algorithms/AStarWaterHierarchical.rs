// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/AStar.WaterHierarchical.ts: hierarchical water pathfinding (HPA*) over the abstract graph.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::AStarAbstractGraph::AbstractGraphAStar;
use super::AStarWaterBounded::{AStarWaterBounded, SearchBounds};
use super::AbstractGraph::{AbstractGraph, AbstractNode};
use super::BfsGrid::{BfsGrid, Visit};
use super::ConnectedComponents::LAND_MARKER;
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};
use std::collections::HashMap;
use std::rc::Rc;

/// HPA* over an [`AbstractGraph`] of the mini map; segment paths come from bounded water A*.
pub struct AStarWaterHierarchical {
    tile_bfs: BfsGrid,
    abstract_a_star: AbstractGraphAStar,
    local_a_star: AStarWaterBounded,
    local_a_star_multi_cluster: AStarWaterBounded,
    local_a_star_short_path: AStarWaterBounded,
    graph: Rc<AbstractGraph>,
    cache_paths: bool,
    component_filter: bool,
    bucket_queue: bool,
}

impl AStarWaterHierarchical {
    /// Creates the finder for `map` (the mini map) and `graph`; `cache_paths` stores edge paths on the graph.
    pub fn new(map: &GameMapImpl, graph: Rc<AbstractGraph>, cache_paths: bool) -> Self {
        let cluster_size = graph.cluster_size as usize;
        let multi_cluster_size = cluster_size * 3;
        let short_path_size = 260usize; // 2 * (120 + padding 10)
        Self {
            tile_bfs: BfsGrid::new(map.width() as usize * map.height() as usize),
            abstract_a_star: AbstractGraphAStar::new(Rc::clone(&graph), None, None),
            local_a_star: AStarWaterBounded::new(map, cluster_size * cluster_size, None, None),
            local_a_star_multi_cluster: AStarWaterBounded::new(
                map,
                multi_cluster_size * multi_cluster_size,
                None,
                None,
            ),
            local_a_star_short_path: AStarWaterBounded::new(
                map,
                short_path_size * short_path_size,
                None,
                None,
            ),
            graph,
            cache_paths,
            component_filter: false,
            bucket_queue: false,
        }
    }

    /// Before PR 4479 the nearest-node search walks tiles whose component id is not the land marker (not `isWater`).
    pub fn set_component_filter(&mut self, component_filter: bool) {
        self.component_filter = component_filter;
    }

    /// Before PR 4149 the abstract search queues with a bucket queue instead of a min-heap.
    pub fn set_bucket_queue(&mut self, bucket_queue: bool) {
        self.bucket_queue = bucket_queue;
        self.abstract_a_star = AbstractGraphAStar::with_queue(Rc::clone(&self.graph), bucket_queue);
    }

    /// Swaps in a rebuilt graph, keeping the map-sized scratch buffers.
    pub fn set_graph(&mut self, graph: Rc<AbstractGraph>) {
        self.abstract_a_star = AbstractGraphAStar::with_queue(Rc::clone(&graph), self.bucket_queue);
        self.graph = graph;
    }

    /// Path from one source or several (multi-source takes the array branch even for one element).
    pub fn find_path_from(
        &mut self,
        map: &GameMapImpl,
        from: Source,
        to: TileRef,
    ) -> Option<Vec<TileRef>> {
        match from {
            Source::Multi(sources) => self.find_path_multi_source(map, sources, to),
            Source::Single(source) => self.find_path_single(map, source, to),
        }
    }

    fn find_path_multi_source(
        &mut self,
        map: &GameMapImpl,
        sources: &[TileRef],
        target: TileRef,
    ) -> Option<Vec<TileRef>> {
        // Early exit: bounded A* for sources close to the target
        if let Some(short_path) = self.try_short_path_multi_source(map, sources, target) {
            return Some(short_path);
        }

        // 1. Resolve the target to an abstract node
        let target_node_id = get_cluster_node(map, &self.graph, target)?.id;

        // 2. Map sources to abstract nodes
        let node_to_source = resolve_sources_to_nodes(map, &self.graph, sources);
        if node_to_source.is_empty() {
            return None;
        }

        // 3. Multi-source A* on the abstract graph
        let node_ids: Vec<u32> = node_to_source.iter().map(|&(id, _)| id).collect();
        let node_path = self
            .abstract_a_star
            .find_path_multi(&node_ids, target_node_id)?;

        // 4. Winning source tile (node_path[0] is the winning start node)
        let winning_source = node_to_source
            .iter()
            .find(|&&(id, _)| id == node_path[0])
            .map(|&(_, s)| s)?;

        // 5. Full single-source search from the winner
        self.find_path_single(map, winning_source, target)
    }

    fn try_short_path_multi_source(
        &mut self,
        map: &GameMapImpl,
        sources: &[TileRef],
        target: TileRef,
    ) -> Option<Vec<TileRef>> {
        const SHORT_PATH_THRESHOLD: u32 = 120;
        const PADDING: i64 = 10;

        let candidates: Vec<TileRef> = sources
            .iter()
            .copied()
            .filter(|&s| map.manhattan_dist(s, target) <= SHORT_PATH_THRESHOLD)
            .collect();
        if candidates.is_empty() {
            return None;
        }

        let to_x = map.x(target) as i64;
        let to_y = map.y(target) as i64;
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (to_x, to_x, to_y, to_y);
        for &s in &candidates {
            let sx = map.x(s) as i64;
            let sy = map.y(s) as i64;
            min_x = min_x.min(sx);
            max_x = max_x.max(sx);
            min_y = min_y.min(sy);
            max_y = max_y.max(sy);
        }

        let bounds = SearchBounds {
            min_x: 0.max(min_x - PADDING),
            max_x: (map.width() as i64 - 1).min(max_x + PADDING),
            min_y: 0.max(min_y - PADDING),
            max_y: (map.height() as i64 - 1).min(max_y + PADDING),
        };

        self.local_a_star_short_path
            .search_bounded(map, &candidates, target, bounds)
    }

    /// Single-source search (public: the water manager and tests call it directly).
    pub fn find_path_single(
        &mut self,
        map: &GameMapImpl,
        from: TileRef,
        to: TileRef,
    ) -> Option<Vec<TileRef>> {
        let cluster_size = self.graph.cluster_size;
        let dist = map.manhattan_dist(from, to);

        // Early exit for very short distances
        if dist <= cluster_size {
            let cluster_x = map.x(from) / cluster_size;
            let cluster_y = map.y(from) / cluster_size;
            if let Some(local_path) =
                self.find_local_path(map, from, to, cluster_x, cluster_y, true)
            {
                return Some(local_path);
            }
        }

        let start_node = self.find_nearest_node(map, from)?;
        let end_node = self.find_nearest_node(map, to)?;

        if start_node.id == end_node.id {
            let cluster_x = start_node.x / cluster_size;
            let cluster_y = start_node.y / cluster_size;
            return self.find_local_path(map, from, to, cluster_x, cluster_y, true);
        }

        let node_path = self.abstract_a_star.find_path(start_node.id, end_node.id)?;

        let mut initial_path: Vec<TileRef> = Vec::new();

        // 1. Start to the first node
        let first_node_tile = self.graph.get_node(node_path[0])?.tile;
        let start_cluster_x = map.x(from) / cluster_size;
        let start_cluster_y = map.y(from) / cluster_size;
        let start_segment = self.find_local_path(
            map,
            from,
            first_node_tile,
            start_cluster_x,
            start_cluster_y,
            false,
        )?;
        initial_path.extend_from_slice(&start_segment);

        // 2. Through the abstract nodes
        for i in 0..node_path.len() - 1 {
            let from_node_id = node_path[i];
            let to_node_id = node_path[i + 1];

            let graph = Rc::clone(&self.graph);
            let edge = graph.get_edge_between(from_node_id, to_node_id)?;
            let from_tile = graph.get_node(from_node_id)?.tile;
            let to_tile = graph.get_node(to_node_id)?.tile;

            // Path cache lives on the graph; it is direction-aware
            if self.cache_paths {
                let used = graph.with_cached_path(edge.id, from_node_id, |cached| {
                    if cached.is_empty() {
                        false
                    } else {
                        initial_path.extend_from_slice(&cached[1..]);
                        true
                    }
                });
                if used == Some(true) {
                    continue;
                }
            }

            let segment_path = self.find_local_path(
                map,
                from_tile,
                to_tile,
                edge.cluster_x,
                edge.cluster_y,
                false,
            )?;
            initial_path.extend_from_slice(&segment_path[1..]);

            if self.cache_paths {
                graph.set_cached_path(edge.id, from_node_id, segment_path);
            }
        }

        // 3. Last node to the end
        let last_node_tile = self.graph.get_node(node_path[node_path.len() - 1])?.tile;
        let end_cluster_x = map.x(to) / cluster_size;
        let end_cluster_y = map.y(to) / cluster_size;
        let end_segment =
            self.find_local_path(map, last_node_tile, to, end_cluster_x, end_cluster_y, false)?;
        initial_path.extend_from_slice(&end_segment[1..]);

        Some(initial_path)
    }

    fn find_nearest_node(&mut self, map: &GameMapImpl, tile: TileRef) -> Option<AbstractNode> {
        let cluster_size = self.graph.cluster_size;
        let cluster_x = map.x(tile) / cluster_size;
        let cluster_y = map.y(tile) / cluster_size;

        let min_x = cluster_x * cluster_size;
        let min_y = cluster_y * cluster_size;
        let max_x = (map.width() - 1).min(min_x + cluster_size - 1);
        let max_y = (map.height() - 1).min(min_y + cluster_size - 1);

        let graph = Rc::clone(&self.graph);
        let component_filter = self.component_filter;
        let cluster = graph.get_cluster(cluster_x as i64, cluster_y as i64)?;
        if cluster.node_ids.is_empty() {
            return None;
        }

        let candidate_nodes: Vec<&AbstractNode> = cluster
            .node_ids
            .iter()
            .map(|&id| graph.get_node(id).unwrap())
            .collect();
        let max_distance = (cluster_size * cluster_size) as f64;

        self.tile_bfs.search(
            map.width(),
            map.height(),
            &[tile],
            max_distance,
            |t| {
                if component_filter {
                    graph.get_component_id(t) != LAND_MARKER as u32
                } else {
                    map.is_water(t)
                }
            },
            |t, _dist| {
                let tile_x = map.x(t);
                let tile_y = map.y(t);
                for node in &candidate_nodes {
                    if node.x == tile_x && node.y == tile_y {
                        return Visit::Found((*node).clone());
                    }
                }
                if tile_x < min_x || tile_x > max_x || tile_y < min_y || tile_y > max_y {
                    return Visit::Reject;
                }
                Visit::Continue
            },
        )
    }

    fn find_local_path(
        &mut self,
        map: &GameMapImpl,
        from: TileRef,
        to: TileRef,
        cluster_x: u32,
        cluster_y: u32,
        multi_cluster: bool,
    ) -> Option<Vec<TileRef>> {
        let cluster_size = self.graph.cluster_size as i64;
        let (cx, cy) = (cluster_x as i64, cluster_y as i64);
        let (map_w, map_h) = (map.width() as i64, map.height() as i64);

        let bounds = if multi_cluster {
            // 3x3 clusters centred on the starting cluster
            SearchBounds {
                min_x: 0.max((cx - 1) * cluster_size),
                min_y: 0.max((cy - 1) * cluster_size),
                max_x: (map_w - 1).min((cx + 2) * cluster_size - 1),
                max_y: (map_h - 1).min((cy + 2) * cluster_size - 1),
            }
        } else {
            let min_x = cx * cluster_size;
            let min_y = cy * cluster_size;
            SearchBounds {
                min_x,
                min_y,
                max_x: (map_w - 1).min(min_x + cluster_size - 1),
                max_y: (map_h - 1).min(min_y + cluster_size - 1),
            }
        };

        let selected = if multi_cluster {
            &mut self.local_a_star_multi_cluster
        } else {
            &mut self.local_a_star
        };
        let mut path = selected.search_bounded(map, &[from], to, bounds)?;
        if path.is_empty() {
            return None;
        }

        // Bounded A* clamps tiles to its box; make the path start and end at the requested tiles
        if path[0] != from {
            path.insert(0, from);
        }
        if path[path.len() - 1] != to {
            path.push(to);
        }
        Some(path)
    }
}

impl PathFinder for AStarWaterHierarchical {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        self.find_path_from(ctx.mini_map, from, to)
    }
}

/// Closest gateway node of a tile's cluster (Manhattan distance, first wins ties).
fn get_cluster_node<'g>(
    map: &GameMapImpl,
    graph: &'g AbstractGraph,
    tile: TileRef,
) -> Option<&'g AbstractNode> {
    let x = map.x(tile);
    let y = map.y(tile);
    let cluster_x = (x / graph.cluster_size) as i64;
    let cluster_y = (y / graph.cluster_size) as i64;

    let cluster = graph.get_cluster(cluster_x, cluster_y)?;
    if cluster.node_ids.is_empty() {
        return None;
    }

    let mut best_node: Option<&AbstractNode> = None;
    let mut best_dist = u32::MAX;
    for &node_id in &cluster.node_ids {
        let Some(node) = graph.get_node(node_id) else {
            continue;
        };
        let dist = node.x.abs_diff(x) + node.y.abs_diff(y);
        if dist < best_dist {
            best_dist = dist;
            best_node = Some(node);
        }
    }
    best_node
}

/// Maps sources to abstract nodes, keeping the closest source per node; order is first-insertion order of the nodes.
fn resolve_sources_to_nodes(
    map: &GameMapImpl,
    graph: &AbstractGraph,
    sources: &[TileRef],
) -> Vec<(u32, TileRef)> {
    let mut node_to_source: Vec<(u32, TileRef)> = Vec::new();
    let mut node_to_dist: HashMap<u32, (u32, usize)> = HashMap::new();

    for &source in sources {
        let Some(node) = get_cluster_node(map, graph, source) else {
            continue;
        };
        let x = map.x(source);
        let y = map.y(source);
        let dist = node.x.abs_diff(x) + node.y.abs_diff(y);

        match node_to_dist.get(&node.id) {
            Some(&(prev_dist, index)) => {
                if dist < prev_dist {
                    node_to_source[index].1 = source;
                    node_to_dist.insert(node.id, (dist, index));
                }
            }
            None => {
                node_to_dist.insert(node.id, (dist, node_to_source.len()));
                node_to_source.push((node.id, source));
            }
        }
    }
    node_to_source
}

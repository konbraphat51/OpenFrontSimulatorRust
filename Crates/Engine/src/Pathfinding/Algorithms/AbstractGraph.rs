// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/AbstractGraph.ts: the hierarchical water graph (gateway nodes, intra-cluster edges) and its builder.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! A graph shares its [`ConnectedComponents`] with the water manager (`Rc<RefCell<..>>`), exactly as
//! upstream shares the object: a stale graph keeps reading the live, incrementally updated labels.

use super::BfsGrid::{BfsGrid, Visit};
use super::ConnectedComponents::ConnectedComponents;
use crate::Compat::OrderedMap::FxBuildHasher;
use crate::Game::GameMap::{GameMapImpl, TileRef};
use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// A gateway node on a cluster boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbstractNode {
    /// Node id (index into the node list).
    pub id: u32,
    /// Column.
    pub x: u32,
    /// Row.
    pub y: u32,
    /// Tile ref.
    pub tile: TileRef,
    /// Water component at build time.
    pub component_id: u32,
}

/// A bidirectional edge, stored once with `node_a < node_b`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbstractEdge {
    /// Edge id (index into the edge list).
    pub id: u32,
    /// Lower node id.
    pub node_a: u32,
    /// Higher node id.
    pub node_b: u32,
    /// BFS distance in tiles.
    pub cost: u32,
    /// Cluster column whose BFS found the edge.
    pub cluster_x: u32,
    /// Cluster row whose BFS found the edge.
    pub cluster_y: u32,
}

/// A square block of tiles and the gateway nodes on its borders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    /// Cluster column.
    pub x: u32,
    /// Cluster row.
    pub y: u32,
    /// Ids of the nodes on its borders, in discovery order.
    pub node_ids: Vec<u32>,
}

/// The abstract graph.
#[derive(Debug)]
pub struct AbstractGraph {
    /// Tiles per cluster side.
    pub cluster_size: u32,
    /// Clusters per row.
    pub clusters_x: u32,
    /// Clusters per column.
    pub clusters_y: u32,
    nodes: Vec<AbstractNode>,
    edges: Vec<AbstractEdge>,
    node_edge_ids: Vec<Vec<u32>>,
    clusters: Vec<Cluster>,
    path_cache: RefCell<Vec<Option<Vec<TileRef>>>>,
    water_components: Option<Rc<RefCell<ConnectedComponents>>>,
}

impl AbstractGraph {
    fn new(cluster_size: u32, clusters_x: u32, clusters_y: u32) -> Self {
        Self {
            cluster_size,
            clusters_x,
            clusters_y,
            nodes: Vec::new(),
            edges: Vec::new(),
            node_edge_ids: Vec::new(),
            clusters: Vec::new(),
            path_cache: RefCell::new(Vec::new()),
            water_components: None,
        }
    }

    /// Node by id.
    pub fn get_node(&self, id: u32) -> Option<&AbstractNode> {
        self.nodes.get(id as usize)
    }

    /// Every node, indexed by id.
    pub fn get_all_nodes(&self) -> &[AbstractNode] {
        &self.nodes
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Edge by id.
    pub fn get_edge(&self, id: u32) -> Option<&AbstractEdge> {
        self.edges.get(id as usize)
    }

    /// Edges touching `node_id`, in insertion order.
    pub fn get_node_edges(&self, node_id: u32) -> impl Iterator<Item = &AbstractEdge> {
        self.node_edge_ids
            .get(node_id as usize)
            .into_iter()
            .flatten()
            .filter_map(|&e| self.edges.get(e as usize))
    }

    /// First edge between two nodes.
    pub fn get_edge_between(&self, node_a: u32, node_b: u32) -> Option<&AbstractEdge> {
        let ids = self.node_edge_ids.get(node_a as usize)?;
        for &edge_id in ids {
            let edge = &self.edges[edge_id as usize];
            if edge.node_a == node_b || edge.node_b == node_b {
                return Some(edge);
            }
        }
        None
    }

    /// The end of `edge` that is not `node_id`.
    pub fn get_other_node(&self, edge: &AbstractEdge, node_id: u32) -> u32 {
        if edge.node_a == node_id {
            edge.node_b
        } else {
            edge.node_a
        }
    }

    /// Every edge, indexed by id.
    pub fn get_all_edges(&self) -> &[AbstractEdge] {
        &self.edges
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Cached path of an edge for the traversal direction starting at `from_node_id`.
    pub fn get_cached_path(&self, edge_id: u32, from_node_id: u32) -> Option<Vec<TileRef>> {
        let edge = self.edges.get(edge_id as usize)?;
        let direction = if from_node_id == edge.node_a { 0 } else { 1 };
        self.path_cache
            .borrow()
            .get(edge_id as usize * 2 + direction)
            .cloned()
            .flatten()
    }

    /// Length of the cached path, if any (avoids cloning when only emptiness matters).
    pub fn with_cached_path<R>(
        &self,
        edge_id: u32,
        from_node_id: u32,
        f: impl FnOnce(&[TileRef]) -> R,
    ) -> Option<R> {
        let edge = self.edges.get(edge_id as usize)?;
        let direction = if from_node_id == edge.node_a { 0 } else { 1 };
        let cache = self.path_cache.borrow();
        cache
            .get(edge_id as usize * 2 + direction)?
            .as_deref()
            .map(f)
    }

    /// Stores the path of an edge for one traversal direction.
    pub fn set_cached_path(&self, edge_id: u32, from_node_id: u32, path: Vec<TileRef>) {
        let Some(edge) = self.edges.get(edge_id as usize) else {
            return;
        };
        let direction = if from_node_id == edge.node_a { 0 } else { 1 };
        let index = edge_id as usize * 2 + direction;
        let mut cache = self.path_cache.borrow_mut();
        if cache.len() <= index {
            cache.resize(index + 1, None);
        }
        cache[index] = Some(path);
    }

    /// Water component of a tile from the shared labeling (0 without one).
    pub fn get_component_id(&self, tile: TileRef) -> u32 {
        match &self.water_components {
            Some(wc) => wc.borrow_mut().get_component_id(tile),
            None => 0,
        }
    }

    /// Water tiles in a component (0 if unknown).
    pub fn get_component_size(&self, component_id: u32) -> u32 {
        match &self.water_components {
            Some(wc) => wc.borrow_mut().get_component_size(component_id),
            None => 0,
        }
    }

    /// Index of cluster (`cluster_x`, `cluster_y`) in the cluster list.
    pub fn get_cluster_key(&self, cluster_x: i64, cluster_y: i64) -> i64 {
        cluster_y * self.clusters_x as i64 + cluster_x
    }

    /// Cluster at (`cluster_x`, `cluster_y`); like upstream the key is not range-checked per axis.
    pub fn get_cluster(&self, cluster_x: i64, cluster_y: i64) -> Option<&Cluster> {
        let key = self.get_cluster_key(cluster_x, cluster_y);
        if key < 0 {
            None
        } else {
            self.clusters.get(key as usize)
        }
    }

    /// Nodes of the 3x3 clusters around a cluster.
    pub fn get_nearby_cluster_nodes(&self, cluster_x: i64, cluster_y: i64) -> Vec<&AbstractNode> {
        let mut nodes = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(cluster) = self.get_cluster(cluster_x + dx, cluster_y + dy) {
                    nodes.extend(cluster.node_ids.iter().map(|&id| &self.nodes[id as usize]));
                }
            }
        }
        nodes
    }

    /// The shared water component labeling, if any.
    pub fn water_components(&self) -> Option<Ref<'_, ConnectedComponents>> {
        self.water_components.as_ref().map(|wc| wc.borrow())
    }

    fn add_node(&mut self, node: AbstractNode) {
        let id = node.id as usize;
        if self.nodes.len() <= id {
            self.nodes.resize(id + 1, node.clone());
            self.node_edge_ids.resize(id + 1, Vec::new());
        }
        self.nodes[id] = node;
        self.node_edge_ids[id] = Vec::new();
    }

    fn add_edge(&mut self, edge: AbstractEdge) {
        let id = edge.id as usize;
        self.node_edge_ids[edge.node_a as usize].push(edge.id);
        self.node_edge_ids[edge.node_b as usize].push(edge.id);
        if self.edges.len() <= id {
            self.edges.push(edge);
        } else {
            self.edges[id] = edge;
        }
    }

    /// Renumbers edges by (node_a, node_b) so partial and full builds of the same water match.
    fn sort_edges(&mut self) {
        self.edges.sort_by_key(|e| (e.node_a, e.node_b));
        for ids in &mut self.node_edge_ids {
            ids.clear();
        }
        for (i, edge) in self.edges.iter_mut().enumerate() {
            edge.id = i as u32;
            self.node_edge_ids[edge.node_a as usize].push(i as u32);
            self.node_edge_ids[edge.node_b as usize].push(i as u32);
        }
    }

    fn init_path_cache(&mut self) {
        *self.path_cache.borrow_mut() = vec![None; self.edges.len() * 2];
    }
}

/// Old edge costs by tile pair, used to skip BFS in clean clusters.
type OldEdgeCosts = HashMap<(TileRef, TileRef), (u32, u32, u32), FxBuildHasher>;

/// Builds an [`AbstractGraph`] from a (mini) map, optionally reusing an older graph for clean clusters.
pub struct AbstractGraphBuilder<'a> {
    map: &'a GameMapImpl,
    cluster_size: u32,
    width: u32,
    height: u32,
    clusters_x: u32,
    clusters_y: u32,
    tile_bfs: &'a mut BfsGrid,
    water_components: Rc<RefCell<ConnectedComponents>>,
    shared_water_components: bool,
    old_graph: Option<&'a AbstractGraph>,
    dirty_mini_tiles: Option<&'a [TileRef]>,

    graph: AbstractGraph,
    tile_to_node: HashMap<TileRef, u32, FxBuildHasher>,
    next_node_id: u32,
    next_edge_id: u32,
    edge_between: HashMap<(u32, u32), u32, FxBuildHasher>,
    clean_clusters: Option<HashSet<u32>>,
    old_edge_costs: Option<OldEdgeCosts>,
    sort_edges: bool,
}

impl<'a> AbstractGraphBuilder<'a> {
    /// Upstream `CLUSTER_SIZE`.
    pub const CLUSTER_SIZE: u32 = 32;

    /// Creates a builder.
    ///
    /// `shared_water_components` is an initialised labeling kept up to date by the caller (skips the flood fill);
    /// `dirty_mini_tiles` with `old_graph` enables the partial rebuild; `tile_bfs` is reusable map-sized scratch.
    pub fn new(
        map: &'a GameMapImpl,
        cluster_size: u32,
        old_graph: Option<&'a AbstractGraph>,
        dirty_mini_tiles: Option<&'a [TileRef]>,
        shared_water_components: Option<Rc<RefCell<ConnectedComponents>>>,
        tile_bfs: &'a mut BfsGrid,
    ) -> Self {
        let width = map.width();
        let height = map.height();
        let clusters_x = width.div_ceil(cluster_size);
        let clusters_y = height.div_ceil(cluster_size);
        let shared = shared_water_components.is_some();
        let water_components = shared_water_components
            .unwrap_or_else(|| Rc::new(RefCell::new(ConnectedComponents::new(map))));
        Self {
            map,
            cluster_size,
            width,
            height,
            clusters_x,
            clusters_y,
            tile_bfs,
            water_components,
            shared_water_components: shared,
            old_graph,
            dirty_mini_tiles,
            graph: AbstractGraph::new(cluster_size, clusters_x, clusters_y),
            tile_to_node: HashMap::default(),
            next_node_id: 0,
            next_edge_id: 0,
            edge_between: HashMap::default(),
            clean_clusters: None,
            old_edge_costs: None,
            sort_edges: false,
        }
    }

    /// Renumbers edges by node pair after the build (v0.35.0 on).
    pub fn with_sorted_edges(mut self, sort_edges: bool) -> Self {
        self.sort_edges = sort_edges;
        self
    }

    /// Builds the graph.
    pub fn build(mut self) -> AbstractGraph {
        if !self.shared_water_components {
            self.water_components.borrow_mut().initialize(self.map);
        }

        if self.old_graph.is_some() && self.dirty_mini_tiles.is_some_and(|d| !d.is_empty()) {
            self.compute_partial_rebuild_info();
        }

        // Pre-create all clusters
        for cy in 0..self.clusters_y {
            for cx in 0..self.clusters_x {
                self.graph.clusters.push(Cluster {
                    x: cx,
                    y: cy,
                    node_ids: Vec::new(),
                });
            }
        }

        // Gateway nodes at cluster boundaries
        for cy in 0..self.clusters_y {
            for cx in 0..self.clusters_x {
                self.process_cluster(cx, cy);
            }
        }

        // Edges between nodes of the same cluster
        for cy in 0..self.clusters_y {
            for cx in 0..self.clusters_x {
                let key = (cy * self.clusters_x + cx) as usize;
                if self.graph.clusters[key].node_ids.is_empty() {
                    continue;
                }
                self.build_cluster_connections(cx, cy);
            }
        }

        if self.sort_edges {
            self.graph.sort_edges();
        }
        self.graph.init_path_cache();
        self.graph.water_components = Some(self.water_components);
        self.graph
    }

    fn get_or_create_node(&mut self, x: u32, y: u32) -> u32 {
        let tile = self.map.tile_ref(x as i32, y as i32);
        if let Some(&id) = self.tile_to_node.get(&tile) {
            return id;
        }
        let id = self.next_node_id;
        self.next_node_id += 1;
        let component_id = self.water_components.borrow_mut().get_component_id(tile);
        self.graph.add_node(AbstractNode {
            id,
            x,
            y,
            tile,
            component_id,
        });
        self.tile_to_node.insert(tile, id);
        id
    }

    fn add_node_to_cluster(&mut self, cluster_x: u32, cluster_y: u32, node_id: u32) {
        let key = cluster_y as usize * self.clusters_x as usize + cluster_x as usize;
        let Some(cluster) = self.graph.clusters.get_mut(key) else {
            return;
        };
        // A node at a cluster corner can be found by both edge scans
        if !cluster.node_ids.contains(&node_id) {
            cluster.node_ids.push(node_id);
        }
    }

    fn process_cluster(&mut self, cx: u32, cy: u32) {
        let base_x = cx * self.cluster_size;
        let base_y = cy * self.cluster_size;

        // Right edge (vertical boundary to the next cluster)
        if cx < self.clusters_x - 1 {
            let edge_x = (base_x + self.cluster_size - 1).min(self.width - 1);
            let nodes = self.find_nodes_on_vertical_edge(edge_x, base_y);
            for node in nodes {
                self.add_node_to_cluster(cx, cy, node);
                self.add_node_to_cluster(cx + 1, cy, node);
            }
        }

        // Bottom edge (horizontal boundary to the next cluster)
        if cy < self.clusters_y - 1 {
            let edge_y = (base_y + self.cluster_size - 1).min(self.height - 1);
            let nodes = self.find_nodes_on_horizontal_edge(edge_y, base_x);
            for node in nodes {
                self.add_node_to_cluster(cx, cy, node);
                self.add_node_to_cluster(cx, cy + 1, node);
            }
        }
    }

    fn find_nodes_on_vertical_edge(&mut self, x: u32, base_y: u32) -> Vec<u32> {
        let mut nodes = Vec::new();
        let max_y = (base_y + self.cluster_size).min(self.height);
        let mut span_start: i64 = -1;

        for y in base_y..=max_y {
            let is_entrance = y < max_y && {
                let tile = self.map.tile_ref(x as i32, y as i32);
                x + 1 < self.width && self.map.is_water(tile) && {
                    let next = self.map.tile_ref(x as i32 + 1, y as i32);
                    self.map.is_water(next)
                }
            };
            if is_entrance {
                if span_start == -1 {
                    span_start = y as i64;
                }
            } else if span_start != -1 {
                let span_length = y as i64 - span_start;
                let mid_y = span_start + span_length / 2;
                span_start = -1;
                nodes.push(self.get_or_create_node(x, mid_y as u32));
            }
        }
        nodes
    }

    fn find_nodes_on_horizontal_edge(&mut self, y: u32, base_x: u32) -> Vec<u32> {
        let mut nodes = Vec::new();
        let max_x = (base_x + self.cluster_size).min(self.width);
        let mut span_start: i64 = -1;

        for x in base_x..=max_x {
            let is_entrance = x < max_x && {
                let tile = self.map.tile_ref(x as i32, y as i32);
                y + 1 < self.height && self.map.is_water(tile) && {
                    let next = self.map.tile_ref(x as i32, y as i32 + 1);
                    self.map.is_water(next)
                }
            };
            if is_entrance {
                if span_start == -1 {
                    span_start = x as i64;
                }
            } else if span_start != -1 {
                let span_length = x as i64 - span_start;
                let mid_x = span_start + span_length / 2;
                span_start = -1;
                nodes.push(self.get_or_create_node(mid_x as u32, y));
            }
        }
        nodes
    }

    fn build_cluster_connections(&mut self, cx: u32, cy: u32) {
        let cluster_key = cy * self.clusters_x + cx;

        // Clean clusters copy edge costs from the old graph instead of running BFS
        if self
            .clean_clusters
            .as_ref()
            .is_some_and(|c| c.contains(&cluster_key))
        {
            self.build_cluster_connections_from_cache(cx, cy);
            return;
        }

        let node_ids = self.graph.clusters[cluster_key as usize].node_ids.clone();
        let nodes: Vec<AbstractNode> = node_ids
            .iter()
            .map(|&id| self.graph.nodes[id as usize].clone())
            .collect();

        let cluster_min_x = cx * self.cluster_size;
        let cluster_min_y = cy * self.cluster_size;
        let cluster_max_x = (self.width - 1).min(cluster_min_x + self.cluster_size - 1);
        let cluster_max_y = (self.height - 1).min(cluster_min_y + self.cluster_size - 1);

        for i in 0..nodes.len() {
            let from_node = &nodes[i];

            // Targets not yet processed from this node, in the same water component
            let target_nodes: Vec<&AbstractNode> = nodes[i + 1..]
                .iter()
                .filter(|n| n.component_id == from_node.component_id)
                .collect();
            if target_nodes.is_empty() {
                continue;
            }

            // Single BFS to find all reachable target nodes
            let reachable = self.find_all_reachable_nodes_in_bounds(
                from_node.tile,
                &target_nodes,
                (cluster_min_x, cluster_max_x, cluster_min_y, cluster_max_y),
            );

            for (target_id, cost) in reachable {
                self.add_or_update_edge(from_node.id, target_id, cost, cx, cy);
            }
        }
    }

    /// Adds an edge in canonical order, or lowers the cost of an existing one.
    fn add_or_update_edge(
        &mut self,
        node_id_a: u32,
        node_id_b: u32,
        cost: u32,
        cluster_x: u32,
        cluster_y: u32,
    ) {
        let (lo, hi) = if node_id_a < node_id_b {
            (node_id_a, node_id_b)
        } else {
            (node_id_b, node_id_a)
        };

        if let Some(&edge_id) = self.edge_between.get(&(lo, hi)) {
            let existing = &mut self.graph.edges[edge_id as usize];
            if cost < existing.cost {
                existing.cost = cost;
                existing.cluster_x = cluster_x;
                existing.cluster_y = cluster_y;
            }
            return;
        }

        let id = self.next_edge_id;
        self.next_edge_id += 1;
        self.edge_between.insert((lo, hi), id);
        self.graph.add_edge(AbstractEdge {
            id,
            node_a: lo,
            node_b: hi,
            cost,
            cluster_x,
            cluster_y,
        });
    }

    /// BFS inside the cluster box; returns `(node id, distance)` in discovery order.
    fn find_all_reachable_nodes_in_bounds(
        &mut self,
        from: TileRef,
        target_nodes: &[&AbstractNode],
        (min_x, max_x, min_y, max_y): (u32, u32, u32, u32),
    ) -> Vec<(u32, u32)> {
        let from_x = self.map.x(from);
        let from_y = self.map.y(from);

        let mut tile_to_node_id: HashMap<TileRef, u32, FxBuildHasher> = HashMap::default();
        let mut max_manhattan_dist = 0u32;
        for node in target_nodes {
            tile_to_node_id.insert(node.tile, node.id);
            let dx = node.x.abs_diff(from_x);
            let dy = node.y.abs_diff(from_y);
            max_manhattan_dist = max_manhattan_dist.max(dx + dy);
        }

        let max_distance = (max_manhattan_dist * 4) as f64; // allow path deviation
        let mut reachable: Vec<(u32, u32)> = Vec::new();
        let map = self.map;

        self.tile_bfs.search(
            map.width(),
            map.height(),
            &[from],
            max_distance,
            |tile| map.is_water(tile),
            |tile, dist| {
                let x = map.x(tile);
                let y = map.y(tile);

                // Reject outside the bounding box (except start and targets)
                let is_start_or_target = tile == from || tile_to_node_id.contains_key(&tile);
                if !is_start_or_target && (x < min_x || x > max_x || y < min_y || y > max_y) {
                    return Visit::Reject;
                }

                if let Some(&node_id) = tile_to_node_id.get(&tile) {
                    reachable.push((node_id, dist));
                    // Early exit once every target is found
                    if reachable.len() == target_nodes.len() {
                        return Visit::Found(dist);
                    }
                }
                Visit::Continue
            },
        );

        reachable
    }

    /// Computes the clean clusters (unaffected by water changes) and the old edge cost lookup.
    fn compute_partial_rebuild_info(&mut self) {
        let dirty_mini_tiles = self.dirty_mini_tiles.unwrap();
        let old_graph = self.old_graph.unwrap();

        let mut primary_dirty: HashSet<u32> = HashSet::new();
        for &tile in dirty_mini_tiles {
            let cx = self.map.x(tile) / self.cluster_size;
            let cy = self.map.y(tile) / self.cluster_size;
            primary_dirty.insert(cy * self.clusters_x + cx);
        }

        // Expand by the 1-ring: gateway nodes sit on cluster boundaries and belong to two clusters
        let mut expanded_dirty: HashSet<u32> = HashSet::new();
        for &key in &primary_dirty {
            let cy = (key / self.clusters_x) as i64;
            let cx = key as i64 - cy * self.clusters_x as i64;
            for dy in -1..=1i64 {
                for dx in -1..=1i64 {
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx >= 0
                        && nx < self.clusters_x as i64
                        && ny >= 0
                        && ny < self.clusters_y as i64
                    {
                        expanded_dirty.insert((ny * self.clusters_x as i64 + nx) as u32);
                    }
                }
            }
        }

        let total_clusters = self.clusters_x * self.clusters_y;
        self.clean_clusters = Some(
            (0..total_clusters)
                .filter(|k| !expanded_dirty.contains(k))
                .collect(),
        );

        let mut old_edge_costs: OldEdgeCosts = HashMap::default();
        for edge in old_graph.get_all_edges() {
            let (Some(node_a), Some(node_b)) = (
                old_graph.get_node(edge.node_a),
                old_graph.get_node(edge.node_b),
            ) else {
                continue;
            };
            let key = (node_a.tile.min(node_b.tile), node_a.tile.max(node_b.tile));
            match old_edge_costs.get(&key) {
                Some(existing) if edge.cost >= existing.0 => {}
                _ => {
                    old_edge_costs.insert(key, (edge.cost, edge.cluster_x, edge.cluster_y));
                }
            }
        }
        self.old_edge_costs = Some(old_edge_costs);
    }

    /// Clean clusters: recreate edges from the old graph's costs, keeping the original cluster attribution.
    fn build_cluster_connections_from_cache(&mut self, cx: u32, cy: u32) {
        let node_ids = self.graph.clusters[(cy * self.clusters_x + cx) as usize]
            .node_ids
            .clone();
        let nodes: Vec<AbstractNode> = node_ids
            .iter()
            .map(|&id| self.graph.nodes[id as usize].clone())
            .collect();

        for i in 0..nodes.len() {
            for j in i + 1..nodes.len() {
                if nodes[i].component_id != nodes[j].component_id {
                    continue;
                }
                let key = (
                    nodes[i].tile.min(nodes[j].tile),
                    nodes[i].tile.max(nodes[j].tile),
                );
                let entry = self.old_edge_costs.as_ref().unwrap().get(&key).copied();
                if let Some((cost, cluster_x, cluster_y)) = entry {
                    self.add_or_update_edge(nodes[i].id, nodes[j].id, cost, cluster_x, cluster_y);
                }
            }
        }
    }
}

// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/RailNetworkImpl.ts: stations, railroads and clusters of a game's rail network.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The station manager, spatial grid and arenas of stations, railroads and clusters live here; the
//! station and cluster primitives are in `TrainStation.rs`. Client-only parts (ghost rail previews,
//! `overlappingRailroads`, railroad updates, motion plans) are not ported. Because upstream builds a
//! fresh `AStarRail` and `StationPathFinder` per query, so do we (they keep no state between queries).

use super::Game::{UnitId, UnitType};
use super::GameImpl::GameImpl;
use super::GameMap::TileRef;
use super::Railroad::{RailId, Railroad, StationId};
use super::RailroadSpatialGrid::RailSpatialGrid;
use super::TrainStation::{Cluster, ClusterId, TrainStation};
use crate::Builds::Rules::Change;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Pathfinding::Algorithms::AStarRail::AStarRail;
use crate::Pathfinding::PathFinderStation::find_stations_path;
use crate::Pathfinding::Transformers::MiniMapTransformer::MiniMapTransformer;
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};
use std::collections::{HashSet, VecDeque};

const MAX_CONNECTION_DISTANCE: i64 = 4;
const STATION_RADIUS: f64 = 3.0;
const GRID_CELL_SIZE: f64 = 4.0;
const STRUCTURE_TYPES: [UnitType; 3] = [UnitType::City, UnitType::Factory, UnitType::Port];

/// Rail network of a game: the station manager plus the railroad graph.
#[derive(Debug)]
pub struct RailNetwork {
    /// Station arena; id `n` is at index `n - 1` (removed stations stay).
    pub(crate) stations: Vec<TrainStation>,
    /// Whether the station with that arena index is still registered in the manager.
    pub(crate) registered: Vec<bool>,
    /// The manager's `stations` set, in insertion order.
    pub(crate) station_set: OrderedSet<StationId>,
    /// Registered stations per unit, in registration order (index for `find_station`).
    by_unit: std::collections::HashMap<UnitId, Vec<StationId>>,
    /// Railroad arena; the id is the index.
    pub(crate) rails: Vec<Railroad>,
    pub(crate) clusters: Vec<Cluster>,
    grid: RailSpatialGrid,
    dirty_clusters: OrderedSet<ClusterId>,
    /// Leaving a cluster also happens when the new cluster is the same one (PR 5096 not yet applied).
    pub(crate) leave_same_cluster: bool,
}

impl Default for RailNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl RailNetwork {
    /// Creates the empty network (`createRailNetwork`).
    pub fn new() -> Self {
        Self {
            stations: Vec::new(),
            registered: Vec::new(),
            station_set: OrderedSet::new(),
            by_unit: std::collections::HashMap::new(),
            rails: Vec::new(),
            clusters: Vec::new(),
            grid: RailSpatialGrid::new(GRID_CELL_SIZE),
            dirty_clusters: OrderedSet::new(),
            leave_same_cluster: false,
        }
    }

    /// `stationManager().count()`: the next station id (ids start at 1).
    pub fn station_count(&self) -> usize {
        self.stations.len() + 1
    }

    /// `stationManager().getById(id)`: the station unless it was removed.
    pub fn station_by_id(&self, id: StationId) -> Option<&TrainStation> {
        let index = (id as usize).checked_sub(1)?;
        if *self.registered.get(index)? {
            Some(&self.stations[index])
        } else {
            None
        }
    }

    /// Every registered station as (unit, cluster) in `stationManager().getAll()` order; `None` for
    /// a station outside any cluster.
    pub fn station_clusters(&self) -> Vec<(UnitId, Option<ClusterId>)> {
        self.station_set
            .iter()
            .map(|&s| {
                let station = self.station(s);
                (station.unit, station.cluster)
            })
            .collect()
    }

    /// `stationManager().findStation(unit)`: the first registered station of the unit.
    pub fn find_station(&self, unit: UnitId) -> Option<StationId> {
        self.by_unit.get(&unit).and_then(|v| v.first().copied())
    }

    /// `StationManager.addStation`: registers a new station of `unit` and returns its id.
    fn add_station(&mut self, unit: UnitId) -> StationId {
        let id = self.stations.len() as StationId + 1;
        self.stations.push(TrainStation {
            id,
            unit,
            cluster: None,
            railroads: OrderedSet::new(),
            railroad_by_neighbor: Default::default(),
        });
        self.registered.push(true);
        self.station_set.insert(id);
        self.by_unit.entry(unit).or_default().push(id);
        id
    }

    /// `connectStation(new TrainStation(mg, unit))`: registers the station and links it to the network.
    pub fn connect_station(&mut self, game: &GameImpl, unit: UnitId) -> StationId {
        let station = self.add_station(unit);
        if !self.connect_to_existing_rails(game, station) {
            self.connect_to_nearby_stations(game, station);
        }
        station
    }

    /// `recomputeClusters`: splits clusters whose stations are no longer all connected.
    pub fn recompute_clusters(&mut self, game: &GameImpl) {
        if self.dirty_clusters.is_empty() {
            return;
        }
        let dirty: Vec<ClusterId> = self.dirty_clusters.iter().copied().collect();
        for cluster in dirty {
            let mut all_original: OrderedSet<StationId> = self.clusters[cluster as usize]
                .stations
                .iter()
                .copied()
                .collect();
            while !all_original.is_empty() {
                let next_station = *all_original.iter().next().expect("not empty");
                let all_connected = self.compute_cluster(next_station);
                // Filter stations that are connected to the current cluster
                for s in &all_connected {
                    all_original.remove(s);
                }
                // Those stations were disconnected: new cluster
                if !all_original.is_empty() {
                    let new_cluster = self.new_cluster();
                    // Switching their cluster will automatically remove them from their current cluster
                    self.cluster_add_stations(game, new_cluster, &all_connected);
                }
            }
        }
        self.dirty_clusters.clear();
    }

    /// `removeStation(unit)`: unlinks the unit's station from the network and its cluster.
    pub fn remove_station(&mut self, game: &mut GameImpl, unit: UnitId) {
        let Some(station) = self.find_station(unit) else {
            return;
        };

        self.disconnect_from_network(station);
        // `StationManager.removeStation`
        self.registered[station as usize - 1] = false;
        self.station_set.remove(&station);
        let station_unit = self.station(station).unit;
        if let Some(v) = self.by_unit.get_mut(&station_unit) {
            v.retain(|&s| s != station);
        }
        game.unit_mut(station_unit).set_train_station(false);

        let Some(cluster) = self.station(station).cluster else {
            return;
        };
        self.cluster_remove_station(cluster, station);
        if self.clusters[cluster as usize].size() == 0 {
            self.delete_cluster(cluster);
            self.dirty_clusters.remove(&cluster);
            return;
        }
        self.dirty_clusters.insert(cluster);
    }

    /// `findStationsPath`: the stations along the shortest route (empty when none).
    pub fn find_stations_path(
        &self,
        game: &GameImpl,
        from: StationId,
        to: StationId,
    ) -> Vec<StationId> {
        find_stations_path(self, game, &[from], to).unwrap_or_default()
    }

    /// `RailPathFinderServiceImpl.findTilePath`: rail A* on the mini map (empty when none).
    fn find_tile_path(&self, game: &GameImpl, from: TileRef, to: TileRef) -> Vec<TileRef> {
        let ctx = PathCtx {
            map: &game.map,
            mini_map: game.water_manager.mini_map(),
        };
        MiniMapTransformer::new(AStarRail {
            cross_impassable: !game
                .config()
                .rules()
                .has(crate::Builds::Rules::Change::RailNoImpassable4903),
        })
        .find_path(&ctx, Source::Single(from), to)
        .unwrap_or_default()
    }

    fn new_railroad(&mut self, from: StationId, to: StationId, tiles: Vec<TileRef>) -> RailId {
        let id = self.rails.len() as RailId;
        self.rails.push(Railroad {
            from,
            to,
            tiles,
            id,
        });
        id
    }

    /// Unlinks a railroad from both stations (`Railroad.delete` without the client update).
    fn railroad_delete(&mut self, rail: RailId) {
        let (from, to) = (self.rails[rail as usize].from, self.rails[rail as usize].to);
        self.station_remove_railroad(from, rail);
        self.station_remove_railroad(to, rail);
    }

    fn connect_to_existing_rails(&mut self, game: &GameImpl, station: StationId) -> bool {
        let station_tile = self.station_tile(game, station);
        let rails = self.grid.query(&game.map, station_tile, STATION_RADIUS);

        let mut edited_clusters: OrderedSet<ClusterId> = OrderedSet::new();
        for rail in rails {
            let (from, to) = (self.rails[rail as usize].from, self.rails[rail as usize].to);
            let closest = self.rails[rail as usize].get_closest_tile_index(&game.map, station_tile);
            if closest == 0 || closest >= self.rails[rail as usize].tiles.len() as i64 {
                continue;
            }

            // Disconnect current rail as it will become invalid
            self.railroad_delete(rail);
            self.grid.unregister(rail);

            let tiles = self.rails[rail as usize].tiles.clone();
            // JS `slice` with a negative index counts from the end (only reachable for an empty rail)
            let split = if closest < 0 {
                (tiles.len() as i64 + closest).max(0) as usize
            } else {
                closest as usize
            };
            let new_rail_from = self.new_railroad(from, station, tiles[..split].to_vec());
            let new_rail_to = self.new_railroad(station, to, tiles[split..].to_vec());

            // New station is connected to both new rails
            self.station_add_railroad(station, new_rail_from);
            self.station_add_railroad(station, new_rail_to);
            // From and to are connected to the new segments
            self.station_add_railroad(from, new_rail_from);
            self.station_add_railroad(to, new_rail_to);

            let (rail_to, rail_from) = (
                self.rails[new_rail_to as usize].clone(),
                self.rails[new_rail_from as usize].clone(),
            );
            self.grid.register(&game.map, &rail_to);
            self.grid.register(&game.map, &rail_from);
            if let Some(cluster) = self.station(from).cluster {
                self.cluster_add_station(game, cluster, station);
                edited_clusters.insert(cluster);
            }
        }
        // If multiple clusters own the new station, merge them into a single cluster
        if edited_clusters.len() > 1 {
            self.merge_clusters(game, &edited_clusters);
        }
        !edited_clusters.is_empty()
    }

    fn connect_to_nearby_stations(&mut self, game: &GameImpl, station: StationId) {
        let station_tile = self.station_tile(game, station);
        let station_unit = self.station(station).unit;
        let mut neighbors = game.nearby_units(
            station_tile,
            game.config().train_station_max_range(),
            &STRUCTURE_TYPES,
            None,
            false,
        );

        let mut edited_clusters: OrderedSet<ClusterId> = OrderedSet::new();
        neighbors.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("finite distances"));

        let min_range = game.config().train_station_min_range();
        for (unit, dist_squared) in neighbors {
            if unit == station_unit {
                continue;
            }
            let Some(neighbor_station) = self.find_station(unit) else {
                continue;
            };

            let distance_to_station =
                self.distance_from(neighbor_station, station, MAX_CONNECTION_DISTANCE);

            let Some(neighbor_cluster) = self.station(neighbor_station).cluster else {
                continue;
            };
            let connection_available =
                distance_to_station > MAX_CONNECTION_DISTANCE || distance_to_station == -1;
            if connection_available
                && dist_squared > min_range.powi(2)
                && self.connect(game, station, neighbor_station)
            {
                self.cluster_add_station(game, neighbor_cluster, station);
                edited_clusters.insert(neighbor_cluster);
            }
        }

        // If multiple clusters own the new station, merge them into a single cluster
        if edited_clusters.len() > 1 {
            self.merge_clusters(game, &edited_clusters);
        } else if edited_clusters.is_empty() {
            // If no cluster owns the station, creates a new one for it
            let new_cluster = self.new_cluster();
            self.cluster_add_station(game, new_cluster, station);
        }
    }

    fn disconnect_from_network(&mut self, station: StationId) {
        let rails: Vec<RailId> = self.station(station).railroads.iter().copied().collect();
        for rail in rails {
            self.railroad_delete(rail);
            self.grid.unregister(rail);
        }
        self.station_clear_railroads(station);
    }

    fn delete_cluster(&mut self, cluster: ClusterId) {
        let stations: Vec<StationId> = self.clusters[cluster as usize]
            .stations
            .iter()
            .copied()
            .collect();
        for s in stations {
            self.station_set_cluster(s, None);
        }
        self.cluster_clear(cluster);
    }

    fn connect(&mut self, game: &GameImpl, from: StationId, to: StationId) -> bool {
        let path = self.find_tile_path(
            game,
            self.station_tile(game, from),
            self.station_tile(game, to),
        );
        let within_limit = !game.config().rules().has(Change::RailroadMaxSizeLimit4337)
            || (path.len() as f64) < game.config().railroad_max_size();
        if !path.is_empty() && within_limit {
            let railroad = self.new_railroad(from, to, path);
            self.station_add_railroad(from, railroad);
            self.station_add_railroad(to, railroad);
            let rail = self.rails[railroad as usize].clone();
            self.grid.register(&game.map, &rail);
            return true;
        }
        false
    }

    fn distance_from(&self, start: StationId, dest: StationId, max_distance: i64) -> i64 {
        if start == dest {
            return 0;
        }

        let mut visited: HashSet<StationId> = HashSet::new();
        let mut queue: VecDeque<(StationId, i64)> = VecDeque::new();
        queue.push_back((start, 0));

        while let Some((station, distance)) = queue.pop_front() {
            if visited.contains(&station) {
                continue;
            }
            visited.insert(station);

            if distance >= max_distance {
                continue;
            }

            for neighbor in self.station_neighbors(station) {
                if neighbor == dest {
                    return distance + 1;
                }
                if !visited.contains(&neighbor) {
                    queue.push_back((neighbor, distance + 1));
                }
            }
        }

        // If destination not found within maxDistance
        -1
    }

    /// Stations reachable from `start`, in breadth-first visiting order.
    fn compute_cluster(&self, start: StationId) -> Vec<StationId> {
        let mut visited: HashSet<StationId> = HashSet::new();
        let mut order: Vec<StationId> = Vec::new();
        let mut queue: VecDeque<StationId> = VecDeque::new();
        queue.push_back(start);

        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);
            order.push(current);

            for neighbor in self.station_neighbors(current) {
                if !visited.contains(&neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }

        order
    }

    fn merge_clusters(&mut self, game: &GameImpl, clusters_to_merge: &OrderedSet<ClusterId>) {
        let merged = self.new_cluster();
        for &cluster in clusters_to_merge.iter() {
            self.cluster_merge(game, merged, cluster);
        }
    }
}

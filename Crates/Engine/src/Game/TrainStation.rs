// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TrainStation.ts: train stations, their clusters and the gold of a train stop.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream objects point at each other (station, cluster, railroad); here they are arena entries of
//! the `RailNetwork` addressed by id, and the cluster and station operations are methods of the network.
//! Removed stations stay in the arena because executions keep using them (`isActive`, `neighbors`).

use super::Game::{SmallId, UnitId, UnitType};
use super::GameImpl::GameImpl;
use super::GameMap::TileRef;
use super::RailNetwork::RailNetwork;
use super::Railroad::{RailId, StationId};
use crate::Compat::OrderedSet::OrderedSet;
use crate::Configuration::Config::TrainRelation;
use crate::Core::PseudoRandom::PseudoRandom;
use std::collections::HashMap;

/// Handle of a cluster (index into the network's cluster arena).
pub type ClusterId = u32;

/// A station of one structure; `unit` is the city, port or factory it serves.
#[derive(Debug)]
pub struct TrainStation {
    /// Assigned by the station manager (starts at 1).
    pub id: StationId,
    /// The structure served.
    pub unit: UnitId,
    pub(crate) cluster: Option<ClusterId>,
    pub(crate) railroads: OrderedSet<RailId>,
    pub(crate) railroad_by_neighbor: HashMap<StationId, RailId>,
}

/// Cluster of connected stations.
#[derive(Debug, Default)]
pub struct Cluster {
    pub(crate) stations: OrderedSet<StationId>,
    pub(crate) trade_stations: OrderedSet<StationId>,
}

impl Cluster {
    /// `size`.
    pub fn size(&self) -> usize {
        self.stations.len()
    }
}

impl RailNetwork {
    /// A station by id (removed stations stay readable).
    pub fn station(&self, id: StationId) -> &TrainStation {
        &self.stations[id as usize - 1]
    }

    pub(crate) fn station_mut(&mut self, id: StationId) -> &mut TrainStation {
        &mut self.stations[id as usize - 1]
    }

    /// `TrainStation.isActive`.
    pub fn station_is_active(&self, game: &GameImpl, id: StationId) -> bool {
        game.unit_any(self.station(id).unit).is_active()
    }

    /// `TrainStation.tile`.
    pub fn station_tile(&self, game: &GameImpl, id: StationId) -> TileRef {
        game.unit_any(self.station(id).unit).tile()
    }

    /// `TrainStation.getCluster`.
    pub fn station_cluster(&self, id: StationId) -> Option<ClusterId> {
        self.station(id).cluster
    }

    /// `TrainStation.neighbors`: the other end of every railroad, in railroad insertion order.
    pub fn station_neighbors(&self, id: StationId) -> Vec<StationId> {
        self.station(id)
            .railroads
            .iter()
            .map(|&r| {
                let rail = &self.rails[r as usize];
                if rail.from != id { rail.from } else { rail.to }
            })
            .collect()
    }

    /// `TrainStation.getRailroadTo`.
    pub fn station_railroad_to(&self, id: StationId, other: StationId) -> Option<RailId> {
        self.station(id).railroad_by_neighbor.get(&other).copied()
    }

    /// `TrainStation.addRailroad`.
    pub(crate) fn station_add_railroad(&mut self, id: StationId, rail: RailId) {
        let r = &self.rails[rail as usize];
        let neighbor = if r.from == id { r.to } else { r.from };
        let st = self.station_mut(id);
        st.railroads.insert(rail);
        st.railroad_by_neighbor.insert(neighbor, rail);
    }

    /// `TrainStation.removeRailroad`.
    pub(crate) fn station_remove_railroad(&mut self, id: StationId, rail: RailId) {
        let r = &self.rails[rail as usize];
        let neighbor = if r.from == id { r.to } else { r.from };
        let st = self.station_mut(id);
        st.railroads.remove(&rail);
        st.railroad_by_neighbor.remove(&neighbor);
    }

    /// `TrainStation.clearRailroads`.
    pub(crate) fn station_clear_railroads(&mut self, id: StationId) {
        let st = self.station_mut(id);
        st.railroads.clear();
        st.railroad_by_neighbor.clear();
    }

    /// `TrainStation.setCluster`: leaves the previous cluster first.
    pub(crate) fn station_set_cluster(&mut self, id: StationId, cluster: Option<ClusterId>) {
        if let Some(old) = self.station(id).cluster
            && (self.leave_same_cluster || Some(old) != cluster)
        {
            self.cluster_remove_station(old, id);
        }
        self.station_mut(id).cluster = cluster;
    }

    /// `TrainStation.tradeAvailable(otherPlayer)`.
    pub fn station_trade_available(&self, game: &GameImpl, id: StationId, other: SmallId) -> bool {
        let player = game.unit_any(self.station(id).unit).owner_id();
        other == player || game.player(player).can_trade(other)
    }

    /// `new Cluster()`.
    pub(crate) fn new_cluster(&mut self) -> ClusterId {
        self.clusters.push(Cluster::default());
        (self.clusters.len() - 1) as ClusterId
    }

    /// `Cluster.addStation`.
    pub(crate) fn cluster_add_station(
        &mut self,
        game: &GameImpl,
        cluster: ClusterId,
        id: StationId,
    ) {
        let unit_type = game.unit_any(self.station(id).unit).unit_type();
        let c = &mut self.clusters[cluster as usize];
        c.stations.insert(id);
        if matches!(unit_type, UnitType::City | UnitType::Port) {
            c.trade_stations.insert(id);
        }
        self.station_set_cluster(id, Some(cluster));
    }

    /// `Cluster.removeStation`.
    pub(crate) fn cluster_remove_station(&mut self, cluster: ClusterId, id: StationId) {
        let c = &mut self.clusters[cluster as usize];
        c.stations.remove(&id);
        c.trade_stations.remove(&id);
    }

    /// `Cluster.addStations`.
    pub(crate) fn cluster_add_stations(
        &mut self,
        game: &GameImpl,
        cluster: ClusterId,
        stations: &[StationId],
    ) {
        for &s in stations {
            self.cluster_add_station(game, cluster, s);
        }
    }

    /// `Cluster.merge`: moves every station of `other` into `cluster`.
    pub(crate) fn cluster_merge(&mut self, game: &GameImpl, cluster: ClusterId, other: ClusterId) {
        let stations: Vec<StationId> = self.clusters[other as usize]
            .stations
            .iter()
            .copied()
            .collect();
        for s in stations {
            self.cluster_add_station(game, cluster, s);
        }
    }

    /// `Cluster.clear`.
    pub(crate) fn cluster_clear(&mut self, cluster: ClusterId) {
        let c = &mut self.clusters[cluster as usize];
        c.stations.clear();
        c.trade_stations.clear();
    }

    /// A cluster by handle.
    pub fn cluster(&self, id: ClusterId) -> &Cluster {
        &self.clusters[id as usize]
    }

    /// `Cluster.hasAnyTradeDestination(player)`.
    pub fn cluster_has_any_trade_destination(
        &self,
        game: &GameImpl,
        cluster: ClusterId,
        player: SmallId,
    ) -> bool {
        self.clusters[cluster as usize]
            .trade_stations
            .iter()
            .any(|&s| self.station_trade_available(game, s, player))
    }

    /// `Cluster.randomTradeDestination(player, random)`: reservoir sampling over the eligible stations.
    pub fn cluster_random_trade_destination(
        &self,
        game: &GameImpl,
        cluster: ClusterId,
        player: SmallId,
        random: &mut PseudoRandom,
    ) -> Option<StationId> {
        let mut selected = None;
        let mut eligible_seen = 0.0;
        for &s in self.clusters[cluster as usize].trade_stations.iter() {
            if !self.station_trade_available(game, s, player) {
                continue;
            }
            eligible_seen += 1.0;
            // Reservoir sampling: keep each eligible station with probability 1/eligibleSeen
            if random.next_int(0.0, eligible_seen) == 0.0 {
                selected = Some(s);
            }
        }
        selected
    }
}

/// `rel(player, other)` of upstream.
fn relation(game: &GameImpl, player: SmallId, other: SmallId) -> TrainRelation {
    if player == other {
        return TrainRelation::Self_;
    }
    let p = game.player(player);
    if p.is_on_same_team(other) {
        TrainRelation::Team
    } else if p.is_allied_with(other) {
        TrainRelation::Ally
    } else {
        TrainRelation::Other
    }
}

/// `TrainStation.onTrainStop`: pays the train owner (and the station owner when different) at cities and ports.
pub fn on_train_stop(
    game: &mut GameImpl,
    station_unit: UnitId,
    train_owner: SmallId,
    trade_stops_visited: u32,
) {
    let unit_type = game.unit_any(station_unit).unit_type();
    // The factory handler does nothing and other unit types have none
    if !matches!(unit_type, UnitType::City | UnitType::Port) {
        return;
    }
    let station_owner = game.unit_any(station_unit).owner_id();
    let gold = game.config().train_gold(
        relation(game, train_owner, station_owner),
        trade_stops_visited as f64,
        game.player(train_owner).is_lobby_creator(),
    );
    // Share revenue with the station owner if it's not the current player
    if train_owner != station_owner {
        let mut owner = game.player_mut(station_owner);
        owner.add_gold(gold);
        owner.add_train_gold(gold);
    }
    let mut owner = game.player_mut(train_owner);
    owner.add_gold(gold);
    owner.add_train_gold(gold);
}

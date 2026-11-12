// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/PathFinder.Station.ts: A* over the station graph of the rail network.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream builds a new finder per query, sized by the station count at that moment; so does this.

use super::Algorithms::AStar::{AStar, AStarAdapter};
use crate::Game::GameImpl::GameImpl;
use crate::Game::RailNetwork::RailNetwork;
use crate::Game::Railroad::StationId;

/// `StationPathFinder.findPath`: the stations from any of `from` (in `to`'s cluster) to `to`.
pub fn find_stations_path(
    network: &RailNetwork,
    game: &GameImpl,
    from: &[StationId],
    to: StationId,
) -> Option<Vec<StationId>> {
    let to_cluster = network.station_cluster(to);
    let from_ids: Vec<u32> = from
        .iter()
        .copied()
        .filter(|&s| network.station_cluster(s) == to_cluster)
        .collect();
    if from_ids.is_empty() {
        return None;
    }

    let adapter = StationGraphAdapter { network, game };
    let mut a_star = AStar::new(adapter, None);
    a_star.find_path(&from_ids, to)
}

/// Upstream `StationGraphAdapter`.
struct StationGraphAdapter<'a> {
    network: &'a RailNetwork,
    game: &'a GameImpl,
}

impl AStarAdapter for StationGraphAdapter<'_> {
    fn num_nodes(&self) -> usize {
        self.network.station_count()
    }

    fn max_neighbors(&self) -> usize {
        32
    }

    fn max_priority(&self) -> usize {
        (self.game.map.width() + self.game.map.height()) as usize
    }

    fn neighbors(&self, node: u32, buffer: &mut [u32]) -> usize {
        if self.network.station_by_id(node).is_none() {
            return 0;
        }
        let mut count = 0;
        for n in self.network.station_neighbors(node) {
            // A typed-array write past the buffer is a silent no-op upstream
            if count < buffer.len() {
                buffer[count] = n;
            }
            count += 1;
        }
        count.min(buffer.len())
    }

    fn cost(&self, _from: u32, _to: u32, _prev: Option<u32>) -> f64 {
        1.0
    }

    fn heuristic(&self, node: u32, goal: u32) -> f64 {
        if self.network.station_by_id(node).is_none() || self.network.station_by_id(goal).is_none()
        {
            return 0.0;
        }
        let map = &self.game.map;
        let a = self.network.station_tile(self.game, node);
        let b = self.network.station_tile(self.game, goal);
        ((map.x(a) as f64 - map.x(b) as f64).abs()) + (map.y(a) as f64 - map.y(b) as f64).abs()
    }
}

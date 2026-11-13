// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/PlayerExecution.ts (v0.27.10): territory cleanup with sorted BFS clusters.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Side-by-side variant of `removeClusters` and `removeCluster`, selected by the absence of
//! `Change::ClusterLinearScan2614`. Clusters are found by a FIFO flood over the 8-neighbourhood,
//! sorted by size (stable, descending) and the removed tiles come from a stack `bfs` in N, S, W, E order.

use crate::Execution::PlayerExecution::PlayerExecution;
use crate::Game::Game::Cell;
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::collections::VecDeque;

/// Bounding box (min x, min y, max x, max y) of a tile list.
fn bounding_box(game: &GameImpl, tiles: &[TileRef]) -> (i32, i32, i32, i32) {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for &t in tiles {
        let (x, y) = (game.map.x(t) as i32, game.map.y(t) as i32);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    (min_x, min_y, max_x, max_y)
}

impl PlayerExecution {
    /// `calculateClusters` of v0.27.10: FIFO flood over the border tiles, 8 neighbours.
    fn calculate_clusters_v27(&self, game: &GameImpl) -> Vec<Vec<TileRef>> {
        let player = game.player(self.player);
        let border = player.border_tiles();
        let n = game.map.width() as usize * game.map.height() as usize;
        let mut seen = vec![false; n];
        let mut clusters = Vec::new();
        let mut nbuf = [0; 8];
        for tile in border.iter() {
            if seen[tile as usize] {
                continue;
            }
            let mut cluster = Vec::new();
            let mut queue = VecDeque::new();
            queue.push_back(tile);
            seen[tile as usize] = true;
            while let Some(curr) = queue.pop_front() {
                cluster.push(curr);
                let count = game.map.neighbors8(curr, &mut nbuf);
                for &nb in &nbuf[..count] {
                    if border.has(nb) && !seen[nb as usize] {
                        queue.push_back(nb);
                        seen[nb as usize] = true;
                    }
                }
            }
            clusters.push(cluster);
        }
        clusters
    }

    /// `removeClusters` of v0.27.10.
    pub(super) fn remove_clusters_v27(&mut self, game: &mut GameImpl) {
        let player = self.player;
        let mut clusters = self.calculate_clusters_v27(game);
        if clusters.is_empty() {
            // Upstream throws "No clusters"; a living player always has a border tile.
            game.player_impl_mut(player).largest_cluster_bounding_box = None;
            return;
        }
        // `Array.prototype.sort` is stable, like `sort_by`.
        clusters.sort_by_key(|c| std::cmp::Reverse(c.len()));
        let main = clusters.remove(0);
        let (a, b, c, d) = bounding_box(game, &main);
        game.player_impl_mut(player).largest_cluster_bounding_box =
            Some((Cell::new(a, b), Cell::new(c, d)));
        let surrounded_by = self.surrounded_by_same_player(game, &main, a, b, c, d);
        if let Some(s) = surrounded_by
            && !game.player(s).is_friendly(player, false)
        {
            self.remove_cluster_v27(game, &main);
        }
        for cluster in &clusters {
            let (a, b, c, d) = bounding_box(game, cluster);
            if self.is_surrounded(game, cluster, a, b, c, d) {
                self.remove_cluster_v27(game, cluster);
            }
        }
    }

    /// `removeCluster` of v0.27.10: the annexed tiles come from `GameMap.bfs` (stack order, N, S, W, E).
    fn remove_cluster_v27(&mut self, game: &mut GameImpl, cluster: &[TileRef]) {
        let player = self.player;
        for &t in cluster {
            if game.owner_id(t) != player {
                return;
            }
        }
        let Some(capturing) = self.get_capturing_player_nswe(game, cluster) else {
            return;
        };
        let Some(&first_tile) = cluster.first() else {
            return;
        };
        let n = game.map.width() as usize * game.map.height() as usize;
        let mut seen = vec![false; n];
        let mut tiles: Vec<TileRef> = Vec::new();
        let mut stack: Vec<TileRef> = Vec::new();
        if game.map.owner_id(first_tile) == player {
            seen[first_tile as usize] = true;
            tiles.push(first_tile);
            stack.push(first_tile);
        }
        let mut nbuf = [0; 4];
        while let Some(curr) = stack.pop() {
            let count = game.map.neighbors4_nswe(curr, &mut nbuf);
            for &nb in &nbuf[..count] {
                if !seen[nb as usize] && game.map.owner_id(nb) == player {
                    seen[nb as usize] = true;
                    tiles.push(nb);
                    stack.push(nb);
                }
            }
        }
        if game.player(player).num_tiles_owned() == tiles.len() {
            game.conquer_player(capturing, player);
        }
        for tile in tiles {
            game.player_mut(capturing).conquer(tile);
        }
    }
}

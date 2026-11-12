// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/PlayerExecution.ts: per-player upkeep, growth and territory cleanup.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The stats calls (`goldWork`, `recordDeathPosition`, `playerKilled`) are record-keeping only
//! and dropped. Flood fills use the game's shared generation-stamped scratch like upstream.

use crate::Compat::OrderedMap::OrderedMap;
use crate::Core::Util::{get_mode, simple_hash};
use crate::Execution::Execution::Execution;
use crate::Game::Game::{Cell, STRUCTURES, SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::{GameMapImpl, TileRef};

const TICKS_PER_CLUSTER_CALC: f64 = 20.0;

/// Include predicate of a flood fill: gets the visited array and the candidate tile.
type IncludeFn<'a> = &'a dyn Fn(&[u32], TileRef) -> bool;

/// Upkeep of one player: relations decay, structure capture, troops and gold, alliances, clusters.
pub struct PlayerExecution {
    player: SmallId,
    active: bool,
    last_calc: f64,
}

impl PlayerExecution {
    /// `new PlayerExecution(player)`.
    pub fn new(player: SmallId) -> Self {
        Self {
            player,
            active: true,
            last_calc: 0.0,
        }
    }

    fn remove_clusters(&mut self, game: &mut GameImpl) {
        let (clusters, boxes) = self.calculate_clusters(game);
        let player = self.player;

        if clusters.is_empty() {
            game.player_impl_mut(player).largest_cluster_bounding_box = None;
            return;
        }

        if clusters.len() == 1 {
            game.player_impl_mut(player).largest_cluster_bounding_box =
                Some((Cell::new(boxes[0], boxes[1]), Cell::new(boxes[2], boxes[3])));
            let surrounded_by = self.surrounded_by_same_player(
                game,
                &clusters[0],
                boxes[0],
                boxes[1],
                boxes[2],
                boxes[3],
            );
            if let Some(s) = surrounded_by
                && !game.player(s).is_friendly(player, false)
            {
                self.remove_cluster(game, &clusters[0]);
            }
            return;
        }

        let mut largest_index = 0usize;
        for i in 1..clusters.len() {
            if clusters[i].len() > clusters[largest_index].len() {
                largest_index = i;
            }
        }

        let t_min_x = boxes[largest_index * 4];
        let t_min_y = boxes[largest_index * 4 + 1];
        let t_max_x = boxes[largest_index * 4 + 2];
        let t_max_y = boxes[largest_index * 4 + 3];

        let mut cluster_territory_ids: Option<Vec<i32>> = None;
        let mut next_territory_id = 1i32;

        // Before PR 5545 the largest border cluster was never tested for being a hole.
        let hole_scan_end = if game
            .config()
            .rules()
            .has(crate::Builds::Rules::Change::InternalBorderAnnexFix5545)
        {
            clusters.len()
        } else {
            0
        };
        let mut is_largest_hole = false;
        for j in 0..hole_scan_end {
            if j == largest_index {
                continue;
            }
            let j_idx = j * 4;
            if boxes[j_idx] <= t_min_x
                && boxes[j_idx + 1] <= t_min_y
                && boxes[j_idx + 2] >= t_max_x
                && boxes[j_idx + 3] >= t_max_y
            {
                if cluster_territory_ids.is_none() {
                    cluster_territory_ids = Some(vec![0; clusters.len()]);
                    for (c, cl) in clusters.iter().enumerate() {
                        for &t in cl {
                            game.scratch.cluster_index_map[t as usize] = c as i32 + 1;
                        }
                    }
                }
                next_territory_id = self.check_and_assign_territory(
                    game,
                    &clusters,
                    largest_index,
                    j,
                    cluster_territory_ids.as_mut().unwrap(),
                    next_territory_id,
                );
                let ids = cluster_territory_ids.as_ref().unwrap();
                if ids[largest_index] == ids[j] {
                    is_largest_hole = true;
                    break;
                }
            }
        }

        if is_largest_hole {
            let mut best_index: i64 = -1;
            let mut best_length: i64 = -1;
            for i in 0..clusters.len() {
                if i == largest_index || (clusters[i].len() as i64) <= best_length {
                    continue;
                }
                let mut is_hole = false;
                let i_idx = i * 4;
                let (i_min_x, i_min_y, i_max_x, i_max_y) = (
                    boxes[i_idx],
                    boxes[i_idx + 1],
                    boxes[i_idx + 2],
                    boxes[i_idx + 3],
                );
                for j in 0..clusters.len() {
                    if j == i {
                        continue;
                    }
                    let j_idx = j * 4;
                    if boxes[j_idx] <= i_min_x
                        && boxes[j_idx + 1] <= i_min_y
                        && boxes[j_idx + 2] >= i_max_x
                        && boxes[j_idx + 3] >= i_max_y
                    {
                        next_territory_id = self.check_and_assign_territory(
                            game,
                            &clusters,
                            i,
                            j,
                            cluster_territory_ids.as_mut().unwrap(),
                            next_territory_id,
                        );
                        let ids = cluster_territory_ids.as_ref().unwrap();
                        if ids[i] == ids[j] {
                            is_hole = true;
                            break;
                        }
                    }
                }
                if !is_hole {
                    best_length = clusters[i].len() as i64;
                    best_index = i as i64;
                }
            }
            if best_index != -1 {
                largest_index = best_index as usize;
            }
        }

        if cluster_territory_ids.is_some() {
            for cl in &clusters {
                for &t in cl {
                    game.scratch.cluster_index_map[t as usize] = 0;
                }
            }
        }

        let largest_cluster = &clusters[largest_index];
        let l_idx = largest_index * 4;
        game.player_impl_mut(player).largest_cluster_bounding_box = Some((
            Cell::new(boxes[l_idx], boxes[l_idx + 1]),
            Cell::new(boxes[l_idx + 2], boxes[l_idx + 3]),
        ));

        let surrounded_by = self.surrounded_by_same_player(
            game,
            largest_cluster,
            boxes[l_idx],
            boxes[l_idx + 1],
            boxes[l_idx + 2],
            boxes[l_idx + 3],
        );
        if let Some(s) = surrounded_by
            && !game.player(s).is_friendly(player, false)
        {
            self.remove_cluster(game, largest_cluster);
        }

        for (i, cluster) in clusters.iter().enumerate() {
            if i == largest_index {
                continue;
            }
            let idx = i * 4;
            if self.is_surrounded(
                game,
                cluster,
                boxes[idx],
                boxes[idx + 1],
                boxes[idx + 2],
                boxes[idx + 3],
            ) {
                self.remove_cluster(game, cluster);
            }
        }
    }

    fn check_and_assign_territory(
        &self,
        game: &mut GameImpl,
        clusters: &[Vec<TileRef>],
        c_idx_a: usize,
        c_idx_b: usize,
        cluster_territory_ids: &mut [i32],
        mut next_territory_id: i32,
    ) -> i32 {
        if cluster_territory_ids[c_idx_a] != 0
            && cluster_territory_ids[c_idx_a] == cluster_territory_ids[c_idx_b]
        {
            return next_territory_id;
        }
        if cluster_territory_ids[c_idx_a] != 0 && cluster_territory_ids[c_idx_b] != 0 {
            return next_territory_id;
        }

        let start_idx = if cluster_territory_ids[c_idx_b] == 0 {
            c_idx_b
        } else {
            c_idx_a
        };
        let territory_id = next_territory_id;
        next_territory_id += 1;

        let map = &game.map;
        let scratch = &mut game.scratch;
        let flood_gen = scratch.bump_generation();
        scratch.stack.clear();

        let start = clusters[start_idx][0];
        scratch.visited[start as usize] = flood_gen;
        scratch.stack.push(start);

        let my_owner = self.player;
        cluster_territory_ids[start_idx] = territory_id;

        let mut nbuf8 = [0; 8];
        while let Some(tile) = scratch.stack.pop() {
            let n_neighbors = map.neighbors8(tile, &mut nbuf8);
            for &n in &nbuf8[..n_neighbors] {
                if scratch.visited[n as usize] == flood_gen {
                    continue;
                }
                if map.owner_id(n) == my_owner {
                    scratch.visited[n as usize] = flood_gen;
                    scratch.stack.push(n);
                    let c_idx_plus_one = scratch.cluster_index_map[n as usize];
                    if c_idx_plus_one > 0 {
                        cluster_territory_ids[(c_idx_plus_one - 1) as usize] = territory_id;
                    }
                }
            }
        }
        next_territory_id
    }

    fn surrounded_by_same_player(
        &self,
        game: &GameImpl,
        cluster: &[TileRef],
        box_min_x: i32,
        box_min_y: i32,
        box_max_x: i32,
        box_max_y: i32,
    ) -> Option<SmallId> {
        let mut enemies: Vec<SmallId> = Vec::new();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            1_000_000_000i32,
            1_000_000_000i32,
            -1_000_000_000i32,
            -1_000_000_000i32,
        );
        let map = &game.map;
        let my_small_id = self.player;
        let mut nbuf = [0; 4];
        for &tile in cluster {
            if map.is_ocean_shore(tile) || map.is_on_edge_of_map(tile) {
                return None;
            }
            let n_neighbors = map.neighbors4(tile, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                let owner_id = map.owner_id(n);
                if owner_id == 0 {
                    return None;
                }
                if owner_id != my_small_id {
                    if !enemies.contains(&owner_id) {
                        enemies.push(owner_id);
                    }
                    let px = map.x(n) as i32;
                    let py = map.y(n) as i32;
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px);
                    max_y = max_y.max(py);
                }
            }
            if enemies.len() != 1 {
                return None;
            }
        }
        if enemies.len() != 1 {
            return None;
        }
        if min_x <= box_min_x && min_y <= box_min_y && max_x >= box_max_x && max_y >= box_max_y {
            return Some(enemies[0]);
        }
        None
    }

    fn is_surrounded(
        &self,
        game: &GameImpl,
        cluster: &[TileRef],
        box_min_x: i32,
        box_min_y: i32,
        box_max_x: i32,
        box_max_y: i32,
    ) -> bool {
        let mut has_enemy = false;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            1_000_000_000i32,
            1_000_000_000i32,
            -1_000_000_000i32,
            -1_000_000_000i32,
        );
        let map = &game.map;
        let my_small_id = self.player;
        let mut nbuf = [0; 4];
        for &tr in cluster {
            if map.is_shore(tr) || map.is_on_edge_of_map(tr) {
                return false;
            }
            let n_neighbors = map.neighbors4(tr, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                let owner_id = map.owner_id(n);
                if owner_id != 0 && owner_id != my_small_id {
                    has_enemy = true;
                    let x = map.x(n) as i32;
                    let y = map.y(n) as i32;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if !has_enemy {
            return false;
        }
        min_x <= box_min_x && min_y <= box_min_y && max_x >= box_max_x && max_y >= box_max_y
    }

    fn remove_cluster(&mut self, game: &mut GameImpl, cluster: &[TileRef]) {
        let player = self.player;
        for &t in cluster {
            if game.owner_id(t) != player {
                return;
            }
        }
        let Some(capturing) = self.get_capturing_player(game, cluster) else {
            return;
        };
        let Some(&first_tile) = cluster.first() else {
            return;
        };
        if !self.is_enclosed(game, first_tile) {
            return;
        }
        let tiles = {
            let map = &game.map;
            let scratch = &mut game.scratch;
            let current_gen = scratch.bump_generation();
            flood_fill_with_gen(
                map,
                &mut scratch.visited,
                &mut scratch.stack,
                current_gen,
                &[first_tile],
                false,
                &|_, t| map.owner_id(t) == player,
                None,
            )
        };
        if game.player(player).num_tiles_owned() == tiles.len() {
            game.conquer_player(capturing, player);
        }
        for tile in tiles {
            game.player_mut(capturing).conquer(tile);
        }
    }

    fn is_enclosed(&self, game: &mut GameImpl, start: TileRef) -> bool {
        let map = &game.map;
        let my_small_id = self.player;
        let scratch = &mut game.scratch;
        let gen_ = scratch.bump_generation();
        scratch.stack.clear();
        scratch.visited[start as usize] = gen_;
        scratch.stack.push(start);
        let mut nbuf = [0; 4];
        while let Some(tile) = scratch.stack.pop() {
            if map.is_on_edge_of_map(tile) {
                return false;
            }
            let n_neighbors = map.neighbors4(tile, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                if scratch.visited[n as usize] == gen_ {
                    continue;
                }
                let owner_id = map.owner_id(n);
                if owner_id != 0 && owner_id != my_small_id {
                    continue;
                }
                if owner_id == 0 && !map.is_land(n) {
                    return false;
                }
                scratch.visited[n as usize] = gen_;
                scratch.stack.push(n);
            }
        }
        true
    }

    fn get_capturing_player(&self, game: &GameImpl, cluster: &[TileRef]) -> Option<SmallId> {
        let mut neighbors: OrderedMap<SmallId, f64> = OrderedMap::new();
        let map = &game.map;
        let my_small_id = self.player;
        let mut nbuf = [0; 4];
        for &t in cluster {
            let n_neighbors = map.neighbors4(t, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                let owner_id = map.owner_id(n);
                if owner_id == 0 || owner_id == my_small_id {
                    continue;
                }
                if !game.player(owner_id).is_friendly(my_small_id, false) {
                    let prev = neighbors.get(&owner_id).copied().unwrap_or(0.0);
                    neighbors.insert(owner_id, prev + 1.0);
                }
            }
        }
        if neighbors.is_empty() {
            return None;
        }
        let mut largest_neighbor_attack: Option<SmallId> = None;
        let mut largest_troop_count = 0.0;
        for &neighbor in neighbors.keys() {
            for &attack in game.player(neighbor).outgoing_attacks() {
                let a = &game.attacks[attack as usize];
                if a.target == my_small_id && a.troops > largest_troop_count {
                    largest_troop_count = a.troops;
                    largest_neighbor_attack = Some(neighbor);
                }
            }
        }
        if largest_neighbor_attack.is_some() {
            return largest_neighbor_attack;
        }
        get_mode(&neighbors)
    }

    fn calculate_clusters(&self, game: &mut GameImpl) -> (Vec<Vec<TileRef>>, Vec<i32>) {
        let border_tiles: Vec<TileRef> = game.player(self.player).border_tiles().iter().collect();
        if border_tiles.is_empty() {
            return (vec![], vec![]);
        }
        let map = &game.map;
        let scratch = &mut game.scratch;
        let border_gen = scratch.bump_generation();
        for &tile in &border_tiles {
            scratch.visited[tile as usize] = border_gen;
        }
        let current_gen = scratch.bump_generation();

        let mut clusters: Vec<Vec<TileRef>> = Vec::new();
        let mut boxes: Vec<i32> = Vec::new();
        for &start_tile in &border_tiles {
            if scratch.visited[start_tile as usize] == current_gen {
                continue;
            }
            let mut out_box = [0i32; 4];
            let cluster = flood_fill_with_gen(
                map,
                &mut scratch.visited,
                &mut scratch.stack,
                current_gen,
                &[start_tile],
                true,
                &|visited, t| visited[t as usize] == border_gen,
                Some(&mut out_box),
            );
            clusters.push(cluster);
            boxes.extend_from_slice(&out_box);
        }
        (clusters, boxes)
    }

    fn remove_on_death(&self, game: &mut GameImpl) {
        let player = self.player;
        let gold = game.player(player).gold();
        game.player_mut(player).remove_gold(gold);
        let units: Vec<_> = game.player(player).units().to_vec();
        for u in units {
            let t = game.unit_any(u).unit_type();
            if t != UnitType::AtomBomb
                && t != UnitType::HydrogenBomb
                && t != UnitType::MIRVWarhead
                && t != UnitType::MIRV
            {
                game.unit_mut(u).delete(None, None);
            }
        }
        game.player_mut(player).remove_all_alliances();
    }
}

/// `floodFillWithGen`: DFS over the tiles accepted by `include`, stamping `visited` with the
/// generation; returns the tiles in mark order and optionally their bounding box.
#[allow(clippy::too_many_arguments)]
fn flood_fill_with_gen(
    map: &GameMapImpl,
    visited: &mut [u32],
    stack: &mut Vec<TileRef>,
    current_gen: u32,
    start_tiles: &[TileRef],
    diag: bool,
    include: IncludeFn<'_>,
    out_box: Option<&mut [i32; 4]>,
) -> Vec<TileRef> {
    let mut result: Vec<TileRef> = Vec::new();
    stack.clear();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        1_000_000_000i32,
        1_000_000_000i32,
        -1_000_000_000i32,
        -1_000_000_000i32,
    );
    let track = out_box.is_some();
    for &start in start_tiles {
        if visited[start as usize] == current_gen {
            continue;
        }
        if !include(visited, start) {
            continue;
        }
        visited[start as usize] = current_gen;
        result.push(start);
        stack.push(start);
        if track {
            let x = map.x(start) as i32;
            let y = map.y(start) as i32;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    let mut nbuf8 = [0; 8];
    let mut nbuf4 = [0; 4];
    while let Some(tile) = stack.pop() {
        let (buf, n_neighbors): (&[TileRef], usize) = if diag {
            let n = map.neighbors8(tile, &mut nbuf8);
            (&nbuf8, n)
        } else {
            let n = map.neighbors4(tile, &mut nbuf4);
            (&nbuf4, n)
        };
        for &neighbor in &buf[..n_neighbors] {
            if visited[neighbor as usize] == current_gen {
                continue;
            }
            if !include(visited, neighbor) {
                continue;
            }
            visited[neighbor as usize] = current_gen;
            result.push(neighbor);
            stack.push(neighbor);
            if track {
                let x = map.x(neighbor) as i32;
                let y = map.y(neighbor) as i32;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if let Some(b) = out_box {
        *b = [min_x, min_y, max_x, max_y];
    }
    result
}

impl Execution for PlayerExecution {
    fn init(&mut self, game: &mut GameImpl, ticks: Tick) {
        let h = simple_hash(game.player(self.player).id()) as f64;
        self.last_calc = ticks as f64 + (h % TICKS_PER_CLUSTER_CALC);
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        let player = self.player;
        game.player_mut(player).decay_relations();
        let units: Vec<_> = game.player(player).units().to_vec();
        for u in units {
            let t = game.unit_any(u).unit_type();
            if !STRUCTURES.contains(&t) {
                continue;
            }
            let owner = game.owner_id(game.unit_any(u).tile());
            if owner == 0 {
                game.unit_mut(u).delete(None, None);
                continue;
            }
            if owner == player {
                continue;
            }
            let captor = owner;
            if t == UnitType::DefensePost {
                game.unit_mut(u).delete(Some(true), Some(captor));
            } else {
                game.player_mut(captor).capture_unit(u);
            }
        }

        if !game.player(player).is_alive() {
            self.remove_on_death(game);
            self.active = false;
            return;
        }

        let troop_inc = game.player(player).troop_increase_rate();
        game.player_mut(player).add_troops(troop_inc);
        let gold_from_workers = game.player(player).gold_addition_rate();
        game.player_mut(player).add_gold(gold_from_workers);

        let alliances: Vec<_> = game.player(player).alliances().to_vec();
        for alliance in alliances {
            if game.alliances[alliance as usize].expires_at() as f64 <= game.ticks() as f64 {
                game.expire_alliance_by_id(alliance);
            }
        }

        for embargo in game.player(player).get_embargoes() {
            if embargo.is_temporary
                && game.ticks() as f64 - embargo.created_at as f64
                    > game.config().temporary_embargo_duration() as f64
            {
                game.player_mut(player).stop_embargo(embargo.target);
            }
        }

        if (ticks as f64 - self.last_calc > TICKS_PER_CLUSTER_CALC
            || game.player(player).num_tiles_owned() < 100)
            && game.player(player).last_tile_change() >= self.last_calc
        {
            self.last_calc = ticks as f64;
            self.remove_clusters(game);
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}

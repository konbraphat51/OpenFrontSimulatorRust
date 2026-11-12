// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/PlayerExecution.ts (v0.36.0): territory cleanup ranked by 4-way territory area, with the pinch pass.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Side-by-side variant of `removeClusters` and its helpers, selected by `Change::PlayerClusterRewrite36`.
//! It shares the game's generation-stamped scratch exactly like upstream, because stamps left by one
//! pass are read as "visited" by later ones.

use crate::Execution::PlayerExecution::{PlayerExecution, flood_fill_with_gen};
use crate::Game::Game::{Cell, SmallId};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Per-cluster results of `floodTerritoryAreas`.
struct TerritoryAreas {
    cluster_sizes: Vec<i32>,
    cluster_tile_ids: Vec<Vec<i32>>,
    territory_areas: Vec<i32>,
    max_territory_area: i32,
}

/// Clusters of border tiles with their boxes, and the territory-id bookkeeping of the pinch pass.
struct PinchState {
    cluster_territory_ids: Option<Vec<i32>>,
    next_territory_id: i32,
}

/// `bumpGenerations(count)`: starts `count` passes at once and returns the first generation.
fn bump_generations(game: &mut GameImpl, count: u32) -> u32 {
    loop {
        let first = game.scratch.bump_generation();
        let mut last = first;
        for _ in 1..count {
            last = game.scratch.bump_generation();
        }
        if last.wrapping_sub(first) == count - 1 {
            return first;
        }
    }
}

impl PlayerExecution {
    /// `removeClusters` of v0.36.0.
    pub(super) fn remove_clusters_v36(&mut self, game: &mut GameImpl) {
        let player = self.player;
        let (clusters, boxes) = self.calculate_clusters(game);
        if clusters.is_empty() {
            game.player_impl_mut(player).largest_cluster_bounding_box = None;
            return;
        }

        let is_pinched: Vec<bool> = clusters
            .iter()
            .map(|c| self.has_diagonal_pinch(game, c))
            .collect();

        // A lone unpinched cluster is the largest by definition and the pinch pass skips it.
        let areas = if clusters.len() > 1 || is_pinched[0] {
            Some(self.flood_territory_areas(game, &clusters))
        } else {
            None
        };
        let largest_index = match &areas {
            None => 0,
            Some(a) => find_largest_cluster_index(&clusters, &boxes, &a.cluster_sizes),
        };
        let largest_cluster = &clusters[largest_index];
        let l_idx = largest_index * 4;
        let lb = [
            boxes[l_idx],
            boxes[l_idx + 1],
            boxes[l_idx + 2],
            boxes[l_idx + 3],
        ];
        game.player_impl_mut(player).largest_cluster_bounding_box =
            Some((Cell::new(lb[0], lb[1]), Cell::new(lb[2], lb[3])));

        let mut cluster_removed = vec![false; clusters.len()];

        // Pass 0: the main body goes to a single enclosing enemy.
        let surrounded_by = self.surrounded_by_same_player_v36(game, largest_cluster, lb);
        if let Some(s) = surrounded_by
            && !game.player(s).is_friendly(player, false)
            && self.remove_cluster_v36(
                game,
                largest_cluster,
                lb,
                Some(s),
                is_pinched[largest_index],
            )
        {
            cluster_removed[largest_index] = true;
        }

        // Pass 1: whole-cluster removal for secondary clusters.
        for i in 0..clusters.len() {
            if !game.player(player).is_alive() {
                break;
            }
            if i == largest_index {
                continue;
            }
            let idx = i * 4;
            let bx = [boxes[idx], boxes[idx + 1], boxes[idx + 2], boxes[idx + 3]];
            if self.classify_cluster_v36(game, &clusters[i], bx)
                && self.remove_cluster_v36(game, &clusters[i], bx, None, is_pinched[i])
            {
                cluster_removed[i] = true;
            }
        }

        // Pass 2: pinch pass on unremoved pinched clusters.
        let empty = TerritoryAreas {
            cluster_sizes: Vec::new(),
            cluster_tile_ids: Vec::new(),
            territory_areas: vec![0],
            max_territory_area: 0,
        };
        self.execute_pinch_pass(
            game,
            &clusters,
            &boxes,
            &is_pinched,
            &cluster_removed,
            areas.as_ref().unwrap_or(&empty),
        );
    }

    fn execute_pinch_pass(
        &mut self,
        game: &mut GameImpl,
        clusters: &[Vec<TileRef>],
        boxes: &[i32],
        is_pinched: &[bool],
        cluster_removed: &[bool],
        areas: &TerritoryAreas,
    ) {
        let player = self.player;
        let mut state = PinchState {
            cluster_territory_ids: None,
            next_territory_id: 1,
        };
        for i in 0..clusters.len() {
            if !game.player(player).is_alive() {
                break;
            }
            if cluster_removed[i] || !is_pinched[i] {
                continue;
            }
            // Interior hole clusters are skipped so crater crenellations are not annexed.
            if self.is_hole_cluster(game, i, clusters, boxes, &mut state) {
                continue;
            }
            let tile_ids = areas.cluster_tile_ids.get(i).map(|v| v.as_slice());
            self.evaluate_pinch_candidates(
                game,
                &clusters[i],
                areas.max_territory_area,
                tile_ids,
                &areas.territory_areas,
            );
        }
        if state.cluster_territory_ids.is_some() {
            for cl in clusters {
                for &t in cl {
                    game.scratch.cluster_index_map[t as usize] = 0;
                }
            }
        }
    }

    /// `floodComponent`: size of the 4-way component of `start` among our tiles not yet stamped in this pass.
    fn flood_component(
        &self,
        game: &mut GameImpl,
        start: TileRef,
        t_gen: u32,
        area_gen: u32,
    ) -> i32 {
        let map = &game.map;
        let my = self.player;
        let scratch = &mut game.scratch;
        scratch.visited[start as usize] = t_gen;
        scratch.stack.clear();
        scratch.stack.push(start);
        let mut size = 0;
        let mut nbuf = [0; 4];
        while let Some(cur) = scratch.stack.pop() {
            size += 1;
            let n_neighbors = map.neighbors4(cur, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                if scratch.visited[n as usize] < area_gen && map.owner_id(n) == my {
                    scratch.visited[n as usize] = t_gen;
                    scratch.stack.push(n);
                }
            }
        }
        size
    }

    fn flood_territory_areas(
        &self,
        game: &mut GameImpl,
        clusters: &[Vec<TileRef>],
    ) -> TerritoryAreas {
        let my = self.player;
        let mut cluster_sizes = vec![0i32; clusters.len()];
        let mut cluster_tile_ids: Vec<Vec<i32>> = Vec::with_capacity(clusters.len());
        let mut territory_areas: Vec<i32> = vec![0];
        let mut max_territory_area = 0;
        let area_gen = bump_generations(game, clusters.len() as u32 * 4 + 16);
        let mut next_t_id: i32 = 1;

        for (i, cluster) in clusters.iter().enumerate() {
            let mut tile_ids = vec![0i32; cluster.len()];
            let mut max_cluster = 0;
            for (j, &t) in cluster.iter().enumerate() {
                if game.scratch.visited[t as usize] >= area_gen {
                    let t_id = (game.scratch.visited[t as usize] - area_gen + 1) as i32;
                    tile_ids[j] = t_id;
                    if territory_areas[t_id as usize] > max_cluster {
                        max_cluster = territory_areas[t_id as usize];
                    }
                    continue;
                }
                if game.map.owner_id(t) != my {
                    continue;
                }
                let t_id = next_t_id;
                next_t_id += 1;
                tile_ids[j] = t_id;
                let t_gen = area_gen + t_id as u32 - 1;
                while game.scratch.gen_ < t_gen && game.scratch.gen_ >= area_gen {
                    game.scratch.bump_generation();
                }
                let size = self.flood_component(game, t, t_gen, area_gen);
                // territoryAreas[tId] = size (tIds are dense, so this is a push).
                if territory_areas.len() <= t_id as usize {
                    territory_areas.resize(t_id as usize + 1, 0);
                }
                territory_areas[t_id as usize] = size;
                if size > max_cluster {
                    max_cluster = size;
                }
            }
            cluster_tile_ids.push(tile_ids);
            cluster_sizes[i] = max_cluster;
            if max_cluster > max_territory_area {
                max_territory_area = max_cluster;
            }
        }
        TerritoryAreas {
            cluster_sizes,
            cluster_tile_ids,
            territory_areas,
            max_territory_area,
        }
    }

    /// `hasDiagonalPinch`: a 2x2 block where two diagonal tiles are ours and the other two are not.
    fn has_diagonal_pinch(&self, game: &GameImpl, cluster: &[TileRef]) -> bool {
        let map = &game.map;
        let my = self.player;
        let w = map.width();
        let h = map.height();
        for &t in cluster {
            if map.owner_id(t) != my {
                continue;
            }
            if t >= (h - 1) * w {
                continue;
            }
            let below = t + w;
            if map.owner_id(below) == my {
                continue;
            }
            let x = map.x(t);
            if (x < w - 1 && map.owner_id(below + 1) == my && map.owner_id(t + 1) != my)
                || (x > 0 && map.owner_id(below - 1) == my && map.owner_id(t - 1) != my)
            {
                return true;
            }
        }
        false
    }

    fn evaluate_pinch_candidates(
        &self,
        game: &mut GameImpl,
        cluster: &[TileRef],
        max_territory_area: i32,
        tile_ids: Option<&[i32]>,
        territory_areas: &[i32],
    ) {
        let player = self.player;
        if !game.player(player).is_alive() {
            return;
        }
        let mut pinch_gen = game.scratch.bump_generation();
        let mut nbuf = [0; 4];
        for (j, &start) in cluster.iter().enumerate() {
            if !game.player(player).is_alive() {
                break;
            }
            if let Some(ids) = tile_ids {
                // An absent id reads as undefined upstream, which never compares >=.
                if let Some(&t_id) = ids.get(j)
                    && t_id != 0
                    && territory_areas
                        .get(t_id as usize)
                        .is_some_and(|&a| a >= max_territory_area)
                {
                    continue;
                }
            }
            if game.scratch.gen_ < pinch_gen {
                pinch_gen = game.scratch.bump_generation();
            }
            if game.map.owner_id(start) != player
                || game.scratch.visited[start as usize] >= pinch_gen
            {
                continue;
            }
            // 4-way flood to measure and bound this component.
            let mut component_has_fallout = false;
            game.scratch.visited[start as usize] = pinch_gen;
            game.scratch.stack.clear();
            game.scratch.stack.push(start);
            let mut component_tiles: Vec<TileRef> = vec![start];
            let mut c_min_x = game.map.x(start) as i32;
            let mut c_min_y = game.map.y(start) as i32;
            let mut c_max_x = c_min_x;
            let mut c_max_y = c_min_y;
            while let Some(cur) = game.scratch.stack.pop() {
                let cx = game.map.x(cur) as i32;
                let cy = game.map.y(cur) as i32;
                c_min_x = c_min_x.min(cx);
                c_min_y = c_min_y.min(cy);
                c_max_x = c_max_x.max(cx);
                c_max_y = c_max_y.max(cy);
                let n_neighbors = game.map.neighbors4(cur, &mut nbuf);
                for &n in &nbuf[..n_neighbors] {
                    if !component_has_fallout && game.map.has_fallout(n) {
                        component_has_fallout = true;
                    }
                    if game.scratch.visited[n as usize] < pinch_gen
                        && game.map.owner_id(n) == player
                    {
                        game.scratch.visited[n as usize] = pinch_gen;
                        component_tiles.push(n);
                        game.scratch.stack.push(n);
                    }
                }
            }
            // The component cannot be the main body.
            if component_tiles.len() as i32 >= max_territory_area {
                continue;
            }
            // Only land severed by a nuke (bordering fallout) qualifies.
            if !component_has_fallout {
                continue;
            }
            if self.is_enclosed_v36(game, start, [c_min_x, c_min_y, c_max_x, c_max_y]) {
                let capturing = self.get_capturing_player(game, &component_tiles);
                if let Some(capturing) = capturing {
                    if game.player(player).num_tiles_owned() == component_tiles.len() {
                        game.conquer_player(capturing, player);
                    }
                    for &t in &component_tiles {
                        game.player_mut(capturing).conquer(t);
                    }
                }
            }
        }
    }

    fn init_cluster_territories(&self, game: &mut GameImpl, clusters: &[Vec<TileRef>]) -> Vec<i32> {
        for (c, cl) in clusters.iter().enumerate() {
            for &k in cl {
                game.scratch.cluster_index_map[k as usize] = c as i32 + 1;
            }
        }
        vec![-1; clusters.len()]
    }

    fn is_hole_cluster(
        &self,
        game: &mut GameImpl,
        i: usize,
        clusters: &[Vec<TileRef>],
        boxes: &[i32],
        state: &mut PinchState,
    ) -> bool {
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
                if state.cluster_territory_ids.is_none() {
                    state.cluster_territory_ids =
                        Some(self.init_cluster_territories(game, clusters));
                }
                self.check_and_assign_territory_v36(game, clusters, i, j, state);
                if let Some(ids) = &state.cluster_territory_ids
                    && ids[i] != -1
                    && ids[i] == ids[j]
                {
                    return true;
                }
            }
        }
        false
    }

    fn check_and_assign_territory_v36(
        &self,
        game: &mut GameImpl,
        clusters: &[Vec<TileRef>],
        c_idx_a: usize,
        c_idx_b: usize,
        state: &mut PinchState,
    ) {
        let Some(ids) = state.cluster_territory_ids.as_mut() else {
            return;
        };
        if ids[c_idx_a] != -1 && ids[c_idx_a] == ids[c_idx_b] {
            return;
        }
        if ids[c_idx_a] != -1 && ids[c_idx_b] != -1 {
            return;
        }
        let start_idx = if ids[c_idx_b] == -1 { c_idx_b } else { c_idx_a };
        let territory_id = state.next_territory_id;
        state.next_territory_id += 1;

        let flood_gen = game.scratch.bump_generation();
        game.scratch.stack.clear();
        let my_owner = self.player;
        let Some(&start) = clusters[start_idx].first() else {
            return;
        };
        if game.map.owner_id(start) != my_owner {
            return;
        }
        game.scratch.visited[start as usize] = flood_gen;
        game.scratch.stack.push(start);
        ids[start_idx] = territory_id;

        let mut nbuf8 = [0; 8];
        while let Some(tile) = game.scratch.stack.pop() {
            let n_neighbors = game.map.neighbors8(tile, &mut nbuf8);
            for &n in &nbuf8[..n_neighbors] {
                if game.scratch.visited[n as usize] == flood_gen {
                    continue;
                }
                if game.map.owner_id(n) == my_owner {
                    game.scratch.visited[n as usize] = flood_gen;
                    game.scratch.stack.push(n);
                    let c_idx_plus_one = game.scratch.cluster_index_map[n as usize];
                    if c_idx_plus_one > 0 {
                        ids[(c_idx_plus_one - 1) as usize] = territory_id;
                    }
                }
            }
        }
    }

    /// `surroundedBySamePlayer`: the single enemy whose box encloses the cluster, if any.
    fn surrounded_by_same_player_v36(
        &self,
        game: &GameImpl,
        cluster: &[TileRef],
        bx: [i32; 4],
    ) -> Option<SmallId> {
        let mut single_enemy: SmallId = 0;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            1_000_000_000i32,
            1_000_000_000i32,
            -1_000_000_000i32,
            -1_000_000_000i32,
        );
        let map = &game.map;
        let my = self.player;
        let mut nbuf = [0; 4];
        for &tile in cluster {
            if map.is_ocean_shore(tile) || map.is_on_edge_of_map(tile) {
                return None;
            }
            let n_neighbors = map.neighbors4(tile, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                let owner_id = map.owner_id(n);
                if owner_id == my {
                    continue;
                }
                if owner_id == 0 {
                    return None;
                }
                if single_enemy == 0 {
                    single_enemy = owner_id;
                } else if single_enemy != owner_id {
                    return None;
                }
                let px = map.x(n) as i32;
                let py = map.y(n) as i32;
                min_x = min_x.min(px);
                min_y = min_y.min(py);
                max_x = max_x.max(px);
                max_y = max_y.max(py);
            }
        }
        if single_enemy != 0 && min_x <= bx[0] && min_y <= bx[1] && max_x >= bx[2] && max_y >= bx[3]
        {
            return Some(single_enemy);
        }
        None
    }

    /// `classifyCluster`: whether the cluster is walled in by enemies (coast, edge or open land rule it out).
    fn classify_cluster_v36(&self, game: &GameImpl, cluster: &[TileRef], bx: [i32; 4]) -> bool {
        let mut has_enemy = false;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            1_000_000_000i32,
            1_000_000_000i32,
            -1_000_000_000i32,
            -1_000_000_000i32,
        );
        let map = &game.map;
        let my = self.player;
        let mut nbuf = [0; 4];
        for &tr in cluster {
            if map.is_shore(tr) || map.is_on_edge_of_map(tr) {
                return false;
            }
            let n_neighbors = map.neighbors4(tr, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                let owner_id = map.owner_id(n);
                if owner_id != 0 && owner_id != my {
                    has_enemy = true;
                } else if owner_id == 0 {
                    if !map.has_fallout(n) {
                        return false;
                    }
                } else {
                    continue;
                }
                let x = map.x(n) as i32;
                let y = map.y(n) as i32;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        if !has_enemy {
            return false;
        }
        min_x <= bx[0] && min_y <= bx[1] && max_x >= bx[2] && max_y >= bx[3]
    }

    /// `removeCluster`: hands enclosed land to the capturing player; true when anything changed hands.
    fn remove_cluster_v36(
        &mut self,
        game: &mut GameImpl,
        cluster: &[TileRef],
        bx: [i32; 4],
        known_captor: Option<SmallId>,
        pinched: bool,
    ) -> bool {
        let player = self.player;
        for &t in cluster {
            if game.owner_id(t) != player {
                // Earlier removals may have changed owners.
                return false;
            }
        }
        let Some(&first_tile) = cluster.first() else {
            return false;
        };

        if !pinched {
            // Unpinched clusters are one 4-way component: evaluate once.
            let capturing = match known_captor {
                Some(c) => Some(c),
                None => self.get_capturing_player(game, cluster),
            };
            let Some(capturing) = capturing else {
                return false;
            };
            if !self.is_enclosed_v36(game, first_tile, bx) {
                return false;
            }
            let tiles = self.flood_own_component(game, first_tile, None);
            if game.player(player).num_tiles_owned() == tiles.len() {
                game.conquer_player(capturing, player);
            }
            for tile in tiles {
                game.player_mut(capturing).conquer(tile);
            }
            return true;
        }

        // Pinched clusters: each 4-way component on its own.
        let flood_gen = game.scratch.bump_generation();
        let mut any_removed = false;
        for &t in cluster {
            if !game.player(player).is_alive() {
                break;
            }
            if game.scratch.visited[t as usize] == flood_gen || game.owner_id(t) != player {
                continue;
            }
            if !self.is_enclosed_v36(game, t, bx) {
                self.mark_rejected_component(game, t, flood_gen, bx);
                continue;
            }
            let tiles = self.flood_own_component(game, t, Some(flood_gen));
            if !tiles.is_empty() {
                let component_captor = match known_captor {
                    Some(c) => Some(c),
                    None => self.get_capturing_player(game, &tiles),
                };
                let Some(component_captor) = component_captor else {
                    continue;
                };
                any_removed = true;
                if game.player(player).num_tiles_owned() == tiles.len() {
                    game.conquer_player(component_captor, player);
                }
                for tile in tiles {
                    game.player_mut(component_captor).conquer(tile);
                }
            }
        }
        any_removed
    }

    /// `floodFillWithGen(.., diag=false, owner === player)` from one start tile; a fresh generation unless given.
    fn flood_own_component(
        &self,
        game: &mut GameImpl,
        start: TileRef,
        gen_: Option<u32>,
    ) -> Vec<TileRef> {
        let player = self.player;
        let current_gen = gen_.unwrap_or_else(|| game.scratch.bump_generation());
        let map = &game.map;
        let scratch = &mut game.scratch;
        flood_fill_with_gen(
            map,
            &mut scratch.visited,
            &mut scratch.stack,
            current_gen,
            &[start],
            false,
            &|_, t| map.owner_id(t) == player,
            None,
        )
    }

    /// `markRejectedComponent`: stamps the component within the fence so no other seed repeats the walk.
    fn mark_rejected_component(
        &self,
        game: &mut GameImpl,
        start: TileRef,
        flood_gen: u32,
        bx: [i32; 4],
    ) {
        let map = &game.map;
        let scratch = &mut game.scratch;
        let my = self.player;
        let map_width = map.width();
        let (fmin_x, fmin_y, fmax_x, fmax_y) = (bx[0] - 1, bx[1] - 1, bx[2] + 1, bx[3] + 1);
        scratch.stack.clear();
        scratch.visited[start as usize] = flood_gen;
        scratch.stack.push(start);
        let mut nbuf = [0; 4];
        while let Some(cur) = scratch.stack.pop() {
            let num_n = map.neighbors4(cur, &mut nbuf);
            for &n in &nbuf[..num_n] {
                if scratch.visited[n as usize] == flood_gen || map.owner_id(n) != my {
                    continue;
                }
                let x = (n % map_width) as i32;
                let y = (n / map_width) as i32;
                if x >= fmin_x && x <= fmax_x && y >= fmin_y && y <= fmax_y {
                    scratch.visited[n as usize] = flood_gen;
                    scratch.stack.push(n);
                }
            }
        }
    }

    /// `isEnclosed`: whether our territory reachable from `start` is walled in by other players.
    fn is_enclosed_v36(&self, game: &mut GameImpl, start: TileRef, bx: [i32; 4]) -> bool {
        let map = &game.map;
        let my = self.player;
        let scratch = &mut game.scratch;
        let gen_ = scratch.bump_generation();
        scratch.stack.clear();
        scratch.visited[start as usize] = gen_;
        scratch.stack.push(start);
        let (fmin_x, fmin_y, fmax_x, fmax_y) = (bx[0] - 1, bx[1] - 1, bx[2] + 1, bx[3] + 1);
        let map_width = map.width();
        let mut nbuf = [0; 4];
        while let Some(tile) = scratch.stack.pop() {
            let x = (tile % map_width) as i32;
            let y = (tile / map_width) as i32;
            if x < fmin_x || x > fmax_x || y < fmin_y || y > fmax_y {
                return false;
            }
            if map.is_on_edge_of_map(tile) {
                return false;
            }
            let n_neighbors = map.neighbors4(tile, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                if scratch.visited[n as usize] == gen_ {
                    continue;
                }
                let owner_id = map.owner_id(n);
                if owner_id == my {
                    scratch.visited[n as usize] = gen_;
                    scratch.stack.push(n);
                    continue;
                }
                if owner_id == 0 && (!map.is_land(n) || !map.has_fallout(n)) {
                    // Open water or clean unclaimed land is an escape path.
                    return false;
                }
            }
        }
        true
    }
}

/// `findLargestClusterIndex`: most territory, then box area, then border tile count.
fn find_largest_cluster_index(
    clusters: &[Vec<TileRef>],
    boxes: &[i32],
    cluster_sizes: &[i32],
) -> usize {
    let mut largest = 0usize;
    let area = |i: usize| {
        (boxes[i * 4 + 2] - boxes[i * 4] + 1) * (boxes[i * 4 + 3] - boxes[i * 4 + 1] + 1)
    };
    for i in 1..clusters.len() {
        if cluster_sizes[i] > cluster_sizes[largest] {
            largest = i;
        } else if cluster_sizes[i] == cluster_sizes[largest] {
            let cur = area(i);
            let best = area(largest);
            if cur > best || (cur == best && clusters[i].len() > clusters[largest].len()) {
                largest = i;
            }
        }
    }
    largest
}

// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the severed-land logic of src/core/execution/PlayerExecution.ts (v0.35.0): hands land a nuke cut off from the main body to the enemy around it.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::PlayerExecution::PlayerExecution;
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// The cluster class: surrounded by one enemy (taken over by it).
pub(super) const CLUSTER_SURROUNDED: u8 = 1;
/// The cluster class: may be land cut off from the main body.
const CLUSTER_SEVERED_CANDIDATE: u8 = 2;
/// The cluster class: touches coast, map edge or open land.
const CLUSTER_OPEN: u8 = 4;

/// Border tiles that may belong to land cut off from the main body, with their boxes (four numbers each).
pub(super) struct SeveredCandidates {
    /// The border clusters.
    pub clusters: Vec<Vec<TileRef>>,
    /// minX, minY, maxX, maxY per cluster.
    pub boxes: Vec<i32>,
}

impl SeveredCandidates {
    /// An empty set.
    pub fn new() -> Self {
        Self {
            clusters: Vec::new(),
            boxes: Vec::new(),
        }
    }

    /// Adds a cluster with its box.
    pub fn push(&mut self, cluster: &[TileRef], bx: [i32; 4]) {
        self.clusters.push(cluster.to_vec());
        self.boxes.extend_from_slice(&bx);
    }
}

/// Border tiles in the running for the main body, and whether each has a way out.
struct MainBodyPieces {
    clusters: Vec<Vec<TileRef>>,
    boxes: Vec<i32>,
    open: Vec<bool>,
}

/// A border cluster's single-territory parts with their boxes and classes.
struct TerritoryParts {
    clusters: Vec<Vec<TileRef>>,
    boxes: Vec<i32>,
    classes: Vec<u8>,
}

/// The main body's border tiles in pieces, and the land that may be cut off from it.
pub(super) struct SeveredLand {
    /// Border pieces of the main body.
    pub main_tiles: Vec<Vec<TileRef>>,
    /// Candidate pieces.
    pub candidates: SeveredCandidates,
}

/// A cluster that `split_cluster` leaves whole, as its one part.
fn whole_cluster(cluster: &[TileRef], boxes: &[i32], offset: usize, cls: u8) -> TerritoryParts {
    TerritoryParts {
        clusters: vec![cluster.to_vec()],
        boxes: boxes[offset..offset + 4].to_vec(),
        classes: vec![cls],
    }
}

/// Adds the parts that race for the main body: all with `all`, else the candidates and, with `with_open`, the open ones.
fn add_race_pieces(
    pieces: &mut MainBodyPieces,
    parts: &TerritoryParts,
    all: bool,
    with_open: bool,
) {
    for k in 0..parts.clusters.len() {
        let cls = parts.classes[k];
        let open = cls & CLUSTER_OPEN != 0;
        if !all && cls & CLUSTER_SEVERED_CANDIDATE == 0 && !(with_open && open) {
            continue;
        }
        pieces.clusters.push(parts.clusters[k].clone());
        pieces.open.push(open);
        pieces
            .boxes
            .extend_from_slice(&parts.boxes[k * 4..k * 4 + 4]);
    }
}

/// The flood `part` has merged into.
fn root_of(roots: &[i32], mut part: usize) -> usize {
    while roots[part] as usize != part {
        part = roots[part] as usize;
    }
    part
}

/// `bumpGenerations(count)`: `count` consecutive stamps, returning the first.
fn bump_generations(game: &mut GameImpl, count: usize) -> u32 {
    loop {
        let first = game.scratch.bump_generation();
        let mut last = first;
        for _ in 1..count {
            last = game.scratch.bump_generation();
        }
        if (last - first) as usize == count - 1 {
            return first;
        }
    }
}

impl PlayerExecution {
    /// `classifyCluster`: surrounded, severed-candidate and open flags of a border cluster.
    pub(super) fn classify_cluster(
        &self,
        game: &GameImpl,
        cluster: &[TileRef],
        bx: [i32; 4],
    ) -> u8 {
        let mut has_enemy = false;
        let mut has_fallout = false;
        let mut has_open_land = false;
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
                return CLUSTER_OPEN;
            }
            let n_neighbors = map.neighbors4(tr, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                let owner_id = map.owner_id(n);
                if owner_id != 0 && owner_id != my {
                    has_enemy = true;
                    let x = map.x(n) as i32;
                    let y = map.y(n) as i32;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                } else if owner_id == 0 {
                    if map.has_fallout(n) {
                        has_fallout = true;
                    } else {
                        has_open_land = true;
                    }
                }
            }
        }
        if !has_enemy {
            return if has_open_land { CLUSTER_OPEN } else { 0 };
        }
        let mut result = if has_open_land { CLUSTER_OPEN } else { 0 };
        if min_x <= bx[0] && min_y <= bx[1] && max_x >= bx[2] && max_y >= bx[3] {
            result |= CLUSTER_SURROUNDED;
        }
        if has_fallout && !has_open_land {
            result |= CLUSTER_SEVERED_CANDIDATE;
        }
        result
    }

    /// Picks the main body and returns it with the land that may be cut off from it.
    pub(super) fn severed_land(
        &self,
        game: &mut GameImpl,
        largest_cluster: &[TileRef],
        boxes: &[i32],
        box_offset: usize,
        severed: Option<&SeveredCandidates>,
        open_clusters: Option<&SeveredCandidates>,
    ) -> Option<SeveredLand> {
        // Only fallout can cut land off, so skip all of this in games without it.
        if game.map.num_tiles_with_fallout() == 0 {
            return None;
        }
        let mut largest = self.split_cluster(game, largest_cluster, false);
        if largest.is_none() {
            // On its own, the largest cluster is the main body.
            if severed.is_none() && open_clusters.is_none() {
                return None;
            }
            let cls = self.classify_cluster(
                game,
                largest_cluster,
                [
                    boxes[box_offset],
                    boxes[box_offset + 1],
                    boxes[box_offset + 2],
                    boxes[box_offset + 3],
                ],
            );
            largest = Some(whole_cluster(largest_cluster, boxes, box_offset, cls));
        }
        let largest = largest?;
        let add_open = !largest.classes.iter().any(|c| c & CLUSTER_OPEN != 0)
            && largest
                .classes
                .iter()
                .any(|c| c & CLUSTER_SEVERED_CANDIDATE != 0);

        let mut pieces = MainBodyPieces {
            clusters: Vec::new(),
            boxes: Vec::new(),
            open: Vec::new(),
        };
        add_race_pieces(&mut pieces, &largest, true, true);
        let largest_parts = pieces.clusters.len();
        if let Some(severed) = severed {
            for c in 0..severed.clusters.len() {
                let cluster = &severed.clusters[c];
                let parts = self.split_cluster(game, cluster, false).unwrap_or_else(|| {
                    whole_cluster(cluster, &severed.boxes, c * 4, CLUSTER_SEVERED_CANDIDATE)
                });
                add_race_pieces(&mut pieces, &parts, false, false);
            }
        }
        if let Some(open_clusters) = open_clusters {
            for c in 0..open_clusters.clusters.len() {
                let cluster = &open_clusters.clusters[c];
                let split = self.split_cluster(game, cluster, !add_open);
                if let Some(split) = split {
                    add_race_pieces(&mut pieces, &split, false, add_open);
                } else if add_open {
                    let whole = whole_cluster(cluster, &open_clusters.boxes, c * 4, CLUSTER_OPEN);
                    add_race_pieces(&mut pieces, &whole, false, true);
                }
            }
        }
        if pieces.clusters.len() < 2 || !pieces.open.contains(&false) {
            return None;
        }

        let count = pieces.clusters.len();
        let mut roots = vec![0i32; count];
        let main =
            self.largest_territory_part(game, &pieces.clusters, &mut roots, Some(&pieces.boxes));
        if main == -1 {
            return None;
        }
        let main = main as usize;

        let mut main_tiles: Vec<Vec<TileRef>> = Vec::new();
        let mut candidates = SeveredCandidates::new();
        for k in 0..count {
            let i = (k + largest_parts) % count;
            if root_of(&roots, i) == main {
                main_tiles.push(pieces.clusters[i].clone());
                continue;
            }
            if pieces.open[i] {
                continue;
            }
            candidates.push(
                &pieces.clusters[i],
                [
                    pieces.boxes[i * 4],
                    pieces.boxes[i * 4 + 1],
                    pieces.boxes[i * 4 + 2],
                    pieces.boxes[i * 4 + 3],
                ],
            );
        }
        if candidates.clusters.is_empty() {
            None
        } else {
            Some(SeveredLand {
                main_tiles,
                candidates,
            })
        }
    }

    /// Splits a cluster into parts that each lie in a single territory; None when it does not split.
    fn split_cluster(
        &self,
        game: &mut GameImpl,
        cluster: &[TileRef],
        candidates_only: bool,
    ) -> Option<TerritoryParts> {
        let my = self.player;
        let w = game.map.width() as i64;
        let h = game.map.height() as i64;
        if cluster.is_empty() || game.map.owner_id(cluster[0]) != my {
            return None;
        }

        // Only two of our tiles meeting at a corner with neither tile between them ours can split it.
        let mut pinched = false;
        {
            let map = &game.map;
            for &t in cluster {
                if pinched {
                    break;
                }
                let t = t as i64;
                if t >= (h - 1) * w {
                    continue;
                }
                let below = t + w;
                if map.owner_id(below as TileRef) == my {
                    continue;
                }
                let x = t % w;
                pinched = (x < w - 1
                    && map.owner_id((below + 1) as TileRef) == my
                    && map.owner_id((t + 1) as TileRef) != my)
                    || (x > 0
                        && map.owner_id((below - 1) as TileRef) == my
                        && map.owner_id((t - 1) as TileRef) != my);
            }
        }
        if !pinched {
            return None;
        }
        if candidates_only {
            let map = &game.map;
            let mut nbuf = [0; 4];
            let mut fallout = false;
            'outer: for &t in cluster {
                let n_neighbors = map.neighbors4(t, &mut nbuf);
                for &n in &nbuf[..n_neighbors] {
                    if map.owner_id(n) == 0 && map.has_fallout(n) {
                        fallout = true;
                        break 'outer;
                    }
                }
            }
            if !fallout {
                return None;
            }
        }

        let member_gen = bump_generations(game, 2);
        let done_gen = member_gen + 1;
        let map = &game.map;
        let scratch = &mut game.scratch;
        for &t in cluster {
            scratch.visited[t as usize] = member_gen;
        }
        let mut parts: Vec<Vec<TileRef>> = Vec::new();
        let mut part_boxes: Vec<i32> = Vec::new();
        let mut classes: Vec<u8> = Vec::new();
        let mut nbuf8 = [0; 8];
        for &start in cluster {
            if scratch.visited[start as usize] != member_gen {
                continue;
            }
            if map.owner_id(start) != my {
                continue;
            }
            scratch.visited[start as usize] = done_gen;
            scratch.stack.clear();
            scratch.stack.push(start);
            let mut part: Vec<TileRef> = Vec::new();
            let mut touches_open = false;
            let mut has_enemy = false;
            let mut has_fallout = false;
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, -1i64, -1i64);
            while let Some(t) = scratch.stack.pop() {
                part.push(t);
                let ti = t as i64;
                let x = ti % w;
                let y = (ti - x) / w;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                if map.is_shore(t) || map.is_on_edge_of_map(t) {
                    touches_open = true;
                }
                let n_neighbors = map.neighbors8(t, &mut nbuf8);
                for &n in &nbuf8[..n_neighbors] {
                    let owner_id = map.owner_id(n);
                    let ni = n as i64;
                    let dx = (ni % w) - x;
                    let diagonal = dx != 0 && ni - dx != ti;
                    if !diagonal && owner_id != my {
                        if owner_id != 0 {
                            has_enemy = true;
                        } else if map.has_fallout(n) {
                            has_fallout = true;
                        } else {
                            touches_open = true;
                        }
                    }
                    if scratch.visited[n as usize] != member_gen || owner_id != my {
                        continue;
                    }
                    if diagonal
                        && map.owner_id((ti + dx) as TileRef) != my
                        && map.owner_id((ni - dx) as TileRef) != my
                    {
                        continue;
                    }
                    scratch.visited[n as usize] = done_gen;
                    scratch.stack.push(n);
                }
            }
            parts.push(part);
            part_boxes.extend_from_slice(&[min_x as i32, min_y as i32, max_x as i32, max_y as i32]);
            classes.push(if touches_open {
                CLUSTER_OPEN
            } else if has_enemy && has_fallout {
                CLUSTER_SEVERED_CANDIDATE
            } else {
                0
            });
        }
        if parts.len() < 2 {
            return None;
        }
        Some(TerritoryParts {
            clusters: parts,
            boxes: part_boxes,
            classes,
        })
    }

    /// Index of the piece whose territory is largest (ties to the lowest index), or -1.
    fn largest_territory_part(
        &self,
        game: &mut GameImpl,
        parts: &[Vec<TileRef>],
        roots: &mut [i32],
        boxes: Option<&[i32]>,
    ) -> i64 {
        let my = self.player;
        let w = game.map.width() as i64;
        let h = game.map.height() as i64;
        let count = parts.len();
        let base = bump_generations(game, count);
        let map = &game.map;
        let visited = &mut game.scratch.visited;
        let mut bounds: Option<Vec<i32>> = boxes.map(|b| b[..count * 4].to_vec());
        if let Some(bounds) = bounds.as_mut() {
            let mut b = 0;
            while b < bounds.len() {
                if bounds[b] == 0
                    || bounds[b + 1] == 0
                    || bounds[b + 2] as i64 == w - 1
                    || bounds[b + 3] as i64 == h - 1
                {
                    bounds[b] = 0;
                    bounds[b + 1] = 0;
                    bounds[b + 2] = (w - 1) as i32;
                    bounds[b + 3] = (h - 1) as i32;
                }
                b += 4;
            }
        }
        let mut stacks: Vec<Vec<TileRef>> = Vec::with_capacity(count);
        let mut sizes: Vec<i64> = Vec::with_capacity(count);
        let mut dropped: Vec<bool> = Vec::with_capacity(count);
        for i in 0..count {
            roots[i] = i as i32;
            for &t in &parts[i] {
                visited[t as usize] = base + i as u32;
            }
            stacks.push(parts[i].clone());
            sizes.push(parts[i].len() as i64);
            dropped.push(false);
        }

        let mut growing = count as i64;
        let mut best: i64 = -1;
        let mut nbuf = [0; 4];
        loop {
            for i in 0..count {
                if roots[i] as usize != i || dropped[i] || stacks[i].is_empty() {
                    continue;
                }
                let t = stacks[i].pop().expect("non-empty");
                if let Some(bounds) = bounds.as_ref() {
                    let x = t as i64 % w;
                    let y = (t as i64 - x) / w;
                    let b = i * 4;
                    if x < bounds[b] as i64
                        || y < bounds[b + 1] as i64
                        || x > bounds[b + 2] as i64
                        || y > bounds[b + 3] as i64
                    {
                        // Kept for a flood that takes this one over.
                        stacks[i].push(t);
                        dropped[i] = true;
                        growing -= 1;
                        continue;
                    }
                }
                let n_neighbors = map.neighbors4(t, &mut nbuf);
                for &n in &nbuf[..n_neighbors] {
                    if map.owner_id(n) != my {
                        continue;
                    }
                    let part = visited[n as usize] as i64 - base as i64;
                    if part < 0 || part >= count as i64 {
                        visited[n as usize] = base + i as u32;
                        sizes[i] += 1;
                        stacks[i].push(n);
                        continue;
                    }
                    let other = root_of(roots, part as usize);
                    if other == i {
                        continue;
                    }
                    roots[other] = i as i32;
                    sizes[i] += sizes[other];
                    let moved = std::mem::take(&mut stacks[other]);
                    stacks[i].extend(moved);
                    if !dropped[other] {
                        growing -= 1;
                    }
                    if let Some(bounds) = bounds.as_mut() {
                        let b = i * 4;
                        let o = other * 4;
                        if bounds[o] < bounds[b] {
                            bounds[b] = bounds[o];
                        }
                        if bounds[o + 1] < bounds[b + 1] {
                            bounds[b + 1] = bounds[o + 1];
                        }
                        if bounds[o + 2] > bounds[b + 2] {
                            bounds[b + 2] = bounds[o + 2];
                        }
                        if bounds[o + 3] > bounds[b + 3] {
                            bounds[b + 3] = bounds[o + 3];
                        }
                    }
                }
                if stacks[i].is_empty() {
                    growing -= 1;
                    if best == -1
                        || sizes[i] > sizes[best as usize]
                        || (sizes[i] == sizes[best as usize] && (i as i64) < best)
                    {
                        best = i as i64;
                    }
                }
            }
            if growing == 0 {
                return best;
            }
            if growing == 1 {
                let mut last = 0usize;
                while roots[last] as usize != last || dropped[last] || stacks[last].is_empty() {
                    last += 1;
                }
                if best == -1 || sizes[last] > sizes[best as usize] {
                    return last as i64;
                }
            }
        }
    }

    /// Hands land cut off from the main body to the enemy around it.
    pub(super) fn annex_severed_clusters(
        &self,
        game: &mut GameImpl,
        candidates: &SeveredCandidates,
        main_tiles: &[Vec<TileRef>],
    ) {
        let my = self.player;
        for c in 0..candidates.clusters.len() {
            let mut main_gen = self.stamp_main_body(game, main_tiles);
            for &start in &candidates.clusters[c] {
                // An earlier annex in this pass may already have taken it.
                if game.map.owner_id(start) != my {
                    continue;
                }
                let mark = game.scratch.visited[start as usize];
                if mark >= main_gen && mark <= main_gen + 2 {
                    continue;
                }
                let Some(territory) = self.severed_territory(
                    game,
                    start,
                    main_gen,
                    main_gen + 1,
                    main_gen + 2,
                    &candidates.boxes,
                    c * 4,
                ) else {
                    continue;
                };
                let Some(capturing) = self.get_capturing_player(game, &territory) else {
                    continue;
                };
                if game.player(my).num_tiles_owned() == territory.len() {
                    game.conquer_player(capturing, my);
                }
                for &t in &territory {
                    game.player_mut(capturing).conquer(t);
                }
                // Conquering may run code that uses the shared scratch.
                main_gen = self.stamp_main_body(game, main_tiles);
            }
        }
    }

    /// Stamps the main body's border pieces with a fresh generation and reserves two more.
    fn stamp_main_body(&self, game: &mut GameImpl, main_tiles: &[Vec<TileRef>]) -> u32 {
        let main_gen = bump_generations(game, 3);
        for piece in main_tiles {
            for &t in piece {
                game.scratch.visited[t as usize] = main_gen;
            }
        }
        main_gen
    }

    /// Floods our territory from `start` and returns it if it is severed.
    #[allow(clippy::too_many_arguments)]
    fn severed_territory(
        &self,
        game: &mut GameImpl,
        start: TileRef,
        main_gen: u32,
        seen_gen: u32,
        fail_gen: u32,
        bx: &[i32],
        box_offset: usize,
    ) -> Option<Vec<TileRef>> {
        let my = self.player;
        let map = &game.map;
        let scratch = &mut game.scratch;
        let (min_x, min_y, max_x, max_y) = (
            bx[box_offset],
            bx[box_offset + 1],
            bx[box_offset + 2],
            bx[box_offset + 3],
        );
        scratch.stack.clear();
        let mut tiles: Vec<TileRef> = vec![start];
        scratch.visited[start as usize] = seen_gen;
        scratch.stack.push(start);
        let mut has_enemy = false;
        let mut has_fallout = false;
        let mut nbuf = [0; 4];

        fn stamp_all(visited: &mut [u32], tiles: &[TileRef], gen_: u32) -> Option<Vec<TileRef>> {
            for &t in tiles {
                visited[t as usize] = gen_;
            }
            None
        }

        while let Some(tile) = scratch.stack.pop() {
            let x = map.x(tile) as i32;
            let y = map.y(tile) as i32;
            if x < min_x
                || x > max_x
                || y < min_y
                || y > max_y
                || map.is_shore(tile)
                || map.is_on_edge_of_map(tile)
            {
                return stamp_all(&mut scratch.visited, &tiles, fail_gen);
            }
            let n_neighbors = map.neighbors4(tile, &mut nbuf);
            for &n in &nbuf[..n_neighbors] {
                let owner_id = map.owner_id(n);
                if owner_id == my {
                    let mark = scratch.visited[n as usize];
                    if mark == main_gen {
                        return stamp_all(&mut scratch.visited, &tiles, main_gen);
                    }
                    if mark == fail_gen {
                        return stamp_all(&mut scratch.visited, &tiles, fail_gen);
                    }
                    if mark == seen_gen {
                        continue;
                    }
                    scratch.visited[n as usize] = seen_gen;
                    tiles.push(n);
                    scratch.stack.push(n);
                } else if owner_id != 0 {
                    has_enemy = true;
                } else if map.has_fallout(n) {
                    has_fallout = true;
                } else {
                    return stamp_all(&mut scratch.visited, &tiles, fail_gen);
                }
            }
        }
        if has_enemy && has_fallout {
            Some(tiles)
        } else {
            None
        }
    }
}

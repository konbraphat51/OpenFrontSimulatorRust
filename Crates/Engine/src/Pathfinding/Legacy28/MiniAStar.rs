// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/MiniAStar.ts: `SerialAStar` on the half-size mini map, with the path scaled back up.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::SerialAStar::{GraphAdapter, PathFindResultType, SerialAStar};
use crate::Compat::JsNumber::round;
use crate::Game::GameMap::{GameMapImpl, TileRef};

/// Upstream `GameMapAdapter`: a map as an A* graph.
pub struct GameMapAdapter<'a> {
    game_map: &'a GameMapImpl,
    water_path: bool,
}

const WATER_PENALTY: f64 = 3.0;

impl<'a> GameMapAdapter<'a> {
    /// Adapter over `game_map`; `water_path` limits the search to water.
    pub fn new(game_map: &'a GameMapImpl, water_path: bool) -> Self {
        Self {
            game_map,
            water_path,
        }
    }
}

impl GraphAdapter for GameMapAdapter<'_> {
    fn neighbors(&self, node: u32) -> Vec<u32> {
        self.game_map.neighbors(node)
    }

    fn cost(&self, node: u32) -> f64 {
        let mut base = self.game_map.cost(node) as f64;
        // Avoid crossing water when possible
        if !self.water_path && self.game_map.is_water(node) {
            base += WATER_PENALTY;
        }
        base
    }

    fn position(&self, node: u32) -> (f64, f64) {
        (self.game_map.x(node) as f64, self.game_map.y(node) as f64)
    }

    fn is_traversable(&self, from: u32, to: u32) -> bool {
        let to_water = self.game_map.is_water(to);
        if self.water_path {
            return to_water;
        }
        // Allow water access from/to shore
        let from_shore = self.game_map.is_shoreline(from);
        let to_shore = self.game_map.is_shoreline(to);
        !to_water || from_shore || to_shore
    }
}

type Cell = (i32, i32);

/// `MiniAStar`: the search runs on the mini map, the reconstructed path is on the full map.
#[derive(Clone, Debug)]
pub struct MiniAStar {
    a_star: SerialAStar,
    water_path: bool,
    src: Vec<TileRef>,
    src_is_array: bool,
    dst: TileRef,
}

fn mini_tile(map: &GameMapImpl, mini_map: &GameMapImpl, tile: TileRef) -> TileRef {
    mini_map.tile_ref((map.x(tile) / 2) as i32, (map.y(tile) / 2) as i32)
}

impl MiniAStar {
    /// `new MiniAStar(gameMap, miniMap, src, dst, iterations, maxTries, waterPath, directionChangePenalty)`;
    /// `src_is_array` keeps upstream's `TileRef | TileRef[]` distinction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        map: &GameMapImpl,
        mini_map: &GameMapImpl,
        src: &[TileRef],
        src_is_array: bool,
        dst: TileRef,
        iterations: f64,
        max_tries: f64,
        water_path: bool,
        direction_change_penalty: f64,
    ) -> Self {
        let mini_src: Vec<u32> = src.iter().map(|&s| mini_tile(map, mini_map, s)).collect();
        let mini_dst = mini_tile(map, mini_map, dst);
        let a_star = SerialAStar::new(
            &mini_src,
            mini_dst,
            iterations,
            max_tries,
            &GameMapAdapter::new(mini_map, water_path),
            direction_change_penalty,
        );
        Self {
            a_star,
            water_path,
            src: src.to_vec(),
            src_is_array,
            dst,
        }
    }

    /// `compute()`.
    pub fn compute(&mut self, mini_map: &GameMapImpl) -> PathFindResultType {
        self.a_star
            .compute(&GameMapAdapter::new(mini_map, self.water_path))
    }

    /// `reconstructPath()`: the full-map tiles of the path.
    pub fn reconstruct_path(&self, map: &GameMapImpl, mini_map: &GameMapImpl) -> Vec<TileRef> {
        let cell_src: Option<Cell> = if self.src_is_array {
            None
        } else {
            Some((map.x(self.src[0]) as i32, map.y(self.src[0]) as i32))
        };
        let cell_dst: Cell = (map.x(self.dst) as i32, map.y(self.dst) as i32);
        let mini_path: Vec<Cell> = self
            .a_star
            .reconstruct_path()
            .iter()
            .map(|&t| (mini_map.x(t) as i32, mini_map.y(t) as i32))
            .collect();
        let upscaled = fix_extremes(upscale_path(&mini_path, 2), cell_dst, cell_src);
        upscaled
            .into_iter()
            .map(|(x, y)| map.tile_ref(x, y))
            .collect()
    }
}

fn fix_extremes(mut upscaled: Vec<Cell>, cell_dst: Cell, cell_src: Option<Cell>) -> Vec<Cell> {
    if let Some(src) = cell_src {
        match find_cell(&upscaled, src) {
            None => upscaled.insert(0, src),
            Some(0) => {}
            // found the start tile but not at the start: drop everything before it
            Some(i) => upscaled = upscaled.split_off(i),
        }
    }
    match find_cell(&upscaled, cell_dst) {
        None => upscaled.push(cell_dst),
        Some(i) if i != upscaled.len() - 1 => upscaled.truncate(i + 1),
        Some(_) => {}
    }
    upscaled
}

fn upscale_path(path: &[Cell], scale_factor: i32) -> Vec<Cell> {
    let scaled: Vec<Cell> = path
        .iter()
        .map(|&(x, y)| (x * scale_factor, y * scale_factor))
        .collect();
    let mut smooth: Vec<Cell> = Vec::new();
    for i in 0..scaled.len().saturating_sub(1) {
        let current = scaled[i];
        let next = scaled[i + 1];
        smooth.push(current);
        let dx = next.0 - current.0;
        let dy = next.1 - current.1;
        let steps = dx.abs().max(dy.abs());
        for step in 1..steps {
            smooth.push((
                round(current.0 as f64 + (dx * step) as f64 / steps as f64) as i32,
                round(current.1 as f64 + (dy * step) as f64 / steps as f64) as i32,
            ));
        }
    }
    if let Some(&last) = scaled.last() {
        smooth.push(last);
    }
    smooth
}

fn find_cell(cells: &[Cell], target: Cell) -> Option<usize> {
    cells.iter().position(|&c| c == target)
}

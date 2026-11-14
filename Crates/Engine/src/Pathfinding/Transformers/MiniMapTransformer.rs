// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/transformers/MiniMapTransformer.ts: searches on the mini map and upscales the path to the game map.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Compat::JsNumber::round;
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};

/// Converts game coordinates to mini map coordinates, searches, and upscales the result.
pub struct MiniMapTransformer<P> {
    inner: P,
}

impl<P> MiniMapTransformer<P> {
    /// Wraps `inner`, which searches `ctx.mini_map`.
    pub fn new(inner: P) -> Self {
        Self { inner }
    }

    /// The wrapped finder.
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }
}

impl<P: PathFinder> PathFinder for MiniMapTransformer<P> {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        let (map, mini) = (ctx.map, ctx.mini_map);

        // Game coords to mini map coords (multi-source supported)
        let mini_from_array: Vec<TileRef> = from
            .as_slice()
            .iter()
            .map(|&f| mini.tile_ref((map.x(f) / 2) as i32, (map.y(f) / 2) as i32))
            .collect();
        let mini_to = mini.tile_ref((map.x(to) / 2) as i32, (map.y(to) / 2) as i32);

        let path = if mini_from_array.len() == 1 {
            self.inner
                .find_path(ctx, Source::Single(mini_from_array[0]), mini_to)
        } else {
            self.inner
                .find_path(ctx, Source::Multi(&mini_from_array), mini_to)
        }?;
        if path.is_empty() {
            return None;
        }

        let upscaled = upscale_path(map, mini, &path, 2);

        // Multi-source: the source closest to the path start
        let mut src_ref: Option<TileRef> = None;
        match from {
            Source::Multi(sources) => {
                if !upscaled.is_empty() {
                    let start_x = map.x(upscaled[0]);
                    let start_y = map.y(upscaled[0]);
                    let mut min_dist = u32::MAX;
                    for &f in sources {
                        let dist = map.x(f).abs_diff(start_x) + map.y(f).abs_diff(start_y);
                        if dist < min_dist {
                            min_dist = dist;
                            src_ref = Some(f);
                        }
                    }
                }
            }
            Source::Single(f) => src_ref = Some(f),
        }
        Some(fix_extremes(upscaled, to, src_ref))
    }
}

/// Scales a mini map path up, inserting interpolated points so consecutive tiles stay adjacent.
fn upscale_path(
    main: &GameMapImpl,
    mini: &GameMapImpl,
    path: &[TileRef],
    scale_factor: i64,
) -> Vec<TileRef> {
    let mut smooth_path: Vec<TileRef> = Vec::new();

    for i in 0..path.len().saturating_sub(1) {
        let cur_x = mini.x(path[i]) as i64 * scale_factor;
        let cur_y = mini.y(path[i]) as i64 * scale_factor;
        let next_x = mini.x(path[i + 1]) as i64 * scale_factor;
        let next_y = mini.y(path[i + 1]) as i64 * scale_factor;

        smooth_path.push(main.tile_ref(cur_x as i32, cur_y as i32));

        let dx = next_x - cur_x;
        let dy = next_y - cur_y;
        let steps = dx.abs().max(dy.abs());

        for step in 1..steps {
            let x = round(cur_x as f64 + (dx * step) as f64 / steps as f64);
            let y = round(cur_y as f64 + (dy * step) as f64 / steps as f64);
            smooth_path.push(main.tile_ref(x as i32, y as i32));
        }
    }

    if let Some(&last) = path.last() {
        smooth_path.push(main.tile_ref(
            (mini.x(last) as i64 * scale_factor) as i32,
            (mini.y(last) as i64 * scale_factor) as i32,
        ));
    }

    smooth_path
}

/// Trims or extends the upscaled path so it starts at `src` and ends at `dst`.
fn fix_extremes(mut upscaled: Vec<TileRef>, dst: TileRef, src: Option<TileRef>) -> Vec<TileRef> {
    if let Some(src) = src {
        match upscaled.iter().position(|&t| t == src) {
            None => upscaled.insert(0, src),
            Some(0) => {}
            Some(index) => upscaled.drain(..index).for_each(drop),
        }
    }

    match upscaled.iter().position(|&t| t == dst) {
        None => upscaled.push(dst),
        Some(index) if index != upscaled.len() - 1 => upscaled.truncate(index + 1),
        Some(_) => {}
    }
    upscaled
}

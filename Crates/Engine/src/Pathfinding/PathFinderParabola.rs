// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/PathFinder.Parabola.ts: Bezier-arc paths of nukes and MIRVs.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::Line::{DistanceBasedBezierCurve, Point};
use super::LineV33::LegacyBezierCurve;
use super::Types::PathResult;
use crate::Core::Util::within;
use crate::Game::GameMap::{GameMapImpl, TileRef};

/// Upstream `ParabolaOptions`; `None` fields take upstream's defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParabolaOptions {
    /// Distance between sampled points (default 3).
    pub increment: Option<f64>,
    /// Arc height grows with distance (default true).
    pub distance_based_height: Option<bool>,
    /// The arc bows upward (default true).
    pub direction_up: Option<bool>,
    /// Do not clamp control points to the map (default false).
    pub ignore_map_bounds: Option<bool>,
    /// Lowest arc height when the height follows the distance (default 50).
    pub min_height: Option<f64>,
    /// Sample the curve with the pre-PR-5188 floating-point table.
    pub legacy_curve: bool,
    /// With the legacy curve: reset the sampled distance to 0 after each point (before PR 4770; later builds subtract the spacing).
    pub legacy_reset: bool,
}

/// The curve of either generation.
#[derive(Clone, Debug)]
enum Curve {
    Fixed(DistanceBasedBezierCurve),
    Legacy(LegacyBezierCurve),
}

impl Curve {
    fn points(&self) -> &[Point] {
        match self {
            Curve::Fixed(c) => c.get_all_points(),
            Curve::Legacy(c) => c.get_all_points(),
        }
    }

    fn increment(&mut self, distance: f64) -> Option<Point> {
        match self {
            Curve::Fixed(c) => c.increment(distance),
            Curve::Legacy(c) => c.increment(distance),
        }
    }

    fn current_index(&self) -> usize {
        match self {
            Curve::Fixed(c) => c.get_current_index(),
            Curve::Legacy(c) => c.get_current_index(),
        }
    }
}

const PARABOLA_MIN_HEIGHT: f64 = 50.0;

/// The four Bezier control points of an arc from `from` to `to`.
pub fn get_parabola_control_points(
    map: &GameMapImpl,
    from: TileRef,
    to: TileRef,
    options: &ParabolaOptions,
) -> [Point; 4] {
    let distance_based_height = options.distance_based_height.unwrap_or(true);
    let direction_up = options.direction_up.unwrap_or(true);
    let ignore_map_bounds = options.ignore_map_bounds.unwrap_or(false);

    let p0 = Point {
        x: map.x(from) as f64,
        y: map.y(from) as f64,
    };
    let p3 = Point {
        x: map.x(to) as f64,
        y: map.y(to) as f64,
    };
    let dx = p3.x - p0.x;
    let dy = p3.y - p0.y;
    let distance = (dx * dx + dy * dy).sqrt();
    let max_height = if distance_based_height {
        f64::max(
            distance / 3.0,
            options.min_height.unwrap_or(PARABOLA_MIN_HEIGHT),
        )
    } else {
        0.0
    };
    let height_mult = if direction_up { -1.0 } else { 1.0 };
    let map_height = map.height() as f64;

    let p1y = p0.y + dy / 4.0 + height_mult * max_height;
    let p2y = p0.y + (dy * 3.0) / 4.0 + height_mult * max_height;

    let p1 = Point {
        x: p0.x + dx / 4.0,
        y: if ignore_map_bounds {
            p1y
        } else {
            within(p1y, 0.0, map_height - 1.0)
        },
    };
    let p2 = Point {
        x: p0.x + (dx * 3.0) / 4.0,
        y: if ignore_map_bounds {
            p2y
        } else {
            within(p2y, 0.0, map_height - 1.0)
        },
    };

    [p0, p1, p2, p3]
}

/// Stepping path finder along a Bezier arc.
#[derive(Clone, Debug, Default)]
pub struct ParabolaUniversalPathFinder {
    curve: Option<Curve>,
    last_to: Option<TileRef>,
    options: ParabolaOptions,
}

impl ParabolaUniversalPathFinder {
    /// Creates a finder with `options` (`UniversalPathFinding.Parabola`).
    pub fn new(options: ParabolaOptions) -> Self {
        Self {
            curve: None,
            last_to: None,
            options,
        }
    }

    fn create_curve(&self, map: &GameMapImpl, from: TileRef, to: TileRef) -> Curve {
        let increment = self.options.increment.unwrap_or(3.0);
        let [p0, p1, p2, p3] = get_parabola_control_points(map, from, to, &self.options);
        if self.options.legacy_curve {
            Curve::Legacy(LegacyBezierCurve::new(
                p0,
                p1,
                p2,
                p3,
                increment,
                self.options.legacy_reset,
            ))
        } else {
            Curve::Fixed(DistanceBasedBezierCurve::new(p0, p1, p2, p3, increment))
        }
    }

    /// Every sampled tile of the arc (does not touch the stepping state).
    pub fn find_path(&self, map: &GameMapImpl, from: TileRef, to: TileRef) -> Vec<TileRef> {
        let curve = self.create_curve(map, from, to);
        curve
            .points()
            .iter()
            .map(|p| map.tile_ref(p.x.floor() as i32, p.y.floor() as i32))
            .collect()
    }

    /// Advances `speed` tiles (default 1) along the arc towards `to`.
    pub fn next(
        &mut self,
        map: &GameMapImpl,
        from: TileRef,
        to: TileRef,
        speed: Option<f64>,
    ) -> PathResult<TileRef> {
        if self.last_to != Some(to) {
            self.curve = Some(self.create_curve(map, from, to));
            self.last_to = Some(to);
        }

        let Some(next_point) = self
            .curve
            .as_mut()
            .expect("curve set above")
            .increment(speed.unwrap_or(1.0))
        else {
            return PathResult::Complete(to);
        };
        PathResult::Next(map.tile_ref(next_point.x.floor() as i32, next_point.y.floor() as i32))
    }

    /// Drops the curve.
    pub fn invalidate(&mut self) {
        self.curve = None;
        self.last_to = None;
    }

    /// Index of the current curve point.
    pub fn current_index(&self) -> usize {
        self.curve.as_ref().map_or(0, |c| c.current_index())
    }
}

/// `BouncingParabolaUniversalPathFinder`: the arc to the target, then a second arc that overshoots it by half the distance.
#[derive(Clone, Debug)]
pub struct BouncingParabolaPathFinder {
    parabola: ParabolaUniversalPathFinder,
    options: ParabolaOptions,
    bouncing: bool,
    from_bounce: TileRef,
    to_bounce: TileRef,
    previous_index: usize,
}

impl BouncingParabolaPathFinder {
    /// Creates the finder with the first arc's options.
    pub fn new(options: ParabolaOptions) -> Self {
        Self {
            parabola: ParabolaUniversalPathFinder::new(options),
            options,
            bouncing: false,
            from_bounce: 0,
            to_bounce: 0,
            previous_index: 0,
        }
    }

    fn compute_bounce_destination(
        map: &GameMapImpl,
        src: TileRef,
        dst: TileRef,
    ) -> Option<TileRef> {
        let (dest_x, dest_y) = (map.x(dst) as f64, map.y(dst) as f64);
        let (src_x, src_y) = (map.x(src) as f64, map.y(src) as f64);
        let new_x = f64::min(
            (dest_x + (dest_x - src_x) / 2.0).floor(),
            map.width() as f64 - 1.0,
        );
        let new_y = f64::min(
            (dest_y + (dest_y - src_y) / 2.0).floor(),
            map.height() as f64 - 1.0,
        );
        // Both are integers here, so the upstream integer check reduces to the lower bound.
        (new_x >= 0.0 && new_y >= 0.0).then(|| map.tile_ref(new_x as i32, new_y as i32))
    }

    /// Both arcs' sampled tiles.
    pub fn find_path(&self, map: &GameMapImpl, from: TileRef, to: TileRef) -> Vec<TileRef> {
        let mut tiles = self.parabola.find_path(map, from, to);
        if let Some(new_dest) = Self::compute_bounce_destination(map, from, to) {
            tiles.extend(self.parabola.find_path(map, to, new_dest));
        }
        tiles
    }

    /// Advances along the first arc, then along the bounce.
    pub fn next(
        &mut self,
        map: &GameMapImpl,
        from: TileRef,
        to: TileRef,
        speed: Option<f64>,
    ) -> PathResult<TileRef> {
        if self.bouncing {
            return self
                .parabola
                .next(map, self.from_bounce, self.to_bounce, speed);
        }
        let result = self.parabola.next(map, from, to, speed);
        if matches!(result, PathResult::Complete(_))
            && let Some(dest) = Self::compute_bounce_destination(map, from, to)
        {
            self.previous_index = self.parabola.current_index();
            self.bouncing = true;
            self.from_bounce = to;
            self.to_bounce = dest;
            self.parabola = ParabolaUniversalPathFinder::new(ParabolaOptions {
                increment: Some(self.options.increment.unwrap_or(3.0)),
                distance_based_height: Some(true),
                direction_up: Some(self.options.direction_up.unwrap_or(true)),
                min_height: Some(25.0),
                legacy_curve: self.options.legacy_curve,
                legacy_reset: self.options.legacy_reset,
                ..Default::default()
            });
            return self.parabola.next(map, to, dest, None);
        }
        result
    }

    /// Index along both arcs.
    pub fn current_index(&self) -> usize {
        self.parabola.current_index() + self.previous_index
    }
}

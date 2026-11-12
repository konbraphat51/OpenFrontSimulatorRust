// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/utilities/Line.ts before PR 5188: a floating-point cubic Bezier sampled by a cumulative distance table.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::Line::Point;

/// Precomputed points of a cubic Bezier curve and the distance-driven cursor over them.
#[derive(Clone, Debug)]
pub struct LegacyBezierCurve {
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    cached_points: Vec<Point>,
    /// `prefix[i]` is the polyline length up to point `i`, summed in upstream's order.
    prefix: Vec<f64>,
    total_distance: f64,
    current_index: usize,
}

impl LegacyBezierCurve {
    /// Creates the curve and samples a point every `distance_increment` of arc length.
    pub fn new(
        p0: Point,
        p1: Point,
        p2: Point,
        p3: Point,
        distance_increment: f64,
        reset_cumulative: bool,
    ) -> Self {
        let mut curve = Self {
            p0,
            p1,
            p2,
            p3,
            cached_points: Vec::new(),
            prefix: Vec::new(),
            total_distance: 0.0,
            current_index: 0,
        };
        curve.compute_all_points(distance_increment, 0.002, reset_cumulative);
        curve
    }

    fn point_at(&self, t: f64) -> Point {
        let tt_inv = 1.0 - t;
        let tt_inv2 = tt_inv * tt_inv;
        let tt_inv3 = tt_inv2 * tt_inv;
        let t2 = t * t;
        let t3 = t2 * t;
        let x = tt_inv3 * self.p0.x
            + 3.0 * tt_inv2 * t * self.p1.x
            + 3.0 * tt_inv * t2 * self.p2.x
            + t3 * self.p3.x;
        let y = tt_inv3 * self.p0.y
            + 3.0 * tt_inv2 * t * self.p1.y
            + 3.0 * tt_inv * t2 * self.p2.y
            + t3 * self.p3.y;
        Point { x, y }
    }

    /// Every sampled point.
    pub fn get_all_points(&self) -> &[Point] {
        &self.cached_points
    }

    /// Moves forward by `distance` and returns the current point, or `None` at the end.
    pub fn increment(&mut self, distance: f64) -> Option<Point> {
        self.total_distance += distance;
        while self.current_index + 1 < self.cached_points.len()
            && self.prefix[self.current_index + 1] < self.total_distance
        {
            self.current_index += 1;
        }
        if self.current_index + 1 >= self.cached_points.len() {
            return None;
        }
        Some(self.cached_points[self.current_index])
    }

    /// Index of the current point.
    pub fn get_current_index(&self) -> usize {
        self.current_index
    }

    fn compute_all_points(&mut self, pixel_spacing: f64, precision: f64, reset_cumulative: bool) {
        self.cached_points.clear();
        self.total_distance = 0.0;
        self.current_index = 0;

        let mut t = 0.0;
        let mut prev = self.point_at(t);
        self.cached_points.push(prev);
        let mut cumulative = 0.0;
        while t < 1.0 {
            t = f64::min(t + precision, 1.0);
            let current = self.point_at(t);
            let dx = current.x - prev.x;
            let dy = current.y - prev.y;
            cumulative += (dx * dx + dy * dy).sqrt();
            if cumulative >= pixel_spacing {
                self.cached_points.push(current);
                if reset_cumulative {
                    cumulative = 0.0;
                } else {
                    cumulative -= pixel_spacing;
                }
            }
            prev = current;
        }

        let final_point = self.point_at(1.0);
        let last = self.cached_points[self.cached_points.len() - 1];
        if final_point.x != last.x || final_point.y != last.y {
            self.cached_points.push(final_point);
        }

        self.prefix.clear();
        self.prefix.push(0.0);
        let mut dist = 0.0;
        for i in 1..self.cached_points.len() {
            let a = self.cached_points[i - 1];
            let b = self.cached_points[i];
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            dist += (dx * dx + dy * dy).sqrt();
            self.prefix.push(dist);
        }
    }
}

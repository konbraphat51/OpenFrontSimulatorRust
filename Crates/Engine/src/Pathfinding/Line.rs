// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/utilities/Line.ts: `DistanceBasedBezierCurve`, a fixed-point cubic Bezier sampled at regular distances.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Everything runs in 8-bit fixed point on integers held as `f64` upstream; here as `i64`, with the
//! same truncating shifts and `floor(sqrt(..))` distance steps, so sample points are identical.

use crate::Compat::JsNumber::round;

const SUB_SCALE: i64 = 256;

/// A 2D point (control points may be fractional; sampled points are integral).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

struct SubdivideState {
    last_x: i64,
    last_y: i64,
    accum_dist: i64,
    // Present while sampling points (absent in `get_length`)
    sampling: Option<(i64, Vec<Point>)>,
}

/// Precomputed regularly spaced points along a cubic Bezier curve.
#[derive(Clone, Debug)]
pub struct DistanceBasedBezierCurve {
    cached_points: Vec<Point>,
    current_index: usize,
    pixel_spacing_scaled: i64,
    accumulated_distance_scaled: i64,
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
}

fn fixed(value: f64) -> i64 {
    round(value) as i64 * SUB_SCALE
}

impl DistanceBasedBezierCurve {
    /// Creates the curve and precomputes its points every `distance_increment` tiles.
    pub fn new(p0: Point, p1: Point, p2: Point, p3: Point, distance_increment: f64) -> Self {
        let mut curve = Self {
            cached_points: Vec::new(),
            current_index: 0,
            pixel_spacing_scaled: 1,
            accumulated_distance_scaled: 0,
            p0,
            p1,
            p2,
            p3,
        };
        curve.compute_all_points(distance_increment);
        curve
    }

    /// Length of a Bezier curve, computed without allocating points.
    pub fn get_length(p0: Point, p1: Point, p2: Point, p3: Point) -> f64 {
        let (p0x, p0y) = (fixed(p0.x), fixed(p0.y));
        let (p3x, p3y) = (fixed(p3.x), fixed(p3.y));
        let mut st = SubdivideState {
            last_x: p0x,
            last_y: p0y,
            accum_dist: 0,
            sampling: None,
        };

        Self::shared_subdivide(
            p0x,
            p0y,
            fixed(p1.x),
            fixed(p1.y),
            fixed(p2.x),
            fixed(p2.y),
            p3x,
            p3y,
            0,
            &mut st,
        );

        let edx = p3x - st.last_x;
        let edy = p3y - st.last_y;
        st.accum_dist += ((edx * edx + edy * edy) as f64).sqrt().floor() as i64;

        st.accum_dist as f64 / SUB_SCALE as f64
    }

    /// Every sampled point.
    pub fn get_all_points(&self) -> &[Point] {
        &self.cached_points
    }

    /// Moves forward by `distance` and returns the current point, or `None` at the end.
    pub fn increment(&mut self, distance: f64) -> Option<Point> {
        let step = round(distance * SUB_SCALE as f64).max(1.0);
        self.accumulated_distance_scaled += step as i64;

        while self.current_index + 1 < self.cached_points.len()
            && self.accumulated_distance_scaled >= self.pixel_spacing_scaled
        {
            self.current_index += 1;
            self.accumulated_distance_scaled -= self.pixel_spacing_scaled;
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

    fn compute_all_points(&mut self, pixel_spacing: f64) {
        self.current_index = 0;
        self.accumulated_distance_scaled = 0;
        self.pixel_spacing_scaled = (round(pixel_spacing * SUB_SCALE as f64) as i64).max(SUB_SCALE);

        let (p0x, p0y) = (fixed(self.p0.x), fixed(self.p0.y));
        let (p3x, p3y) = (fixed(self.p3.x), fixed(self.p3.y));

        let first = Point {
            x: ((p0x + 128) >> 8) as f64,
            y: ((p0y + 128) >> 8) as f64,
        };
        let mut st = SubdivideState {
            last_x: p0x,
            last_y: p0y,
            accum_dist: 0,
            sampling: Some((self.pixel_spacing_scaled, vec![first])),
        };

        Self::shared_subdivide(
            p0x,
            p0y,
            fixed(self.p1.x),
            fixed(self.p1.y),
            fixed(self.p2.x),
            fixed(self.p2.y),
            p3x,
            p3y,
            0,
            &mut st,
        );

        let mut points = st
            .sampling
            .take()
            .map(|(_, points)| points)
            .unwrap_or_default();

        // Ensure the endpoint is included if not already P3
        let last = Point {
            x: ((p3x + 128) >> 8) as f64,
            y: ((p3y + 128) >> 8) as f64,
        };
        match points.last() {
            Some(end) if end.x != last.x || end.y != last.y => points.push(last),
            Some(_) => {}
            None => points.push(last),
        }
        self.cached_points = points;
    }

    /// In-order recursive De Casteljau subdivision with inline distance filtering.
    #[allow(clippy::too_many_arguments)]
    fn shared_subdivide(
        ax: i64,
        ay: i64,
        bx: i64,
        by: i64,
        cx: i64,
        cy: i64,
        dx: i64,
        dy: i64,
        depth: u32,
        st: &mut SubdivideState,
    ) {
        let dist = (bx - ax).abs()
            + (by - ay).abs()
            + (cx - bx).abs()
            + (cy - by).abs()
            + (dx - cx).abs()
            + (dy - cy).abs();
        if dist <= 256 || depth >= 10 {
            let edx = ax - st.last_x;
            let edy = ay - st.last_y;

            st.accum_dist += ((edx * edx + edy * edy) as f64).sqrt().floor() as i64;
            st.last_x = ax;
            st.last_y = ay;

            if let Some((threshold, points)) = st.sampling.as_mut() {
                while st.accum_dist >= *threshold {
                    points.push(Point {
                        x: ((ax + 128) >> 8) as f64,
                        y: ((ay + 128) >> 8) as f64,
                    });
                    st.accum_dist -= *threshold;
                }
            }
            return;
        }

        // De Casteljau midpoints via right shift
        let m01_x = (ax + bx) >> 1;
        let m01_y = (ay + by) >> 1;
        let m12_x = (bx + cx) >> 1;
        let m12_y = (by + cy) >> 1;
        let m23_x = (cx + dx) >> 1;
        let m23_y = (cy + dy) >> 1;

        let m012_x = (m01_x + m12_x) >> 1;
        let m012_y = (m01_y + m12_y) >> 1;
        let m123_x = (m12_x + m23_x) >> 1;
        let m123_y = (m12_y + m23_y) >> 1;

        let mx = (m012_x + m123_x) >> 1;
        let my = (m012_y + m123_y) >> 1;

        Self::shared_subdivide(ax, ay, m01_x, m01_y, m012_x, m012_y, mx, my, depth + 1, st);
        Self::shared_subdivide(mx, my, m123_x, m123_y, m23_x, m23_y, dx, dy, depth + 1, st);
    }
}

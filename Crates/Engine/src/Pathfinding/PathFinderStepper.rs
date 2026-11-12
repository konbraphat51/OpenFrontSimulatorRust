// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/PathFinderStepper.ts: step-by-step traversal with path caching and invalidation.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream holds a finder and a config object; here both are the [`StepperEnv`] passed per call,
//! so the stepper itself is plain state that executions can store.

use super::Types::{PathResult, Source};

/// Upstream `StepperConfig` plus the finder, supplied on every call.
pub trait StepperEnv<T: Copy + PartialEq> {
    /// Node equality (default `==`).
    fn equals(&self, a: T, b: T) -> bool {
        a == b
    }
    /// Distance between nodes, when the config has one (enables the `dist` early exit).
    fn distance(&self, _a: T, _b: T) -> Option<f64> {
        None
    }
    /// True when the config defines a `preCheck`.
    fn has_pre_check(&self) -> bool {
        false
    }
    /// Domain-specific pre-check; `Some` short-circuits `next`.
    fn pre_check(&self, _from: T, _to: T) -> Option<PathResult<T>> {
        None
    }
    /// The wrapped finder's `findPath`.
    fn find_path(&mut self, from: Source<T>, to: T) -> Option<Vec<T>>;
}

/// Caches a path and hands out one step per call.
#[derive(Clone, Debug)]
pub struct PathFinderStepper<T> {
    path: Option<Vec<T>>,
    path_index: usize,
    last_to: Option<T>,
}

impl<T> Default for PathFinderStepper<T> {
    fn default() -> Self {
        Self {
            path: None,
            path_index: 0,
            last_to: None,
        }
    }
}

impl<T: Copy + PartialEq> PathFinderStepper<T> {
    /// Creates a stepper without a cached path.
    pub fn new() -> Self {
        Self::default()
    }

    /// The next step from `from` towards `to`; `dist` > 0 ends the walk within that distance.
    pub fn next(
        &mut self,
        env: &mut impl StepperEnv<T>,
        from: T,
        to: T,
        dist: Option<f64>,
    ) -> PathResult<T> {
        // Loop instead of upstream's recursion after an invalidation
        loop {
            if let Some(result) = env.pre_check(from, to) {
                return result;
            }

            if env.equals(from, to) {
                return PathResult::Complete(to);
            }

            // Distance-based early exit
            if let Some(d) = dist
                && d > 0.0
                && let Some(distance) = env.distance(from, to)
                && distance <= d
            {
                return PathResult::Complete(from);
            }

            // Invalidate the cache if the destination changed
            if self.last_to.is_none_or(|last| !env.equals(last, to)) {
                self.path = None;
                self.path_index = 0;
                self.last_to = Some(to);
            }

            if self.path.is_none() {
                let Some(path) = env.find_path(Source::Single(from), to) else {
                    return PathResult::NotFound;
                };
                self.path_index = 0;
                if !path.is_empty() && env.equals(path[0], from) {
                    self.path_index = 1;
                }
                self.path = Some(path);
            }

            let path = self.path.as_ref().expect("path set above");
            if self.path_index > 0 && !env.equals(from, path[self.path_index - 1]) {
                self.invalidate();
                self.last_to = Some(to);
                continue;
            }

            // Reached the end
            if self.path_index >= path.len() {
                return PathResult::Complete(to);
            }

            let next_node = path[self.path_index];
            self.path_index += 1;
            return PathResult::Next(next_node);
        }
    }

    /// Drops the cached path.
    pub fn invalidate(&mut self) {
        self.path = None;
        self.path_index = 0;
        self.last_to = None;
    }

    /// Copy of the active path starting at the node most recently returned by `next`, if any.
    pub fn path_after_next(&self) -> Option<Vec<T>> {
        let path = self.path.as_ref()?;
        if self.path_index == 0 {
            return None;
        }
        Some(path[self.path_index - 1..].to_vec())
    }

    /// One-shot path that leaves the cached route alone; `None` when every source fails the pre-check.
    pub fn find_path(
        &self,
        env: &mut impl StepperEnv<T>,
        from: Source<T>,
        to: T,
    ) -> Option<Vec<T>> {
        if env.has_pre_check() {
            let all_failed = from
                .as_slice()
                .iter()
                .all(|&f| matches!(env.pre_check(f, to), Some(PathResult::NotFound)));
            if all_failed {
                return None;
            }
        }
        env.find_path(from, to)
    }
}

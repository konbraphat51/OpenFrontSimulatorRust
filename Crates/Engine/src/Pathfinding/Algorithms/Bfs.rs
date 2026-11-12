// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/BFS.ts: generic BFS over an adapter with a visitor.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::BfsGrid::Visit;
use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

/// Upstream `BFSAdapter<T>`.
pub trait BfsAdapter<T> {
    /// Neighbours of `node` in visiting order.
    fn neighbors(&self, node: &T) -> Vec<T>;
}

/// Generic BFS (used by the rail station finder, a later phase).
pub struct Bfs<A> {
    adapter: A,
}

impl<A> Bfs<A> {
    /// Wraps an adapter.
    pub fn new(adapter: A) -> Self {
        Self { adapter }
    }

    /// BFS from `starts` up to `max_distance` (`f64::INFINITY` for unlimited).
    pub fn search<T: Clone + Eq + Hash, R>(
        &self,
        starts: &[T],
        max_distance: f64,
        mut visitor: impl FnMut(&T, u32) -> Visit<R>,
    ) -> Option<R>
    where
        A: BfsAdapter<T>,
    {
        let mut visited: HashSet<T> = HashSet::new();
        let mut queue: VecDeque<(T, u32)> = VecDeque::new();
        for s in starts {
            visited.insert(s.clone());
            queue.push_back((s.clone(), 0));
        }

        while let Some((node, dist)) = queue.pop_front() {
            match visitor(&node, dist) {
                Visit::Found(result) => return Some(result),
                Visit::Reject => continue,
                Visit::Continue => {}
            }
            let next_dist = dist + 1;
            if next_dist as f64 > max_distance {
                continue;
            }
            for neighbor in self.adapter.neighbors(&node) {
                if visited.contains(&neighbor) {
                    continue;
                }
                visited.insert(neighbor.clone());
                queue.push_back((neighbor, next_dist));
            }
        }
        None
    }
}

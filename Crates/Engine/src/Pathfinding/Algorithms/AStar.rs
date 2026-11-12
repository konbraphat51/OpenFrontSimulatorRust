// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/AStar.ts: generic A* over an adapter (rail and station finders).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::PriorityQueue::{BucketQueue, PriorityQueue};

/// Upstream `AStarAdapter`.
pub trait AStarAdapter {
    /// Writes the neighbours of `node` into `buffer` and returns the count.
    fn neighbors(&self, node: u32, buffer: &mut [u32]) -> usize;
    /// Cost of the step `from` to `to`; `prev` is the node before `from` on the current path.
    fn cost(&self, from: u32, to: u32, prev: Option<u32>) -> f64;
    /// Heuristic estimate from `node` to `goal`.
    fn heuristic(&self, node: u32, goal: u32) -> f64;
    /// Number of nodes.
    fn num_nodes(&self) -> usize;
    /// Largest priority the bucket queue must hold.
    fn max_priority(&self) -> usize;
    /// Largest neighbour count.
    fn max_neighbors(&self) -> usize;
}

/// Generic A* with a bucket queue.
pub struct AStar<A: AStarAdapter> {
    stamp: u32,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    g_score: Vec<u32>,
    came_from: Vec<i32>,
    queue: BucketQueue,
    adapter: A,
    neighbor_buffer: Vec<u32>,
    max_iterations: i64,
}

impl<A: AStarAdapter> AStar<A> {
    /// Creates a finder; `max_iterations` defaults to 500,000 upstream.
    pub fn new(adapter: A, max_iterations: Option<i64>) -> Self {
        let n = adapter.num_nodes();
        Self {
            stamp: 1,
            closed_stamp: vec![0; n],
            g_score_stamp: vec![0; n],
            g_score: vec![0; n],
            came_from: vec![0; n],
            queue: BucketQueue::new(adapter.max_priority()),
            neighbor_buffer: vec![0; adapter.max_neighbors()],
            adapter,
            max_iterations: max_iterations.unwrap_or(500_000),
        }
    }

    /// Path from any of `starts` to `goal`.
    pub fn find_path(&mut self, starts: &[u32], goal: u32) -> Option<Vec<u32>> {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.closed_stamp.fill(0);
            self.g_score_stamp.fill(0);
            self.stamp = 1;
        }
        let stamp = self.stamp;

        self.queue.clear();
        for &s in starts {
            self.g_score[s as usize] = 0;
            self.g_score_stamp[s as usize] = stamp;
            self.came_from[s as usize] = -1;
            self.queue.push(s, self.adapter.heuristic(s, goal));
        }

        let mut iterations = self.max_iterations;
        while !self.queue.is_empty() {
            iterations -= 1;
            if iterations <= 0 {
                return None;
            }
            let current = self.queue.pop();
            if self.closed_stamp[current as usize] == stamp {
                continue;
            }
            self.closed_stamp[current as usize] = stamp;
            if current == goal {
                return Some(self.build_path(goal));
            }

            let current_g = self.g_score[current as usize];
            let prev = self.came_from[current as usize];
            let count = self.adapter.neighbors(current, &mut self.neighbor_buffer);
            for i in 0..count {
                let neighbor = self.neighbor_buffer[i];
                if self.closed_stamp[neighbor as usize] == stamp {
                    continue;
                }
                let prev_arg = if prev == -1 { None } else { Some(prev as u32) };
                let step = self.adapter.cost(current, neighbor, prev_arg);
                // Uint32Array stores truncate; costs are integers in practice.
                let tentative_g = current_g as f64 + step;
                if self.g_score_stamp[neighbor as usize] != stamp
                    || tentative_g < self.g_score[neighbor as usize] as f64
                {
                    self.came_from[neighbor as usize] = current as i32;
                    self.g_score[neighbor as usize] = tentative_g as u32;
                    self.g_score_stamp[neighbor as usize] = stamp;
                    self.queue.push(
                        neighbor,
                        tentative_g + self.adapter.heuristic(neighbor, goal),
                    );
                }
            }
        }
        None
    }

    fn build_path(&self, goal: u32) -> Vec<u32> {
        let mut path = Vec::new();
        let mut current = goal as i32;
        while current != -1 {
            path.push(current as u32);
            current = self.came_from[current as usize];
        }
        path.reverse();
        path
    }
}

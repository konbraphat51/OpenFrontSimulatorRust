// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/BFS.Grid.ts: 4-direction grid BFS with stamp-based visited tracking.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

/// What a BFS visitor decides about a visited node (upstream returns `R`, `undefined` or `null`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Visit<R> {
    /// Found the target; stop the search and return it.
    Found(R),
    /// Valid node; explore its neighbours.
    Continue,
    /// Reject the node; do not explore its neighbours.
    Reject,
}

/// Grid BFS over a `width * height` node space; the scratch arrays are reused between searches.
#[derive(Clone, Debug)]
pub struct BfsGrid {
    stamp: u32,
    visited_stamp: Vec<u32>,
    queue: Vec<u32>,
    dist: Vec<u16>,
}

impl BfsGrid {
    /// Creates scratch for `num_nodes` nodes.
    pub fn new(num_nodes: usize) -> Self {
        Self {
            stamp: 1,
            visited_stamp: vec![0; num_nodes],
            queue: vec![0; num_nodes],
            dist: vec![0; num_nodes],
        }
    }

    /// Visits nodes in BFS order (north, south, west, east) from `starts` up to `max_distance`.
    ///
    /// `is_valid_node` filters traversable nodes; `visitor` receives `(node, distance)`.
    pub fn search<R>(
        &mut self,
        width: u32,
        height: u32,
        starts: &[u32],
        max_distance: f64,
        mut is_valid_node: impl FnMut(u32) -> bool,
        mut visitor: impl FnMut(u32, u32) -> Visit<R>,
    ) -> Option<R> {
        let stamp = self.next_stamp();
        let last_row_start = (height - 1) * width;

        let mut head = 0usize;
        let mut tail = 0usize;

        for &s in starts {
            self.visited_stamp[s as usize] = stamp;
            self.dist[s as usize] = 0;
            self.queue[tail] = s;
            tail += 1;
        }

        while head < tail {
            let node = self.queue[head];
            head += 1;
            let dist = self.dist[node as usize] as u32;

            match visitor(node, dist) {
                Visit::Found(result) => return Some(result),
                Visit::Reject => continue,
                Visit::Continue => {}
            }

            let next_dist = dist + 1;
            if next_dist as f64 > max_distance {
                continue;
            }

            let x = node % width;

            // North, south, west, east
            if node >= width {
                let n = node - width;
                self.try_enqueue(n, stamp, next_dist, &mut tail, &mut is_valid_node);
            }
            if node < last_row_start {
                let s = node + width;
                self.try_enqueue(s, stamp, next_dist, &mut tail, &mut is_valid_node);
            }
            if x != 0 {
                let w = node - 1;
                self.try_enqueue(w, stamp, next_dist, &mut tail, &mut is_valid_node);
            }
            if x != width - 1 {
                let e = node + 1;
                self.try_enqueue(e, stamp, next_dist, &mut tail, &mut is_valid_node);
            }
        }

        None
    }

    fn try_enqueue(
        &mut self,
        node: u32,
        stamp: u32,
        dist: u32,
        tail: &mut usize,
        is_valid_node: &mut impl FnMut(u32) -> bool,
    ) {
        if self.visited_stamp[node as usize] != stamp && is_valid_node(node) {
            self.visited_stamp[node as usize] = stamp;
            self.dist[node as usize] = dist as u16;
            self.queue[*tail] = node;
            *tail += 1;
        }
    }

    fn next_stamp(&mut self) -> u32 {
        let stamp = self.stamp;
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.visited_stamp.fill(0);
            self.stamp = 1;
        }
        stamp
    }
}

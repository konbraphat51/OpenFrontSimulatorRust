// SPDX-License-Identifier: AGPL-3.0-only
//! Port of npm `fastpriorityqueue` 0.7.5 as used by `SerialAStar`: a binary heap ordered by `a.fScore < b.fScore`.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Ties are broken by the exact sift order of the library, so `add` and `poll` mirror it step by step.

/// A queued node with its estimated total cost.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// The node (tile or station).
    pub tile: u32,
    /// Estimated total cost through the node.
    pub f_score: f64,
}

/// Binary min-heap on `f_score` (strict `<` comparator of upstream).
#[derive(Clone, Debug, Default)]
pub struct FastPriorityQueue {
    array: Vec<Entry>,
}

impl FastPriorityQueue {
    /// An empty queue.
    pub fn new() -> Self {
        Self { array: Vec::new() }
    }

    /// True when nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.array.is_empty()
    }

    /// `add(value)`: sifts the value up while it is strictly smaller than its parent.
    pub fn add(&mut self, value: Entry) {
        let mut i = self.array.len();
        self.array.push(value);
        while i > 0 {
            let p = (i - 1) >> 1;
            let ap = self.array[p];
            if value.f_score >= ap.f_score {
                break;
            }
            self.array[i] = ap;
            i = p;
        }
        self.array[i] = value;
    }

    /// `poll()`: removes and returns the smallest entry (`None` when empty).
    pub fn poll(&mut self) -> Option<Entry> {
        if self.array.is_empty() {
            return None;
        }
        let ans = self.array[0];
        let last = self.array.pop().expect("not empty");
        if !self.array.is_empty() {
            self.array[0] = last;
            self.percolate_down(0);
        }
        Some(ans)
    }

    fn percolate_down(&mut self, mut i: usize) {
        let size = self.array.len();
        let hsize = size >> 1;
        let ai = self.array[i];
        while i < hsize {
            let mut l = (i << 1) + 1;
            let r = l + 1;
            let mut bestc = self.array[l];
            if r < size && self.array[r].f_score < bestc.f_score {
                l = r;
                bestc = self.array[r];
            }
            if bestc.f_score >= ai.f_score {
                break;
            }
            self.array[i] = bestc;
            i = l;
        }
        self.array[i] = ai;
    }
}

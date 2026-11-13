// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/FlatBinaryHeap.ts: min-heap of (priority, tile) pairs.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Priorities are stored as `f32` like upstream's `Float32Array`, while a pushed priority is
//! compared as `f64` against the stored (rounded) ones; both effects decide tie order.

use crate::Game::GameMap::TileRef;

/// Min-heap keyed by an `f32`-rounded priority; (priority, tile) pairs share one array for locality.
#[derive(Debug, Default)]
pub struct FlatBinaryHeap {
    entries: Vec<(f32, TileRef)>,
}

impl FlatBinaryHeap {
    /// `new FlatBinaryHeap()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `clear`.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// `size`.
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// `enqueue(tile, priority)`.
    pub fn enqueue(&mut self, tile: TileRef, priority: f64) {
        let mut i = self.entries.len();
        self.entries.push((0.0, 0));
        while i > 0 {
            let parent = (i - 1) >> 1;
            let parent_entry = self.entries[parent];
            if priority >= parent_entry.0 as f64 {
                break;
            }
            self.entries[i] = parent_entry;
            i = parent;
        }
        self.entries[i] = (priority as f32, tile);
    }

    /// `dequeue()`: removes and returns the lowest-priority tile; panics when empty.
    pub fn dequeue(&mut self) -> TileRef {
        assert!(!self.entries.is_empty(), "heap empty");
        let top_tile = self.entries[0].1;
        let last = self.entries.pop().expect("non-empty");
        let len = self.entries.len();
        let mut i = 0;
        loop {
            let left = (i << 1) + 1;
            if left >= len {
                break;
            }
            let right = left + 1;
            let child = if right < len && self.entries[right].0 < self.entries[left].0 {
                right
            } else {
                left
            };
            if last.0 <= self.entries[child].0 {
                break;
            }
            self.entries[i] = self.entries[child];
            i = child;
        }
        if len > 0 {
            self.entries[i] = last;
        }
        top_tile
    }
}

#[cfg(test)]
mod tests {
    use super::FlatBinaryHeap;

    #[test]
    fn dequeues_in_priority_order() {
        let mut h = FlatBinaryHeap::new();
        for (t, p) in [(1, 5.5), (2, 1.25), (3, 9.0), (4, 3.0), (5, 3.0)] {
            h.enqueue(t, p);
        }
        let mut out = vec![];
        while h.size() > 0 {
            out.push(h.dequeue());
        }
        assert_eq!(out.len(), 5);
        assert_eq!(out[0], 2);
        assert_eq!(out[4], 3);
    }
}

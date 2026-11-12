// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/FlatBinaryHeap.ts: min-heap of (priority, tile) pairs.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Priorities are stored as `f32` like upstream's `Float32Array`, while a pushed priority is
//! compared as `f64` against the stored (rounded) ones; both effects decide tie order.

use crate::Game::GameMap::TileRef;

/// Min-heap keyed by an `f32`-rounded priority.
#[derive(Debug, Default)]
pub struct FlatBinaryHeap {
    pri: Vec<f32>,
    tiles: Vec<TileRef>,
}

impl FlatBinaryHeap {
    /// `new FlatBinaryHeap()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `clear`.
    pub fn clear(&mut self) {
        self.pri.clear();
        self.tiles.clear();
    }

    /// `size`.
    pub fn size(&self) -> usize {
        self.pri.len()
    }

    /// `enqueue(tile, priority)`.
    pub fn enqueue(&mut self, tile: TileRef, priority: f64) {
        let mut i = self.pri.len();
        self.pri.push(0.0);
        self.tiles.push(0);
        while i > 0 {
            let parent = (i - 1) >> 1;
            if priority >= self.pri[parent] as f64 {
                break;
            }
            self.pri[i] = self.pri[parent];
            self.tiles[i] = self.tiles[parent];
            i = parent;
        }
        self.pri[i] = priority as f32;
        self.tiles[i] = tile;
    }

    /// `dequeue()`: removes and returns the lowest-priority tile; panics when empty.
    pub fn dequeue(&mut self) -> TileRef {
        assert!(!self.pri.is_empty(), "heap empty");
        let top_tile = self.tiles[0];
        let last_pri = self.pri.pop().expect("non-empty");
        let last_tile = self.tiles.pop().expect("non-empty");
        let len = self.pri.len();
        let mut i = 0;
        loop {
            let left = (i << 1) + 1;
            if left >= len {
                break;
            }
            let right = left + 1;
            let child = if right < len && self.pri[right] < self.pri[left] {
                right
            } else {
                left
            };
            if last_pri <= self.pri[child] {
                break;
            }
            self.pri[i] = self.pri[child];
            self.tiles[i] = self.tiles[child];
            i = child;
        }
        if len > 0 {
            self.pri[i] = last_pri;
            self.tiles[i] = last_tile;
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

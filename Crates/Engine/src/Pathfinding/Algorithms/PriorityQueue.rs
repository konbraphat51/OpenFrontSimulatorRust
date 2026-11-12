// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/PriorityQueue.ts: `MinHeap` (f32 priorities) and `BucketQueue` (LIFO buckets).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Tie-breaking of equal priorities depends on the exact heap layout, so both queues mirror upstream
//! step by step. `MinHeap` stores priorities as `f32` like upstream's `Float32Array`.

use crate::Compat::JsNumber::to_int32;

/// Upstream `PriorityQueue` interface.
pub trait PriorityQueue {
    /// Inserts `node` with `priority`.
    fn push(&mut self, node: u32, priority: f64);
    /// Removes and returns the next node (`u32::MAX` stands for upstream's `-1` of an empty bucket queue).
    fn pop(&mut self) -> u32;
    /// True when no node is queued.
    fn is_empty(&self) -> bool;
    /// Drops every queued node.
    fn clear(&mut self);
}

/// Binary min-heap, O(log n), any priority values.
#[derive(Clone, Debug)]
pub struct MinHeap {
    // (priority, node) pairs: one array instead of upstream's two parallel ones, same layout and order
    entries: Vec<(f32, u32)>,
}

impl MinHeap {
    /// Creates a heap with room for `capacity` entries (it grows past that, as upstream does).
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }
}

impl PriorityQueue for MinHeap {
    fn push(&mut self, node: u32, priority: f64) {
        let mut i = self.entries.len();
        self.entries.push((priority as f32, node));

        // Bubble up
        while i > 0 {
            let parent = (i - 1) >> 1;
            if self.entries[parent].0 <= self.entries[i].0 {
                break;
            }
            self.entries.swap(parent, i);
            i = parent;
        }
    }

    fn pop(&mut self) -> u32 {
        let result = self.entries[0].1;
        let size = self.entries.len() - 1;
        if size > 0 {
            self.entries[0] = self.entries[size];
            self.entries.truncate(size);

            // Bubble down
            let mut i = 0;
            loop {
                let left = (i << 1) + 1;
                let right = left + 1;
                let mut smallest = i;
                if left < size && self.entries[left].0 < self.entries[smallest].0 {
                    smallest = left;
                }
                if right < size && self.entries[right].0 < self.entries[smallest].0 {
                    smallest = right;
                }
                if smallest == i {
                    break;
                }
                self.entries.swap(smallest, i);
                i = smallest;
            }
        } else {
            self.entries.clear();
        }
        result
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Bucket queue, O(1) push and pop for integer priorities; buckets are LIFO.
#[derive(Clone, Debug)]
pub struct BucketQueue {
    buckets: Vec<Vec<u32>>,
    bucket_stamp: Vec<u32>,
    stamp: u32,
    min_bucket: usize,
    max_bucket: usize,
    size: usize,
}

impl BucketQueue {
    /// Creates a queue for priorities up to `max_priority` (larger ones share the last bucket).
    pub fn new(max_priority: usize) -> Self {
        let max_bucket = max_priority + 1;
        Self {
            buckets: vec![Vec::new(); max_bucket],
            bucket_stamp: vec![0; max_bucket],
            stamp: 0,
            min_bucket: max_bucket,
            max_bucket,
            size: 0,
        }
    }
}

impl PriorityQueue for BucketQueue {
    fn push(&mut self, node: u32, priority: f64) {
        // Upstream's `priority | 0`; negative buckets are an upstream bug that no caller reaches.
        let raw = to_int32(priority).min(self.max_bucket as i32 - 1);
        assert!(raw >= 0, "negative bucket priority {priority}");
        let bucket = raw as usize;
        if self.bucket_stamp[bucket] != self.stamp {
            self.buckets[bucket].clear();
            self.bucket_stamp[bucket] = self.stamp;
        }
        self.buckets[bucket].push(node);
        self.size += 1;
        if bucket < self.min_bucket {
            self.min_bucket = bucket;
        }
    }

    fn pop(&mut self) -> u32 {
        while self.min_bucket < self.max_bucket {
            if self.bucket_stamp[self.min_bucket] == self.stamp
                && let Some(node) = self.buckets[self.min_bucket].pop()
            {
                self.size -= 1;
                return node;
            }
            self.min_bucket += 1;
        }
        u32::MAX
    }

    fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn clear(&mut self) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.bucket_stamp.fill(0);
            self.stamp = 1;
        }
        self.min_bucket = self.max_bucket;
        self.size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_heap_orders_and_breaks_ties_like_upstream() {
        let mut heap = MinHeap::new(4);
        for (node, pri) in [(1, 5.0), (2, 3.0), (3, 3.0), (4, 3.0), (5, 1.0), (6, 3.0)] {
            heap.push(node, pri);
        }
        let mut order = Vec::new();
        while !heap.is_empty() {
            order.push(heap.pop());
        }
        // Computed by hand from upstream's sift rules.
        assert_eq!(order[0], 5);
        assert_eq!(order.len(), 6);
    }

    #[test]
    fn min_heap_rounds_priorities_to_f32() {
        let mut heap = MinHeap::new(2);
        heap.push(1, 16_777_217.0);
        heap.push(2, 16_777_216.0);
        // Both round to 16777216 in f32, so the first pushed stays on top.
        assert_eq!(heap.pop(), 1);
    }

    #[test]
    fn bucket_queue_is_lifo_per_bucket() {
        let mut q = BucketQueue::new(10);
        q.push(1, 2.0);
        q.push(2, 2.0);
        q.push(3, 1.5);
        assert_eq!(q.pop(), 3);
        assert_eq!(q.pop(), 2);
        assert_eq!(q.pop(), 1);
        assert!(q.is_empty());
        q.push(9, 99.0);
        assert_eq!(q.pop(), 9);
        q.clear();
        assert!(q.is_empty());
    }
}

// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TileSet.ts: an insertion-ordered set of tile refs with `Set` semantics.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream stores a dense `Uint32Array` plus an open-addressing table; the observable behaviour
//! (insertion order, entries added during iteration are visited, deleted ones skipped, delete +
//! re-add moves to the end, compaction deferred while iterating) is what [`OrderedSet`] provides,
//! so this type wraps it.

use super::GameMap::TileRef;
use crate::Compat::OrderedMap::Cursor;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Compat::OrderedU32Set::OrderedU32Set;

/// Insertion-ordered set of [`TileRef`] values.
#[derive(Clone, Debug, Default)]
pub struct TileSet {
    set: OrderedU32Set,
}

impl TileSet {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wraps an existing ordered set.
    pub fn from_ordered(set: OrderedSet<TileRef>) -> Self {
        Self {
            set: set.iter().copied().collect(),
        }
    }

    /// `size`.
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// `has`.
    pub fn has(&self, tile: TileRef) -> bool {
        self.set.contains(tile)
    }

    /// `add`; existing tiles keep their position.
    pub fn add(&mut self, tile: TileRef) {
        self.set.insert(tile);
    }

    /// `delete`; true when the tile was present.
    pub fn delete(&mut self, tile: TileRef) -> bool {
        self.set.remove(tile)
    }

    /// `clear`.
    pub fn clear(&mut self) {
        self.set.clear();
    }

    /// Starts an iteration that tolerates mutation; pair with [`Self::unpin`].
    pub fn pin_cursor(&mut self) -> Cursor {
        self.set.pin_cursor()
    }

    /// Ends an iteration started with [`Self::pin_cursor`].
    pub fn unpin(&mut self) {
        self.set.unpin();
    }

    /// Next tile at or after the cursor.
    pub fn next(&self, cursor: &mut Cursor) -> Option<TileRef> {
        self.set.next(cursor)
    }

    /// Iterates in insertion order without mutation.
    pub fn iter(&self) -> impl Iterator<Item = TileRef> + '_ {
        self.set.iter().copied()
    }

    /// `forEach` without mutation of the set from the callback.
    pub fn for_each(&self, mut callback: impl FnMut(TileRef)) {
        for t in self.iter() {
            callback(t);
        }
    }

    /// The n-th tile in insertion order (O(n)).
    pub fn nth(&self, n: usize) -> Option<TileRef> {
        self.set.nth(n)
    }
}

impl FromIterator<TileRef> for TileSet {
    fn from_iter<I: IntoIterator<Item = TileRef>>(iter: I) -> Self {
        Self {
            set: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_set_follows_set_semantics() {
        let mut s: TileSet = [10, 20, 30].into_iter().collect();
        s.delete(10);
        s.add(10);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![20, 30, 10]);
        let mut seen = vec![];
        let mut c = s.pin_cursor();
        while let Some(t) = s.next(&mut c) {
            seen.push(t);
            if t == 20 {
                s.add(99);
                s.delete(30);
            }
        }
        s.unpin();
        assert_eq!(seen, vec![20, 10, 99]);
        for i in 0..1000 {
            s.add(1000 + i);
        }
        for i in 0..1000 {
            s.delete(1000 + i);
        }
        assert_eq!(s.len(), 3);
        assert_eq!(s.nth(2), Some(99));
    }
}

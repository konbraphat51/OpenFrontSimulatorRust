// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TileSet.ts: an insertion-ordered set of tile refs with `Set` semantics.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream stores a dense `Uint32Array` plus an open-addressing table; the observable behaviour
//! (insertion order, entries added during iteration are visited, deleted ones skipped, delete +
//! re-add moves to the end, compaction deferred while iterating) is what [`OrderedSet`] provides,
//! so this type wraps it. A set can be backed by a shared per-tile position array
//! ([`TilePositions`]) instead of a hash table: the players' `tiles` sets (and likewise their
//! `border_tiles` sets) partition the map, so one array indexed by tile serves all of them.

use super::GameMap::TileRef;
use crate::Compat::OrderedMap::Cursor;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Compat::OrderedU32Set::{EMPTY, OrderedSlots, OrderedU32Set, SlotIndex};
use std::cell::RefCell;
use std::rc::Rc;

/// Per-tile slot number shared by a family of disjoint tile sets (`EMPTY` = in no set).
///
/// An entry may be stale (left by another set of the family); a lookup therefore also checks that
/// the slot of the asking set really holds the tile.
pub type TilePositions = Rc<RefCell<Vec<u32>>>;

/// Creates the position array of a family of tile sets over a map of `num_tiles` tiles.
pub fn new_tile_positions(num_tiles: usize) -> TilePositions {
    Rc::new(RefCell::new(vec![EMPTY; num_tiles]))
}

/// [`SlotIndex`] reading and writing a [`TilePositions`] array.
#[derive(Clone, Debug)]
pub struct PositionIndex {
    positions: TilePositions,
}

impl SlotIndex for PositionIndex {
    const COMPACT_FACTOR: usize = 1;

    #[inline]
    fn find(&self, key: u32, slots: &[u32]) -> Option<u32> {
        let slot = self.positions.borrow()[key as usize];
        (slots.get(slot as usize) == Some(&key)).then_some(slot)
    }

    #[inline]
    fn insert(&mut self, key: u32, slot: u32, slots: &[u32]) -> bool {
        let mut positions = self.positions.borrow_mut();
        let existing = positions[key as usize];
        if slots.get(existing as usize) == Some(&key) {
            return false;
        }
        positions[key as usize] = slot;
        true
    }

    #[inline]
    fn remove(&mut self, key: u32, slots: &[u32]) -> Option<u32> {
        let mut positions = self.positions.borrow_mut();
        let slot = positions[key as usize];
        if slots.get(slot as usize) != Some(&key) {
            return None;
        }
        positions[key as usize] = EMPTY;
        Some(slot)
    }

    fn rebuild(&mut self, slots: &[u32]) {
        let mut positions = self.positions.borrow_mut();
        for (slot, &key) in slots.iter().enumerate() {
            positions[key as usize] = slot as u32;
        }
    }

    fn clear(&mut self, slots: &[u32]) {
        let mut positions = self.positions.borrow_mut();
        for &key in slots {
            if key != EMPTY && slots.get(positions[key as usize] as usize) == Some(&key) {
                positions[key as usize] = EMPTY;
            }
        }
    }
}

/// Backing store of a [`TileSet`].
#[derive(Debug)]
enum Store {
    Hashed(OrderedU32Set),
    Positioned(OrderedSlots<PositionIndex>),
}

/// Insertion-ordered set of [`TileRef`] values.
#[derive(Debug)]
pub struct TileSet {
    store: Store,
}

impl Default for TileSet {
    fn default() -> Self {
        Self {
            store: Store::Hashed(OrderedU32Set::new()),
        }
    }
}

impl Clone for TileSet {
    /// A copy never shares the position array: it is a hashed set with the same order.
    fn clone(&self) -> Self {
        Self {
            store: Store::Hashed(self.iter().collect()),
        }
    }
}

/// Dispatches a call to whichever store backs the set.
macro_rules! on_store {
    ($self:expr, $set:ident => $body:expr) => {
        match &$self.store {
            Store::Hashed($set) => $body,
            Store::Positioned($set) => $body,
        }
    };
    (mut $self:expr, $set:ident => $body:expr) => {
        match &mut $self.store {
            Store::Hashed($set) => $body,
            Store::Positioned($set) => $body,
        }
    };
}

impl TileSet {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty set backed by `positions` (see [`TilePositions`]); no other live set of the
    /// same family may hold a tile this one holds.
    pub fn positioned(positions: &TilePositions) -> Self {
        Self {
            store: Store::Positioned(OrderedSlots::with_index(PositionIndex {
                positions: Rc::clone(positions),
            })),
        }
    }

    /// Wraps an existing ordered set.
    pub fn from_ordered(set: OrderedSet<TileRef>) -> Self {
        Self {
            store: Store::Hashed(set.iter().copied().collect()),
        }
    }

    /// `size`.
    pub fn len(&self) -> usize {
        on_store!(self, set => set.len())
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        on_store!(self, set => set.is_empty())
    }

    /// `has`.
    pub fn has(&self, tile: TileRef) -> bool {
        on_store!(self, set => set.contains(tile))
    }

    /// `add`; existing tiles keep their position.
    pub fn add(&mut self, tile: TileRef) {
        on_store!(mut self, set => set.insert(tile));
    }

    /// `delete`; true when the tile was present.
    pub fn delete(&mut self, tile: TileRef) -> bool {
        on_store!(mut self, set => set.remove(tile))
    }

    /// `clear`.
    pub fn clear(&mut self) {
        on_store!(mut self, set => set.clear());
    }

    /// Starts an iteration that tolerates mutation; pair with [`Self::unpin`].
    pub fn pin_cursor(&mut self) -> Cursor {
        on_store!(mut self, set => set.pin_cursor())
    }

    /// Ends an iteration started with [`Self::pin_cursor`].
    pub fn unpin(&mut self) {
        on_store!(mut self, set => set.unpin());
    }

    /// Next tile at or after the cursor.
    pub fn next(&self, cursor: &mut Cursor) -> Option<TileRef> {
        on_store!(self, set => set.next(cursor))
    }

    /// Iterates in insertion order without mutation.
    pub fn iter(&self) -> impl Iterator<Item = TileRef> + '_ {
        on_store!(self, set => set.iter().copied())
    }

    /// `forEach` without mutation of the set from the callback.
    pub fn for_each(&self, mut callback: impl FnMut(TileRef)) {
        for t in self.iter() {
            callback(t);
        }
    }

    /// The n-th tile in insertion order (O(n)).
    pub fn nth(&self, n: usize) -> Option<TileRef> {
        on_store!(self, set => set.nth(n))
    }
}

impl FromIterator<TileRef> for TileSet {
    fn from_iter<I: IntoIterator<Item = TileRef>>(iter: I) -> Self {
        Self {
            store: Store::Hashed(iter.into_iter().collect()),
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

    #[test]
    fn positioned_sets_match_hashed_sets_under_churn() {
        // Two sets of one family trade tiles (a tile is in at most one of them), like two players.
        let positions = new_tile_positions(500);
        let mut sets = [
            TileSet::positioned(&positions),
            TileSet::positioned(&positions),
        ];
        let mut reference = [TileSet::new(), TileSet::new()];
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for step in 0..100_000u32 {
            let tile = (next() % 500) as u32;
            let to = (next() % 3) as usize;
            for k in 0..2 {
                if sets[k].has(tile) {
                    assert!(sets[k].delete(tile) && reference[k].delete(tile));
                }
            }
            if to < 2 {
                sets[to].add(tile);
                reference[to].add(tile);
            }
            if step % 997 == 0 {
                for k in 0..2 {
                    assert!(sets[k].iter().eq(reference[k].iter()));
                    assert_eq!(sets[k].len(), reference[k].len());
                }
            }
        }
        for k in 0..2 {
            assert!(sets[k].iter().eq(reference[k].iter()));
            for t in 0..500 {
                assert_eq!(sets[k].has(t), reference[k].has(t));
            }
            let copy = sets[k].clone();
            assert!(copy.iter().eq(sets[k].iter()));
        }
        sets[0].clear();
        assert_eq!(sets[0].len(), 0);
        assert!(sets[1].iter().all(|t| !sets[0].has(t)));
    }
}

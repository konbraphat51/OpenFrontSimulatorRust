// SPDX-License-Identifier: AGPL-3.0-only
//! Insertion-ordered set of `u32` values with JS `Set` semantics, tuned for tile sets.
//!
//! Same observable behaviour as [`OrderedSet`](super::OrderedSet::OrderedSet) (delete + re-add
//! moves to the end, additions during iteration are visited, deleted entries are skipped,
//! compaction deferred while a cursor is pinned). The value-to-slot lookup is a [`SlotIndex`]:
//! [`HashIndex`] is an open-addressing table with backward-shift deletion (about half the
//! instructions of a `HashMap`); the tile sets of the players use a shared per-tile position array.
//! `u32::MAX` is reserved as the empty marker and cannot be stored.

use super::OrderedMap::Cursor;

/// Marks a deleted slot; `u32::MAX` cannot be stored.
pub const EMPTY: u32 = u32::MAX;

/// Key lookup of an [`OrderedSlots`]: maps a value to its slot in the insertion-ordered slot vector.
pub trait SlotIndex {
    /// Compact once the dead slots exceed this multiple of the live values (and 32).
    const COMPACT_FACTOR: usize;
    /// Slot of `key`, if present.
    fn find(&self, key: u32, slots: &[u32]) -> Option<u32>;
    /// Registers `key` at `slot` (the next free one) unless present; true when it was added.
    fn insert(&mut self, key: u32, slot: u32, slots: &[u32]) -> bool;
    /// Unregisters `key` and returns its slot when present.
    fn remove(&mut self, key: u32, slots: &[u32]) -> Option<u32>;
    /// Re-registers every live value after the slots were compacted.
    fn rebuild(&mut self, slots: &[u32]);
    /// Forgets every value of `slots` (called before the slots are cleared).
    fn clear(&mut self, slots: &[u32]);
}

/// Slot and key of one live value, stored together so a probe never touches `slots`.
#[derive(Clone, Copy, Debug)]
struct Entry {
    key: u32,
    slot: u32,
}

const VACANT: Entry = Entry {
    key: EMPTY,
    slot: 0,
};

#[inline]
fn hash_of(key: u32) -> usize {
    (key.wrapping_mul(0x9E37_79B1) ^ (key >> 15)) as usize
}

/// Open-addressing table (power-of-two length, load at most one half) with backward-shift deletion.
#[derive(Clone, Debug, Default)]
pub struct HashIndex {
    table: Vec<Entry>,
    len: u32,
}

impl HashIndex {
    /// Position in `table` of `key`, or of the vacant entry that ends its probe sequence.
    #[inline]
    fn probe(&self, key: u32) -> usize {
        let mask = self.table.len() - 1;
        let mut i = hash_of(key) & mask;
        loop {
            let e = self.table[i];
            if e.key == key || e.key == EMPTY {
                return i;
            }
            i = (i + 1) & mask;
        }
    }

    /// Re-inserts every live value of `slots` into a table of `cap` entries.
    fn rebuild_with(&mut self, slots: &[u32], cap: usize) {
        self.table.clear();
        self.table.resize(cap, VACANT);
        let mask = cap - 1;
        for (slot, &key) in slots.iter().enumerate() {
            if key == EMPTY {
                continue;
            }
            let mut i = hash_of(key) & mask;
            while self.table[i].key != EMPTY {
                i = (i + 1) & mask;
            }
            self.table[i] = Entry {
                key,
                slot: slot as u32,
            };
        }
    }
}

impl SlotIndex for HashIndex {
    const COMPACT_FACTOR: usize = 2;

    #[inline]
    fn find(&self, key: u32, _slots: &[u32]) -> Option<u32> {
        if self.table.is_empty() {
            return None;
        }
        let e = self.table[self.probe(key)];
        (e.key == key).then_some(e.slot)
    }

    fn insert(&mut self, key: u32, slot: u32, slots: &[u32]) -> bool {
        debug_assert!(key != EMPTY, "u32::MAX is reserved");
        if (self.len as usize + 1) * 2 > self.table.len() {
            let cap = (self.table.len() * 2).max(16);
            self.rebuild_with(slots, cap);
        }
        let i = self.probe(key);
        if self.table[i].key == key {
            return false;
        }
        self.table[i] = Entry { key, slot };
        self.len += 1;
        true
    }

    fn remove(&mut self, key: u32, _slots: &[u32]) -> Option<u32> {
        if self.table.is_empty() {
            return None;
        }
        let mask = self.table.len() - 1;
        let mut hole = self.probe(key);
        if self.table[hole].key != key {
            return None;
        }
        let slot = self.table[hole].slot;
        self.len -= 1;
        // Backward-shift deletion keeps every probe sequence unbroken without tombstones.
        let mut i = hole;
        loop {
            i = (i + 1) & mask;
            let e = self.table[i];
            if e.key == EMPTY {
                break;
            }
            let home = hash_of(e.key) & mask;
            // `e` may fill the hole unless its home lies cyclically in (hole, i].
            if (i.wrapping_sub(home) & mask) >= (i.wrapping_sub(hole) & mask) {
                self.table[hole] = e;
                hole = i;
            }
        }
        self.table[hole] = VACANT;
        Some(slot)
    }

    fn rebuild(&mut self, slots: &[u32]) {
        let cap = self.table.len();
        self.rebuild_with(slots, cap);
    }

    fn clear(&mut self, _slots: &[u32]) {
        self.table.fill(VACANT);
        self.len = 0;
    }
}

/// Insertion-ordered set of `u32` over a [`SlotIndex`].
#[derive(Clone, Debug, Default)]
pub struct OrderedSlots<I> {
    /// Values in insertion order; `EMPTY` marks a deleted slot.
    slots: Vec<u32>,
    index: I,
    len: u32,
    pins: u32,
    epoch: u64,
}

/// Insertion-ordered set of `u32` with a hash index.
pub type OrderedU32Set = OrderedSlots<HashIndex>;

impl<I: SlotIndex + Default> OrderedSlots<I> {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<I: SlotIndex> OrderedSlots<I> {
    /// Creates an empty set over the given index.
    pub fn with_index(index: I) -> Self {
        Self {
            slots: Vec::new(),
            index,
            len: 0,
            pins: 0,
            epoch: 0,
        }
    }

    /// Number of live values.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `Set.prototype.has`.
    #[inline]
    pub fn contains(&self, key: u32) -> bool {
        self.index.find(key, &self.slots).is_some()
    }

    /// `Set.prototype.add`; returns true when the value was new (existing values keep their position).
    #[inline]
    pub fn insert(&mut self, key: u32) -> bool {
        if !self.index.insert(key, self.slots.len() as u32, &self.slots) {
            return false;
        }
        self.slots.push(key);
        self.len += 1;
        true
    }

    /// `Set.prototype.delete`; returns true when the value was present.
    #[inline]
    pub fn remove(&mut self, key: u32) -> bool {
        let Some(slot) = self.index.remove(key, &self.slots) else {
            return false;
        };
        self.slots[slot as usize] = EMPTY;
        self.len -= 1;
        self.maybe_compact();
        true
    }

    /// `Set.prototype.clear`. Slots are kept while a cursor is pinned so positions stay valid.
    pub fn clear(&mut self) {
        self.index.clear(&self.slots);
        self.len = 0;
        if self.pins == 0 {
            self.slots.clear();
            self.epoch += 1;
        } else {
            self.slots.fill(EMPTY);
        }
    }

    /// Starts an iteration at the first value and pins storage against compaction.
    pub fn pin_cursor(&mut self) -> Cursor {
        self.pins += 1;
        self.cursor()
    }

    /// Cursor at the first value without pinning; valid until the next compaction.
    pub fn cursor(&self) -> Cursor {
        Cursor::new(0, self.epoch)
    }

    /// Releases one pin and compacts if that is now worthwhile.
    pub fn unpin(&mut self) {
        assert!(self.pins > 0, "unpin without pin");
        self.pins -= 1;
        self.maybe_compact();
    }

    /// Advances `cursor` to the next live value; values appended since the last call are visited.
    pub fn next(&self, cursor: &mut Cursor) -> Option<u32> {
        assert_eq!(
            cursor.epoch(),
            self.epoch,
            "cursor used after compaction; pin it with pin_cursor"
        );
        while cursor.pos() < self.slots.len() {
            let v = self.slots[cursor.pos()];
            cursor.advance();
            if v != EMPTY {
                return Some(v);
            }
        }
        None
    }

    /// Iterates live values in insertion order (no mutation during the loop).
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            slots: self.slots.iter(),
            remaining: self.len(),
        }
    }

    /// Value at logical position `n` in insertion order (O(n)).
    pub fn nth(&self, n: usize) -> Option<u32> {
        self.iter().nth(n).copied()
    }

    fn maybe_compact(&mut self) {
        let dead = self.slots.len() - self.len as usize;
        if self.pins != 0 || dead < 32 || dead <= self.len as usize * I::COMPACT_FACTOR {
            return;
        }
        self.slots.retain(|&v| v != EMPTY);
        self.epoch += 1;
        self.index.rebuild(&self.slots);
    }
}

/// Iterator over the live values; its exact size hint lets `collect` allocate once.
pub struct Iter<'a> {
    slots: std::slice::Iter<'a, u32>,
    remaining: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a u32;

    fn next(&mut self) -> Option<&'a u32> {
        let found = self.slots.by_ref().find(|&&v| v != EMPTY)?;
        self.remaining -= 1;
        Some(found)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<I: SlotIndex + Default> FromIterator<u32> for OrderedSlots<I> {
    fn from_iter<T: IntoIterator<Item = u32>>(iter: T) -> Self {
        let mut s = Self::new();
        for v in iter {
            s.insert(v);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compat::OrderedSet::OrderedSet;

    #[test]
    fn follows_js_set_semantics() {
        let mut s = OrderedU32Set::new();
        for v in [5, 6, 7] {
            s.insert(v);
        }
        assert!(!s.insert(5));
        assert!(s.remove(5));
        assert!(!s.remove(5));
        s.insert(5);
        assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![6, 7, 5]);
        assert_eq!(s.nth(1), Some(7));
        let mut seen = vec![];
        let mut c = s.pin_cursor();
        while let Some(v) = s.next(&mut c) {
            seen.push(v);
            if v == 6 {
                s.insert(100);
                s.remove(7);
            }
        }
        s.unpin();
        assert_eq!(seen, vec![6, 5, 100]);
    }

    #[test]
    fn clear_during_iteration() {
        let mut s = OrderedU32Set::new();
        s.insert(1);
        s.insert(2);
        let mut c = s.pin_cursor();
        assert_eq!(s.next(&mut c), Some(1));
        s.clear();
        s.insert(5);
        assert_eq!(s.next(&mut c), Some(5));
        assert_eq!(s.next(&mut c), None);
        s.unpin();
    }

    #[test]
    fn matches_ordered_set_under_random_churn() {
        // Small xorshift so the test needs no dependency; keys collide often on purpose.
        let mut state = 0x1234_5678_9abc_def0_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut a = OrderedU32Set::new();
        let mut b = OrderedSet::<u32>::new();
        for step in 0..200_000u32 {
            let key = (next() % 700) as u32 * 64;
            match next() % 5 {
                0 | 1 => assert_eq!(a.insert(key), b.insert(key)),
                2 | 3 => assert_eq!(a.remove(key), b.remove(&key)),
                _ => assert_eq!(a.contains(key), b.contains(&key)),
            }
            if step % 20_011 == 0 {
                a.clear();
                b.clear();
            }
            assert_eq!(a.len(), b.len());
        }
        assert!(a.iter().eq(b.iter()));
    }
}

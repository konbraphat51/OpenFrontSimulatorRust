// SPDX-License-Identifier: AGPL-3.0-only
//! Insertion-ordered map with JS `Map` semantics and a cursor API for iterate-while-mutating.
//!
//! Semantics: `set` on an existing key keeps its position; delete then re-insert moves the entry
//! to the end; entries added during iteration are visited; deleted entries not yet visited are
//! skipped. Entries live in a slot vector with tombstones; compaction only runs while no cursor
//! is pinned and bumps an epoch so a stale cursor is detected.

use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

/// Fast non-cryptographic hasher (FxHash); iteration order never depends on it.
#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

const FX_SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(FX_SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(buf));
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// Build hasher for [`FxHasher`].
pub type FxBuildHasher = BuildHasherDefault<FxHasher>;

/// `HashMap` with [`FxHasher`]; for lookup-only maps whose iteration order is never observable.
pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

/// `HashSet` with [`FxHasher`]; for lookup-only sets whose iteration order is never observable.
pub type FxHashSet<T> = std::collections::HashSet<T, FxBuildHasher>;

/// Position in an ordered collection; stays valid across insertions and deletions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pos: usize,
    epoch: u64,
}

impl Cursor {
    /// Cursor at slot `pos` of the storage generation `epoch`.
    pub(crate) fn new(pos: usize, epoch: u64) -> Self {
        Self { pos, epoch }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn advance(&mut self) {
        self.pos += 1;
    }
}

/// Insertion-ordered map with JS `Map` iteration semantics.
#[derive(Clone, Debug)]
pub struct OrderedMap<K, V, S = FxBuildHasher> {
    slots: Vec<Option<(K, V)>>,
    index: HashMap<K, u32, S>,
    pins: u32,
    epoch: u64,
}

impl<K, V, S: Default> Default for OrderedMap<K, V, S> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            index: HashMap::default(),
            pins: 0,
            epoch: 0,
        }
    }
}

impl<K: Hash + Eq + Clone, V, S: BuildHasher + Default> OrderedMap<K, V, S> {
    /// Creates an empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live entries (`Map.prototype.size`).
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// True when there are no live entries.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// `Map.prototype.has`.
    pub fn contains_key(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    /// `Map.prototype.get`.
    pub fn get(&self, key: &K) -> Option<&V> {
        let &i = self.index.get(key)?;
        self.slots[i as usize].as_ref().map(|(_, v)| v)
    }

    /// Mutable `Map.prototype.get`.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let &i = self.index.get(key)?;
        self.slots[i as usize].as_mut().map(|(_, v)| v)
    }

    /// `Map.prototype.set`: replaces the value in place or appends a new entry. Returns the old value.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        use std::collections::hash_map::Entry;
        match self.index.entry(key.clone()) {
            Entry::Occupied(e) => {
                let slot = self.slots[*e.get() as usize]
                    .as_mut()
                    .expect("indexed slot is live");
                Some(std::mem::replace(&mut slot.1, value))
            }
            Entry::Vacant(e) => {
                e.insert(self.slots.len() as u32);
                self.slots.push(Some((key, value)));
                None
            }
        }
    }

    /// `Map.prototype.delete`: removes the entry and returns its value.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let i = self.index.remove(key)?;
        let (_, v) = self.slots[i as usize].take().expect("indexed slot is live");
        self.maybe_compact();
        Some(v)
    }

    /// `Map.prototype.clear`. Slots are kept while a cursor is pinned so positions stay valid.
    pub fn clear(&mut self) {
        self.index.clear();
        if self.pins == 0 {
            self.slots.clear();
            self.epoch += 1;
        } else {
            for s in &mut self.slots {
                *s = None;
            }
        }
    }

    /// Starts an iteration at the first entry and pins storage against compaction.
    ///
    /// Every `pin_cursor` must be matched by one [`Self::unpin`].
    pub fn pin_cursor(&mut self) -> Cursor {
        self.pins += 1;
        Cursor {
            pos: 0,
            epoch: self.epoch,
        }
    }

    /// Cursor at the first entry without pinning; valid until the next compaction.
    pub fn cursor(&self) -> Cursor {
        Cursor {
            pos: 0,
            epoch: self.epoch,
        }
    }

    /// Releases one pin and compacts if that is now worthwhile.
    pub fn unpin(&mut self) {
        assert!(self.pins > 0, "unpin without pin");
        self.pins -= 1;
        self.maybe_compact();
    }

    /// Advances `cursor` to the next live entry and returns it, or `None` at the end.
    ///
    /// Entries appended since the last call are visited; deleted ones are skipped.
    pub fn next(&self, cursor: &mut Cursor) -> Option<(&K, &V)> {
        assert_eq!(
            cursor.epoch, self.epoch,
            "cursor used after compaction; pin it with pin_cursor"
        );
        while cursor.pos < self.slots.len() {
            let i = cursor.pos;
            cursor.pos += 1;
            if let Some((k, v)) = &self.slots[i] {
                return Some((k, v));
            }
        }
        None
    }

    /// Like [`Self::next`] but yields a cloned key, so the caller may mutate the map afterwards.
    pub fn next_key(&self, cursor: &mut Cursor) -> Option<K> {
        self.next(cursor).map(|(k, _)| k.clone())
    }

    /// Iterates live entries in insertion order (no mutation during the loop).
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref().map(|(k, v)| (k, v)))
    }

    /// Iterates keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// Iterates values in insertion order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// Iterates values mutably in insertion order.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.slots
            .iter_mut()
            .filter_map(|s| s.as_mut().map(|(_, v)| v))
    }

    /// Key at logical position `n` (O(n)), for `randFromSet`-style picks.
    pub fn nth_key(&self, n: usize) -> Option<&K> {
        self.keys().nth(n)
    }

    fn maybe_compact(&mut self) {
        let dead = self.slots.len() - self.index.len();
        if self.pins != 0 || dead < 32 || dead <= self.index.len() {
            return;
        }
        self.slots.retain(|s| s.is_some());
        self.epoch += 1;
        for (i, s) in self.slots.iter().enumerate() {
            let (k, _) = s.as_ref().expect("retained slots are live");
            *self.index.get_mut(k).expect("live key is indexed") = i as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Map = OrderedMap<u32, &'static str>;

    #[test]
    fn delete_and_readd_moves_to_end() {
        let mut m = Map::new();
        m.insert(1, "a");
        m.insert(2, "b");
        m.insert(3, "c");
        m.remove(&1);
        m.insert(1, "a2");
        m.insert(2, "b2");
        let all: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(all, vec![(2, "b2"), (3, "c"), (1, "a2")]);
    }

    #[test]
    fn additions_during_iteration_are_visited_and_deleted_skipped() {
        // JS: for (k of m) { if (k==1) { m.delete(2); m.set(9) } }
        let mut m = Map::new();
        m.insert(1, "a");
        m.insert(2, "b");
        m.insert(3, "c");
        let mut seen = Vec::new();
        let mut cur = m.pin_cursor();
        while let Some(k) = m.next_key(&mut cur) {
            seen.push(k);
            if k == 1 {
                m.remove(&2);
                m.insert(9, "z");
            }
            if k == 3 {
                m.remove(&1);
                m.insert(1, "again");
            }
        }
        m.unpin();
        assert_eq!(seen, vec![1, 3, 9, 1]);
    }

    #[test]
    fn compaction_deferred_while_pinned() {
        let mut m = OrderedMap::<u32, u32>::new();
        for i in 0..200 {
            m.insert(i, i);
        }
        let mut cur = m.pin_cursor();
        let mut seen = 0;
        while let Some(k) = m.next_key(&mut cur) {
            seen += 1;
            if k == 0 {
                for j in 1..190 {
                    m.remove(&j);
                }
            }
        }
        assert_eq!(seen, 11);
        m.unpin();
        assert_eq!(m.len(), 11);
        assert_eq!(m.slots.len(), 11);
        assert_eq!(m.get(&199), Some(&199));
    }

    #[test]
    fn clear_during_iteration() {
        let mut m = Map::new();
        m.insert(1, "a");
        m.insert(2, "b");
        let mut cur = m.pin_cursor();
        assert_eq!(m.next_key(&mut cur), Some(1));
        m.clear();
        m.insert(5, "e");
        assert_eq!(m.next_key(&mut cur), Some(5));
        assert_eq!(m.next_key(&mut cur), None);
        m.unpin();
    }

    #[test]
    fn set_existing_keeps_position() {
        let mut m = Map::new();
        m.insert(1, "a");
        m.insert(2, "b");
        assert_eq!(m.insert(1, "c"), Some("a"));
        let keys: Vec<_> = m.keys().copied().collect();
        assert_eq!(keys, vec![1, 2]);
    }

    #[test]
    fn amortised_churn_stays_small() {
        let mut m = OrderedMap::<u32, ()>::new();
        for i in 0..100_000u32 {
            m.insert(i, ());
            if i >= 10 {
                m.remove(&(i - 10));
            }
        }
        assert_eq!(m.len(), 10);
        assert!(m.slots.len() < 100);
    }
}

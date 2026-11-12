//! Insertion-ordered set with JS `Set` semantics, built on [`OrderedMap`](super::OrderedMap::OrderedMap).

use super::OrderedMap::{Cursor, FxBuildHasher, OrderedMap};
use std::hash::{BuildHasher, Hash};

/// Insertion-ordered set: delete + re-add moves to the end, additions during iteration are visited.
#[derive(Clone, Debug)]
pub struct OrderedSet<T, S = FxBuildHasher> {
    map: OrderedMap<T, (), S>,
}

impl<T, S: Default> Default for OrderedSet<T, S> {
    fn default() -> Self {
        Self {
            map: OrderedMap::default(),
        }
    }
}

impl<T: Hash + Eq + Clone, S: BuildHasher + Default> OrderedSet<T, S> {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Set.prototype.size`.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `Set.prototype.has`.
    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    /// `Set.prototype.add`; returns true when the value was new (existing values keep their position).
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_none()
    }

    /// `Set.prototype.delete`; returns true when the value was present.
    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    /// `Set.prototype.clear`.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Starts an iteration and pins storage; pair with [`Self::unpin`].
    pub fn pin_cursor(&mut self) -> Cursor {
        self.map.pin_cursor()
    }

    /// Cursor at the start without pinning; valid until the next compaction.
    pub fn cursor(&self) -> Cursor {
        self.map.cursor()
    }

    /// Releases a pin taken by [`Self::pin_cursor`].
    pub fn unpin(&mut self) {
        self.map.unpin();
    }

    /// Next live value at or after the cursor, cloned so the caller may mutate the set.
    pub fn next(&self, cursor: &mut Cursor) -> Option<T> {
        self.map.next_key(cursor)
    }

    /// Iterates values in insertion order (no mutation during the loop).
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.map.keys()
    }

    /// Value at logical position `n` in insertion order (O(n)); used by `randFromSet`.
    pub fn nth(&self, n: usize) -> Option<&T> {
        self.map.nth_key(n)
    }
}

impl<T: Hash + Eq + Clone, S: BuildHasher + Default> FromIterator<T> for OrderedSet<T, S> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
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

    #[test]
    fn js_set_semantics() {
        let mut s = OrderedSet::<u32>::new();
        for v in [5, 6, 7] {
            s.insert(v);
        }
        s.insert(5);
        s.remove(&5);
        s.insert(5);
        assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![6, 7, 5]);
        assert_eq!(s.nth(1), Some(&7));
        let mut seen = vec![];
        let mut c = s.pin_cursor();
        while let Some(v) = s.next(&mut c) {
            seen.push(v);
            if v == 6 {
                s.insert(100);
                s.remove(&7);
            }
        }
        s.unpin();
        assert_eq!(seen, vec![6, 5, 100]);
    }
}

use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    mem::ManuallyDrop,
};

use rand::{Rng, SeedableRng, rngs::SmallRng};

use crate::NonEmptyStorage;

/// An ordered map backed by a skiplist.
pub struct SkipMap<K, V, R, const N: usize>(Option<NonEmptyStorage<Entry<K, V>, R, N>>)
where
    R: Rng;

impl<K, V, const N: usize> Default for SkipMap<K, V, SmallRng, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V, R, const N: usize> fmt::Debug for SkipMap<K, V, R, N>
where
    R: Rng,
    Entry<K, V>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(storage) = &self.0 {
            write!(f, "{storage:?}")
        } else {
            write!(f, "SkipMap(None)")
        }
    }
}

impl<K, V, const N: usize> SkipMap<K, V, SmallRng, N> {
    /// Creates an empty skipmap.
    #[must_use]
    pub const fn new() -> Self {
        Self(None)
    }
}

impl<K, V, R, const N: usize> SkipMap<K, V, R, N>
where
    R: Rng,
    Entry<K, V>: Ord,
{
    /// Returns whether a key exists in the skipmap.
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        Q: Ord + ?Sized,
        Entry<K, V>: Borrow<Q>,
    {
        let Some(storage) = &self.0 else {
            return false;
        };
        storage.get(key).is_some()
    }

    /// Returns a shared reference to the value associated with the given key.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        Q: Ord + ?Sized,
        Entry<K, V>: Borrow<Q>,
    {
        let Some(storage) = &self.0 else {
            return None;
        };
        storage.get(key).map(|e| &e.value)
    }

    /// Inserts a value at the given key into the skipmap.
    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        R: SeedableRng,
    {
        let Some(storage) = &mut self.0 else {
            self.0 = Some(NonEmptyStorage::new(Entry { key, value }));
            return None;
        };
        storage.upsert(Entry { key, value }).map(|e| e.value)
    }

    /// Removes a value at the given key from the skipmap, returning it if it exists.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        Q: Ord + ?Sized,
        Entry<K, V>: Borrow<Q>,
    {
        let storage = self.0.take()?;
        let (storage, entry) = NonEmptyStorage::remove(ManuallyDrop::new(storage), key);
        self.0 = storage;
        entry.map(|e| e.value)
    }
}

pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> Borrow<K> for Entry<K, V> {
    fn borrow(&self) -> &K {
        &self.key
    }
}

impl<K, V> Hash for Entry<K, V>
where
    K: Hash,
{
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.key.hash(state);
    }
}

impl<K, V> Eq for Entry<K, V> where K: Eq {}

impl<K, V> PartialEq for Entry<K, V>
where
    K: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}

impl<K, V> Ord for Entry<K, V>
where
    K: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

impl<K, V> PartialOrd for Entry<K, V>
where
    K: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.key.partial_cmp(&other.key)
    }
}

impl<K, V> fmt::Debug for Entry<K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("")
            .field(&self.key)
            .field(&self.value)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use proptest::{collection::btree_map, prelude::*};

    use super::SkipMap;

    proptest! {
        #[cfg_attr(miri, ignore)]
        #[test]
        fn test_insert_get(items in btree_map(any::<usize>(), any::<usize>(), 1000)) {
            let mut skipmap = SkipMap::<usize, usize, _, 32>::new();
            for (k, v) in &items {
                assert!(skipmap.insert(*k, *v).is_none());
            }
            for (k, v)in items.iter().rev() {
                assert!(skipmap.get(k).is_some_and(|x| x == v));
            }
        }

        #[test]
        fn test_insert_get_small(items in btree_map(any::<usize>(), any::<usize>(), 8)) {
            let mut skipmap = SkipMap::<usize, usize, _, 4>::new();
            for (k, v) in &items {
                assert!(skipmap.insert(*k, *v).is_none());
            }
            for (k, v)in items.iter().rev() {
                assert!(skipmap.get(k).is_some_and(|x| x == v));
            }
        }

        #[cfg_attr(miri, ignore)]
        #[test]
        fn test_insert_remove(items in btree_map(any::<usize>(), any::<usize>(), 1000)) {
            let mut skipmap = SkipMap::<usize, usize, _, 32>::new();
            for (k, v) in &items {
                assert!(skipmap.insert(*k, *v).is_none());
            }
            for (k, v) in items.iter().rev() {
                assert!(skipmap.remove(k).is_some_and(|x| x == *v));
            }
        }

        #[test]
        fn test_insert_remove_small(items in btree_map(any::<usize>(), any::<usize>(), 8)) {
            let mut skipmap = SkipMap::<usize, usize, _, 32>::new();
            for (k, v) in &items {
                assert!(skipmap.insert(*k, *v).is_none());
            }
            for (k, v) in items.iter().rev() {
                assert!(skipmap.remove(k).is_some_and(|x| x == *v));
            }
        }

        #[cfg_attr(miri, ignore)]
        #[test]
        fn test_insert_duplicates(items in btree_map(any::<usize>(), any::<usize>(), 1000)) {
            let mut skipmap = SkipMap::<usize, usize, _, 32>::new();
            for (k, v) in &items {
                assert!(skipmap.insert(*k, *v).is_none());
            }
            for (k, v) in items.iter().rev() {
                assert!(skipmap.insert(*k, 0).is_some_and(|x| x == *v));
            }
        }

        #[test]
        fn test_insert_duplicates_small(items in btree_map(any::<usize>(), any::<usize>(), 8)) {
            let mut skipmap = SkipMap::<usize, usize, _, 32>::new();
            for (k, v) in &items {
                assert!(skipmap.insert(*k, *v).is_none());
            }
            for (k, v) in items.iter().rev() {
                assert!(skipmap.insert(*k, 0).is_some_and(|x| x == *v));
            }
        }
    }
}

use std::{borrow::Borrow, fmt, mem::ManuallyDrop};

use rand::{Rng, SeedableRng, rngs::SmallRng};

use crate::NonEmptyStorage;

/// A skiplist.
pub struct SkipList<T, R, const N: usize>(Option<NonEmptyStorage<T, R, N>>)
where
    R: Rng;

impl<T, const N: usize> Default for SkipList<T, SmallRng, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, R, const N: usize> fmt::Debug for SkipList<T, R, N>
where
    T: fmt::Debug,
    R: Rng,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(storage) = &self.0 {
            write!(f, "{storage:?}")
        } else {
            write!(f, "SkipList(None)")
        }
    }
}

impl<T, const N: usize> SkipList<T, SmallRng, N> {
    /// Creates an empty skiplist.
    #[must_use]
    pub const fn new() -> Self {
        Self(None)
    }
}

impl<T, R, const N: usize> SkipList<T, R, N>
where
    T: Ord,
    R: Rng,
{
    /// Returns whether a value exists in the skiplist.
    pub fn contains<U>(&self, value: &U) -> bool
    where
        T: Borrow<U>,
        U: Ord + ?Sized,
    {
        let Some(storage) = &self.0 else {
            return false;
        };
        storage.get(value).is_some()
    }

    /// Inserts a value into the skiplist.
    pub fn insert(&mut self, value: T)
    where
        R: SeedableRng,
    {
        let Some(storage) = &mut self.0 else {
            self.0 = Some(NonEmptyStorage::new(value));
            return;
        };
        storage.insert(value);
    }

    /// Removes a value from the skiplist, returning it if it exists.
    pub fn remove<U>(&mut self, value: &U) -> Option<T>
    where
        T: Borrow<U>,
        U: Ord + ?Sized,
    {
        let storage = self.0.take()?;
        let (storage, value) = NonEmptyStorage::remove(ManuallyDrop::new(storage), value);
        self.0 = storage;
        value
    }
}

#[cfg(test)]
mod tests {
    use proptest::{collection::vec, prelude::*};

    use super::SkipList;

    proptest! {
        #[cfg_attr(miri, ignore)]
        #[test]
        fn test_insert_contains(items in vec(any::<usize>(), 1000)) {
            let mut skiplist = SkipList::<usize, _, 32>::new();
            for item in &items {
                skiplist.insert(*item);
            }
            for item in items.iter().rev() {
                assert!(skiplist.contains(item));
            }
        }

        #[test]
        fn test_insert_contains_small(items in vec(any::<usize>(), 8)) {
            let mut skiplist = SkipList::<usize, _, 4>::new();
            for item in &items {
                skiplist.insert(*item);
            }
            for item in items.iter().rev() {
                assert!(skiplist.contains(item));
            }
        }

        #[cfg_attr(miri, ignore)]
        #[test]
        fn test_insert_remove(items in vec(any::<usize>(), 1000)) {
            let mut skiplist = SkipList::<usize, _, 32>::new();
            for item in &items {
                skiplist.insert(*item);
            }
            for item in items.iter().rev() {
                assert!(skiplist.remove(item).is_some_and(|v| v == *item));
            }
        }

        #[test]
        fn test_insert_remove_small(items in vec(any::<usize>(), 8)) {
            let mut skiplist = SkipList::<usize, _, 4>::new();
            for item in &items {
                skiplist.insert(*item);
            }
            for item in items.iter().rev() {
                assert!(skiplist.remove(item).is_some_and(|v| v == *item));
            }
        }
    }
}

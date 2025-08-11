//! A skiplist implementation.

#![warn(
    rustdoc::all,
    clippy::cargo,
    clippy::pedantic,
    clippy::nursery,
    missing_debug_implementations
)]
#![deny(clippy::all, missing_docs, rust_2018_idioms, rust_2021_compatibility)]

use std::{borrow::Borrow, fmt, mem::MaybeUninit, num::NonZeroUsize, ptr::NonNull};

use rand::{Rng, SeedableRng, rngs::SmallRng};

/// An enumeration of all possible outcomes resulted from removing an item from
/// the skiplist.
enum Removal<T> {
    // No item was removed.
    None,
    // A single item was removed.
    Some(T),
    // A single item was removed, leaving the skiplist empty.
    Last(T),
}

/// A skiplist.
#[derive(Debug)]
pub struct SkipList<T, R, const N: usize>(Option<NonEmptySkipList<T, R, N>>)
where
    R: Rng;

impl<T, R, const N: usize> Drop for SkipList<T, R, N>
where
    R: Rng,
{
    fn drop(&mut self) {
        if let Some(mut curr_ptr) = self.0.take().map(|l| l.head) {
            loop {
                let next_ptr = {
                    let curr = unsafe { curr_ptr.as_ref() };
                    curr.nexts[0]
                };
                unsafe {
                    SkipNode::dealloc(curr_ptr);
                }
                let Some(next_ptr) = next_ptr else {
                    break;
                };
                curr_ptr = next_ptr;
            }
        }
    }
}

impl<T, const N: usize> Default for SkipList<T, SmallRng, N> {
    fn default() -> Self {
        Self::new()
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
    R: Rng,
{
    /// Inserts a value into the skiplist.
    pub fn insert(&mut self, val: T)
    where
        T: Ord + fmt::Debug,
        R: SeedableRng,
    {
        let Some(skiplist) = &mut self.0 else {
            self.0 = Some(NonEmptySkipList::new(val));
            return;
        };
        skiplist.insert(val);
    }

    /// Removes a value from the skiplist, returning it if it exists.
    pub fn remove<U>(&mut self, val: &U) -> Option<T>
    where
        T: Ord + Borrow<U>,
        U: Ord,
    {
        let Some(skiplist) = &mut self.0 else {
            return None;
        };
        match skiplist.remove(val) {
            Removal::None => None,
            Removal::Some(val) => Some(val),
            Removal::Last(val) => {
                self.0 = None;
                Some(val)
            }
        }
    }

    /// Returns whether a value exists in the skiplist.
    pub fn contains<U>(&self, val: &U) -> bool
    where
        T: Ord + Borrow<U>,
        U: Ord,
    {
        let Some(skiplist) = &self.0 else {
            return false;
        };
        skiplist.contains(val)
    }
}

struct NonEmptySkipList<T, R: Rng, const N: usize> {
    rng: R,
    head: NonNull<SkipNode<T, N>>,
    levels: NonZeroUsize,
}

impl<T, R, const N: usize> fmt::Debug for NonEmptySkipList<T, R, N>
where
    T: fmt::Debug,
    R: Rng,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for level in 0..self.levels.get() {
            write!(f, "[")?;
            let mut curr_ptr = self.head;
            loop {
                let curr = unsafe { curr_ptr.as_ref() };
                write!(f, "{:?} ({:#p})", curr.val, curr_ptr)?;
                let Some(next_ptr) = curr.nexts[level] else {
                    break;
                };
                write!(f, ", ")?;
                curr_ptr = next_ptr;
            }
            if level >= self.levels.get() - 1 {
                write!(f, "]")?;
            } else {
                writeln!(f, "]")?;
            }
        }
        Ok(())
    }
}

impl<T, R, const N: usize> NonEmptySkipList<T, R, N>
where
    R: Rng,
{
    fn new(val: T) -> Self
    where
        R: SeedableRng,
    {
        Self {
            rng: R::from_os_rng(),
            head: SkipNode::new(val).alloc(),
            levels: NonZeroUsize::MIN,
        }
    }

    fn insert(&mut self, val: T)
    where
        T: Ord + fmt::Debug,
    {
        {
            let head = unsafe { self.head.as_ref() };
            if head.val > val {
                // Adds the existing head as the next node of the new head at
                // every level.
                let mut head = SkipNode::new(val);
                for next in &mut head.nexts[..self.levels.get()] {
                    *next = Some(self.head);
                }
                // Replaces the skiplist's head when the current head's value is
                // greater than the inserted value.
                self.head = head.alloc();
                return;
            }
        }
        // Traverses the skiplist and searches for the value, while tracking the
        // nodes that might get updated due to the insertion.
        let mut trace = [MaybeUninit::uninit(); N];
        self.descend(&val, |level, ptr| {
            trace[level].write(ptr);
        });
        // Adds the new node to the base level.
        let mut curr_ptr = SkipNode::new(val).alloc();
        let curr = unsafe { curr_ptr.as_mut() };
        {
            let prev = unsafe { trace[0].assume_init_mut().as_mut() };
            curr.nexts[0] = prev.nexts[0];
            prev.nexts[0] = Some(curr_ptr);
        }
        // Determines whether a node is added to a level based on the number of
        // consecutive one bits in the representation of a random number.
        let random: u64 = self.rng.random();
        for (level, mut prev_ptr) in trace
            .into_iter()
            .enumerate()
            // Attempts to go to one level higher than the current level.
            .take(self.levels.saturating_add(1).get().min(N))
            // Skips the base level.
            .skip(1)
        {
            // The chance to get added to a level drops by half when getting
            // to a higher level.
            if random & (1 << level) == 0 {
                break;
            }
            let prev = if level >= self.levels.get() {
                // Increases the current number of levels and uses the current
                // head as the "previous" node. This ensures the head can skip
                // to the new node in the new level.
                self.levels = self.levels.saturating_add(1);
                unsafe { self.head.as_mut() }
            } else {
                unsafe { prev_ptr.assume_init_mut().as_mut() }
            };
            // Adds the new node to the current level.
            curr.nexts[level] = prev.nexts[level];
            prev.nexts[level] = Some(curr_ptr);
        }
    }

    fn remove<U>(&mut self, val: &U) -> Removal<T>
    where
        T: Ord + Borrow<U>,
        U: Ord,
    {
        {
            let head = unsafe { self.head.as_ref() };
            if head.val.borrow() > val {
                return Removal::None;
            }
            if head.val.borrow() == val {
                let Some(mut head_ptr) = head.nexts[0] else {
                    // The head gets removed and there's no next node,
                    // resulting in an empty skiplist.
                    let val = unsafe { SkipNode::dealloc(self.head) };
                    return Removal::Last(val);
                };
                // Adds the next head node to higher levels when it's not
                // already added.
                let next_head = unsafe { head_ptr.as_mut() };
                for level in (1..self.levels.get()).rev() {
                    if next_head.nexts[level].is_some() {
                        break;
                    }
                    next_head.nexts[level] = head.nexts[level];
                }
                let val = unsafe { SkipNode::dealloc(self.head) };
                self.head = head_ptr;
                return Removal::Some(val);
            }
        }
        // Traverses the skiplist and searches for the value, while tracking the
        // nodes that might get updated due to the removal.
        let mut trace = [MaybeUninit::uninit(); N];
        self.descend(val, |level, ptr| {
            trace[level].write(ptr);
        });
        // Checks if the value exists. The trace only includes upto the node
        // right before the one that will potentially be removed.
        let Some(curr_ptr) = ({
            let prev = unsafe { trace[0].assume_init_ref().as_ref() };
            prev.nexts[0]
        }) else {
            return Removal::None;
        };
        {
            let curr = unsafe { curr_ptr.as_ref() };
            if curr.val.borrow() != val {
                return Removal::None;
            }
            // Removes the node at every level.
            for (level, mut prev_ptr) in trace.into_iter().enumerate().take(self.levels.get()) {
                let prev = unsafe { prev_ptr.assume_init_mut().as_mut() };
                if prev.nexts[level].is_none_or(|ptr| ptr != curr_ptr) {
                    break;
                }
                prev.nexts[level] = curr.nexts[level];
            }
        }
        // Updates the skiplist's level by counting the number of next pointers
        // that was removed from the head.
        let head = unsafe { self.head.as_mut() };
        while self.levels.get() > 1 && head.nexts[self.levels.get() - 1].is_none() {
            self.levels = unsafe { NonZeroUsize::new_unchecked(self.levels.get() - 1) };
        }
        let val = unsafe { SkipNode::dealloc(curr_ptr) };
        Removal::Some(val)
    }

    fn contains<U>(&self, val: &U) -> bool
    where
        T: Ord + Borrow<U>,
        U: Ord,
    {
        {
            let head = unsafe { self.head.as_ref() };
            if head.val.borrow() > val {
                return false;
            }
            if head.val.borrow() == val {
                return true;
            }
        }
        // Traverses the skiplist and searches for the value.
        let mut prev_ptr = self.head;
        self.descend(val, |_, ptr| prev_ptr = ptr);
        // Checks if the value exists. The trace only includes upto the node
        // right before the one that will potentially be matched.
        let Some(curr_ptr) = ({
            let prev = unsafe { prev_ptr.as_ref() };
            prev.nexts[0]
        }) else {
            return false;
        };
        let curr = unsafe { curr_ptr.as_ref() };
        curr.val.borrow() == val
    }

    /// Traverses the skiplist, descending down all levels, and calling the
    /// given function on the last encountered node at each level.
    fn descend<U, V>(&self, val: &U, mut visit: V)
    where
        T: Ord + Borrow<U>,
        U: Ord,
        V: FnMut(usize, NonNull<SkipNode<T, N>>),
    {
        let mut prev_node_ptr = self.head;
        for level in (0..self.levels.get()).rev() {
            while let Some(curr_node_ptr) = {
                let prev_node = unsafe { prev_node_ptr.as_ref() };
                prev_node.nexts[level]
            } && {
                let curr_node = unsafe { curr_node_ptr.as_ref() };
                curr_node.val.borrow() < val
            } {
                prev_node_ptr = curr_node_ptr;
            }
            visit(level, prev_node_ptr);
        }
    }
}

#[derive(Debug)]
struct SkipNode<T, const N: usize> {
    val: T,
    nexts: [Option<NonNull<Self>>; N],
}

impl<T, const N: usize> SkipNode<T, N> {
    const fn new(val: T) -> Self {
        Self {
            val,
            nexts: [None; N],
        }
    }

    fn alloc(self) -> NonNull<Self> {
        let ptr = Box::into_raw(Box::new(self));
        unsafe { NonNull::new_unchecked(ptr) }
    }

    unsafe fn dealloc(ptr: NonNull<Self>) -> T {
        let node = unsafe { Box::from_raw(ptr.as_ptr()) };
        node.val
    }
}

#[cfg(test)]
mod tests {
    use rand::{Rng, SeedableRng, rngs::SmallRng, seq::SliceRandom};

    use crate::SkipList;

    #[test]
    fn it_works() {
        let mut skiplist = SkipList::<usize, _, 4>::new();
        let items = [10, 5, 7, 3, 8, 2];
        for item in &items {
            assert!(!skiplist.contains(item));
        }
        for item in &items {
            skiplist.insert(*item);
        }
        for item in &items {
            assert!(skiplist.contains(item));
        }
        for item in &items {
            assert!(skiplist.remove(item).is_some_and(|v| v == *item));
        }
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn random() {
        const COUNT: usize = 1_000_000;

        let mut rng = SmallRng::from_os_rng();
        let mut items: Vec<u32> = (&mut rng).random_iter().take(COUNT).collect();

        let mut skiplist = SkipList::<usize, _, 32>::new();
        for item in &items {
            skiplist.insert(*item as usize);
        }

        items.shuffle(&mut rng);
        for item in &items {
            assert!(skiplist.contains(&(*item as usize)));
        }

        items.shuffle(&mut rng);
        for item in &items {
            let item = *item as usize;
            assert!(skiplist.remove(&item).is_some_and(|v| v == item));
        }
    }
}

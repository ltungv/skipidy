//! Implementations of ordered collections backed by skiplists.

#![warn(
    rustdoc::all,
    clippy::cargo,
    clippy::pedantic,
    clippy::nursery,
    missing_debug_implementations
)]
#![deny(clippy::all, missing_docs, rust_2018_idioms, rust_2021_compatibility)]

mod skiplist;
mod skipmap;

use std::{
    borrow::Borrow,
    cmp, fmt,
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZeroUsize,
    ptr::NonNull,
};

use rand::{Rng, SeedableRng};

pub use skiplist::SkipList;
pub use skipmap::SkipMap;

struct NonEmptyStorage<T, R: Rng, const N: usize> {
    rng: R,
    head: NonNull<SkipNode<T, N>>,
    levels: NonZeroUsize,
}

impl<T, R, const N: usize> Drop for NonEmptyStorage<T, R, N>
where
    R: Rng,
{
    fn drop(&mut self) {
        let mut curr_ptr = self.head;
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

impl<T, R, const N: usize> fmt::Debug for NonEmptyStorage<T, R, N>
where
    T: fmt::Debug,
    R: Rng,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for level in (0..self.levels.get()).rev() {
            write!(f, "[")?;
            let mut curr_ptr = self.head;
            loop {
                let curr = unsafe { curr_ptr.as_ref() };
                write!(f, "{:?} ({:#p})", curr.value, curr_ptr)?;
                let Some(next_ptr) = curr.nexts[level] else {
                    break;
                };
                write!(f, ", ")?;
                curr_ptr = next_ptr;
            }
            if level == 0 {
                write!(f, "]")?;
            } else {
                writeln!(f, "]")?;
            }
        }
        Ok(())
    }
}

impl<T, R, const N: usize> NonEmptyStorage<T, R, N>
where
    R: Rng + SeedableRng,
{
    fn new(value: T) -> Self {
        Self {
            rng: R::from_os_rng(),
            head: SkipNode::new(value).alloc(),
            levels: NonZeroUsize::MIN,
        }
    }
}

impl<T, R, const N: usize> NonEmptyStorage<T, R, N>
where
    T: Ord,
    R: Rng,
{
    fn get<'t, U>(&'t self, value: &U) -> Option<&'t T>
    where
        T: Borrow<U>,
        U: Ord + ?Sized,
    {
        match self.head_cmp(value) {
            cmp::Ordering::Greater => None,
            cmp::Ordering::Equal => {
                let head = unsafe { self.head.as_ref() };
                Some(&head.value)
            }
            cmp::Ordering::Less => {
                // Traverses the storage and searches for the value.
                let mut prev_ptr = self.head;
                self.descend(value, |_, ptr| prev_ptr = ptr);
                // Checks if the value exists. The trace only includes upto the node right before
                // the one that will potentially be matched.
                let curr_ptr = {
                    let prev = unsafe { prev_ptr.as_ref() };
                    prev.nexts[0]?
                };
                let curr = unsafe { curr_ptr.as_ref() };
                (curr.value.borrow() == value).then_some(&curr.value)
            }
        }
    }

    fn upsert(&mut self, value: T) -> Option<T> {
        match self.head_cmp(&value) {
            cmp::Ordering::Greater => {
                self.insert_head(value);
                None
            }
            cmp::Ordering::Equal => {
                let head = unsafe { self.head.as_mut() };
                Some(std::mem::replace(&mut head.value, value))
            }
            cmp::Ordering::Less => {
                // Traverses the storage and searches for the value, while tracking the nodes that
                // might get updated due to the insertion.
                let mut trace = [MaybeUninit::uninit(); N];
                self.descend(&value, |level, ptr| {
                    trace[level].write(ptr);
                });
                {
                    let prev = unsafe { trace[0].assume_init_mut().as_mut() };
                    if let Some(mut curr_ptr) = prev.nexts[0] {
                        let curr = unsafe { curr_ptr.as_mut() };
                        if curr.value == value {
                            return Some(std::mem::replace(&mut curr.value, value));
                        }
                    }
                };
                self.insert_after(trace, value);
                None
            }
        }
    }

    fn insert(&mut self, value: T) {
        match self.head_cmp(&value) {
            cmp::Ordering::Greater | cmp::Ordering::Equal => {
                self.insert_head(value);
            }
            cmp::Ordering::Less => {
                // Traverses the storage and searches for the value, while tracking the nodes that
                // might get updated due to the insertion.
                let mut trace = [MaybeUninit::uninit(); N];
                self.descend(&value, |level, ptr| {
                    trace[level].write(ptr);
                });
                self.insert_after(trace, value);
            }
        }
    }

    fn insert_head(&mut self, value: T) {
        // Adds the existing head's next nodes as the next nodes of the new head at every level.
        let mut new_head = SkipNode::new(value);
        new_head.nexts[0] = Some(self.head);
        let old_head = unsafe { self.head.as_mut() };
        for level in 1..self.levels.get() {
            new_head.nexts[level] = old_head.nexts[level].take();
        }
        // Replaces the storage's head when the current head's value is greater than the
        // inserted value.
        self.head = new_head.alloc();
    }

    fn insert_after(&mut self, mut trace: [MaybeUninit<NonNull<SkipNode<T, N>>>; N], value: T) {
        // Adds the new node to the base level.
        let mut curr_ptr = SkipNode::new(value).alloc();
        let curr = unsafe { curr_ptr.as_mut() };
        {
            let prev = unsafe { trace[0].assume_init_mut().as_mut() };
            curr.nexts[0] = prev.nexts[0];
            prev.nexts[0] = Some(curr_ptr);
        }
        // Determines whether a node is added to a level based on the number of consecutive one
        // bits in the representation of a random number.
        let random: u64 = self.rng.random();
        for (level, mut prev_ptr) in trace
            .into_iter()
            .enumerate()
            // Attempts to go to one level higher than the current level.
            .take(self.levels.saturating_add(1).get().min(N))
            // Skips the base level.
            .skip(1)
        {
            // The chance to get added to a level drops by half when getting to a higher level.
            if random & (1 << level) == 0 {
                break;
            }
            let prev = if level >= self.levels.get() {
                // Increases the current number of levels and uses the current head as the
                // "previous" node. This ensures the head can skip to the new node.
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

    fn remove<U>(mut storage: ManuallyDrop<Self>, value: &U) -> (Option<Self>, Option<T>)
    where
        T: Borrow<U>,
        U: Ord + ?Sized,
    {
        let value = match storage.head_cmp(value) {
            cmp::Ordering::Greater => return (Some(ManuallyDrop::into_inner(storage)), None),
            cmp::Ordering::Equal => {
                let head = unsafe { storage.head.as_ref() };
                let old_head_ptr = if let Some(new_head_ptr) = head.nexts[0] {
                    std::mem::replace(&mut storage.head, new_head_ptr)
                } else {
                    // The head gets removed and there's no next node.
                    let value = unsafe { SkipNode::dealloc(storage.head) };
                    return (None, Some(value));
                };
                // Adds the next head node to higher levels when it's not already added.
                let new_head = unsafe { storage.head.as_mut() };
                for level in (1..storage.levels.get()).rev() {
                    if head.nexts[level] == head.nexts[0] || new_head.nexts[level].is_some() {
                        break;
                    }
                    new_head.nexts[level] = head.nexts[level];
                }
                unsafe { SkipNode::dealloc(old_head_ptr) }
            }
            cmp::Ordering::Less => {
                // Traverses the storage and searches for the value, while tracking the nodes that
                // might get updated due to the removal.
                let mut trace = [MaybeUninit::uninit(); N];
                storage.descend(value, |level, ptr| {
                    trace[level].write(ptr);
                });
                // Checks if the value exists. The trace only includes upto the node right before
                // the one that will potentially be removed.
                let Some(curr_ptr) = ({
                    let prev = unsafe { trace[0].assume_init_ref().as_ref() };
                    prev.nexts[0]
                }) else {
                    return (Some(ManuallyDrop::into_inner(storage)), None);
                };
                {
                    let curr = unsafe { curr_ptr.as_ref() };
                    if curr.value.borrow() != value {
                        return (Some(ManuallyDrop::into_inner(storage)), None);
                    }
                    // Removes the node at every level.
                    for (level, mut prev_ptr) in
                        trace.into_iter().enumerate().take(storage.levels.get())
                    {
                        let prev = unsafe { prev_ptr.assume_init_mut().as_mut() };
                        if prev.nexts[level].is_none_or(|ptr| ptr != curr_ptr) {
                            break;
                        }
                        prev.nexts[level] = curr.nexts[level];
                    }
                }
                unsafe { SkipNode::dealloc(curr_ptr) }
            }
        };
        // Updates the storage's level by counting the number of next pointers that was removed
        // from the head.
        let head = unsafe { storage.head.as_ref() };
        while storage.levels.get() > 1 && head.nexts[storage.levels.get() - 1].is_none() {
            storage.levels = unsafe { NonZeroUsize::new_unchecked(storage.levels.get() - 1) };
        }
        (Some(ManuallyDrop::into_inner(storage)), Some(value))
    }

    /// Traverses the storage, descending down all levels, and calling the given function on the
    /// last encountered node at each level.
    fn descend<U, V>(&self, value: &U, mut visit: V)
    where
        T: Borrow<U>,
        U: Ord + ?Sized,
        V: FnMut(usize, NonNull<SkipNode<T, N>>),
    {
        let mut prev_node_ptr = self.head;
        for level in (0..self.levels.get()).rev() {
            while let Some(curr_node_ptr) = {
                let prev_node = unsafe { prev_node_ptr.as_ref() };
                prev_node.nexts[level]
            } && {
                let curr_node = unsafe { curr_node_ptr.as_ref() };
                curr_node.value.borrow() < value
            } {
                prev_node_ptr = curr_node_ptr;
            }
            visit(level, prev_node_ptr);
        }
    }

    fn head_cmp<U>(&self, value: &U) -> cmp::Ordering
    where
        T: Borrow<U>,
        U: Ord + ?Sized,
    {
        let head = unsafe { self.head.as_ref() };
        let head_value: &U = head.value.borrow();
        head_value.cmp(value)
    }
}

#[derive(Debug)]
struct SkipNode<T, const N: usize> {
    value: T,
    nexts: [Option<NonNull<Self>>; N],
}

impl<T, const N: usize> SkipNode<T, N> {
    const fn new(value: T) -> Self {
        Self {
            value,
            nexts: [None; N],
        }
    }

    fn alloc(self) -> NonNull<Self> {
        let ptr = Box::into_raw(Box::new(self));
        unsafe { NonNull::new_unchecked(ptr) }
    }

    unsafe fn dealloc(ptr: NonNull<Self>) -> T {
        let node = unsafe { Box::from_raw(ptr.as_ptr()) };
        node.value
    }
}

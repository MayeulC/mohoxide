//! Moho's bounded prop list (`EntityDB` +0x2c, a `gpg::PriorityQueue`):
//! `Prop:AddBoundedProp(priority)` puts a prop in it, and once it holds
//! [`CAP`] props, adding one destroys the prop at its front first. The base
//! game's `Unit.lua` and LOUD's add every wreck, so wrecks are capped at
//! 1000, the lowest priority and then the oldest going first.
//!
//! The heap is moho's own, operation for operation, so which prop goes on
//! equal keys is the real game's (MohoEngine.dll):
//! - a key is `(ceil(priority), tick)` as two `i32`, compared
//!   lexicographically, smallest at the front (the binding at 0x102d7396
//!   stores them at prop +0x26c / +0x270; the tick is `Sim` +0x910, which
//!   `Sim::AdvanceBeat` counts up);
//! - push: append, then sift up while the parent isn't strictly smaller
//!   (so a new entry passes equal keys; 0x1026a3b0);
//! - remove (the front, or a destroyed prop's entry): swap with the last
//!   entry, sift that down over the rest, drop the last (0x1026a460; no sift
//!   up after it, as moho);
//! - sift down: to the smaller child, the left on a tie, while it's strictly
//!   smaller (0x1026b250).

use serde::{Deserialize, Serialize};

use super::entity::EntityId;

/// Props the list holds before adding one destroys the front one
/// (0x10268940: `cmp eax, 0x3e8`).
pub const CAP: usize = 1000;

/// One prop in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub priority: i32,
    pub tick: i32,
    pub prop: EntityId,
    /// Which add made it (a prop added twice has two entries; moho keeps
    /// the latest handle on the prop, so destroying it removes that one).
    pub serial: u64,
}

impl Entry {
    fn key(&self) -> (i32, i32) {
        (self.priority, self.tick)
    }
}

/// The list: a binary min-heap in moho's layout.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BoundedProps {
    heap: Vec<Entry>,
    next_serial: u64,
}

impl BoundedProps {
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// The front prop (the next to go), if any.
    pub fn front(&self) -> Option<EntityId> {
        self.heap.first().map(|e| e.prop)
    }

    /// Whether the list is full: the next add destroys the front first.
    pub fn is_full(&self) -> bool {
        self.heap.len() >= CAP
    }

    /// Takes the front prop off the list (for the caller to destroy).
    pub fn pop_front(&mut self) -> Option<EntityId> {
        if self.heap.is_empty() {
            return None;
        }
        Some(self.remove_at(0).prop)
    }

    /// Adds `prop` with moho's key: `priority` rounded up, then the tick.
    pub fn push(&mut self, prop: EntityId, priority: i32, tick: i32) {
        let serial = self.next_serial;
        self.next_serial += 1;
        self.heap.push(Entry { priority, tick, prop, serial });
        let mut i = self.heap.len() - 1;
        while i > 0 {
            let parent = (i - 1) / 2;
            if self.heap[parent].key() < self.heap[i].key() {
                break;
            }
            self.heap.swap(parent, i);
            i = parent;
        }
    }

    /// Takes `prop`'s entry off the list (its latest add), as a destroyed
    /// prop leaves it (`EntityDB::RemoveBoundedProp`). Whether it was there.
    pub fn remove(&mut self, prop: EntityId) -> bool {
        let latest = self.heap.iter().enumerate().filter(|(_, e)| e.prop == prop).max_by_key(|(_, e)| e.serial).map(|(i, _)| i);
        match latest {
            Some(i) => {
                self.remove_at(i);
                true
            }
            None => false,
        }
    }

    fn remove_at(&mut self, i: usize) -> Entry {
        let last = self.heap.len() - 1;
        if i != last {
            self.heap.swap(i, last);
            self.sift_down(i, last);
        }
        self.heap.pop().expect("not empty")
    }

    /// Moho's sift down over the first `n` entries.
    fn sift_down(&mut self, mut i: usize, n: usize) {
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            if l >= n {
                return;
            }
            let mut smallest = i;
            if self.heap[l].key() < self.heap[i].key() {
                smallest = l;
            }
            if r < n && self.heap[r].key() < self.heap[smallest].key() {
                smallest = r;
            }
            if smallest == i {
                return;
            }
            self.heap.swap(i, smallest);
            i = smallest;
        }
    }
}

/// Moho's priority: the binding reads a float and rounds it up to an
/// integer (`frndint`, then one more when the float was above it).
pub fn priority_of(value: f64) -> i32 {
    (value as f32).ceil() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_front_is_the_lowest_priority_then_the_oldest() {
        let mut b = BoundedProps::default();
        b.push(1, 5, 10);
        b.push(2, 3, 20);
        b.push(3, 3, 15);
        b.push(4, 9, 1);
        assert_eq!(b.pop_front(), Some(3));
        assert_eq!(b.pop_front(), Some(2));
        assert_eq!(b.pop_front(), Some(1));
        assert_eq!(b.pop_front(), Some(4));
        assert_eq!(b.pop_front(), None);
    }

    #[test]
    fn a_destroyed_prop_leaves_the_list() {
        let mut b = BoundedProps::default();
        for i in 0..10 {
            b.push(i, (i % 3) as i32, i as i32);
        }
        assert!(b.remove(4));
        assert!(!b.remove(4));
        let mut order = Vec::new();
        while let Some(p) = b.pop_front() {
            order.push(p);
        }
        assert_eq!(order.len(), 9);
        assert!(!order.contains(&4));
    }

    #[test]
    fn priority_rounds_up() {
        assert_eq!(priority_of(2.0), 2);
        assert_eq!(priority_of(2.1), 3);
        assert_eq!(priority_of(2.5), 3);
        assert_eq!(priority_of(-2.5), -2);
        assert_eq!(priority_of(0.0), 0);
    }
}

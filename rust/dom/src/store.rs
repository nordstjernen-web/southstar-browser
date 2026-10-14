//! Southstar — the node and attribute stores: per-thread slots that are never handed back to the allocator, recycled under a generation count so a weak handle can tell a freed node from a live one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ptr;
use std::sync::Mutex;

use crate::Node;
use crate::data::{AttrSlot, Kind, NodeSlot};

const CHUNK_SLOTS: usize = 256;

pub(crate) trait Slot: Sized + 'static {
    fn vacant() -> Self;
    fn spare() -> &'static Mutex<Vec<Spare<Self>>>;
}

pub(crate) struct Spare<T: 'static>(&'static T);

// SAFETY: a slot is spared only while vacant, by the store of a thread that is exiting; handles to slots cannot leave their thread, so the adopting thread becomes the slot's only user.
unsafe impl<T> Send for Spare<T> {}

static SPARE_NODES: Mutex<Vec<Spare<NodeSlot>>> = Mutex::new(Vec::new());
static SPARE_ATTRS: Mutex<Vec<Spare<AttrSlot>>> = Mutex::new(Vec::new());

impl Slot for NodeSlot {
    fn vacant() -> Self {
        NodeSlot::vacant()
    }

    fn spare() -> &'static Mutex<Vec<Spare<Self>>> {
        &SPARE_NODES
    }
}

impl Slot for AttrSlot {
    fn vacant() -> Self {
        AttrSlot::vacant()
    }

    fn spare() -> &'static Mutex<Vec<Spare<Self>>> {
        &SPARE_ATTRS
    }
}

pub(crate) struct Store<T: Slot> {
    vacant: RefCell<Vec<&'static T>>,
    live: Cell<usize>,
}

impl<T: Slot> Store<T> {
    const fn new() -> Self {
        Store {
            vacant: RefCell::new(Vec::new()),
            live: Cell::new(0),
        }
    }

    fn refill(vacant: &mut Vec<&'static T>) {
        if let Ok(mut spare) = T::spare().lock()
            && !spare.is_empty()
        {
            let keep = spare.len().saturating_sub(CHUNK_SLOTS);
            vacant.extend(spare.drain(keep..).map(|slot| slot.0));
            return;
        }
        let chunk: &'static [T] = Box::leak((0..CHUNK_SLOTS).map(|_| T::vacant()).collect());
        vacant.extend(chunk.iter().rev());
    }

    fn take(&self) -> Option<&'static T> {
        self.live.set(self.live.get() + 1);
        let mut vacant = self.vacant.borrow_mut();
        if vacant.is_empty() {
            Self::refill(&mut vacant);
        }
        vacant.pop()
    }

    fn give_back(&self, slot: &'static T) {
        self.live.set(self.live.get().saturating_sub(1));
        self.vacant.borrow_mut().push(slot);
    }
}

impl<T: Slot> Drop for Store<T> {
    fn drop(&mut self) {
        let vacant = self.vacant.take();
        if let Ok(mut spare) = T::spare().lock() {
            spare.extend(vacant.into_iter().map(Spare));
        }
    }
}

thread_local! {
    static NODES: Store<NodeSlot> = const { Store::new() };
    static ATTRS: Store<AttrSlot> = const { Store::new() };
}

fn take<T: Slot>(store: &'static std::thread::LocalKey<Store<T>>) -> &'static T {
    store
        .try_with(Store::take)
        .ok()
        .flatten()
        .unwrap_or_else(|| Box::leak(Box::new(T::vacant())))
}

pub(crate) fn occupy(kind: Kind) -> &'static NodeSlot {
    let slot = take(&NODES);
    slot.occupy(kind);
    slot
}

pub(crate) fn vacate(slot: &'static NodeSlot) {
    slot.vacate();
    let _ = NODES.try_with(|store| store.give_back(slot));
}

pub(crate) fn new_attr() -> &'static AttrSlot {
    take(&ATTRS)
}

pub(crate) fn drop_attr(slot: &'static AttrSlot) {
    slot.vacate();
    let _ = ATTRS.try_with(|store| store.give_back(slot));
}

pub fn live_nodes() -> usize {
    NODES.try_with(|store| store.live.get()).unwrap_or(0)
}

pub fn live_attrs() -> usize {
    ATTRS.try_with(|store| store.live.get()).unwrap_or(0)
}

#[derive(Clone, Copy)]
pub struct WeakNode {
    slot: &'static NodeSlot,
    generation: u32,
}

impl WeakNode {
    pub(crate) fn new(slot: &'static NodeSlot) -> Self {
        WeakNode {
            slot,
            generation: slot.generation.get(),
        }
    }

    pub fn get(self) -> Option<Node<'static>> {
        (self.slot.generation.get() == self.generation).then(|| Node::of(self.slot))
    }
}

impl PartialEq for WeakNode {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.slot, other.slot) && self.generation == other.generation
    }
}

impl Eq for WeakNode {}

impl fmt::Debug for WeakNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WeakNode")
            .field("at", &ptr::from_ref(self.slot))
            .field("generation", &self.generation)
            .finish()
    }
}

impl Hash for WeakNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ptr::hash(self.slot, state);
        self.generation.hash(state);
    }
}

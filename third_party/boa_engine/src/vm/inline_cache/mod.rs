use arrayvec::ArrayVec;
use itertools::Itertools;
use std::{cell::Cell, fmt};

use boa_gc::GcRefCell;
use boa_macros::{Finalize, Trace};

use crate::{
    JsString,
    object::shape::{
        Shape, WeakShape,
        slot::{Slot, SlotAttributes},
    },
};

fn prototype_shape(shape: &Shape) -> Option<Shape> {
    shape
        .prototype()
        .map(|prototype| prototype.borrow().shape().clone())
}

fn prototype_kept_shape(shape: &Shape, cached: Option<usize>) -> bool {
    let (Some(prototype), Some(cached)) = (shape.prototype(), cached) else {
        return false;
    };
    prototype.borrow().shape().to_addr_usize() == cached
}

/// The shapes a property lookup that missed the inline cache started from: the object's, and its
/// prototype's.
pub(crate) struct LookedUp {
    shape: Shape,
    prototype_shape: Option<Shape>,
}

impl LookedUp {
    pub(crate) fn new(shape: &Shape) -> Self {
        Self {
            shape: shape.clone(),
            prototype_shape: prototype_shape(shape),
        }
    }
}

#[cfg(test)]
mod tests;

pub(crate) const PIC_CAPACITY: usize = 4;

/// A cached shape-to-slot mapping for a polymorphic inline cache.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct CacheEntry {
    /// A weak reference is kept to the shape to avoid the shape preventing deallocation.
    pub(crate) shape: WeakShape,
    /// For a slot found on the prototype, the prototype's shape when the slot was cached.
    ///
    /// Deleting a property of the prototype, or turning one between data and accessor, shifts
    /// the prototype's storage and gives it a new shape, while objects inheriting from it keep
    /// theirs. The entry is valid only while the prototype still has this shape.
    pub(crate) prototype_shape: Option<WeakShape>,
    #[unsafe_ignore_trace]
    pub(crate) slot: Slot,
}

/// An inline cache entry for a property access.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct InlineCache {
    /// The property that is accessed.
    pub(crate) name: JsString,

    /// Multiple cached shape-to-slot entries.
    pub(crate) entries: GcRefCell<ArrayVec<CacheEntry, PIC_CAPACITY>>,

    /// Whether this access site has seen too many shapes and should no longer be cached.
    #[unsafe_ignore_trace]
    pub(crate) megamorphic: Cell<bool>,
}

impl fmt::Display for InlineCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(name:{} entries:", self.name.display_escaped())?;

        if self.megamorphic.get() {
            return write!(f, "(megamorphic))");
        }

        let entries = self.entries.borrow();
        let entries = entries.iter().map(|e| e.shape.to_addr_usize()).format(", ");

        write!(f, "({entries:#x}))")
    }
}

impl InlineCache {
    pub(crate) fn new(name: JsString) -> Self {
        Self {
            name,
            entries: GcRefCell::new(ArrayVec::new()),
            megamorphic: Cell::new(false),
        }
    }

    pub(crate) fn set(&self, shape: &Shape, slot: Slot) {
        if self.megamorphic.get() {
            return;
        }

        let prototype_shape = if slot.attributes.contains(SlotAttributes::PROTOTYPE) {
            prototype_shape(shape).as_ref().map(WeakShape::from)
        } else {
            None
        };

        let mut entries = self.entries.borrow_mut();

        // Add a new entry if there's space.
        if entries
            .try_push(CacheEntry {
                shape: shape.into(),
                prototype_shape,
                slot,
            })
            .is_err()
        {
            // Polymorphic cache is full, transition to megamorphic.
            self.megamorphic.set(true);
            entries.clear();
        }
    }

    /// Caches `slot`, which a property lookup that started from the shapes in `looked_up` found,
    /// for the object's `current` shape.
    ///
    /// An accessor's getter or setter runs between the lookup and this call and can reshape the
    /// object it runs on (redefining the accessor as a data property shifts the storage), or the
    /// prototype the slot was found on (deleting a property of the prototype shifts its storage).
    /// The slot then describes the old layout, so it is not cached.
    pub(crate) fn set_after_access(&self, looked_up: &LookedUp, current: &Shape, slot: Slot) {
        if slot.attributes.is_accessor_descriptor()
            && looked_up.shape.to_addr_usize() != current.to_addr_usize()
        {
            return;
        }
        if slot.attributes.contains(SlotAttributes::PROTOTYPE)
            && !prototype_kept_shape(
                current,
                looked_up.prototype_shape.as_ref().map(Shape::to_addr_usize),
            )
        {
            return;
        }
        self.set(current, slot);
    }

    /// Returns the cached `(Shape, Slot)` if a matching shape exists in the inline cache.
    ///
    /// A prototype slot matches only while the prototype keeps the shape it had when the slot
    /// was cached; an entry whose prototype has changed shape is dropped.
    ///
    /// Opportunistically cleans up stale weak shape references during lookup.
    pub(crate) fn get(&self, shape: &Shape) -> Option<(Shape, Slot)> {
        if self.megamorphic.get() {
            return None;
        }

        let mut entries = self.entries.borrow_mut();
        let mut i = 0;
        let mut result = None;
        let shape_addr = shape.to_addr_usize();

        while i < entries.len() {
            if let Some(upgraded) = entries[i].shape.upgrade() {
                if upgraded.to_addr_usize() == shape_addr {
                    let slot = entries[i].slot;
                    let cached_prototype = entries[i].prototype_shape.as_ref();
                    if slot.attributes.contains(SlotAttributes::PROTOTYPE)
                        && !prototype_kept_shape(
                            &upgraded,
                            cached_prototype.map(WeakShape::to_addr_usize),
                        )
                    {
                        entries.swap_remove(i);
                        break;
                    }
                    result = Some((upgraded, slot));
                    break;
                }
                i += 1;
            } else {
                // Opportunistically clean up stale weak shapes.
                entries.swap_remove(i);
            }
        }

        result
    }
}

use arrayvec::ArrayVec;
use itertools::Itertools;
use std::{cell::Cell, fmt};

use boa_gc::GcRefCell;
use boa_macros::{Finalize, Trace};

use crate::{
    JsObject, JsString, JsValue,
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

const ADD_CHAIN_CAPACITY: usize = 4;

/// A prototype's shape, and its version, when an [`AddTransition`] was cached.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct ChainLink {
    shape: WeakShape,
    #[unsafe_ignore_trace]
    version: u32,
}

/// A cached property addition: an extensible object with the shape `from` and the internal
/// methods at `methods` gains the property as a writable, enumerable and configurable data
/// property at the end of its storage and takes the shape `to`, as long as the objects on its
/// prototype chain still have the shapes and versions in `chain`, none of which had the
/// property when it was cached.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct AddTransition {
    from: WeakShape,
    to: WeakShape,
    chain: ArrayVec<ChainLink, ADD_CHAIN_CAPACITY>,
    #[unsafe_ignore_trace]
    methods: usize,
}

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

    /// The last property addition made through this site.
    pub(crate) add: GcRefCell<Option<Box<AddTransition>>>,
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
            add: GcRefCell::new(None),
        }
    }

    /// Adds the property to `object` with `value` through the cached [`AddTransition`], when
    /// it applies; `false` when it does not, having changed nothing.
    ///
    /// The prototype chain is identified by shapes: a shared shape never changes and fixes its
    /// prototype, a unique shape counts its changes in its version, so equal shapes and versions
    /// mean the same objects with the same properties, of which none has the property.
    pub(crate) fn add_property(&self, object: &JsObject, value: &JsValue) -> bool {
        let add = self.add.borrow();
        let Some(add) = add.as_deref() else {
            return false;
        };
        if std::ptr::from_ref(object.vtable()).addr() != add.methods {
            return false;
        }
        let mut object = object.borrow_mut();
        if !object.extensible || !add.from.points_to(object.shape()) {
            return false;
        }
        let mut prototype = object.shape().prototype();
        for link in &add.chain {
            let Some(current) = prototype else {
                return false;
            };
            let current = current.borrow();
            if !link.shape.points_to(current.shape()) || current.shape().version() != link.version
            {
                return false;
            }
            prototype = current.prototype();
        }
        if prototype.is_some() {
            return false;
        }
        let Some(to) = add.to.upgrade() else {
            return false;
        };
        object.properties_mut().push_transition(to, value.clone());
        true
    }

    /// Caches the addition of the property to `object`, whose shape was the one in
    /// `looked_up` before, as a writable, enumerable and configurable data property by the
    /// ordinary `[[Set]]` or `[[DefineOwnProperty]]`, with nothing on its prototype chain having
    /// the property and every object on it using the ordinary internal methods.
    pub(crate) fn set_add(&self, looked_up: &LookedUp, object: &JsObject) {
        let from = &looked_up.shape;
        let methods = std::ptr::from_ref(object.vtable()).addr();
        let object = object.borrow();
        let to = object.shape();
        if !from.is_shared() || !to.is_shared() {
            return;
        }
        let mut chain = ArrayVec::new();
        let mut prototype = from.prototype();
        while let Some(current) = prototype {
            let current = current.borrow();
            let link = ChainLink {
                shape: current.shape().into(),
                version: current.shape().version(),
            };
            if chain.try_push(link).is_err() {
                return;
            }
            prototype = current.prototype();
        }
        *self.add.borrow_mut() = Some(Box::new(AddTransition {
            from: from.into(),
            to: to.into(),
            chain,
            methods,
        }));
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
        entries.retain(|entry| entry.shape.upgrade().is_some());

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

    /// Returns the cached slot if a matching shape exists in the inline cache.
    ///
    /// A prototype slot matches only while the prototype keeps the shape it had when the slot
    /// was cached; an entry whose prototype has changed shape is dropped.
    ///
    /// Shapes are compared through their weak references without upgrading them, so a hit
    /// creates no new reference.
    pub(crate) fn get(&self, shape: &Shape) -> Option<Slot> {
        if self.megamorphic.get() {
            return None;
        }

        let entries = self.entries.borrow();
        let position = entries
            .iter()
            .position(|entry| entry.shape.points_to(shape))?;
        let entry = &entries[position];
        let slot = entry.slot;
        if slot.attributes.contains(SlotAttributes::PROTOTYPE) {
            let kept = match (shape.prototype(), entry.prototype_shape.as_ref()) {
                (Some(prototype), Some(cached)) => cached.points_to(prototype.borrow().shape()),
                _ => false,
            };
            if !kept {
                drop(entries);
                self.entries.borrow_mut().swap_remove(position);
                return None;
            }
        }
        Some(slot)
    }
}

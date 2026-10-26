//! `VTable` implementations for [`RopeString`], the lazy concatenation of two strings.
use crate::iter::CodePointsIter;
use crate::r#type::{InternalStringType, Latin1, Utf16};
use crate::vtable::{JsStringVTable, SequenceString};
use crate::{JsStr, JsStrVariant, JsString, JsStringKind};
use std::cell::{Cell, UnsafeCell};
use std::process::abort;
use std::ptr::NonNull;

/// The contents of a [`RopeString`]: the two halves it joins until a caller needs its
/// characters, and the flat string that replaces them from then on.
enum RopeState {
    Pair(JsString, JsString),
    Flat(JsString),
}

/// The concatenation of two strings, flattened into a sequence string the first time
/// its characters are read.
///
/// Repeated `s = s + t` builds a chain of ropes in linear time where flat strings copy
/// the whole prefix on every step. Every walk over a chain (flattening and dropping) is
/// iterative, so a chain of any depth is safe.
#[repr(C)]
pub(crate) struct RopeString {
    /// Embedded `VTable` - must be the first field for vtable dispatch.
    vtable: JsStringVTable,
    refcount: Cell<usize>,
    latin1: bool,
    state: UnsafeCell<RopeState>,
}

impl RopeString {
    /// Creates the rope of `left` followed by `right`.
    pub(crate) fn new(left: JsString, right: JsString) -> Self {
        let latin1 = is_latin1(&left) && is_latin1(&right);
        RopeString {
            vtable: JsStringVTable {
                clone: rope_clone,
                drop: rope_drop,
                as_str: rope_as_str,
                code_points: rope_code_points,
                refcount: rope_refcount,
                len: left.len() + right.len(),
                kind: JsStringKind::Rope,
            },
            refcount: Cell::new(1),
            latin1,
            state: UnsafeCell::new(RopeState::Pair(left, right)),
        }
    }

    fn state(&self) -> &RopeState {
        // SAFETY: the state is only replaced by `flatten`, which no reference handed out
        // by this function outlives: callers read it and finish before flattening.
        unsafe { &*self.state.get() }
    }
}

/// Returns the rope behind `string`, if it is one.
fn as_rope(string: &JsString) -> Option<&RopeString> {
    if string.kind() == JsStringKind::Rope {
        // SAFETY: just verified the kind.
        Some(unsafe { string.as_inner::<RopeString>() })
    } else {
        None
    }
}

fn is_latin1(string: &JsString) -> bool {
    match as_rope(string) {
        Some(rope) => rope.latin1,
        None => string.as_str().is_latin1(),
    }
}

/// Visits the flat pieces of `root` from left to right without recursion.
fn for_each_piece(root: &RopeString, mut visit: impl FnMut(JsStr<'_>)) {
    let RopeState::Pair(left, right) = root.state() else {
        return;
    };
    let mut pending: Vec<&JsString> = vec![right, left];
    while let Some(string) = pending.pop() {
        match as_rope(string).map(RopeString::state) {
            Some(RopeState::Pair(left, right)) => {
                pending.push(right);
                pending.push(left);
            }
            Some(RopeState::Flat(flat)) => visit(flat.as_str()),
            None => visit(string.as_str()),
        }
    }
}

fn build_flat<T: InternalStringType>(
    rope: &RopeString,
    mut write: impl FnMut(*mut T::Byte, JsStr<'_>) -> usize,
) -> JsString {
    let ptr = SequenceString::<T>::allocate(rope.vtable.len);
    // SAFETY: `allocate` returned a valid sequence string with room for `len` characters.
    let mut data = unsafe { (*ptr.as_ptr()).data().cast::<T::Byte>().cast_mut() };
    for_each_piece(rope, |piece| {
        let written = write(data, piece);
        // SAFETY: the pieces add up to the rope's length, so `data` stays in bounds.
        data = unsafe { data.add(written) };
    });
    // SAFETY: the sequence string was fully written above.
    unsafe { JsString::from_ptr(ptr.cast()) }
}

fn flatten_latin1(rope: &RopeString) -> JsString {
    build_flat::<Latin1>(rope, |data, piece| match piece.variant() {
        JsStrVariant::Latin1(bytes) => {
            // SAFETY: `data` has room for the piece and does not overlap it.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len()) };
            bytes.len()
        }
        JsStrVariant::Utf16(_) => unreachable!("a Latin1 rope has only Latin1 pieces"),
    })
}

fn flatten_utf16(rope: &RopeString) -> JsString {
    build_flat::<Utf16>(rope, |data, piece| match piece.variant() {
        JsStrVariant::Latin1(bytes) => {
            for (i, byte) in bytes.iter().enumerate() {
                // SAFETY: `data` has room for the piece.
                unsafe { data.add(i).write(u16::from(*byte)) };
            }
            bytes.len()
        }
        JsStrVariant::Utf16(units) => {
            // SAFETY: `data` has room for the piece and does not overlap it.
            unsafe { std::ptr::copy_nonoverlapping(units.as_ptr(), data, units.len()) };
            units.len()
        }
    })
}

/// Drops `strings` and everything only they keep alive, taking chains of ropes apart
/// iteratively.
fn release(mut strings: Vec<JsString>) {
    while let Some(string) = strings.pop() {
        let unique_rope = as_rope(&string).is_some_and(|rope| rope.refcount.get() == 1);
        if !unique_rope {
            drop(string);
            continue;
        }
        let ptr = string.into_raw().cast::<RopeString>();
        // SAFETY: this was the last reference to a boxed rope, so the box can be reclaimed.
        let rope = unsafe { Box::from_raw(ptr.as_ptr()) };
        match rope.state.into_inner() {
            RopeState::Pair(left, right) => {
                strings.push(left);
                strings.push(right);
            }
            RopeState::Flat(flat) => drop(flat),
        }
    }
}

/// Drops the two halves of a rope, without allocating unless one of them is a rope that
/// only this one keeps alive.
fn release_pair(left: JsString, right: JsString) {
    let unique_rope = |string: &JsString| as_rope(string).is_some_and(|r| r.refcount.get() == 1);
    if unique_rope(&left) || unique_rope(&right) {
        release(vec![left, right]);
    }
}

fn flat_str(rope: &RopeString) -> JsStr<'static> {
    if let RopeState::Flat(flat) = rope.state() {
        // SAFETY: the flat string lives as long as the rope, which outlives the result.
        return unsafe { flat.as_str().as_static() };
    }
    let flat = if rope.latin1 {
        flatten_latin1(rope)
    } else {
        flatten_utf16(rope)
    };
    // SAFETY: no reference into the state is alive here; the pieces were copied out above.
    let old = unsafe { std::ptr::replace(rope.state.get(), RopeState::Flat(flat)) };
    if let RopeState::Pair(left, right) = old {
        release_pair(left, right);
    }
    flat_str(rope)
}

#[inline]
fn rope_clone(vtable: NonNull<JsStringVTable>) -> JsString {
    // SAFETY: This is part of the correct vtable which is validated on construction.
    let this: &RopeString = unsafe { vtable.cast().as_ref() };
    let Some(strong) = this.refcount.get().checked_add(1) else {
        abort();
    };
    this.refcount.set(strong);
    // SAFETY: validated the string outside this function.
    unsafe { JsString::from_ptr(vtable) }
}

fn rope_drop(vtable: NonNull<JsStringVTable>) {
    // SAFETY: This is part of the correct vtable which is validated on construction.
    let this: &RopeString = unsafe { vtable.cast().as_ref() };
    let Some(new) = this.refcount.get().checked_sub(1) else {
        abort();
    };
    this.refcount.set(new);
    if new != 0 {
        return;
    }
    // SAFETY: This is the last reference, so the box can be reclaimed.
    let rope = unsafe { Box::from_raw(vtable.cast::<RopeString>().as_ptr()) };
    match rope.state.into_inner() {
        RopeState::Pair(left, right) => release_pair(left, right),
        RopeState::Flat(flat) => drop(flat),
    }
}

fn rope_as_str(vtable: NonNull<JsStringVTable>) -> JsStr<'static> {
    // SAFETY: This is part of the correct vtable which is validated on construction.
    let this: &RopeString = unsafe { vtable.cast().as_ref() };
    flat_str(this)
}

fn rope_code_points(vtable: NonNull<JsStringVTable>) -> CodePointsIter<'static> {
    CodePointsIter::new(rope_as_str(vtable))
}

/// `VTable` function for refcount, need to return an `Option<usize>`.
#[allow(clippy::unnecessary_wraps)]
fn rope_refcount(vtable: NonNull<JsStringVTable>) -> Option<usize> {
    // SAFETY: This is part of the correct vtable which is validated on construction.
    let this: &RopeString = unsafe { vtable.cast().as_ref() };
    Some(this.refcount.get())
}

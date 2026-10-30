//! Southstar — a strong GObject reference, its weak counterpart, and typed Rust values attached to an object.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::any::Any;
use core::ffi::{CStr, c_void};
use core::ptr::NonNull;
use std::ffi::CString;

use southstar_gtk_ffi as sys;

use crate::ffi;

#[derive(PartialEq, Eq)]
pub struct Object(NonNull<c_void>);

impl Object {
    /// # Safety
    /// `object` is null or a live GObject; the new handle takes its own reference, sinking a floating one.
    pub unsafe fn from_ptr(object: *mut c_void) -> Option<Object> {
        let object = NonNull::new(object)?;
        // SAFETY: the caller guarantees object is a live GObject.
        unsafe { ffi::g_object_ref_sink(object.as_ptr()) };
        Some(Object(object))
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn is_a(&self, type_: sys::GType) -> bool {
        // SAFETY: self is a live GObject instance.
        unsafe { sys::g_type_check_instance_is_a(self.as_ptr(), type_) != 0 }
    }

    pub fn downgrade(&self) -> WeakObject {
        let weak = Box::new(ffi::GWeakRef {
            priv_: core::ptr::null_mut(),
        });
        let weak = Box::into_raw(weak);
        // SAFETY: weak is a fresh, boxed GWeakRef that never moves, and self is a live GObject.
        unsafe { ffi::g_weak_ref_init(weak, self.as_ptr()) };
        WeakObject(weak)
    }

    pub fn set_data<T: Clone + 'static>(&self, key: &CStr, value: Option<T>) {
        let data = value.map_or(core::ptr::null_mut(), |value| {
            let any: Box<dyn Any> = Box::new(value);
            Box::into_raw(Box::new(any)).cast::<c_void>()
        });
        // SAFETY: self is a live GObject; data is null or a boxed `Box<dyn Any>` that drop_data frees.
        unsafe { ffi::g_object_set_qdata_full(self.as_ptr(), quark(key), data, Some(drop_data)) };
    }

    pub fn data<T: Clone + 'static>(&self, key: &CStr) -> Option<T> {
        // SAFETY: self is a live GObject.
        let data = unsafe { ffi::g_object_get_qdata(self.as_ptr(), quark(key)) };
        // SAFETY: the quark is private to set_data, so data is null or a `Box<dyn Any>` it stored, alive while self holds it.
        let any = unsafe { data.cast::<Box<dyn Any>>().as_ref() }?;
        any.downcast_ref::<T>().cloned()
    }
}

fn quark(key: &CStr) -> ffi::Quark {
    let mut name = b"southstar-gtk:".to_vec();
    name.extend_from_slice(key.to_bytes());
    let name = CString::new(name).unwrap_or_default();
    // SAFETY: name is NUL-terminated; GLib copies it into the quark table.
    unsafe { ffi::g_quark_from_string(name.as_ptr()) }
}

unsafe extern "C" fn drop_data(data: *mut c_void) {
    if !data.is_null() {
        // SAFETY: set_data stored this pointer from Box::into_raw of a `Box<dyn Any>`; GLib frees it once.
        drop(unsafe { Box::from_raw(data.cast::<Box<dyn Any>>()) });
    }
}

impl Clone for Object {
    fn clone(&self) -> Object {
        // SAFETY: self is a live GObject; the clone owns the new reference.
        unsafe { sys::g_object_ref(self.as_ptr()) };
        Object(self.0)
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        // SAFETY: releases the reference this handle owns.
        unsafe { sys::g_object_unref(self.as_ptr()) };
    }
}

pub struct WeakObject(*mut ffi::GWeakRef);

impl WeakObject {
    pub fn upgrade(&self) -> Option<Object> {
        // SAFETY: self.0 is the boxed GWeakRef initialized in downgrade; the result is a new strong reference or null.
        let object = NonNull::new(unsafe { ffi::g_weak_ref_get(self.0) })?;
        Some(Object(object))
    }
}

impl Clone for WeakObject {
    fn clone(&self) -> WeakObject {
        match self.upgrade() {
            Some(object) => object.downgrade(),
            None => {
                let weak = Box::into_raw(Box::new(ffi::GWeakRef {
                    priv_: core::ptr::null_mut(),
                }));
                WeakObject(weak)
            }
        }
    }
}

impl Drop for WeakObject {
    fn drop(&mut self) {
        // SAFETY: self.0 came from Box::into_raw; clearing a never-initialized, zeroed GWeakRef is also valid.
        unsafe { ffi::g_weak_ref_clear(self.0) };
        // SAFETY: the box is released exactly once, after GLib forgot its location.
        drop(unsafe { Box::from_raw(self.0) });
    }
}

//! Southstar — the C ABI of display values, overflow-clip-margin, counter lists and list-style values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::sync::OnceLock;

use southstar_glib as glib;

use crate::counter::{self, CounterProp};
use crate::display::{self, Display};

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

fn counter_prop(prop: c_int) -> CounterProp {
    static IDS: OnceLock<[c_int; 2]> = OnceLock::new();
    let ids = IDS.get_or_init(|| unsafe {
        [
            ns_css_prop_id(c"counter-increment".as_ptr()),
            ns_css_prop_id(c"counter-reset".as_ptr()),
        ]
    });
    if prop == ids[0] {
        CounterProp::Increment
    } else if prop == ids[1] {
        CounterProp::Reset
    } else {
        CounterProp::Set
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_display_serialize(d: Display) -> *mut c_char {
    glib::strdup(&display::serialize(d))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_display_from_keyword(canonical: *const c_char) -> Display {
    display::from_keyword(unsafe { bytes(canonical) })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_display_blockified(d: Display) -> Display {
    display::blockified(d)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_display_normalize(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(display::normalize))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_display_canonical(value: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(value) }.and_then(display::canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_overflow_clip_margin_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(display::overflow_clip_margin_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_counter_list_canonical(
    text: *const c_char,
    prop: c_int,
) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(|t| counter::list_canonical(t, counter_prop(prop))))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_list_style_type_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(counter::list_style_type_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_list_style_serialize(
    type_: *const c_char,
    position: *const c_char,
    image: *const c_char,
) -> *mut c_char {
    glib::strdup(&counter::list_style_serialize(
        unsafe { bytes(type_) },
        unsafe { bytes(position) },
        unsafe { bytes(image) },
    ))
}

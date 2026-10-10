//! Southstar — the C ABI of container queries: the container map layout fills, the container stack the cascade pushes and pops, container units, and @container conditions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::mem::size_of;
use core::ptr;

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean, GHashTable};

use crate::container::{self, Query, TYPE_INLINE, TYPE_SIZE};

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Container {
    names: *mut c_char,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) kind: c_int,
    pub(crate) vertical: GBoolean,
    pub(crate) sibling_index: c_int,
    pub(crate) sibling_count: c_int,
}

const _: () = assert!(size_of::<Container>() == 40);

impl Container {
    pub(crate) fn names(&self) -> Option<&CStr> {
        (!self.names.is_null()).then(|| unsafe { CStr::from_ptr(self.names) })
    }
}

unsafe extern "C" {
    fn g_str_hash(v: *const c_void) -> c_uint;
    fn g_hash_table_size(table: *mut GHashTable) -> c_uint;
}

thread_local! {
    static MAP: Cell<*mut GHashTable> = const { Cell::new(ptr::null_mut()) };
}

pub(crate) fn container_map() -> *mut GHashTable {
    MAP.with(Cell::get)
}

unsafe fn text<'a>(s: *const c_char) -> Option<&'a CStr> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) })
}

fn owned(bytes: Option<Vec<u8>>) -> *mut c_char {
    bytes.map_or(ptr::null_mut(), |bytes| glib::strdup(&bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_container_map(map: *mut GHashTable) {
    MAP.with(|current| current.set(map));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_container_dims(inline_px: c_double, block_px: c_double) {
    container::set_unit_dims(inline_px, block_px);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_w() -> c_double {
    container::unit_dims().0
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_h() -> c_double {
    container::unit_dims().1
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_features_begin() {
    container::features_begin();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_features_used() -> GBoolean {
    glib::boolean(container::features_used())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_features_note() {
    container::note_features();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_unit_resolve(v: c_double, unit: c_uint) -> c_double {
    container::unit_resolve(v, unit)
}

unsafe extern "C" fn container_free(data: *mut c_void) {
    let container = data.cast::<Container>();
    if let Some(container) = unsafe { container.as_ref() } {
        unsafe { glib::g_free(container.names.cast()) };
    }
    unsafe { glib::g_free(data) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_map_new() -> *mut GHashTable {
    unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_direct_hash),
            Some(glib::g_direct_equal),
            None,
            Some(container_free),
        )
    }
}

fn same_container(a: &Container, b: &Container) -> bool {
    a.kind == b.kind
        && a.vertical == b.vertical
        && a.width == b.width
        && a.height == b.height
        && a.sibling_index == b.sibling_index
        && a.sibling_count == b.sibling_count
        && a.names() == b.names()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_maps_equal(
    a: *mut GHashTable,
    b: *mut GHashTable,
) -> GBoolean {
    if a.is_null() || b.is_null() {
        return glib::boolean(a == b);
    }
    if unsafe { g_hash_table_size(a) != g_hash_table_size(b) } {
        return glib::FALSE;
    }
    for (key, value) in unsafe { glib::hash_table_entries(a) } {
        let other = unsafe { glib::g_hash_table_lookup(b, key) }.cast::<Container>();
        let (Some(other), Some(value)) = (unsafe { other.as_ref() }, unsafe {
            value.cast::<Container>().as_ref()
        }) else {
            return glib::FALSE;
        };
        if !same_container(value, other) {
            return glib::FALSE;
        }
    }
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_map_add(
    map: *mut GHashTable,
    node: *const c_void,
    type_kw: *const c_char,
    name_kw: *const c_char,
    w: c_double,
    h: c_double,
    vertical: GBoolean,
) {
    let Some(type_kw) = (unsafe { text(type_kw) }) else {
        return;
    };
    if map.is_null() || node.is_null() {
        return;
    }
    let kind = if type_kw.to_bytes().eq_ignore_ascii_case(b"size") {
        TYPE_SIZE
    } else {
        TYPE_INLINE
    };
    let names = match unsafe { text(name_kw) } {
        Some(name) if !name.to_bytes().eq_ignore_ascii_case(b"none") => unsafe {
            glib::g_strdup(name.as_ptr())
        },
        _ => ptr::null_mut(),
    };
    let mut container = Container {
        names,
        width: w,
        height: h,
        kind,
        vertical,
        sibling_index: 1,
        sibling_count: 1,
    };
    if let Some(element) = unsafe { Node::from_ptr(node.cast()) }
        && let Some(parent) = element.parent()
    {
        let mut count = 0;
        let mut sibling = parent.first_child();
        while let Some(sib) = sibling {
            if sib.is_element() {
                count += 1;
                if sib.as_ptr() == element.as_ptr() {
                    container.sibling_index = count;
                }
            }
            sibling = sib.next_sibling();
        }
        container.sibling_count = count;
    }
    unsafe {
        let slot = glib::g_malloc0(size_of::<Container>()).cast::<Container>();
        slot.write(container);
        glib::g_hash_table_insert(map, node.cast_mut(), slot.cast());
    }
}

fn container_hash(node: *const c_void, c: &Container) -> u64 {
    let w = c.width.to_bits();
    let h = c.height.to_bits();
    let mut x = (node as usize as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= w
        .wrapping_add(0x7f4a_7c15_9e37_79b9)
        .wrapping_add(x << 6)
        .wrapping_add(x >> 2);
    x ^= h
        .wrapping_add(0x1656_67b1_9e37_79f9)
        .wrapping_add(x << 6)
        .wrapping_add(x >> 2);
    x ^= ((c.kind as i64 as u64) << 40)
        ^ ((c.vertical as i64 as u64) << 39)
        ^ (u64::from(c.sibling_index as u32) << 20)
        ^ u64::from(c.sibling_count as u32);
    if !c.names.is_null() {
        x ^= u64::from(unsafe { g_str_hash(c.names.cast()) });
    }
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_map_signature() -> u64 {
    let map = container_map();
    if map.is_null() {
        return 0;
    }
    let mut signature = u64::from(unsafe { g_hash_table_size(map) }) + 1;
    for (key, value) in unsafe { glib::hash_table_entries(map) } {
        if let Some(container) = unsafe { value.cast::<Container>().as_ref() } {
            signature = signature.wrapping_add(container_hash(key, container));
        }
    }
    signature
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_stack_reset() {
    container::stack_reset();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_stack_push(node: *const c_void) -> GBoolean {
    let map = container_map();
    if map.is_null() {
        return glib::FALSE;
    }
    let info = unsafe { glib::g_hash_table_lookup(map, node) }.cast::<Container>();
    match unsafe { info.as_ref() } {
        Some(info) => {
            container::stack_push(*info);
            glib::TRUE
        }
        None => glib::FALSE,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_container_stack_pop() {
    container::stack_pop();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_stack_copy(out: *mut u8, max_bytes: usize) -> usize {
    container::with_stack(|stack| {
        let bytes = core::mem::size_of_val(stack);
        if !out.is_null() && bytes <= max_bytes {
            unsafe { ptr::copy_nonoverlapping(stack.as_ptr().cast::<u8>(), out, bytes) };
        }
        bytes
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_name_canonical(s: *const c_char) -> *mut c_char {
    owned(unsafe { text(s) }.and_then(|s| container::name_canonical(s.to_bytes())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_shorthand_canonical(s: *const c_char) -> *mut c_char {
    owned(unsafe { text(s) }.and_then(|s| container::shorthand_canonical(s.to_bytes())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_condition_canonical(s: *const c_char) -> *mut c_char {
    owned(unsafe { text(s) }.and_then(|s| container::condition_canonical(s.to_bytes())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_rule_matches(
    condition: *const c_char,
    cache: *mut *mut c_void,
) -> GBoolean {
    if cache.is_null() {
        return glib::FALSE;
    }
    if unsafe { (*cache).is_null() } {
        let condition = unsafe { text(condition) }.map_or(&[][..], CStr::to_bytes);
        unsafe { *cache = Box::into_raw(Box::new(Query::compile(condition))).cast() };
    }
    let query = unsafe { &*(*cache).cast::<Query>() };
    glib::boolean(query.matches())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_container_query_free(query: *mut c_void) {
    if !query.is_null() {
        drop(unsafe { Box::from_raw(query.cast::<Query>()) });
    }
}

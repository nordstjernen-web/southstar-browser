//! Southstar — the storage behind container queries: the container map layout fills, the container stack the cascade pushes and pops, container units, and @container conditions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::collections::HashMap;
use std::sync::Arc;

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean};

use crate::container::{self, Query, TYPE_INLINE, TYPE_SIZE};

#[derive(Clone, PartialEq)]
pub(crate) struct Container {
    names: Option<Arc<CStr>>,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) kind: c_int,
    pub(crate) vertical: GBoolean,
    pub(crate) sibling_index: c_int,
    pub(crate) sibling_count: c_int,
}

impl Container {
    pub(crate) fn names(&self) -> Option<&CStr> {
        self.names.as_deref()
    }
}

#[derive(Default, PartialEq)]
pub struct ContainerMap(HashMap<usize, Container>);

impl ContainerMap {
    pub fn new() -> ContainerMap {
        ContainerMap::default()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn add(
        &mut self,
        node: Node<'_>,
        type_kw: &CStr,
        name_kw: Option<&CStr>,
        width: f64,
        height: f64,
        vertical: bool,
    ) {
        let kind = if type_kw.to_bytes().eq_ignore_ascii_case(b"size") {
            TYPE_SIZE
        } else {
            TYPE_INLINE
        };
        let names = name_kw
            .filter(|name| !name.to_bytes().eq_ignore_ascii_case(b"none"))
            .map(Arc::from);
        let mut container = Container {
            names,
            width,
            height,
            kind,
            vertical: glib::boolean(vertical),
            sibling_index: 1,
            sibling_count: 1,
        };
        if let Some(parent) = node.parent() {
            let mut count = 0;
            let mut sibling = parent.first_child();
            while let Some(sib) = sibling {
                if sib.is_element() {
                    count += 1;
                    if sib.as_ptr() == node.as_ptr() {
                        container.sibling_index = count;
                    }
                }
                sibling = sib.next_sibling();
            }
            container.sibling_count = count;
        }
        self.0.insert(node.as_ptr() as usize, container);
    }
}

thread_local! {
    static MAP: Cell<*const ContainerMap> = const { Cell::new(ptr::null()) };
}

pub(super) fn with_container_map<R>(map: Option<&ContainerMap>, f: impl FnOnce() -> R) -> R {
    struct Restore(*const ContainerMap);

    impl Drop for Restore {
        fn drop(&mut self) {
            MAP.with(|current| current.set(self.0));
        }
    }

    let installed = map.map_or(ptr::null(), ptr::from_ref);
    let _restore = Restore(MAP.with(|current| current.replace(installed)));
    f()
}

fn with_current_map<R>(f: impl FnOnce(Option<&ContainerMap>) -> R) -> R {
    let map = MAP.with(Cell::get);
    // SAFETY: MAP is null or a map with_container_map borrows until its closure, which this call runs within, returns.
    f(unsafe { map.as_ref() })
}

pub(crate) fn container_map_installed() -> bool {
    !MAP.with(Cell::get).is_null()
}

unsafe fn text<'a>(s: *const c_char) -> Option<&'a CStr> {
    // SAFETY: the caller passes null or a NUL-terminated string that outlives 'a.
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) })
}

fn str_hash(text: &CStr) -> u32 {
    text.to_bytes().iter().fold(5381u32, |h, &c| {
        (h << 5).wrapping_add(h).wrapping_add(c as i8 as u32)
    })
}

fn container_hash(node: usize, c: &Container) -> u64 {
    let w = c.width.to_bits();
    let h = c.height.to_bits();
    let mut x = (node as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
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
    if let Some(names) = c.names() {
        x ^= u64::from(str_hash(names));
    }
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x
}

pub(super) fn container_map_signature() -> u64 {
    with_current_map(|map| {
        let Some(map) = map else {
            return 0;
        };
        let mut signature = map.len() as u64 + 1;
        for (&node, container) in &map.0 {
            signature = signature.wrapping_add(container_hash(node, container));
        }
        signature
    })
}

pub(super) fn container_stack_push(node: Node<'_>) -> bool {
    let info = with_current_map(|map| map?.0.get(&(node.as_ptr() as usize)).cloned());
    match info {
        Some(info) => {
            container::stack_push(info);
            true
        }
        None => false,
    }
}

pub(super) unsafe fn container_rule_matches(
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

pub(super) unsafe fn container_query_free(query: *mut c_void) {
    if !query.is_null() {
        drop(unsafe { Box::from_raw(query.cast::<Query>()) });
    }
}

//! Southstar — computed values read from ns_style, selector lists, box hit testing and the box's DOM node.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable, GPtrArray};
use southstar_layout::{BoxRef, NsBox, Style};

#[repr(C)]
#[derive(Clone, Copy)]
struct Length {
    v: f64,
    unit: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CalcHead {
    pct: f64,
    px: f64,
}

#[repr(C)]
union ValueHead {
    keyword: *const c_char,
    length: Length,
    color: [u8; 4],
    calc: CalcHead,
    url: *const c_char,
}

#[repr(C)]
struct NsCssValue {
    kind: c_uint,
    _ref: c_int,
    u: ValueHead,
}

const KIND_KEYWORD: c_uint = 0;
const KIND_LENGTH: c_uint = 1;
const KIND_COLOR: c_uint = 3;
const KIND_CALC: c_uint = 4;
const KIND_URL: c_uint = 8;

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_css_parse_selector_list(text: *const c_char) -> *mut GPtrArray;
    fn ns_css_selector_matches(sel: *const c_void, el: *const NsNode) -> GBoolean;
    fn ns_box_hit_test(root: *const NsBox, x: f64, y: f64) -> *const NsBox;
}

pub enum Value<'a> {
    Keyword(Option<&'a CStr>),
    Length(f64, c_uint),
    Calc(f64, f64),
    Color([u8; 4]),
    Url(Option<&'a CStr>),
    Other,
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[derive(Clone, Copy)]
pub struct StyleRef<'a>(NonNull<Style>, PhantomData<&'a Style>);

impl<'a> StyleRef<'a> {
    fn from_ptr(p: *const Style) -> Option<StyleRef<'a>> {
        NonNull::new(p.cast_mut()).map(|p| StyleRef(p, PhantomData))
    }

    pub fn value(self, name: &CStr) -> Option<Value<'a>> {
        let prop = usize::try_from(unsafe { ns_css_prop_id(name.as_ptr()) }).ok()?;
        let slot = unsafe { *self.0.as_ptr().cast::<*const NsCssValue>().add(prop) };
        let v = unsafe { slot.as_ref() }?;
        Some(unsafe {
            match v.kind {
                KIND_KEYWORD => Value::Keyword(c_str(v.u.keyword)),
                KIND_LENGTH => Value::Length(v.u.length.v, v.u.length.unit),
                KIND_COLOR => Value::Color(v.u.color),
                KIND_CALC => Value::Calc(v.u.calc.pct, v.u.calc.px),
                KIND_URL => Value::Url(c_str(v.u.url)),
                _ => Value::Other,
            }
        })
    }
}

pub fn box_style(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    StyleRef::from_ptr(b.style())
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn hit_test(root: Option<BoxRef<'_>>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    let root = root.map_or(ptr::null(), BoxRef::as_ptr);
    unsafe { BoxRef::from_ptr(ns_box_hit_test(root, x, y)) }
}

#[derive(Clone, Copy)]
pub struct StyleTable(*mut GHashTable);

impl StyleTable {
    pub unsafe fn from_ptr(table: *mut GHashTable) -> StyleTable {
        StyleTable(table)
    }

    pub fn get(self, node: Node<'_>) -> Option<StyleRef<'_>> {
        if self.0.is_null() {
            return None;
        }
        let style = unsafe { glib::g_hash_table_lookup(self.0, node.as_ptr().cast()) };
        StyleRef::from_ptr(style.cast())
    }
}

pub struct Selectors(NonNull<GPtrArray>);

impl Selectors {
    pub fn parse(text: &CStr) -> Option<Selectors> {
        let list = NonNull::new(unsafe { ns_css_parse_selector_list(text.as_ptr()) })?;
        let sels = Selectors(list);
        (sels.len() > 0).then_some(sels)
    }

    fn len(&self) -> usize {
        unsafe { self.0.as_ref() }.len as usize
    }

    pub fn any_matches(&self, el: Node) -> bool {
        let data = unsafe { self.0.as_ref() }.pdata;
        (0..self.len()).any(|i| unsafe { ns_css_selector_matches(*data.add(i), el.as_ptr()) } != 0)
    }
}

impl Drop for Selectors {
    fn drop(&mut self) {
        unsafe { glib::g_ptr_array_free(self.0.as_ptr(), glib::TRUE) };
    }
}

//! Southstar — computed values read from ns_style, selector lists, box hit testing and the box's DOM node.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::marker::PhantomData;
use core::ptr::NonNull;

use southstar_dom::Node;
use southstar_glib::{self as glib, GHashTable};
use southstar_layout::{BoxRef, Style};

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
        let prop = southstar_css::Prop::from_name(name.to_bytes())?.id();
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
    root.and_then(|root| southstar_layout_hit::hit_test(root, x, y))
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

pub struct Selectors(southstar_css::SelectorList);

impl Selectors {
    pub fn parse(text: &CStr) -> Option<Selectors> {
        southstar_css::SelectorList::parse_lenient(text.to_bytes()).map(Selectors)
    }

    pub fn any_matches(&self, el: Node) -> bool {
        self.0.matches_any(el)
    }
}

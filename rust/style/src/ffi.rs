//! Southstar — ns_style's leading values array, the ns_css_value head and the css.h calls over them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::marker::PhantomData;
use core::ptr::NonNull;
use std::sync::OnceLock;

use southstar_dom::Node;
use southstar_glib::{GBoolean, GHashTable, GStr, g_hash_table_lookup};
use southstar_layout::Style;

#[repr(C)]
#[derive(Clone, Copy)]
struct Length {
    v: f64,
    unit: c_uint,
}

#[repr(C)]
union ValueHead {
    keyword: *const c_char,
    length: Length,
    color: [u8; 4],
}

#[repr(C)]
pub struct NsCssValue {
    kind: c_uint,
    _ref: c_int,
    u: ValueHead,
}

const KIND_KEYWORD: c_uint = 0;
const KIND_LENGTH: c_uint = 1;
const KIND_COLOR: c_uint = 3;
const KIND_CALC: c_uint = 4;

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_style_keyword(style: *const Style, prop: c_int) -> *const c_char;
    fn ns_css_length_or(v: *const NsCssValue, fallback: f64) -> f64;
    fn ns_css_font_weight_number(v: *const NsCssValue, fallback: c_int) -> c_int;
    fn ns_css_resolve_style_vars(text: *const c_char, style: *const Style) -> *mut c_char;
    fn ns_css_font_family_for_pango(css_family: *const c_char) -> *mut c_char;
    fn ns_css_parse_color(
        s: *const c_char,
        r: *mut u8,
        g: *mut u8,
        b: *mut u8,
        a: *mut u8,
    ) -> GBoolean;
}

pub struct Prop {
    name: &'static CStr,
    id: OnceLock<Option<usize>>,
}

impl Prop {
    pub const fn new(name: &'static CStr) -> Prop {
        Prop {
            name,
            id: OnceLock::new(),
        }
    }

    fn id(&self) -> Option<usize> {
        *self
            .id
            .get_or_init(|| usize::try_from(unsafe { ns_css_prop_id(self.name.as_ptr()) }).ok())
    }
}

pub enum Value<'a> {
    Keyword(Option<&'a CStr>),
    Length(f64, u32),
    Color([u8; 4]),
    Calc,
    Other,
}

#[derive(Clone, Copy)]
pub struct ValueRef<'a>(NonNull<NsCssValue>, PhantomData<&'a NsCssValue>);

impl<'a> ValueRef<'a> {
    fn raw(self) -> &'a NsCssValue {
        unsafe { &*self.0.as_ptr() }
    }

    pub fn get(self) -> Value<'a> {
        let v = self.raw();
        unsafe {
            match v.kind {
                KIND_KEYWORD => {
                    Value::Keyword((!v.u.keyword.is_null()).then(|| CStr::from_ptr(v.u.keyword)))
                }
                KIND_LENGTH => Value::Length(v.u.length.v, v.u.length.unit),
                KIND_COLOR => Value::Color(v.u.color),
                KIND_CALC => Value::Calc,
                _ => Value::Other,
            }
        }
    }

    pub fn length_or(self, fallback: f64) -> f64 {
        unsafe { ns_css_length_or(self.0.as_ptr(), fallback) }
    }

    pub fn font_weight_or(self, fallback: i32) -> i32 {
        unsafe { ns_css_font_weight_number(self.0.as_ptr(), fallback) }
    }
}

#[derive(Clone, Copy)]
pub struct StyleRef<'a>(NonNull<Style>, PhantomData<&'a Style>);

impl<'a> StyleRef<'a> {
    pub unsafe fn from_ptr(p: *const Style) -> Option<StyleRef<'a>> {
        NonNull::new(p.cast_mut()).map(|p| StyleRef(p, PhantomData))
    }

    pub fn as_ptr(self) -> *const Style {
        self.0.as_ptr()
    }

    pub fn value(self, prop: &Prop) -> Option<ValueRef<'a>> {
        let id = prop.id()?;
        let slot = unsafe { *self.0.as_ptr().cast::<*const NsCssValue>().add(id) };
        NonNull::new(slot.cast_mut()).map(|p| ValueRef(p, PhantomData))
    }

    pub fn keyword(self, prop: &Prop) -> Option<&'a CStr> {
        let id = c_int::try_from(prop.id()?).ok()?;
        let kw = unsafe { ns_style_keyword(self.0.as_ptr(), id) };
        (!kw.is_null()).then(|| unsafe { CStr::from_ptr(kw) })
    }

    pub fn resolve_vars(self, text: &CStr) -> Option<GStr> {
        unsafe { GStr::take(ns_css_resolve_style_vars(text.as_ptr(), self.0.as_ptr())) }
    }
}

#[derive(Clone, Copy)]
pub struct StyleTable(*mut GHashTable);

impl StyleTable {
    pub unsafe fn from_ptr(table: *mut GHashTable) -> StyleTable {
        StyleTable(table)
    }

    pub fn none() -> StyleTable {
        StyleTable(core::ptr::null_mut())
    }

    pub fn as_ptr(self) -> *mut GHashTable {
        self.0
    }

    pub fn get(self, node: Node<'_>) -> Option<StyleRef<'_>> {
        if self.0.is_null() {
            return None;
        }
        let style = unsafe { g_hash_table_lookup(self.0, node.as_ptr().cast()) };
        unsafe { StyleRef::from_ptr(style.cast()) }
    }
}

pub fn parse_color(text: &CStr) -> Option<[u8; 4]> {
    let mut c = [0u8; 4];
    let [r, g, b, a] = &mut c;
    let ok = unsafe { ns_css_parse_color(text.as_ptr(), r, g, b, a) };
    (ok != 0).then_some(c)
}

pub fn font_family_for_pango(family: &CStr) -> Option<GStr> {
    unsafe { GStr::take(ns_css_font_family_for_pango(family.as_ptr())) }
}

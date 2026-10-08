//! Southstar — struct ns_style and struct ns_css_value as css.h lays them out, and the css.h calls over them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::marker::PhantomData;
use core::ptr::NonNull;
use std::sync::OnceLock;

use southstar_dom::Node;
use southstar_glib::{
    GBoolean, GHashTable, GPtrArray, GStr, g_hash_table_lookup, g_ptr_array_unref,
};
use southstar_layout::Style;

#[repr(C)]
#[derive(Clone, Copy)]
struct Length {
    v: f64,
    unit: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    w: f64,
    h: f64,
    w_unit: c_uint,
    h_unit: c_uint,
    w_auto: GBoolean,
    h_auto: GBoolean,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CalcHead {
    pct: f64,
    px: f64,
    em: f64,
    rem: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    v: [f64; 4],
    unit: [c_uint; 4],
    is_auto: [GBoolean; 4],
}

#[repr(C)]
union ValueUnion {
    keyword: *const c_char,
    length: Length,
    size: Size,
    color: [u8; 4],
    calc: CalcHead,
    url: *const c_char,
    rect: Rect,
    _storage: [u64; 381],
}

#[repr(C)]
pub struct NsCssValue {
    kind: c_uint,
    _ref: c_int,
    u: ValueUnion,
    _image_set_text: *mut c_char,
    _specified: *mut c_char,
    next_layer: *const NsCssValue,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsCssValue>() == 3080
        && core::mem::offset_of!(NsCssValue, u) == 8
        && core::mem::offset_of!(NsCssValue, next_layer) == 3072
        && core::mem::size_of::<Size>() == 32
        && core::mem::offset_of!(Rect, unit) == 32
        && core::mem::offset_of!(Rect, is_auto) == 48
);

pub const PROP_COUNT: usize = 242;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Display {
    pub box_: u8,
    pub outer: u8,
    pub inner: u8,
    pub internal: u8,
    pub list_item: u8,
}

pub const DISPLAY_BOX_NONE: u8 = 1;

impl Display {
    pub fn is_none(self) -> bool {
        self.box_ == DISPLAY_BOX_NONE
    }
}

#[repr(C)]
struct NsStyle {
    values: [*const NsCssValue; PROP_COUNT],
    display: Display,
    _specified_inline: u8,
    before: *const Style,
    after: *const Style,
    first_letter: *const Style,
    first_line: *const Style,
    placeholder: *const Style,
    selection: *const Style,
    marker: *const Style,
    backdrop: *const Style,
    file_selector_button: *const Style,
    _hidden_before: *const Style,
    _hidden_after: *const Style,
    _share_id: u64,
    _ref: c_int,
    _currentcolor_bits: u32,
    vars: *const VarMap,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsStyle>() == 2056
        && core::mem::offset_of!(NsStyle, display) == 1936
        && core::mem::offset_of!(NsStyle, before) == 1944
        && core::mem::offset_of!(NsStyle, file_selector_button) == 2008
        && core::mem::offset_of!(NsStyle, vars) == 2048
);

#[repr(C)]
struct VarMap {
    _private: [u8; 0],
}

const KIND_KEYWORD: c_uint = 0;
const KIND_LENGTH: c_uint = 1;
const KIND_SIZE: c_uint = 2;
const KIND_COLOR: c_uint = 3;
const KIND_CALC: c_uint = 4;
const KIND_URL: c_uint = 8;
const KIND_RECT: c_uint = 12;

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_style_keyword(style: *const Style, prop: c_int) -> *const c_char;
    fn ns_css_length_or(v: *const NsCssValue, fallback: f64) -> f64;
    fn ns_css_font_weight_number(v: *const NsCssValue, fallback: c_int) -> c_int;
    fn ns_css_resolve_style_vars(text: *const c_char, style: *const Style) -> *mut c_char;
    fn ns_css_font_family_for_pango(css_family: *const c_char) -> *mut c_char;
    fn ns_css_display_of(style: *const Style) -> Display;
    fn ns_var_map_names(map: *const VarMap) -> *mut GPtrArray;
    fn ns_var_map_lookup(map: *const VarMap, name: *const c_char) -> *const c_char;
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

#[derive(PartialEq)]
enum Payload<'a> {
    Keyword(Option<&'a CStr>),
    Length(f64, c_uint),
    Size([f64; 2], [c_uint; 2], [GBoolean; 2]),
    Color([u8; 4]),
    Calc([f64; 4]),
    Url(Option<&'a CStr>),
    Rect([f64; 4], [c_uint; 4], [GBoolean; 4]),
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

    pub fn as_ptr(self) -> *const NsCssValue {
        self.0.as_ptr()
    }

    pub fn keyword_text(self) -> Option<&'a CStr> {
        match self.get() {
            Value::Keyword(kw) => kw,
            _ => None,
        }
    }

    fn payload(self) -> Option<Payload<'a>> {
        let v = self.raw();
        let text = |p: *const c_char| (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) });
        unsafe {
            Some(match v.kind {
                KIND_KEYWORD => Payload::Keyword(text(v.u.keyword)),
                KIND_LENGTH => Payload::Length(v.u.length.v, v.u.length.unit),
                KIND_SIZE => {
                    let s = v.u.size;
                    Payload::Size([s.w, s.h], [s.w_unit, s.h_unit], [s.w_auto, s.h_auto])
                }
                KIND_COLOR => Payload::Color(v.u.color),
                KIND_CALC => {
                    let c = v.u.calc;
                    Payload::Calc([c.pct, c.px, c.em, c.rem])
                }
                KIND_URL => Payload::Url(text(v.u.url)),
                KIND_RECT => Payload::Rect(v.u.rect.v, v.u.rect.unit, v.u.rect.is_auto),
                _ => return None,
            })
        }
    }

    fn next_layer(self) -> Option<ValueRef<'a>> {
        NonNull::new(self.raw().next_layer.cast_mut()).map(|p| ValueRef(p, PhantomData))
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

    fn fields(self) -> &'a NsStyle {
        unsafe { &*self.0.as_ptr().cast::<NsStyle>() }
    }

    pub fn value_at(self, index: usize) -> Option<ValueRef<'a>> {
        let slot = *self.fields().values.get(index)?;
        NonNull::new(slot.cast_mut()).map(|p| ValueRef(p, PhantomData))
    }

    pub fn display(self) -> Display {
        self.fields().display
    }

    pub fn before(self) -> Option<StyleRef<'a>> {
        unsafe { StyleRef::from_ptr(self.fields().before) }
    }

    pub fn after(self) -> Option<StyleRef<'a>> {
        unsafe { StyleRef::from_ptr(self.fields().after) }
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

pub fn display_of(style: Option<StyleRef<'_>>) -> Display {
    unsafe { ns_css_display_of(style.map_or(core::ptr::null(), StyleRef::as_ptr)) }
}

pub fn values_equal(a: Option<ValueRef<'_>>, b: Option<ValueRef<'_>>) -> bool {
    let (a, b) = match (a, b) {
        (None, None) => return true,
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    if a.0 == b.0 {
        return true;
    }
    if a.raw().kind != b.raw().kind {
        return false;
    }
    match (a.payload(), b.payload()) {
        (Some(pa), Some(pb)) => pa == pb && values_equal(a.next_layer(), b.next_layer()),
        _ => false,
    }
}

struct Names(*mut GPtrArray);

impl Names {
    fn of(map: *const VarMap) -> Names {
        Names(unsafe { ns_var_map_names(map) })
    }

    fn get(&self) -> Vec<&CStr> {
        let Some(array) = (unsafe { self.0.as_ref() }) else {
            return Vec::new();
        };
        (0..array.len as usize)
            .map(|i| unsafe { CStr::from_ptr((*array.pdata.add(i)).cast()) })
            .collect()
    }
}

impl Drop for Names {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { g_ptr_array_unref(self.0) };
        }
    }
}

fn var_value<'a>(map: *const VarMap, name: &CStr) -> Option<&'a CStr> {
    let value = unsafe { ns_var_map_lookup(map, name.as_ptr()) };
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) })
}

fn vars_equal(a: *const VarMap, b: *const VarMap) -> bool {
    if a == b {
        return true;
    }
    let (names_a, names_b) = (Names::of(a), Names::of(b));
    let (na, nb) = (names_a.get(), names_b.get());
    na.len() == nb.len()
        && na
            .iter()
            .zip(&nb)
            .all(|(x, y)| x == y && var_value(a, x) == var_value(b, y))
}

pub fn styles_equal(a: Option<StyleRef<'_>>, b: Option<StyleRef<'_>>) -> bool {
    let (a, b) = match (a, b) {
        (None, None) => return true,
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    if a.0 == b.0 {
        return true;
    }
    let (fa, fb) = (a.fields(), b.fields());
    if fa.display != fb.display {
        return false;
    }
    if !(0..PROP_COUNT).all(|i| values_equal(a.value_at(i), b.value_at(i))) {
        return false;
    }
    let pseudo = |s: *const Style| unsafe { StyleRef::from_ptr(s) };
    vars_equal(fa.vars, fb.vars)
        && [
            (fa.before, fb.before),
            (fa.after, fb.after),
            (fa.first_letter, fb.first_letter),
            (fa.first_line, fb.first_line),
            (fa.placeholder, fb.placeholder),
            (fa.selection, fb.selection),
            (fa.marker, fb.marker),
            (fa.backdrop, fb.backdrop),
            (fa.file_selector_button, fb.file_selector_button),
        ]
        .into_iter()
        .all(|(x, y)| styles_equal(pseudo(x), pseudo(y)))
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

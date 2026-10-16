//! Southstar — computed styles read from Rust over rust/css's own style and value types: values by property, their gradients, shadows, transforms and layers, display and pseudo-element styles, custom properties, and comparing two styles.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_css::{
    BorderImageParams, Calc, Gradient, GridAreas, GridTracks, Prop as PropId, ShadowList,
    StyleData, Transform,
};
pub use southstar_css::{NsCssValue, ValueKind as Kind};
use southstar_dom::Node;
use southstar_glib::{GHashTable, g_hash_table_lookup};
use southstar_layout::{BoxRef, Style};

pub const PROP_COUNT: usize = southstar_css::PROP_COUNT;

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
const DISPLAY_INNER_GRID: u8 = 4;

impl Display {
    pub fn is_none(self) -> bool {
        self.box_ == DISPLAY_BOX_NONE
    }

    pub fn is_grid_container(self) -> bool {
        self.box_ == 0 && self.internal == 0 && self.inner == DISPLAY_INNER_GRID
    }

    fn of(parts: [u8; 5]) -> Display {
        let [box_, outer, inner, internal, list_item] = parts;
        Display {
            box_,
            outer,
            inner,
            internal,
            list_item,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SizeValue {
    pub w: f64,
    pub h: f64,
    pub w_unit: u32,
    pub h_unit: u32,
    pub w_auto: bool,
    pub h_auto: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RectValue {
    pub v: [f64; 4],
    pub is_auto: [bool; 4],
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
            .get_or_init(|| PropId::from_name(self.name.to_bytes()).map(PropId::id))
    }
}

#[derive(PartialEq)]
enum Payload<'a> {
    Keyword(Option<&'a CStr>),
    Length(f64, c_uint),
    Size([f64; 2], [c_uint; 2], [bool; 2]),
    Color([u8; 4]),
    Calc([f64; 4]),
    Url(Option<&'a CStr>),
    Rect([f64; 4], [c_uint; 4], [bool; 4]),
}

pub enum Value<'a> {
    Keyword(Option<&'a CStr>),
    Length(f64, u32),
    Color([u8; 4]),
    Calc,
    Other,
}

#[derive(Clone, Copy)]
pub struct ValueRef<'a>(&'a NsCssValue);

impl<'a> ValueRef<'a> {
    pub fn get(self) -> Value<'a> {
        match self.kind() {
            Kind::Keyword => Value::Keyword(self.0.keyword()),
            Kind::Length => self
                .0
                .length()
                .map_or(Value::Other, |(v, unit)| Value::Length(v, unit)),
            Kind::Color => self.0.color_rgba().map_or(Value::Other, Value::Color),
            Kind::Calc => Value::Calc,
            _ => Value::Other,
        }
    }

    pub fn as_ptr(self) -> *const NsCssValue {
        ptr::from_ref(self.0)
    }

    pub fn data(self) -> &'a NsCssValue {
        self.0
    }

    pub fn font_slanted(self) -> bool {
        self.keyword_text().is_some_and(|kw| {
            let kw = kw.to_bytes();
            kw == b"italic" || kw.starts_with(b"oblique")
        })
    }

    pub fn keyword_text(self) -> Option<&'a CStr> {
        self.0.keyword()
    }

    fn payload(self) -> Option<Payload<'a>> {
        let v = self.0;
        Some(match self.kind() {
            Kind::Keyword => Payload::Keyword(v.keyword()),
            Kind::Length => v.length().map(|(len, unit)| Payload::Length(len, unit))?,
            Kind::Size => v
                .size_parts()
                .map(|(len, unit, auto)| Payload::Size(len, unit, auto))?,
            Kind::Color => Payload::Color(v.color_rgba()?),
            Kind::Calc => v
                .calc()
                .map(|c| Payload::Calc([c.pct, c.px, c.em, c.rem]))?,
            Kind::Url => Payload::Url(v.url()),
            Kind::Rect => v
                .rect_parts()
                .map(|(len, unit, auto)| Payload::Rect(len, unit, auto))?,
            _ => return None,
        })
    }

    pub fn next_layer(self) -> Option<ValueRef<'a>> {
        self.0.next_layer().map(ValueRef)
    }

    /// # Safety
    ///
    /// `p` is null or a value made by rust/css that outlives `'a`.
    pub unsafe fn from_ptr(p: *const NsCssValue) -> Option<ValueRef<'a>> {
        // SAFETY: the caller passes null or a live value.
        unsafe { p.as_ref() }.map(ValueRef)
    }

    pub fn kind(self) -> Kind {
        self.0.value_kind()
    }

    pub fn is_keyword(self, kw: &CStr) -> bool {
        self.keyword_text() == Some(kw)
    }

    pub fn length(self) -> Option<(f64, u32)> {
        self.0.length()
    }

    pub fn color(self) -> Option<[u8; 4]> {
        self.0.color_rgba()
    }

    pub fn size(self) -> Option<SizeValue> {
        self.0.size_parts().map(|(len, unit, auto)| SizeValue {
            w: len[0],
            h: len[1],
            w_unit: unit[0],
            h_unit: unit[1],
            w_auto: auto[0],
            h_auto: auto[1],
        })
    }

    fn calc_head(self) -> Option<Calc> {
        self.0.calc()
    }

    pub fn calc_terms(self) -> Option<[f64; 4]> {
        self.calc_head().map(|c| [c.pct, c.px, c.em, c.rem])
    }

    pub fn calc_arg_pcts(self) -> Vec<f64> {
        self.calc_head().map_or_else(Vec::new, |c| {
            c.args[..usize::from(c.n_args).min(4)]
                .iter()
                .map(|a| a.pct)
                .collect()
        })
    }

    pub fn calc(self) -> Option<(f64, f64)> {
        self.calc_head().map(|c| (c.pct, c.px))
    }

    pub fn is_math_fn(self) -> bool {
        self.calc_head()
            .is_some_and(|c| c.func != 0 && c.n_args > 0)
    }

    pub fn math_fn_px(self, basis: f64) -> f64 {
        self.calc_head()
            .map_or(0.0, |c| southstar_css::calc_math_fn_px(&c, basis))
    }

    pub fn url(self) -> Option<&'a CStr> {
        self.0.url()
    }

    pub fn rect(self) -> Option<RectValue> {
        self.0
            .rect_parts()
            .map(|(v, _, is_auto)| RectValue { v, is_auto })
    }

    pub fn gradient(self) -> Option<&'a Gradient> {
        self.0.gradient()
    }

    pub fn shadows(self) -> Option<&'a ShadowList> {
        self.0.shadows()
    }

    pub fn tracks(self) -> Option<&'a GridTracks> {
        self.0.tracks()
    }

    pub fn areas(self) -> Option<&'a GridAreas> {
        self.0.areas()
    }

    pub fn transform(self) -> Option<&'a Transform> {
        self.0.transform()
    }

    pub fn layers(self) -> impl Iterator<Item = ValueRef<'a>> {
        core::iter::successors(Some(self), |v| v.next_layer())
    }

    pub fn length_or(self, fallback: f64) -> f64 {
        self.0.length_or(fallback)
    }

    pub fn font_weight_or(self, fallback: i32) -> i32 {
        southstar_css::font_weight_number(self.keyword_text().map(CStr::to_bytes), fallback)
    }

    pub fn font_stretch_rank(self) -> i32 {
        let percent = self
            .length()
            .filter(|&(_, unit)| unit == crate::UNIT_PERCENT)
            .map(|(v, _)| v);
        southstar_css::font_stretch_rank(self.keyword_text().map(CStr::to_bytes), percent)
    }
}

pub fn layer_count(head: Option<ValueRef<'_>>) -> i32 {
    head.map_or(0, |h| h.layers().count() as i32)
}

pub fn layer(head: Option<ValueRef<'_>>, index: i32) -> Option<ValueRef<'_>> {
    let n = layer_count(head);
    if n == 0 {
        return None;
    }
    let mut index = index % n;
    let mut at = head;
    while index > 0 {
        at = at.and_then(ValueRef::next_layer);
        index -= 1;
    }
    at
}

#[derive(Clone, Copy)]
pub struct StyleRef<'a>(NonNull<StyleData>, PhantomData<&'a StyleData>);

const BEFORE: usize = 0;
const AFTER: usize = 1;
const FIRST_LETTER: usize = 2;
const FIRST_LINE: usize = 3;
const PLACEHOLDER: usize = 4;
const SELECTION: usize = 5;
const MARKER: usize = 6;
const BACKDROP: usize = 7;
const FILE_SELECTOR_BUTTON: usize = 8;
const HIDDEN_BEFORE: usize = 9;
const HIDDEN_AFTER: usize = 10;

impl<'a> StyleRef<'a> {
    /// # Safety
    ///
    /// `p` is null or a computed style made by rust/css that outlives `'a`.
    pub unsafe fn from_ptr(p: *const Style) -> Option<StyleRef<'a>> {
        NonNull::new(p.cast::<StyleData>().cast_mut()).map(|p| StyleRef(p, PhantomData))
    }

    pub fn as_ptr(self) -> *const Style {
        self.0.as_ptr().cast_const().cast()
    }

    pub fn retained(style: &'a southstar_css::RetainedStyle) -> StyleRef<'a> {
        let p = style.as_ptr().cast_mut();
        StyleRef(NonNull::new(p).expect("a retained style"), PhantomData)
    }

    pub fn data(self) -> &'a StyleData {
        // SAFETY: a StyleRef points at a live computed style for 'a.
        unsafe { self.0.as_ref() }
    }

    pub fn value(self, prop: &Prop) -> Option<ValueRef<'a>> {
        self.value_at(prop.id()?)
    }

    pub fn keyword(self, prop: &Prop) -> Option<&'a CStr> {
        self.data().keyword(PropId::from_id(prop.id()?)?)
    }

    pub fn value_at(self, index: usize) -> Option<ValueRef<'a>> {
        self.data().value_at(index).map(ValueRef)
    }

    pub fn display(self) -> Display {
        Display::of(self.data().display_parts())
    }

    pub fn specified_inline(self) -> bool {
        self.data().specified_inline()
    }

    pub fn text_fill_color(self) -> Option<ValueRef<'a>> {
        self.get(PropId::WebkitTextFillColor)
            .filter(|v| v.color().is_some())
            .or_else(|| self.get(PropId::Color))
    }

    pub fn from_currentcolor(self, prop: PropId) -> bool {
        southstar_css::currentcolor_bit(prop)
            .is_some_and(|bit| (self.data().currentcolor_bits() >> bit) & 1 != 0)
    }

    pub fn vars_ptr(self) -> *const c_void {
        self.data().vars_ptr()
    }

    pub fn get(self, prop: PropId) -> Option<ValueRef<'a>> {
        self.value_at(prop as usize)
    }

    pub fn keyword_of(self, prop: PropId) -> Option<&'a CStr> {
        self.data().keyword(prop)
    }

    fn pseudo(self, slot: usize) -> Option<StyleRef<'a>> {
        let pseudo = self.data().pseudo_ptr(slot);
        NonNull::new(pseudo.cast_mut()).map(|p| StyleRef(p, PhantomData))
    }

    pub fn first_line(self) -> Option<StyleRef<'a>> {
        self.pseudo(FIRST_LINE)
    }

    pub fn selection(self) -> Option<StyleRef<'a>> {
        self.pseudo(SELECTION)
    }

    pub fn marker(self) -> Option<StyleRef<'a>> {
        self.pseudo(MARKER)
    }

    pub fn backdrop(self) -> Option<StyleRef<'a>> {
        self.pseudo(BACKDROP)
    }

    pub fn before(self) -> Option<StyleRef<'a>> {
        self.pseudo(BEFORE)
    }

    pub fn after(self) -> Option<StyleRef<'a>> {
        self.pseudo(AFTER)
    }

    pub fn first_letter(self) -> Option<StyleRef<'a>> {
        self.pseudo(FIRST_LETTER)
    }

    pub fn placeholder(self) -> Option<StyleRef<'a>> {
        self.pseudo(PLACEHOLDER)
    }

    pub fn file_selector_button(self) -> Option<StyleRef<'a>> {
        self.pseudo(FILE_SELECTOR_BUTTON)
    }

    pub fn hidden_before(self) -> Option<StyleRef<'a>> {
        self.pseudo(HIDDEN_BEFORE)
    }

    pub fn hidden_after(self) -> Option<StyleRef<'a>> {
        self.pseudo(HIDDEN_AFTER)
    }

    pub fn has_vars(self) -> bool {
        self.data().has_vars()
    }

    pub fn var(self, name: &CStr) -> Option<&'a CStr> {
        self.data().var(name.to_bytes())
    }

    pub fn var_names(self) -> Vec<Vec<u8>> {
        self.data()
            .var_names()
            .into_iter()
            .map(|name| name.to_bytes().to_vec())
            .collect()
    }

    pub fn var_count(self) -> usize {
        self.data().var_names().len()
    }

    pub fn resolve_vars(self, text: &CStr) -> Option<Vec<u8>> {
        self.data().resolve_vars(text.to_bytes())
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

/// # Safety
///
/// `style` is null or a live computed style made by rust/css that nothing else is reading or writing.
pub unsafe fn value_slot(style: *mut Style, index: usize) -> Option<*mut *mut NsCssValue> {
    // SAFETY: the caller's obligations are value_slot's.
    unsafe { southstar_css::style_value_slot(style, index) }
}

pub fn box_style(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    // SAFETY: a box's style pointer is null or the computed style the box holds for as long as it lives.
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub fn display_of(style: Option<StyleRef<'_>>) -> Display {
    style.map_or(Display::of([0; 5]), StyleRef::display)
}

pub fn values_equal(a: Option<ValueRef<'_>>, b: Option<ValueRef<'_>>) -> bool {
    let (a, b) = match (a, b) {
        (None, None) => return true,
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    if ptr::eq(a.0, b.0) {
        return true;
    }
    if a.0.kind() != b.0.kind() {
        return false;
    }
    match (a.payload(), b.payload()) {
        (Some(pa), Some(pb)) => pa == pb && values_equal(a.next_layer(), b.next_layer()),
        _ => false,
    }
}

fn vars_equal(a: StyleRef<'_>, b: StyleRef<'_>) -> bool {
    if a.data().same_vars(b.data()) {
        return true;
    }
    let (na, nb) = (a.data().var_names(), b.data().var_names());
    na.len() == nb.len()
        && na
            .iter()
            .zip(&nb)
            .all(|(x, y)| x == y && a.data().var(x.to_bytes()) == b.data().var(y.to_bytes()))
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
    if a.display() != b.display() {
        return false;
    }
    if !(0..PROP_COUNT).all(|i| values_equal(a.value_at(i), b.value_at(i))) {
        return false;
    }
    vars_equal(a, b)
        && (BEFORE..=FILE_SELECTOR_BUTTON).all(|slot| styles_equal(a.pseudo(slot), b.pseudo(slot)))
}

pub fn font_family_for_pango(family: &CStr) -> CString {
    CString::new(southstar_css::font_family_for_pango(family.to_bytes())).unwrap_or_default()
}

pub fn font_stretch_rank(v: Option<ValueRef<'_>>) -> i32 {
    v.map_or_else(
        || southstar_css::font_stretch_rank(None, None),
        ValueRef::font_stretch_rank,
    )
}

pub fn effective_transform<'a>(
    style: Option<StyleRef<'a>>,
    transform_override: Option<&'a Transform>,
) -> Transform {
    let tf = |prop| {
        style
            .and_then(|s| s.get(prop))
            .and_then(ValueRef::transform)
    };
    southstar_css::effective_transform(
        [tf(PropId::Translate), tf(PropId::Rotate), tf(PropId::Scale)],
        transform_override.or_else(|| tf(PropId::Transform)),
    )
}

pub fn border_image_source(style: Option<StyleRef<'_>>) -> Option<ValueRef<'_>> {
    style
        .and_then(|s| s.get(PropId::BorderImageSource))
        .filter(|v| matches!(v.kind(), Kind::Url | Kind::Gradient))
}

pub fn border_image_params(style: StyleRef<'_>) -> BorderImageParams {
    let text = |prop, fallback: &'static [u8]| {
        style
            .get(prop)
            .and_then(ValueRef::keyword_text)
            .map(CStr::to_bytes)
            .filter(|t| !t.is_empty())
            .unwrap_or(fallback)
    };
    southstar_css::border_image_params(
        text(PropId::BorderImageSlice, b"100%"),
        text(PropId::BorderImageWidth, b"1"),
        text(PropId::BorderImageOutset, b"0"),
        text(PropId::BorderImageRepeat, b"stretch"),
    )
}

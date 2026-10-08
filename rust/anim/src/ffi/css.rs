//! Southstar — the css.h side of the animation engine: refcounted values, declaration blocks, @keyframes rules, transition and animation lists and computed styles, as css.c lays them out.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean};
use southstar_layout::Style;
use southstar_style::{NsCssValue, StyleRef};

use crate::timing::Timing;

pub const PROP_COUNT: usize = southstar_style::PROP_COUNT;

const KIND_KEYWORD: c_uint = 0;
const KIND_LENGTH: c_uint = 1;
const KIND_COLOR: c_uint = 3;
const KIND_TRANSFORM: c_uint = 9;
const UNIT_PERCENT: c_uint = 3;
const REDUCED_MOTION_REDUCE: c_int = 1;

unsafe extern "C" {
    fn ns_css_value_dup(v: *const NsCssValue) -> *mut NsCssValue;
    fn ns_css_value_free(v: *mut NsCssValue);
    fn ns_css_value_equal(a: *const NsCssValue, b: *const NsCssValue) -> GBoolean;
    fn ns_css_value_interpolate(
        a: *const NsCssValue,
        b: *const NsCssValue,
        t: f64,
    ) -> *mut NsCssValue;
    fn ns_css_prop_name(prop: c_int) -> *const c_char;
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_css_prop_inherits(prop: c_int) -> GBoolean;
    fn ns_css_initial_value_text(name: *const c_char) -> *const c_char;
    fn ns_css_parse_declarations(text: *const c_char) -> *mut GArray;
    fn ns_css_declarations_free(decls: *mut GArray);
    fn ns_css_timing_parse(text: *const c_char, out: *mut Timing) -> GBoolean;
    fn ns_css_timing_serialize(t: *const Timing) -> *mut c_char;
    fn ns_css_anim_effective(style: *const Style, is_animation: GBoolean, out: *mut NsAnimList);
    fn ns_css_anim_list_clear(list: *mut NsAnimList);
    fn ns_css_style_may_animate(style: *const Style) -> GBoolean;
    fn ns_css_style_before_change(node: *const c_void) -> *const Style;
    fn ns_style_prop_from_currentcolor(style: *const Style, prop: c_int) -> GBoolean;
    fn ns_style_free(style: *mut Style);
    fn ns_css_get_reduced_motion() -> c_int;
    fn ns_css_incremental_exclude(node: *const c_void, exclude: GBoolean);
    fn ns_css_keyframes_resolve(kf: *const NsKeyframes, vars: *const c_void) -> *mut NsKeyframes;
    fn ns_css_keyframes_resolved_free(kf: *mut NsKeyframes);
    fn g_array_remove_index(array: *mut GArray, index: c_uint) -> *mut GArray;
    fn g_array_sized_new(
        zero_terminated: GBoolean,
        clear: GBoolean,
        element_size: c_uint,
        reserved: c_uint,
    ) -> *mut GArray;
    fn g_intern_string(s: *const c_char) -> *const c_char;
}

#[repr(C)]
struct NsDecl {
    prop: c_int,
    value: *mut NsCssValue,
    important: GBoolean,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NsAnimEntry {
    target: c_int,
    name: *const c_char,
    duration_ms: f64,
    delay_ms: f64,
    timing: Timing,
    _iter_count: c_int,
    iterations: f64,
    direction: c_int,
    fill: c_int,
    paused: GBoolean,
    _duration_auto: GBoolean,
    allow_discrete: GBoolean,
}

#[repr(C)]
struct NsAnimList {
    n: c_int,
    entries: [NsAnimEntry; 8],
}

#[repr(C)]
struct KeyframeStop {
    pct: f64,
    _body: [u8; 2160],
    raw_props: *mut c_char,
}

#[repr(C)]
pub struct NsKeyframes {
    name: *mut c_char,
    n_stops: c_int,
    stops: *mut KeyframeStop,
}

#[repr(C)]
struct SheetHead {
    _rules: *mut c_void,
    _imports: *mut c_void,
    _layer_names: *mut c_void,
    _layers: *mut c_void,
    _font_faces: *mut c_void,
    keyframes: *mut GArray,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsDecl>() == 24
        && core::mem::size_of::<NsAnimEntry>() == 120
        && core::mem::offset_of!(NsAnimEntry, timing) == 32
        && core::mem::offset_of!(NsAnimEntry, iterations) == 88
        && core::mem::offset_of!(NsAnimEntry, allow_discrete) == 112
        && core::mem::size_of::<NsAnimList>() == 968
        && core::mem::size_of::<KeyframeStop>() == 2176
        && core::mem::size_of::<NsKeyframes>() == 24
        && core::mem::offset_of!(SheetHead, keyframes) == 40
);

fn text<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub struct Val(NonNull<NsCssValue>);

unsafe impl Send for Val {}

impl Val {
    fn adopt(p: *mut NsCssValue) -> Option<Val> {
        NonNull::new(p).map(Val)
    }

    pub fn retain(v: Borrowed) -> Option<Val> {
        Val::adopt(unsafe { ns_css_value_dup(v.0) })
    }

    pub fn placeholder() -> Val {
        let raw = unsafe { glib::g_malloc0(core::mem::size_of::<NsCssValue>()) }.cast();
        Val(NonNull::new(raw).expect("g_malloc0 aborts on failure"))
    }

    pub fn borrow(&self) -> Borrowed {
        Borrowed(self.0.as_ptr())
    }

    pub fn interpolate(a: Borrowed, b: Borrowed, t: f64) -> Option<Val> {
        Val::adopt(unsafe { ns_css_value_interpolate(a.0, b.0, t) })
    }

    fn into_raw(self) -> *mut NsCssValue {
        let raw = self.0.as_ptr();
        core::mem::forget(self);
        raw
    }
}

impl Clone for Val {
    fn clone(&self) -> Val {
        Val::retain(self.borrow()).expect("dup of a live value")
    }
}

impl Drop for Val {
    fn drop(&mut self) {
        unsafe { ns_css_value_free(self.0.as_ptr()) };
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Borrowed(*const NsCssValue);

impl Borrowed {
    pub const NULL: Borrowed = Borrowed(ptr::null());

    pub fn of(v: Option<&Val>) -> Borrowed {
        v.map_or(Borrowed::NULL, Val::borrow)
    }

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn as_ptr(self) -> *const NsCssValue {
        self.0
    }

    pub fn equal(self, other: Borrowed) -> bool {
        unsafe { ns_css_value_equal(self.0, other.0) != 0 }
    }

    fn head(self) -> Option<&'static ValueHead> {
        unsafe { self.0.cast::<ValueHead>().as_ref() }
    }

    pub fn is_placeholder(self) -> bool {
        self.head()
            .is_some_and(|h| h.kind == KIND_KEYWORD && unsafe { h.u.keyword.is_null() })
    }

    pub fn keyword(self) -> Option<&'static CStr> {
        let h = self.head().filter(|h| h.kind == KIND_KEYWORD)?;
        text(unsafe { h.u.keyword })
    }

    pub fn opacity(self) -> Option<f64> {
        let h = self.head().filter(|h| h.kind == KIND_LENGTH)?;
        let length = unsafe { h.u.length };
        Some(if length.unit == UNIT_PERCENT {
            length.v / 100.0
        } else {
            length.v
        })
    }

    pub fn color(self) -> Option<[u8; 4]> {
        let h = self.head().filter(|h| h.kind == KIND_COLOR)?;
        Some(unsafe { h.u.color })
    }

    pub fn transform(self) -> Option<*const c_void> {
        let h = self.head().filter(|h| h.kind == KIND_TRANSFORM)?;
        (unsafe { h.u.n_ops } != 0).then(|| core::ptr::addr_of!(h.u).cast())
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Length {
    v: f64,
    unit: c_uint,
}

#[repr(C)]
union HeadUnion {
    keyword: *const c_char,
    length: Length,
    color: [u8; 4],
    n_ops: c_int,
}

#[repr(C)]
struct ValueHead {
    kind: c_uint,
    _ref: c_int,
    u: HeadUnion,
}

pub fn prop_name(prop: i32) -> Option<&'static CStr> {
    text(unsafe { ns_css_prop_name(prop) })
}

pub fn prop_id(name: &CStr) -> i32 {
    unsafe { ns_css_prop_id(name.as_ptr()) }
}

pub fn prop_inherits(prop: i32) -> bool {
    unsafe { ns_css_prop_inherits(prop) != 0 }
}

pub fn reduced_motion() -> bool {
    unsafe { ns_css_get_reduced_motion() == REDUCED_MOTION_REDUCE }
}

pub fn intern(name: &CStr) -> *const c_char {
    unsafe { g_intern_string(name.as_ptr()) }
}

pub fn initial_value(prop: i32) -> Option<Val> {
    let name = prop_name(prop)?;
    let text = text(unsafe { ns_css_initial_value_text(name.as_ptr()) })?;
    let mut decl = name.to_bytes().to_vec();
    decl.extend_from_slice(b": ");
    decl.extend_from_slice(text.to_bytes());
    decl.push(0);
    let decls = Decls::parse_bytes(&decl)?;
    decls.take_first(prop)
}

impl Timing {
    pub fn parse_into(&mut self, text: &CStr) -> bool {
        unsafe { ns_css_timing_parse(text.as_ptr(), self) != 0 }
    }

    pub fn serialize(&self) -> Option<glib::GStr> {
        unsafe { glib::GStr::take(ns_css_timing_serialize(self)) }
    }
}

pub struct Decls(NonNull<GArray>);

unsafe impl Send for Decls {}

impl Decls {
    fn adopt(p: *mut GArray) -> Option<Decls> {
        NonNull::new(p).map(Decls)
    }

    pub fn parse(text: Option<&CStr>) -> Option<Decls> {
        Decls::adopt(unsafe { ns_css_parse_declarations(text.map_or(ptr::null(), CStr::as_ptr)) })
    }

    fn parse_bytes(nul_terminated: &[u8]) -> Option<Decls> {
        Decls::adopt(unsafe { ns_css_parse_declarations(nul_terminated.as_ptr().cast()) })
    }

    pub fn as_ptr(&self) -> *const GArray {
        self.0.as_ptr()
    }

    fn entries(&self) -> &[NsDecl] {
        let array = unsafe { self.0.as_ref() };
        if array.data.is_null() {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(array.data.cast::<NsDecl>(), array.len as usize) }
    }

    pub fn props(&self) -> impl Iterator<Item = i32> + '_ {
        self.entries().iter().map(|d| d.prop)
    }

    pub fn value_of(&self, prop: i32) -> Borrowed {
        self.entries()
            .iter()
            .filter(|d| d.prop == prop)
            .last()
            .map_or(Borrowed::NULL, |d| Borrowed(d.value))
    }

    pub fn drop_important(&mut self) {
        for index in (0..self.entries().len()).rev() {
            let entry = &self.entries()[index];
            if entry.important == 0 {
                continue;
            }
            unsafe {
                ns_css_value_free(entry.value);
                g_array_remove_index(self.0.as_ptr(), index as c_uint);
            }
        }
    }

    fn take_first(self, prop: i32) -> Option<Val> {
        let first = self.entries().first()?;
        if first.prop != prop {
            return None;
        }
        let value = first.value;
        let data = unsafe { (*self.0.as_ptr()).data.cast::<NsDecl>() };
        unsafe { (*data).value = ptr::null_mut() };
        Val::adopt(value)
    }

    pub fn duplicate(&self) -> Decls {
        let entries = self.entries();
        let array = unsafe {
            g_array_sized_new(
                0,
                0,
                core::mem::size_of::<NsDecl>() as c_uint,
                entries.len() as c_uint,
            )
        };
        for d in entries {
            let copy = NsDecl {
                prop: d.prop,
                value: unsafe { ns_css_value_dup(d.value) },
                important: d.important,
            };
            unsafe { glib::g_array_append_vals(array, ptr::addr_of!(copy).cast(), 1) };
        }
        Decls::adopt(array).expect("g_array_sized_new aborts on failure")
    }
}

impl Drop for Decls {
    fn drop(&mut self) {
        unsafe { ns_css_declarations_free(self.0.as_ptr()) };
    }
}

pub trait KeyframeSource {
    fn raw(&self) -> *const NsKeyframes;

    fn stops(&self) -> Vec<(f64, Option<&CStr>)> {
        let kf = unsafe { &*self.raw() };
        if kf.stops.is_null() || kf.n_stops <= 0 {
            return Vec::new();
        }
        let stops = unsafe { core::slice::from_raw_parts(kf.stops, kf.n_stops as usize) };
        stops.iter().map(|s| (s.pct, text(s.raw_props))).collect()
    }
}

pub struct Keyframes(NonNull<NsKeyframes>);

unsafe impl Send for Keyframes {}

impl Keyframes {
    fn copy_of(src: &NsKeyframes) -> Keyframes {
        let copy =
            unsafe { glib::g_malloc0(core::mem::size_of::<NsKeyframes>()) }.cast::<NsKeyframes>();
        unsafe {
            (*copy).name = glib::g_strdup(src.name);
            (*copy).n_stops = src.n_stops;
            if src.n_stops > 0 {
                let n = src.n_stops as usize;
                let stops =
                    glib::g_malloc(n * core::mem::size_of::<KeyframeStop>()).cast::<KeyframeStop>();
                ptr::copy_nonoverlapping(src.stops, stops, n);
                for i in 0..n {
                    let stop = &mut *stops.add(i);
                    stop.raw_props = glib::g_strdup(stop.raw_props);
                }
                (*copy).stops = stops;
            }
        }
        Keyframes(NonNull::new(copy).expect("g_malloc0 aborts on failure"))
    }

    pub fn resolve(&self, vars: *const c_void) -> Option<ResolvedKeyframes> {
        NonNull::new(unsafe { ns_css_keyframes_resolve(self.0.as_ptr(), vars) })
            .map(ResolvedKeyframes)
    }
}

impl KeyframeSource for Keyframes {
    fn raw(&self) -> *const NsKeyframes {
        self.0.as_ptr()
    }
}

impl Drop for Keyframes {
    fn drop(&mut self) {
        let kf = self.0.as_ptr();
        unsafe {
            glib::g_free((*kf).name.cast());
            for i in 0..(*kf).n_stops.max(0) as usize {
                glib::g_free((*(*kf).stops.add(i)).raw_props.cast());
            }
            glib::g_free((*kf).stops.cast());
            glib::g_free(kf.cast());
        }
    }
}

pub struct ResolvedKeyframes(NonNull<NsKeyframes>);

unsafe impl Send for ResolvedKeyframes {}

impl KeyframeSource for ResolvedKeyframes {
    fn raw(&self) -> *const NsKeyframes {
        self.0.as_ptr()
    }
}

impl Drop for ResolvedKeyframes {
    fn drop(&mut self) {
        unsafe { ns_css_keyframes_resolved_free(self.0.as_ptr()) };
    }
}

pub fn sheet_keyframes(sheet: *const c_void) -> Vec<(Vec<u8>, Keyframes)> {
    let Some(head) = (unsafe { sheet.cast::<SheetHead>().as_ref() }) else {
        return Vec::new();
    };
    let Some(array) = (unsafe { head.keyframes.as_ref() }) else {
        return Vec::new();
    };
    if array.data.is_null() {
        return Vec::new();
    }
    let rules = unsafe {
        core::slice::from_raw_parts(array.data.cast::<NsKeyframes>(), array.len as usize)
    };
    rules
        .iter()
        .filter_map(|kf| Some((text(kf.name)?.to_bytes().to_vec(), Keyframes::copy_of(kf))))
        .collect()
}

#[derive(Clone, Copy)]
pub struct Entry<'a>(&'a NsAnimEntry);

pub const TARGET_ALL: c_int = 1;
const TARGET_OPACITY: c_int = 2;
const TARGET_TRANSFORM: c_int = 3;
const TARGET_COLOR: c_int = 4;
const TARGET_BG_COLOR: c_int = 5;
const TARGET_OTHER: c_int = 6;

pub enum Target<'a> {
    All,
    Opacity,
    Transform,
    Color,
    BackgroundColor,
    Other(Option<&'a CStr>),
    None,
}

impl<'a> Entry<'a> {
    pub fn target(self) -> Target<'a> {
        match self.0.target {
            TARGET_ALL => Target::All,
            TARGET_OPACITY => Target::Opacity,
            TARGET_TRANSFORM => Target::Transform,
            TARGET_COLOR => Target::Color,
            TARGET_BG_COLOR => Target::BackgroundColor,
            TARGET_OTHER => Target::Other(text(self.0.name)),
            _ => Target::None,
        }
    }

    pub fn name(self) -> Option<&'a CStr> {
        text(self.0.name)
    }

    pub fn duration_ms(self) -> f64 {
        self.0.duration_ms
    }

    pub fn delay_ms(self) -> f64 {
        self.0.delay_ms
    }

    pub fn timing(self) -> Timing {
        self.0.timing
    }

    pub fn iterations(self) -> f64 {
        self.0.iterations
    }

    pub fn direction(self) -> c_int {
        self.0.direction
    }

    pub fn fill(self) -> c_int {
        self.0.fill
    }

    pub fn paused(self) -> bool {
        self.0.paused != 0
    }

    pub fn allow_discrete(self) -> bool {
        self.0.allow_discrete != 0
    }
}

pub struct AnimList(Box<NsAnimList>);

impl AnimList {
    pub fn effective(style: StyleRef<'_>, animations: bool) -> AnimList {
        let mut list = Box::new(NsAnimList {
            n: 0,
            entries: [NsAnimEntry {
                target: 0,
                name: ptr::null(),
                duration_ms: 0.0,
                delay_ms: 0.0,
                timing: Timing::default(),
                _iter_count: 0,
                iterations: 0.0,
                direction: 0,
                fill: 0,
                paused: 0,
                _duration_auto: 0,
                allow_discrete: 0,
            }; 8],
        });
        unsafe { ns_css_anim_effective(style.as_ptr(), glib::boolean(animations), &mut *list) };
        AnimList(list)
    }

    pub fn entries(&self) -> Vec<Entry<'_>> {
        let n = usize::try_from(self.0.n)
            .unwrap_or(0)
            .min(self.0.entries.len());
        self.0.entries[..n].iter().map(Entry).collect()
    }

    pub fn len(&self) -> usize {
        usize::try_from(self.0.n).unwrap_or(0)
    }

    pub fn clear(&mut self) {
        unsafe { ns_css_anim_list_clear(&mut *self.0) };
        self.0.n = 0;
    }
}

impl Drop for AnimList {
    fn drop(&mut self) {
        unsafe { ns_css_anim_list_clear(&mut *self.0) };
    }
}

pub struct RetainedStyle(NonNull<Style>);

unsafe impl Send for RetainedStyle {}

impl RetainedStyle {
    pub fn retain(style: StyleRef<'_>) -> RetainedStyle {
        let p = style.as_ptr().cast_mut();
        unsafe { southstar_style::retain(p) };
        RetainedStyle(NonNull::new(p).expect("a live style"))
    }

    pub fn get(&self) -> StyleRef<'static> {
        unsafe { StyleRef::from_ptr(self.0.as_ptr()) }.expect("a live style")
    }

    pub fn ptr(&self) -> *const Style {
        self.0.as_ptr()
    }
}

impl Drop for RetainedStyle {
    fn drop(&mut self) {
        unsafe { ns_style_free(self.0.as_ptr()) };
    }
}

pub fn style_value(style: StyleRef<'_>, prop: i32) -> Borrowed {
    usize::try_from(prop)
        .ok()
        .and_then(|i| style.value_at(i))
        .map_or(Borrowed::NULL, |v| Borrowed(v.as_ptr()))
}

pub fn from_currentcolor(style: StyleRef<'_>, prop: i32) -> bool {
    unsafe { ns_style_prop_from_currentcolor(style.as_ptr(), prop) != 0 }
}

pub fn may_animate(style: StyleRef<'_>) -> bool {
    unsafe { ns_css_style_may_animate(style.as_ptr()) != 0 }
}

fn style_before_change<'a>(node: Node<'_>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(ns_css_style_before_change(node.as_ptr().cast())) }
}

fn incremental_exclude(node: *const NsNode, exclude: bool) {
    unsafe { ns_css_incremental_exclude(node.cast(), glib::boolean(exclude)) };
}

pub struct StyleMut(NonNull<Style>);

impl StyleMut {
    pub fn of(style: StyleRef<'_>) -> StyleMut {
        StyleMut(NonNull::new(style.as_ptr().cast_mut()).expect("a live style"))
    }

    pub fn value(&self, prop: i32) -> Borrowed {
        self.slot(prop)
            .map_or(Borrowed::NULL, |slot| Borrowed(unsafe { *slot }))
    }

    fn slot(&self, prop: i32) -> Option<*mut *mut NsCssValue> {
        let index = usize::try_from(prop).ok()?;
        unsafe { southstar_style::value_slot(self.0.as_ptr(), index) }
    }

    pub fn replace(&self, prop: i32, value: Val) -> Option<Val> {
        let slot = self.slot(prop)?;
        let old = unsafe { core::mem::replace(&mut *slot, value.into_raw()) };
        Val::adopt(old)
    }

    pub fn set_retained(&self, prop: i32, value: &Val) {
        let Some(slot) = self.slot(prop) else {
            return;
        };
        let current = unsafe { *slot };
        if current == value.borrow().0.cast_mut() {
            return;
        }
        unsafe {
            ns_css_value_free(current);
            *slot = ns_css_value_dup(value.borrow().0);
        }
    }
}

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
}

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodePtr(*const NsNode);

unsafe impl Send for NodePtr {}

impl NodePtr {
    pub fn new(node: *const NsNode) -> Option<NodePtr> {
        (!node.is_null()).then_some(NodePtr(node))
    }

    pub fn key(self) -> usize {
        self.0 as usize
    }

    pub fn as_ptr(self) -> *const NsNode {
        self.0
    }

    fn node(self) -> Node<'static> {
        unsafe { Node::from_ptr(self.0) }.expect("a live node")
    }

    pub fn parent(self) -> Option<NodePtr> {
        self.node().parent().and_then(|p| NodePtr::new(p.as_ptr()))
    }

    pub fn first_child(self) -> Option<NodePtr> {
        self.node()
            .first_child()
            .and_then(|c| NodePtr::new(c.as_ptr()))
    }

    pub fn next_in_subtree(self, root: NodePtr, descend: bool) -> Option<NodePtr> {
        southstar_dom::index::next_in_subtree(self.node(), Some(root.node()), descend)
            .and_then(|n| NodePtr::new(n.as_ptr()))
    }

    pub fn ancestors(self) -> impl Iterator<Item = NodePtr> {
        core::iter::successors(self.parent(), |p| p.parent())
    }

    pub fn style_in(self, styles: StylesTable) -> Option<StyleRef<'static>> {
        if styles.0.is_null() {
            return None;
        }
        let style = unsafe { glib::g_hash_table_lookup(styles.0, self.0.cast()) };
        unsafe { StyleRef::from_ptr(style.cast()) }
    }

    pub fn style_before_change(self) -> Option<StyleRef<'static>> {
        style_before_change(self.node())
    }

    pub fn exclude_from_incremental(self, exclude: bool) {
        incremental_exclude(self.0, exclude);
    }
}

#[derive(Clone, Copy)]
pub struct StylesTable(*mut glib::GHashTable);

impl StylesTable {
    pub fn new(table: *mut glib::GHashTable) -> Option<StylesTable> {
        (!table.is_null()).then_some(StylesTable(table))
    }

    pub fn contains(self, node: NodePtr) -> bool {
        unsafe { glib::g_hash_table_contains(self.0, node.0.cast()) != 0 }
    }

    pub fn entries(self) -> Vec<(NodePtr, StyleRef<'static>)> {
        unsafe { glib::hash_table_entries(self.0) }
            .filter_map(|(k, v)| {
                Some((NodePtr::new(k.cast())?, unsafe {
                    StyleRef::from_ptr(v.cast())
                }?))
            })
            .collect()
    }
}

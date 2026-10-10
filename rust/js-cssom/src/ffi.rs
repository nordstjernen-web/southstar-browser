//! Southstar — the C ABI of the CSSOM bindings as declared in src/js_internal.h, and the js.c, style-engine and animation calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_mat4::Mat4;
use southstar_style::{NsCssValue, StyleRef, StyleTable, Transform, ValueRef};

use crate::{computed, css, declaration};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn ptr(self) -> *const NsJs {
        self.0 as *const NsJs
    }

    fn mut_ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Timing {
    kind: c_int,
    steps: c_int,
    step_pos: c_int,
    jump_keyword: c_int,
    cb: [f64; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct AnimEntry {
    pub target: c_int,
    pub name: *mut c_char,
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub timing: Timing,
    pub iter_count: c_int,
    pub iterations: f64,
    pub direction: c_int,
    pub fill: c_int,
    pub paused: GBoolean,
    pub duration_auto: GBoolean,
    pub allow_discrete: GBoolean,
}

const ANIM_ENTRIES_MAX: usize = 8;

#[repr(C)]
struct RawAnimList {
    n: c_int,
    entries: [AnimEntry; ANIM_ENTRIES_MAX],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<AnimEntry>() == 120
        && core::mem::offset_of!(AnimEntry, timing) == 32
        && core::mem::offset_of!(AnimEntry, iterations) == 88
        && core::mem::size_of::<RawAnimList>() == 968
);

pub(crate) struct AnimList(RawAnimList);

impl AnimList {
    fn empty() -> AnimList {
        AnimList(unsafe { core::mem::zeroed() })
    }

    pub fn entries(&self) -> &[AnimEntry] {
        let n = usize::try_from(self.0.n).unwrap_or(0).min(ANIM_ENTRIES_MAX);
        &self.0.entries[..n]
    }

    pub fn shorthand_serialize(&self, is_animation: bool) -> String {
        take_text(unsafe { ns_css_anim_shorthand_serialize(&self.0, glib::boolean(is_animation)) })
            .unwrap_or_default()
    }
}

impl Drop for AnimList {
    fn drop(&mut self) {
        unsafe { ns_css_anim_list_clear(&mut self.0) };
    }
}

pub(crate) fn anim_entry_timing(entry: &AnimEntry) -> String {
    take_text(unsafe { ns_css_timing_serialize(&entry.timing) }).unwrap_or_default()
}

#[repr(C)]
pub(crate) struct AnimInfo {
    pub node: *const NsNode,
    pub prop: c_int,
    pub run: c_int,
    pub name: *const c_char,
    pub fill: *const c_char,
    pub direction: *const c_char,
    pub easing: [c_char; 96],
    pub current_ms: f64,
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub iterations: f64,
    pub active: GBoolean,
    pub paused: GBoolean,
    pub pending: GBoolean,
    pub finished: GBoolean,
    pub generation: c_uint,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<AnimInfo>() == 192
        && core::mem::offset_of!(AnimInfo, current_ms) == 136
        && core::mem::offset_of!(AnimInfo, generation) == 184
);

#[repr(C)]
struct AnimScriptTiming {
    duration_ms: f64,
    delay_ms: f64,
    iterations: f64,
    direction: *const c_char,
    fill: *const c_char,
    easing: *const c_char,
}

#[repr(C)]
struct RawDecl {
    prop: c_int,
    value: *const NsCssValue,
    important: GBoolean,
}

#[repr(C)]
struct RawSheet {
    rules: *mut GPtrArray,
}

#[repr(C)]
struct RawRule {
    _selectors: *mut GPtrArray,
    decls: *mut GArray,
}

type AnimVisitCb = unsafe extern "C" fn(info: *const AnimInfo, user: *mut c_void);
type KeyframeCb = unsafe extern "C" fn(
    offset: f64,
    easing: *const c_char,
    decls: *const GArray,
    user: *mut c_void,
);

unsafe extern "C" {
    fn JS_IsHostAccess(ctx: *mut JSContext) -> bool;
    fn ns_js_style_table(js: *const NsJs) -> *mut GHashTable;
    fn ns_js_anim(js: *const NsJs) -> *mut c_void;
    fn ns_js_flush_layout(js: *mut NsJs);
    fn ns_js_flush_style(js: *mut NsJs);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_layout_root(js: *const NsJs) -> *const NsBox;
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_element_is_rendered(ctx: *mut JSContext, value: JSValue) -> GBoolean;
    fn ns_style_decl_node(value: JSValue) -> *mut NsNode;
    fn ns_style_decl_proto(ctx: *mut JSContext) -> JSValue;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_unwrap_element(value: JSValue) -> *const NsNode;
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;
    fn ns_layout_grid_resolved_tracks(b: *const NsBox, columns: GBoolean) -> *mut c_char;
    fn ns_input_is_one_line_text(node: *const NsNode) -> GBoolean;
    fn ns_paint_normal_line_height_px(style: *const Style) -> f64;
    fn ns_paint_css_line_height_px(style: *const Style) -> f64;
    fn ns_engine_linked_css_text(url: *const c_char) -> *mut c_char;

    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_css_prop_name(prop: c_int) -> *const c_char;
    fn ns_css_initial_value_text(name: *const c_char) -> *const c_char;
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
    fn ns_css_transform_to_mat4(tf: *const Transform, bw: f64, bh: f64, out: *mut Mat4);
    fn ns_css_value_serialize(v: *const NsCssValue) -> *mut c_char;
    fn ns_css_dimension_px(v: *const NsCssValue, font_size: f64, basis: f64) -> f64;
    fn ns_css_parse_declarations(text: *const c_char) -> *mut GArray;
    fn ns_css_declarations_free(decls: *mut GArray);
    fn ns_css_stylesheet_parse(text: *const c_char, len: isize) -> *mut RawSheet;
    fn ns_css_stylesheet_free(sheet: *mut RawSheet);
    fn ns_css_background_position_join(xs: *const c_char, ys: *const c_char) -> *mut c_char;
    fn ns_css_grid_placement_compose(values: *const *mut c_char, area: GBoolean) -> *mut c_char;
    fn ns_css_grid_shorthand_compose(values: *const *mut c_char, full: GBoolean) -> *mut c_char;
    fn ns_css_tracks_computed_serialize(
        s: *const Style,
        root: *const Style,
        prop: c_int,
    ) -> *mut c_char;
    fn ns_css_node_dir(el: *const NsNode) -> *const c_char;
    fn ns_css_list_style_serialize(
        kind: *const c_char,
        position: *const c_char,
        image: *const c_char,
    ) -> *mut c_char;
    fn ns_style_overflow_keyword(s: *const Style, axis: c_int) -> *const c_char;
    fn ns_css_individual_transform_serialize(v: *const NsCssValue, prop: c_int) -> *mut c_char;
    fn ns_css_container_condition_canonical(cond: *const c_char) -> *mut c_char;
    fn ns_css_media_list_serialize(query: *const c_char) -> *mut c_char;
    fn ns_css_named_property_supported(name: *const c_char) -> GBoolean;
    fn ns_css_named_declaration_valid(name: *const c_char, text: *const c_char) -> GBoolean;
    fn ns_css_specified_canonical(prop: *const c_char, value: *const c_char) -> *mut c_char;
    fn ns_inline_style_get(style: *const c_char, prop: *const c_char) -> *mut c_char;
    fn ns_inline_style_set(
        style: *const c_char,
        prop: *const c_char,
        value: *const c_char,
    ) -> *mut c_char;
    fn ns_inline_style_serialize(style: *const c_char) -> *mut c_char;
    fn ns_inline_value_strip_important(value: *mut c_char) -> GBoolean;
    fn ns_css_supports_declaration(property: *const c_char, value: *const c_char) -> GBoolean;
    fn ns_css_supports_condition(condition: *const c_char, allow_bare: GBoolean) -> GBoolean;
    fn ns_css_syntax_def_parse(text: *const c_char) -> *mut c_void;
    fn ns_css_syntax_def_free(syntax: *mut c_void);
    fn ns_css_syntax_def_universal(syntax: *const c_void) -> GBoolean;
    fn ns_css_syntax_def_initial_valid(syntax: *const c_void, value: *const c_char) -> GBoolean;
    fn ns_css_register_property(
        name: *const c_char,
        syntax_text: *const c_char,
        inherits: GBoolean,
        initial_value: *const c_char,
        has_initial: GBoolean,
    ) -> c_int;
    fn ns_css_anim_lists(
        style: *const Style,
        is_animation: GBoolean,
        out: *mut RawAnimList,
        out_mismatch: *mut GBoolean,
    );
    fn ns_css_anim_effective(style: *const Style, is_animation: GBoolean, out: *mut RawAnimList);
    fn ns_css_anim_list_clear(list: *mut RawAnimList);
    fn ns_css_anim_shorthand_serialize(
        list: *const RawAnimList,
        is_animation: GBoolean,
    ) -> *mut c_char;
    fn ns_css_timing_serialize(t: *const Timing) -> *mut c_char;
    fn ns_css_timing_parse(text: *const c_char, out: *mut Timing) -> GBoolean;
    fn ns_css_animation_range_serialize(start: *const c_char, end: *const c_char) -> *mut c_char;
    fn ns_css_time_computed(value: *const c_char) -> *mut c_char;

    fn ns_anim_visit(a: *mut c_void, node: *const NsNode, cb: AnimVisitCb, user: *mut c_void);
    fn ns_anim_info_for(
        a: *mut c_void,
        node: *const NsNode,
        prop: c_int,
        out: *mut AnimInfo,
    ) -> GBoolean;
    fn ns_anim_keyframes_visit(
        a: *mut c_void,
        node: *const NsNode,
        prop: c_int,
        cb: KeyframeCb,
        user: *mut c_void,
    );
    fn ns_anim_seek(a: *mut c_void, node: *const NsNode, prop: c_int, ms: f64) -> GBoolean;
    fn ns_anim_base_value(a: *mut c_void, node: *const NsNode, prop: c_int) -> *const NsCssValue;
    fn ns_anim_control(
        a: *mut c_void,
        node: *const NsNode,
        prop: c_int,
        op: *const c_char,
    ) -> GBoolean;
    #[allow(clippy::too_many_arguments)]
    fn ns_anim_script_start(
        a: *mut c_void,
        node: *const NsNode,
        stop_css: *const *const c_char,
        stop_pct: *const f64,
        n_stops: c_int,
        t: *const AnimScriptTiming,
        out_prop: *mut c_int,
        out_generation: *mut c_uint,
    ) -> GBoolean;

    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
    fn g_ascii_strtod(text: *const c_char, end: *mut *mut c_char) -> f64;
}

pub(crate) fn c_string(text: &[u8]) -> CString {
    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    CString::new(&text[..end]).unwrap_or_default()
}

fn opt_ptr(text: &Option<CString>) -> *const c_char {
    text.as_ref()
        .map_or(core::ptr::null(), |text| text.as_ptr())
}

fn take_text(p: *mut c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { glib::g_free(p.cast()) };
    Some(text)
}

fn static_text(p: *const c_char) -> Option<&'static str> {
    if p.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

pub(crate) fn fmt_g(value: f64) -> String {
    take_text(unsafe { g_strdup_printf(c"%g".as_ptr(), value) }).unwrap_or_default()
}

pub(crate) fn ascii_strtod(text: &str) -> (f64, usize) {
    let c = c_string(text.as_bytes());
    let mut end: *mut c_char = core::ptr::null_mut();
    let value = unsafe { g_ascii_strtod(c.as_ptr(), &mut end) };
    let used = if end.is_null() {
        0
    } else {
        (end as usize).saturating_sub(c.as_ptr() as usize)
    };
    (value, used.min(text.len()))
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope) as usize)
}

pub(crate) fn host_access(scope: &Scope<'_>) -> bool {
    unsafe { JS_IsHostAccess(quickjs::raw_context(scope)) }
}

pub(crate) fn style_table(js: Js) -> StyleTable {
    if js.is_null() {
        return StyleTable::none();
    }
    unsafe { StyleTable::from_ptr(ns_js_style_table(js.ptr())) }
}

pub(crate) fn has_style_table(js: Js) -> bool {
    !style_table(js).as_ptr().is_null()
}

pub(crate) fn style_of(js: Js, node: Node<'_>) -> Option<StyleRef<'static>> {
    let table = style_table(js);
    let style = table.get(node)?;
    unsafe { StyleRef::from_ptr(style.as_ptr()) }
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    if js.is_null() {
        return None;
    }
    unsafe { BoxRef::from_ptr(ns_js_layout_root(js.ptr())) }
}

pub(crate) fn find_box(js: Js, node: Node<'_>) -> Option<BoxRef<'static>> {
    let root = layout_root(js)?;
    unsafe { BoxRef::from_ptr(ns_box_find_by_dom(root.as_ptr(), node.as_ptr())) }
}

pub(crate) fn flush_layout(js: Js) {
    if !js.is_null() {
        unsafe { ns_js_flush_layout(js.mut_ptr()) };
    }
}

pub(crate) fn flush_style(js: Js) {
    if !js.is_null() {
        unsafe { ns_js_flush_style(js.mut_ptr()) };
    }
}

pub(crate) fn mark_mutated(js: Js) {
    if !js.is_null() {
        unsafe { ns_js_mark_mutated(js.mut_ptr()) };
    }
}

pub(crate) fn anim(js: Js) -> Option<*mut c_void> {
    if js.is_null() {
        return None;
    }
    let a = unsafe { ns_js_anim(js.ptr()) };
    (!a.is_null()).then_some(a)
}

pub(crate) fn set_attr_recorded(js: Js, node: Node<'_>, name: &CStr, value: &[u8]) {
    let value = c_string(value);
    unsafe {
        ns_js_set_attr_recorded(
            js.mut_ptr(),
            node.as_mut_ptr(),
            name.as_ptr(),
            value.as_ptr(),
        )
    };
}

pub(crate) fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn make_element(scope: &mut Scope<'_>, node: *const NsNode) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn style_decl_node(value: &Value) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_style_decl_node(quickjs::raw(value))) }
}

pub(crate) fn style_decl_proto(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_style_decl_proto(quickjs::raw_context(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn element_is_rendered(scope: &mut Scope<'_>, value: &Value) -> bool {
    unsafe { ns_js_element_is_rendered(quickjs::raw_context(scope), quickjs::raw(value)) != 0 }
}

pub(crate) fn grid_resolved_tracks(b: BoxRef<'_>, columns: bool) -> Option<String> {
    take_text(unsafe { ns_layout_grid_resolved_tracks(b.as_ptr(), glib::boolean(columns)) })
}

pub(crate) fn input_is_one_line_text(node: Node<'_>) -> bool {
    unsafe { ns_input_is_one_line_text(node.as_ptr()) != 0 }
}

pub(crate) fn line_heights(style: Option<StyleRef<'_>>) -> (f64, f64) {
    let p = style.map_or(core::ptr::null(), StyleRef::as_ptr);
    unsafe {
        (
            ns_paint_normal_line_height_px(p),
            ns_paint_css_line_height_px(p),
        )
    }
}

pub(crate) fn linked_css_text(url: &str) -> Option<String> {
    let url = c_string(url.as_bytes());
    take_text(unsafe { ns_engine_linked_css_text(url.as_ptr()) })
}

pub(crate) fn prop_id(name: &str) -> Option<usize> {
    let name = c_string(name.as_bytes());
    usize::try_from(unsafe { ns_css_prop_id(name.as_ptr()) }).ok()
}

pub(crate) fn prop_name(prop: usize) -> Option<&'static str> {
    static_text(unsafe { ns_css_prop_name(prop as c_int) })
}

pub(crate) fn initial_value(name: &str) -> Option<&'static str> {
    let name = c_string(name.as_bytes());
    static_text(unsafe { ns_css_initial_value_text(name.as_ptr()) })
}

pub(crate) fn viewport() -> (f64, f64) {
    unsafe { (ns_css_viewport_w(), ns_css_viewport_h()) }
}

pub(crate) fn transform_matrix(t: &Transform, bw: f64, bh: f64) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    unsafe { ns_css_transform_to_mat4(t, bw, bh, &mut m) };
    m
}

pub(crate) fn serialize(v: ValueRef<'_>) -> Option<String> {
    take_text(unsafe { ns_css_value_serialize(v.as_ptr()) })
}

pub(crate) fn serialize_raw(v: *const NsCssValue) -> Option<String> {
    take_text(unsafe { ns_css_value_serialize(v) })
}

pub(crate) fn dimension_px(v: ValueRef<'_>, font_px: f64, basis: f64) -> f64 {
    unsafe { ns_css_dimension_px(v.as_ptr(), font_px, basis) }
}

pub(crate) struct Decl {
    pub prop: c_int,
    pub value: *const NsCssValue,
}

unsafe fn decls_of(array: *const GArray) -> Vec<Decl> {
    let Some(array) = (unsafe { array.as_ref() }) else {
        return Vec::new();
    };
    let base = array.data.cast::<RawDecl>();
    (0..array.len as usize)
        .map(|i| {
            let d = unsafe { &*base.add(i) };
            Decl {
                prop: d.prop,
                value: d.value,
            }
        })
        .collect()
}

pub(crate) fn with_declarations<R>(text: &str, f: impl FnOnce(&[Decl]) -> R) -> R {
    let text = c_string(text.as_bytes());
    let array = unsafe { ns_css_parse_declarations(text.as_ptr()) };
    let decls = unsafe { decls_of(array) };
    let result = f(&decls);
    unsafe { ns_css_declarations_free(array) };
    result
}

pub(crate) fn with_sheet_declarations<R>(text: &str, f: impl FnOnce(&[Vec<Decl>]) -> R) -> R {
    let text = c_string(text.as_bytes());
    let sheet = unsafe { ns_css_stylesheet_parse(text.as_ptr(), -1) };
    let mut rules = Vec::new();
    if let Some(s) = unsafe { sheet.as_ref() }
        && let Some(array) = unsafe { s.rules.as_ref() }
    {
        for i in 0..array.len as usize {
            let rule = unsafe { *array.pdata.add(i) }.cast::<RawRule>();
            if let Some(rule) = unsafe { rule.as_ref() } {
                rules.push(unsafe { decls_of(rule.decls) });
            }
        }
    }
    let result = f(&rules);
    if !sheet.is_null() {
        unsafe { ns_css_stylesheet_free(sheet) };
    }
    result
}

pub(crate) fn background_position_join(x: &str, y: &str) -> Option<String> {
    let (x, y) = (c_string(x.as_bytes()), c_string(y.as_bytes()));
    take_text(unsafe { ns_css_background_position_join(x.as_ptr(), y.as_ptr()) })
}

fn compose(
    values: &[Option<String>],
    f: impl FnOnce(*const *mut c_char) -> *mut c_char,
) -> Option<String> {
    let owned: Vec<Option<CString>> = values
        .iter()
        .map(|v| v.as_ref().map(|v| c_string(v.as_bytes())))
        .collect();
    let ptrs: Vec<*mut c_char> = owned.iter().map(|v| opt_ptr(v).cast_mut()).collect();
    take_text(f(ptrs.as_ptr()))
}

pub(crate) fn grid_placement_compose(values: &[Option<String>; 4], area: bool) -> Option<String> {
    compose(values, |p| unsafe {
        ns_css_grid_placement_compose(p, glib::boolean(area))
    })
}

pub(crate) fn grid_shorthand_compose(values: &[Option<String>; 6], full: bool) -> Option<String> {
    compose(values, |p| unsafe {
        ns_css_grid_shorthand_compose(p, glib::boolean(full))
    })
}

pub(crate) fn tracks_computed_serialize(
    style: StyleRef<'_>,
    root: Option<StyleRef<'_>>,
    prop: usize,
) -> Option<String> {
    let root = root.map_or(core::ptr::null(), StyleRef::as_ptr);
    take_text(unsafe { ns_css_tracks_computed_serialize(style.as_ptr(), root, prop as c_int) })
}

pub(crate) fn node_dir(node: Node<'_>) -> String {
    static_text(unsafe { ns_css_node_dir(node.as_ptr()) })
        .unwrap_or("")
        .to_string()
}

pub(crate) fn list_style_serialize(kind: &str, position: &str, image: &str) -> Option<String> {
    let (k, p, i) = (
        c_string(kind.as_bytes()),
        c_string(position.as_bytes()),
        c_string(image.as_bytes()),
    );
    take_text(unsafe { ns_css_list_style_serialize(k.as_ptr(), p.as_ptr(), i.as_ptr()) })
}

pub(crate) fn overflow_keyword(style: StyleRef<'_>, axis: usize) -> String {
    static_text(unsafe { ns_style_overflow_keyword(style.as_ptr(), axis as c_int) })
        .unwrap_or("")
        .to_string()
}

pub(crate) fn individual_transform_serialize(v: ValueRef<'_>, prop: usize) -> Option<String> {
    take_text(unsafe { ns_css_individual_transform_serialize(v.as_ptr(), prop as c_int) })
}

pub(crate) fn container_condition_canonical(text: &str) -> Option<String> {
    let text = c_string(text.as_bytes());
    take_text(unsafe { ns_css_container_condition_canonical(text.as_ptr()) })
}

pub(crate) fn media_list_serialize(query: Option<&str>) -> String {
    let query = query.map(|q| c_string(q.as_bytes()));
    take_text(unsafe { ns_css_media_list_serialize(opt_ptr(&query)) }).unwrap_or_default()
}

pub(crate) fn named_property_supported(name: &str) -> bool {
    let name = c_string(name.as_bytes());
    unsafe { ns_css_named_property_supported(name.as_ptr()) != 0 }
}

pub(crate) fn named_declaration_valid(name: &str, value: &str) -> bool {
    let (n, v) = (c_string(name.as_bytes()), c_string(value.as_bytes()));
    unsafe { ns_css_named_declaration_valid(n.as_ptr(), v.as_ptr()) != 0 }
}

pub(crate) fn specified_canonical(prop: &str, value: &str) -> Option<String> {
    let (p, v) = (c_string(prop.as_bytes()), c_string(value.as_bytes()));
    take_text(unsafe { ns_css_specified_canonical(p.as_ptr(), v.as_ptr()) })
}

pub(crate) fn inline_style_get(style: Option<&CStr>, prop: &str) -> Option<String> {
    let prop = c_string(prop.as_bytes());
    let style = style.map_or(core::ptr::null(), CStr::as_ptr);
    take_text(unsafe { ns_inline_style_get(style, prop.as_ptr()) })
}

pub(crate) fn inline_style_set(style: Option<&CStr>, prop: &str, value: &str) -> Vec<u8> {
    let (p, v) = (c_string(prop.as_bytes()), c_string(value.as_bytes()));
    let style = style.map_or(core::ptr::null(), CStr::as_ptr);
    let out = unsafe { ns_inline_style_set(style, p.as_ptr(), v.as_ptr()) };
    take_text(out).unwrap_or_default().into_bytes()
}

pub(crate) fn inline_style_serialize(style: Option<&CStr>) -> String {
    let style = style.map_or(core::ptr::null(), CStr::as_ptr);
    take_text(unsafe { ns_inline_style_serialize(style) }).unwrap_or_default()
}

pub(crate) fn strip_important(value: &mut String) -> bool {
    let c = c_string(value.as_bytes());
    let raw = c.into_raw();
    let important = unsafe { ns_inline_value_strip_important(raw) } != 0;
    let c = unsafe { CString::from_raw(raw) };
    *value = c.to_string_lossy().into_owned();
    important
}

pub(crate) fn supports_declaration(prop: Option<&str>, value: Option<&str>) -> bool {
    let prop = prop.map(|p| c_string(p.as_bytes()));
    let value = value.map(|v| c_string(v.as_bytes()));
    unsafe { ns_css_supports_declaration(opt_ptr(&prop), opt_ptr(&value)) != 0 }
}

pub(crate) fn supports_condition(condition: &str) -> bool {
    let condition = c_string(condition.as_bytes());
    unsafe { ns_css_supports_condition(condition.as_ptr(), glib::TRUE) != 0 }
}

pub(crate) fn property_rule_valid(
    syntax: Option<&str>,
    initial: Option<&str>,
    has_inherits: bool,
) -> bool {
    let syntax_def = syntax.map_or(core::ptr::null_mut(), |s| {
        let s = c_string(s.as_bytes());
        unsafe { ns_css_syntax_def_parse(s.as_ptr()) }
    });
    let initial = initial.map(|i| c_string(i.as_bytes()));
    let mut ok = !syntax_def.is_null() && has_inherits;
    if ok && unsafe { ns_css_syntax_def_universal(syntax_def) } == 0 {
        ok = initial.is_some()
            && unsafe { ns_css_syntax_def_initial_valid(syntax_def, opt_ptr(&initial)) } != 0;
    } else if ok && initial.is_some() {
        ok = unsafe { ns_css_syntax_def_initial_valid(syntax_def, opt_ptr(&initial)) } != 0;
    }
    if !syntax_def.is_null() {
        unsafe { ns_css_syntax_def_free(syntax_def) };
    }
    ok
}

pub(crate) enum RegisterStatus {
    Ok,
    BadName,
    BadSyntax,
    BadInitial,
    Exists,
}

pub(crate) fn register_property(
    name: Option<&str>,
    syntax: &str,
    inherits: bool,
    initial: Option<&str>,
    has_initial: bool,
) -> RegisterStatus {
    let name = name.map(|n| c_string(n.as_bytes()));
    let syntax = c_string(syntax.as_bytes());
    let initial = initial.map(|i| c_string(i.as_bytes()));
    let status = unsafe {
        ns_css_register_property(
            opt_ptr(&name),
            syntax.as_ptr(),
            glib::boolean(inherits),
            opt_ptr(&initial),
            glib::boolean(has_initial),
        )
    };
    match status {
        0 => RegisterStatus::Ok,
        1 => RegisterStatus::BadName,
        2 => RegisterStatus::BadSyntax,
        4 => RegisterStatus::Exists,
        _ => RegisterStatus::BadInitial,
    }
}

pub(crate) fn anim_lists(style: Option<StyleRef<'_>>, is_animation: bool) -> (AnimList, bool) {
    let mut list = AnimList::empty();
    let mut mismatch: GBoolean = 0;
    let p = style.map_or(core::ptr::null(), StyleRef::as_ptr);
    unsafe { ns_css_anim_lists(p, glib::boolean(is_animation), &mut list.0, &mut mismatch) };
    (list, mismatch != 0)
}

pub(crate) fn anim_effective(style: StyleRef<'_>, is_animation: bool) -> AnimList {
    let mut list = AnimList::empty();
    unsafe { ns_css_anim_effective(style.as_ptr(), glib::boolean(is_animation), &mut list.0) };
    list
}

pub(crate) fn anim_entry_name(entry: &AnimEntry) -> Option<String> {
    (!entry.name.is_null()).then(|| {
        unsafe { CStr::from_ptr(entry.name) }
            .to_string_lossy()
            .into_owned()
    })
}

pub(crate) fn timing_text_canonical(text: &str) -> String {
    let mut timing = Timing {
        kind: TIMING_EASE,
        ..Timing::default()
    };
    let text = c_string(text.as_bytes());
    unsafe { ns_css_timing_parse(text.as_ptr(), &mut timing) };
    take_text(unsafe { ns_css_timing_serialize(&timing) }).unwrap_or_default()
}

const TIMING_EASE: c_int = 1;

pub(crate) fn animation_range_serialize(start: Option<&str>, end: Option<&str>) -> Option<String> {
    let start = start.map(|s| c_string(s.as_bytes()));
    let end = end.map(|e| c_string(e.as_bytes()));
    take_text(unsafe { ns_css_animation_range_serialize(opt_ptr(&start), opt_ptr(&end)) })
}

pub(crate) fn time_computed(text: &str) -> Option<String> {
    let text = c_string(text.as_bytes());
    take_text(unsafe { ns_css_time_computed(text.as_ptr()) })
}

pub(crate) struct AnimRecord {
    pub node: *const NsNode,
    pub prop: c_int,
    pub run: c_int,
    pub name: Option<String>,
    pub fill: String,
    pub direction: String,
    pub easing: String,
    pub current_ms: f64,
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub iterations: f64,
    pub active: bool,
    pub paused: bool,
    pub pending: bool,
    pub finished: bool,
    pub generation: u32,
}

fn optional_text(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

impl AnimRecord {
    fn of(info: &AnimInfo) -> AnimRecord {
        let easing = unsafe { CStr::from_ptr(info.easing.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        AnimRecord {
            node: info.node,
            prop: info.prop,
            run: info.run,
            name: optional_text(info.name),
            fill: optional_text(info.fill).unwrap_or_else(|| "none".into()),
            direction: optional_text(info.direction).unwrap_or_else(|| "normal".into()),
            easing,
            current_ms: info.current_ms,
            duration_ms: info.duration_ms,
            delay_ms: info.delay_ms,
            iterations: info.iterations,
            active: info.active != 0,
            paused: info.paused != 0,
            pending: info.pending != 0,
            finished: info.finished != 0,
            generation: info.generation,
        }
    }
}

unsafe extern "C" fn collect_info(info: *const AnimInfo, user: *mut c_void) {
    let records = unsafe { &mut *user.cast::<Vec<AnimRecord>>() };
    if let Some(info) = unsafe { info.as_ref() } {
        records.push(AnimRecord::of(info));
    }
}

pub(crate) fn anim_visit(a: *mut c_void, node: Option<Node<'_>>) -> Vec<AnimRecord> {
    let mut records: Vec<AnimRecord> = Vec::new();
    unsafe {
        ns_anim_visit(
            a,
            Node::ptr_or_null(node),
            collect_info,
            (&mut records as *mut Vec<AnimRecord>).cast(),
        )
    };
    records
}

pub(crate) fn anim_info_for(a: *mut c_void, node: Node<'_>, prop: i32) -> Option<AnimRecord> {
    let mut info: AnimInfo = unsafe { core::mem::zeroed() };
    let found = unsafe { ns_anim_info_for(a, node.as_ptr(), prop, &mut info) } != 0;
    found.then(|| AnimRecord::of(&info))
}

pub(crate) struct Keyframe {
    pub offset: f64,
    pub easing: Option<String>,
    pub decls: Vec<(c_int, *const NsCssValue)>,
}

unsafe extern "C" fn collect_keyframe(
    offset: f64,
    easing: *const c_char,
    decls: *const GArray,
    user: *mut c_void,
) {
    let frames = unsafe { &mut *user.cast::<Vec<Keyframe>>() };
    let decls = unsafe { decls_of(decls) }
        .into_iter()
        .map(|d| (d.prop, d.value))
        .collect();
    frames.push(Keyframe {
        offset,
        easing: optional_text(easing),
        decls,
    });
}

pub(crate) fn anim_keyframes(
    a: *mut c_void,
    node: Node<'_>,
    prop: i32,
    f: impl FnOnce(&[Keyframe]),
) {
    let mut frames: Vec<Keyframe> = Vec::new();
    unsafe {
        ns_anim_keyframes_visit(
            a,
            node.as_ptr(),
            prop,
            collect_keyframe,
            (&mut frames as *mut Vec<Keyframe>).cast(),
        )
    };
    f(&frames);
}

pub(crate) fn anim_seek(a: *mut c_void, node: Node<'_>, prop: i32, ms: f64) -> bool {
    unsafe { ns_anim_seek(a, node.as_ptr(), prop, ms) != 0 }
}

pub(crate) fn anim_control(a: *mut c_void, node: Node<'_>, prop: i32, op: &str) -> bool {
    let op = c_string(op.as_bytes());
    unsafe { ns_anim_control(a, node.as_ptr(), prop, op.as_ptr()) != 0 }
}

pub(crate) fn anim_base_value(a: *mut c_void, node: Node<'_>, prop: usize) -> *const NsCssValue {
    unsafe { ns_anim_base_value(a, node.as_ptr(), prop as c_int) }
}

pub(crate) struct ScriptTiming {
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub iterations: f64,
    pub direction: Option<String>,
    pub fill: Option<String>,
    pub easing: Option<String>,
}

pub(crate) fn anim_script_start(
    a: *mut c_void,
    node: Node<'_>,
    stops: &[(Option<String>, f64)],
    timing: &ScriptTiming,
) -> Option<(i32, u32)> {
    let css: Vec<Option<CString>> = stops
        .iter()
        .map(|(c, _)| c.as_ref().map(|c| c_string(c.as_bytes())))
        .collect();
    let css_ptrs: Vec<*const c_char> = css.iter().map(opt_ptr).collect();
    let pct: Vec<f64> = stops.iter().map(|(_, p)| *p).collect();
    let direction = timing.direction.as_ref().map(|d| c_string(d.as_bytes()));
    let fill = timing.fill.as_ref().map(|f| c_string(f.as_bytes()));
    let easing = timing.easing.as_ref().map(|e| c_string(e.as_bytes()));
    let t = AnimScriptTiming {
        duration_ms: timing.duration_ms,
        delay_ms: timing.delay_ms,
        iterations: timing.iterations,
        direction: opt_ptr(&direction),
        fill: opt_ptr(&fill),
        easing: opt_ptr(&easing),
    };
    let (mut prop, mut generation): (c_int, c_uint) = (0, 0);
    let ok = unsafe {
        ns_anim_script_start(
            a,
            node.as_ptr(),
            if css_ptrs.is_empty() {
                core::ptr::null()
            } else {
                css_ptrs.as_ptr()
            },
            if pct.is_empty() {
                core::ptr::null()
            } else {
                pct.as_ptr()
            },
            stops.len() as c_int,
            &t,
            &mut prop,
            &mut generation,
        )
    } != 0;
    ok.then_some((prop, generation))
}

pub(crate) fn computed_text_raw(js: Js, node: Node<'_>, name: &str) -> *mut c_char {
    match computed::lookup(js, node, name) {
        Some(text) => glib::strdup(text.as_bytes()),
        None => core::ptr::null_mut(),
    }
}

type Install = for<'a> fn(&mut Scope<'a>, &Value) -> Result<(), Value>;

unsafe fn install(ctx: *mut JSContext, target: JSValue, f: Install) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let target = quickjs::borrow_value(scope, target);
            f(scope, &target).ok();
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_install_window(ctx: *mut JSContext, global: JSValue) {
    unsafe { install(ctx, global, css::install_window) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_install_css(ctx: *mut JSContext, global: JSValue) {
    unsafe { install(ctx, global, css::install_css) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_install_style_proto(ctx: *mut JSContext, proto: JSValue) {
    unsafe { install(ctx, proto, declaration::install_style_proto) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_computed_text(
    ctx: *mut JSContext,
    node: *const NsNode,
    name: *const c_char,
) -> *mut c_char {
    let (Some(node), false) = (unsafe { Node::from_ptr(node) }, name.is_null()) else {
        return core::ptr::null_mut();
    };
    let js = if ctx.is_null() {
        Js(0)
    } else {
        unsafe { quickjs::with_context(ctx, |scope| js_of(scope)) }
    };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    computed_text_raw(js, node, &name)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_style_own_value(
    node: *const NsNode,
    name: *const c_char,
    writable: *mut GBoolean,
) -> *mut c_char {
    let (Some(node), false) = (unsafe { Node::from_ptr(node) }, name.is_null()) else {
        return core::ptr::null_mut();
    };
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    match declaration::own_value(node, &name) {
        Some((value, is_writable)) => {
            if let Some(w) = unsafe { writable.as_mut() } {
                *w = glib::boolean(is_writable);
            }
            glib::strdup(value.as_bytes())
        }
        None => core::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_style_set(
    ctx: *mut JSContext,
    node: *mut NsNode,
    name: *const c_char,
    value: JSValue,
) {
    let (Some(node), false) = (unsafe { Node::from_ptr(node) }, name.is_null()) else {
        return;
    };
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            declaration::set_named(scope, node, &name, &value);
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_teardown(js: *const NsJs) {
    crate::teardown(Js(js as usize));
}

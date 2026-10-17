//! Southstar — the C and Rust-ABI entry points of the CSSOM bindings, and the realm, style-engine and animation calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use southstar_anim::AnimHandle;
use southstar_css::{self as css_engine, Prop};
use southstar_css_prop_syntax as prop_syntax;
use southstar_dom::{Node, NsNode};
use southstar_glib as glib;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};
use southstar_mat4::Mat4;
use southstar_style::{NsCssValue, StyleRef, StyleTable, Transform, ValueRef};

use crate::{computed, css};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn page(self) -> *const api::NsJs {
        self.0 as *const api::NsJs
    }

    fn mut_ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

pub(crate) type AnimEntry = css_engine::AnimationEntry;

pub(crate) struct AnimList(Vec<AnimEntry>);

impl AnimList {
    pub fn entries(&self) -> &[AnimEntry] {
        &self.0
    }

    pub fn shorthand_serialize(&self, is_animation: bool) -> String {
        let text = css_engine::animation_shorthand_serialize(&self.0, is_animation);
        lossy(c_string(&text).to_bytes())
    }
}

pub(crate) fn anim_entry_timing(entry: &AnimEntry) -> String {
    lossy(&css_engine::timing_serialize(Some(&entry.timing)))
}

unsafe extern "Rust" {
    safe fn nsr_event_new(scope: &mut Scope<'_>) -> Result<Value, Value>;
    safe fn nsr_port_add_event_listener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_port_remove_event_listener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_js_element_is_rendered(scope: &mut Scope<'_>, value: &Value) -> bool;
}

unsafe extern "C" {
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;

    fn g_ascii_strtod(text: *const c_char, end: *mut *mut c_char) -> f64;
}

fn until_nul(text: &str) -> &[u8] {
    let text = text.as_bytes();
    &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())]
}

fn lossy(text: &[u8]) -> String {
    String::from_utf8_lossy(text).into_owned()
}

pub(crate) fn c_string(text: &[u8]) -> CString {
    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    CString::new(&text[..end]).unwrap_or_default()
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
    Js(scope.context_opaque() as usize)
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    nsr_event_new(scope).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn port_add_event_listener(scope: &mut Scope<'_>) -> Value {
    scope.function("addEventListener", 2, nsr_port_add_event_listener)
}

pub(crate) fn port_remove_event_listener(scope: &mut Scope<'_>) -> Value {
    scope.function("removeEventListener", 2, nsr_port_remove_event_listener)
}

pub(crate) fn host_access(scope: &mut Scope<'_>) -> bool {
    scope.is_host_access()
}

pub(crate) fn style_table(js: Js) -> StyleTable {
    if js.is_null() {
        return StyleTable::none();
    }
    unsafe { StyleTable::from_ptr(api::style_table(js.page())) }
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
    unsafe { BoxRef::from_ptr(api::layout_root(js.page()).cast()) }
}

pub(crate) fn find_box(js: Js, node: Node<'_>) -> Option<BoxRef<'static>> {
    let root = layout_root(js)?;
    unsafe { BoxRef::from_ptr(ns_box_find_by_dom(root.as_ptr(), node.as_ptr())) }
}

pub(crate) fn flush_layout(js: Js) {
    if !js.is_null() {
        api::flush_layout(js.page());
    }
}

pub(crate) fn flush_style(js: Js) {
    if !js.is_null() {
        api::flush_layout(js.page());
    }
}

pub(crate) fn mark_mutated(js: Js) {
    if !js.is_null() {
        api::mark_mutated(js.page());
    }
}

pub(crate) fn anim(js: Js) -> Option<AnimHandle> {
    if js.is_null() {
        return None;
    }
    // SAFETY: the page's animation slot holds null or the live engine the browser handed it, which outlives the page's script.
    unsafe { AnimHandle::from_raw(api::anim(js.page())) }
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
    api::unwrap_element(value)
}

pub(crate) fn make_element(scope: &mut Scope<'_>, node: *const NsNode) -> Value {
    api::make_element(scope, node).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn style_decl_node(value: &Value) -> Option<Node<'static>> {
    crate::class::node_of(value)
}

pub(crate) fn is_style_decl(value: &Value) -> bool {
    crate::class::is_style_decl(value)
}

pub(crate) fn new_style_decl(scope: &mut Scope<'_>, element: &Value) -> Result<Value, Value> {
    crate::class::new_style_decl(scope, element)
}

pub(crate) fn style_decl_proto(scope: &mut Scope<'_>) -> Value {
    crate::class::prototype(scope)
}

pub(crate) fn write_ignored(scope: &Scope<'_>, node: Node<'_>) -> bool {
    api::node_unpinned(scope.context_opaque().cast(), node)
}

pub(crate) fn element_is_rendered(scope: &mut Scope<'_>, value: &Value) -> bool {
    nsr_js_element_is_rendered(scope, value)
}

pub(crate) fn grid_resolved_tracks(b: BoxRef<'_>, columns: bool) -> Option<String> {
    southstar_layout_grid::grid_resolved_tracks(b, columns).map(|text| lossy(&text))
}

pub(crate) fn input_is_one_line_text(node: Node<'_>) -> bool {
    southstar_dom::controls::is_one_line_text(node)
}

pub(crate) fn line_heights(style: Option<StyleRef<'_>>) -> (f64, f64) {
    (
        southstar_paint::normal_line_height_px(style),
        southstar_paint::css_line_height_px(style),
    )
}

pub(crate) fn linked_css_text(url: &str) -> Option<String> {
    southstar_engine::linked_css_text(until_nul(url)).map(|css| lossy(&css))
}

pub(crate) fn prop_id(name: &str) -> Option<usize> {
    Prop::from_name(until_nul(name)).map(Prop::id)
}

pub(crate) fn prop_name(prop: usize) -> Option<&'static str> {
    Prop::from_id(prop).and_then(|p| p.c_name().to_str().ok())
}

pub(crate) fn initial_value(name: &str) -> Option<&'static str> {
    css_engine::initial_value(until_nul(name)).and_then(|text| text.to_str().ok())
}

pub(crate) fn viewport() -> (f64, f64) {
    css_engine::viewport()
}

pub(crate) fn transform_matrix(t: &Transform, bw: f64, bh: f64) -> Mat4 {
    southstar_style::transform_to_mat4(t, bw, bh)
}

fn text_until_nul(text: &[u8]) -> String {
    lossy(&text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())])
}

pub(crate) fn serialize(v: ValueRef<'_>) -> Option<String> {
    Some(text_until_nul(&css_engine::value_serialize(v.data())))
}

pub(crate) fn serialize_value(v: &NsCssValue) -> String {
    text_until_nul(&css_engine::value_serialize(v))
}

pub(crate) fn serialize_raw(v: *const NsCssValue) -> Option<String> {
    // SAFETY: callers pass null or a value the style or animation engine keeps alive across this call.
    let v = unsafe { ValueRef::from_ptr(v) };
    Some(v.map_or_else(String::new, |v| {
        text_until_nul(&css_engine::value_serialize(v.data()))
    }))
}

pub(crate) fn dimension_px(v: ValueRef<'_>, font_px: f64, basis: f64) -> f64 {
    css_engine::dimension_px(v.data(), font_px, basis)
}

pub(crate) struct Decl {
    pub value: *const NsCssValue,
}

pub(crate) fn with_declarations<R>(text: &str, f: impl FnOnce(&[Decl]) -> R) -> R {
    let block = css_engine::Declarations::parse(until_nul(text));
    let decls: Vec<Decl> = block
        .iter()
        .map(|d| Decl {
            value: core::ptr::from_ref(d.value()),
        })
        .collect();
    f(&decls)
}

pub(crate) fn sheet_declared_value(text: &str, prop: usize) -> Option<String> {
    let text = c_string(text.as_bytes());
    let sheet = css_engine::OwnedSheet::parse(text.to_bytes());
    let (_, value) = sheet.declared_values().find(|&(id, _)| id == prop)?;
    serialize_raw(value)
}

pub(crate) fn columns_shorthand(values: &[Option<String>]) -> Option<String> {
    let [width, count, height]: [&[u8]; 3] = all_present(values)?;
    Some(lossy(&css_engine::columns_serialize(width, count, height)))
}

pub(crate) fn mask_shorthand(values: &[Option<String>]) -> Option<String> {
    let parts: [&[u8]; 8] = all_present(values)?;
    css_engine::mask_shorthand_serialize(parts).map(|text| lossy(&text))
}

pub(crate) fn background_position_join(x: &str, y: &str) -> Option<String> {
    css_engine::background_position_join(until_nul(x), until_nul(y)).map(|text| lossy(&text))
}

fn all_present<const N: usize>(values: &[Option<String>]) -> Option<[&[u8]; N]> {
    let mut out = [&b""[..]; N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = until_nul(value.as_deref()?);
    }
    (values.len() >= N).then_some(out)
}

pub(crate) fn grid_placement_compose(values: &[Option<String>; 4], area: bool) -> Option<String> {
    let text = if area {
        all_present::<4>(values).map(|v| css_engine::grid_placement_compose(&v, true))
    } else {
        all_present::<2>(values).map(|v| css_engine::grid_placement_compose(&v, false))
    };
    Some(lossy(&text.unwrap_or_default()))
}

pub(crate) fn grid_shorthand_compose(values: &[Option<String>; 6], full: bool) -> Option<String> {
    let text = if full {
        all_present::<6>(values)
            .filter(|v| v.iter().all(|part| !part.is_empty()))
            .map(|v| css_engine::grid_shorthand_compose(&v))
    } else {
        all_present::<3>(values)
            .filter(|v| v.iter().all(|part| !part.is_empty()))
            .map(|[rows, cols, areas]| css_engine::grid_template_compose(rows, cols, areas))
    };
    Some(lossy(&text.unwrap_or_default()))
}

pub(crate) fn tracks_computed_serialize(
    style: StyleRef<'_>,
    root: Option<StyleRef<'_>>,
    prop: usize,
) -> Option<String> {
    let text = css_engine::tracks_computed_text(
        style.data(),
        root.map(StyleRef::data),
        Prop::from_id(prop)?,
    )?;
    Some(text_until_nul(&text))
}

pub(crate) fn node_dir(node: Node<'_>) -> String {
    css_engine::node_dir(node)
        .to_str()
        .unwrap_or("")
        .to_string()
}

pub(crate) fn list_style_serialize(kind: &str, position: &str, image: &str) -> Option<String> {
    Some(lossy(&css_engine::list_style_serialize(
        Some(until_nul(kind)),
        Some(until_nul(position)),
        Some(until_nul(image)),
    )))
}

pub(crate) fn overflow_keyword(style: StyleRef<'_>, axis: Prop) -> String {
    css_engine::overflow_keyword(Some(style.data()), axis)
        .to_str()
        .unwrap_or("")
        .to_string()
}

pub(crate) fn individual_transform_serialize(v: ValueRef<'_>, prop: usize) -> Option<String> {
    if let Some(keyword) = v.keyword_text() {
        return Some(keyword.to_string_lossy().into_owned());
    }
    let text = css_engine::individual_property_serialize(v.transform()?, Prop::from_id(prop)?)?;
    Some(lossy(&text))
}

pub(crate) fn container_condition_canonical(text: &str) -> Option<String> {
    css_engine::container_condition_canonical(until_nul(text)).map(|text| lossy(&text))
}

pub(crate) fn media_list_serialize(query: Option<&str>) -> String {
    lossy(&southstar_css_media::list_serialize(
        query.map(until_nul).unwrap_or_default(),
    ))
}

pub(crate) fn named_property_supported(name: &str) -> bool {
    css_engine::named_property_supported(until_nul(name))
}

pub(crate) fn named_declaration_valid(name: &str, value: &str) -> bool {
    css_engine::named_declaration_valid(until_nul(name), until_nul(value))
}

pub(crate) fn specified_canonical(prop: &str, value: &str) -> Option<String> {
    css_engine::specified_canonical(Some(until_nul(prop)), until_nul(value))
        .map(|text| lossy(&text))
}

pub(crate) fn inline_style_get(style: Option<&CStr>, prop: &str) -> Option<String> {
    css_engine::inline_style_get(style?.to_bytes(), until_nul(prop)).map(|text| lossy(&text))
}

pub(crate) fn inline_style_set(style: Option<&CStr>, prop: &str, value: &str) -> Vec<u8> {
    css_engine::inline_style_set(
        style.map(CStr::to_bytes),
        until_nul(prop),
        Some(until_nul(value)),
    )
}

pub(crate) fn inline_style_longhands(style: Option<&CStr>) -> Vec<String> {
    css_engine::inline_style_longhands(style.map_or(&b""[..], CStr::to_bytes))
        .iter()
        .map(|name| lossy(name))
        .collect()
}

pub(crate) fn inline_style_serialize(style: Option<&CStr>) -> String {
    lossy(&css_engine::inline_style_serialize(
        style.map_or(&b""[..], CStr::to_bytes),
    ))
}

pub(crate) fn strip_important(value: &mut String) -> bool {
    let text = until_nul(value);
    let (kept, important) = css_engine::strip_important(text);
    *value = lossy(if important { kept } else { text });
    important
}

pub(crate) fn supports_declaration(prop: Option<&str>, value: Option<&str>) -> bool {
    match (prop, value) {
        (Some(prop), Some(value)) => {
            css_engine::supports_declaration(until_nul(prop), until_nul(value))
        }
        _ => false,
    }
}

pub(crate) fn supports_condition(condition: &str) -> bool {
    css_engine::supports_condition(until_nul(condition), true)
}

pub(crate) fn property_rule_valid(
    syntax: Option<&str>,
    initial: Option<&str>,
    has_inherits: bool,
) -> bool {
    let Some(syntax_def) = syntax.and_then(|s| prop_syntax::parse(until_nul(s))) else {
        return false;
    };
    let initial_valid = |initial: &str| prop_syntax::initial_valid(&syntax_def, until_nul(initial));
    if !has_inherits {
        false
    } else if !syntax_def.universal() {
        initial.is_some_and(initial_valid)
    } else {
        initial.is_none_or(initial_valid)
    }
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
    let registration = css_engine::Registration {
        name: name.map(until_nul),
        syntax: until_nul(syntax),
        inherits,
        initial: initial.map(until_nul),
        has_initial,
    };
    match css_engine::register_property(&registration) {
        Ok(()) => RegisterStatus::Ok,
        Err(css_engine::RegisterError::BadName) => RegisterStatus::BadName,
        Err(css_engine::RegisterError::BadSyntax) => RegisterStatus::BadSyntax,
        Err(css_engine::RegisterError::Exists) => RegisterStatus::Exists,
        Err(css_engine::RegisterError::BadInitial) => RegisterStatus::BadInitial,
    }
}

pub(crate) fn anim_lists(style: Option<StyleRef<'_>>, is_animation: bool) -> (AnimList, bool) {
    let (entries, mismatch) = css_engine::animation_lists(style.map(StyleRef::data), is_animation);
    (AnimList(entries), mismatch)
}

pub(crate) fn anim_effective(style: StyleRef<'_>, is_animation: bool) -> AnimList {
    AnimList(css_engine::animation_effective(style.data(), is_animation))
}

pub(crate) fn anim_entry_name(entry: &AnimEntry) -> Option<String> {
    entry.name.as_deref().map(lossy)
}

pub(crate) fn easing_canonical(text: &str) -> Option<String> {
    css_engine::easing_canonical(until_nul(text)).map(|text| lossy(&text))
}

pub(crate) fn timing_list_canonical(text: &str) -> String {
    lossy(&css_engine::timing_list_canonical(until_nul(text)))
}

pub(crate) fn animation_range_serialize(start: Option<&str>, end: Option<&str>) -> Option<String> {
    Some(lossy(&css_engine::animation_range_serialize(
        start.map_or(&b"normal"[..], until_nul),
        end.map_or(&b"normal"[..], until_nul),
    )))
}

pub(crate) fn time_computed(text: &str) -> Option<String> {
    css_engine::time_list_serialize(until_nul(text), true).map(|text| lossy(&text))
}

pub(crate) fn computed_text_raw(js: Js, node: Node<'_>, name: &str) -> *mut c_char {
    match computed::lookup(js, node, name) {
        Some(text) => glib::strdup(text.as_bytes()),
        None => core::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub fn nsr_cssom_install_window(scope: &mut Scope<'_>, global: &Value) {
    css::install_window(scope, global).ok();
}

#[unsafe(no_mangle)]
pub fn nsr_cssom_install_css(scope: &mut Scope<'_>, global: &Value) {
    css::install_css(scope, global).ok();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_computed_text(
    ctx: ContextHandle,
    node: *const NsNode,
    name: *const c_char,
) -> *mut c_char {
    let (Some(node), false) = (unsafe { Node::from_ptr(node) }, name.is_null()) else {
        return core::ptr::null_mut();
    };
    let js = ctx.with(|scope| js_of(scope)).unwrap_or(Js(0));
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    computed_text_raw(js, node, &name)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_teardown(js: *const NsJs) {
    crate::teardown(Js(js as usize));
}

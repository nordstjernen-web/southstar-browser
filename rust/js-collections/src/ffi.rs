//! Southstar — the selector queries' and live collections' calls into the page record and the other binding crates, and the entry points those call back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::{CStr, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::live::{self, Live, LiveKind};
use crate::{Element, nodelist, page, query};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(usize);

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *const NsJs {
        self.0 as *const NsJs
    }

    pub(crate) fn is_null(self) -> bool {
        self.0 == 0
    }
}

const SYNTAX_ERR: i32 = 12;

unsafe extern "C" {
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_form_listed_controls(form: *const NsNode, include_image: GBoolean, out: *mut GPtrArray);
    fn g_ptr_array_new() -> *mut GPtrArray;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(scope.context_opaque().cast())
}

fn node<'a>(p: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p) }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    api::current_document(js.ptr())
}

pub(crate) fn focused_node(js: Js) -> Option<Element> {
    if js.is_null() {
        return None;
    }
    node(unsafe { ns_js_focused_node(js.ptr()) })
}

pub(crate) fn current_url_fragment(js: Js) -> Option<Vec<u8>> {
    if js.is_null() {
        return None;
    }
    let url = api::current_url(js.ptr());
    let url = url.to_bytes();
    let hash = url.iter().position(|&c| c == b'#')?;
    let fragment = &url[hash + 1..];
    (!fragment.is_empty()).then(|| fragment.to_vec())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    if !value.is_object() {
        return None;
    }
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    api::make_element(scope, Node::ptr_or_null(node)).unwrap_or_else(|_| Value::null())
}

pub(crate) fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> Option<Element> {
    node(api::document_root_for(scope, this))
}

pub(crate) fn selector_syntax_error(scope: &mut Scope<'_>, selector: &[u8]) -> Value {
    let mut message = Vec::with_capacity(selector.len() + 27);
    message.push(b'\'');
    message.extend(selector.iter().copied().filter(|&c| c != 0));
    message.extend_from_slice(b"' is not a valid selector");
    imports::nsr_dom_exception(scope, b"SyntaxError", SYNTAX_ERR, &message)
}

pub(crate) fn live_new_object(scope: &mut Scope<'_>, back: Live) -> Value {
    crate::class::new_live(scope, back).unwrap_or_else(|_| Value::null())
}

pub(crate) fn live_of<'a>(value: &Value) -> Option<&'a Live> {
    let back = value.with_instance(&crate::class::LIVE_CLASS, |back: &Live| {
        core::ptr::from_ref(back)
    })?;
    unsafe { back.as_ref() }
}

pub(crate) fn build_attributes(scope: &mut Scope<'_>, owner: &Value) -> Value {
    api::build_attributes(scope, owner)
}

pub(crate) fn build_labels(scope: &mut Scope<'_>, control: Element) -> Value {
    unsafe { imports::nsr_live_build_labels(scope, control.as_ptr()) }
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn form_elements_named(scope: &mut Scope<'_>, snapshot: &Value, name: &CStr) -> Value {
    imports::nsr_form_elements_named_lookup(scope, snapshot, name.to_bytes())
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn form_listed_controls(form: Element, mut visit: impl FnMut(Element)) {
    unsafe {
        let array = g_ptr_array_new();
        ns_form_listed_controls(form.as_ptr(), glib::FALSE, array);
        let raw = &*array;
        for i in 0..raw.len as usize {
            if let Some(control) = node((*raw.pdata.add(i)).cast()) {
                visit(control);
            }
        }
        glib::g_ptr_array_free(array, glib::TRUE);
    }
}

pub(crate) fn form_owner(control: Element) -> Option<Element> {
    southstar_dom::controls::form_owner(control)
}

pub(crate) fn with_text<R>(
    scope: &mut Scope<'_>,
    value: &Value,
    f: impl FnOnce(&mut Scope<'_>, &[u8], &CStr) -> R,
) -> Result<R, Value> {
    let bytes = scope.to_bytes(value)?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let text = CString::new(&bytes[..end]).unwrap_or_default();
    Ok(f(scope, &bytes, &text))
}

pub(crate) fn live_length(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    match live_of(this) {
        Some(back) => Ok(Value::int64(live::length(scope, back) as i64)),
        None => Err(scope.type_error("Illegal invocation")),
    }
}

pub(crate) fn live_item(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    live::item(scope, this, args.first())
}

macro_rules! rust_methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
                $f(scope, this, args)
            }
        )*
    };
}

rust_methods! {
    nsr_element_querySelector => query::element_query_selector,
    nsr_element_querySelectorAll => query::element_query_selector_all,
    nsr_element_getElementsByTagName => live::element_by_tag,
    nsr_element_getElementsByClassName => live::element_by_class,
    nsr_live_length_get => live_length,
}

#[unsafe(no_mangle)]
pub fn nsr_make_live(
    scope: &mut Scope<'_>,
    owner: &Value,
    kind: c_int,
    param: Option<&[u8]>,
    param2: Option<&[u8]>,
) -> Value {
    match LiveKind::from_raw(kind) {
        Some(kind) => live::make(scope, owner, kind, param, param2),
        None => Value::undefined(),
    }
}

#[unsafe(no_mangle)]
pub fn nsr_live_snapshot(scope: &mut Scope<'_>, obj: &Value) -> Value {
    match live_of(obj) {
        Some(back) => live::snapshot(scope, back),
        None => obj.clone(),
    }
}

#[unsafe(no_mangle)]
pub fn nsr_live_owner_node(obj: &Value) -> *mut NsNode {
    live_of(obj)
        .and_then(|back| unwrap_node(back.owner()))
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub fn nsr_live_owner(obj: &Value) -> Value {
    live_of(obj).map_or_else(Value::undefined, |back| back.owner().clone())
}

#[unsafe(no_mangle)]
pub fn nsr_live_proto(scope: &mut Scope<'_>, which: c_int) -> Value {
    live::proto(js_of(scope), which).unwrap_or_else(Value::undefined)
}

#[unsafe(no_mangle)]
pub fn nsr_live_wire_constructors(scope: &mut Scope<'_>, global: &Value) {
    live::wire_constructors(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_nodelist_from_array(scope: &mut Scope<'_>, array: Value) -> Value {
    nodelist::finish(scope, array)
}

#[unsafe(no_mangle)]
pub fn nsr_qcache_get(scope: &mut Scope<'_>, root: *const c_void, kind: u8, key: &[u8]) -> Value {
    page::qcache_get(js_of(scope), root as usize, kind, key).unwrap_or_else(Value::undefined)
}

#[unsafe(no_mangle)]
pub fn nsr_qcache_put(
    scope: &mut Scope<'_>,
    root: *const c_void,
    kind: u8,
    key: &[u8],
    value: &Value,
) {
    page::qcache_put(js_of(scope), root as usize, kind, key, value);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_qcache_invalidate(js: *const NsJs) {
    page::invalidate(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_collections_teardown(js: *const NsJs) {
    page::teardown(Js::of(js));
}

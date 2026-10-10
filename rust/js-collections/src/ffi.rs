//! Southstar — the C ABI of selector queries and live collections as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::live::{self, Live, LiveKind};
use crate::{Element, nodelist, page, query};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

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

const SYNTAX_ERR: c_int = 12;

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_document_root_for(ctx: *mut JSContext, this_val: JSValue) -> *mut NsNode;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_live_object_new(ctx: *mut JSContext, back: *mut c_void) -> JSValue;
    fn ns_live_back_of(v: JSValue) -> *mut c_void;
    fn ns_live_build_attributes(ctx: *mut JSContext, owner: JSValue) -> JSValue;
    fn ns_live_build_labels(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_form_elements_named_lookup(
        ctx: *mut JSContext,
        this_val: JSValue,
        name: *const c_char,
    ) -> JSValue;
    fn ns_form_listed_controls(form: *const NsNode, include_image: GBoolean, out: *mut GPtrArray);
    fn ns_form_owner(control: *const NsNode, doc: *const NsNode) -> *const NsNode;
    fn g_ptr_array_new() -> *mut GPtrArray;
    fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        plen: *mut usize,
        val: JSValue,
        cesu8: c_int,
    ) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(quickjs::context_opaque(scope).cast())
}

fn node<'a>(p: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p) }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    if js.is_null() {
        return None;
    }
    node(unsafe { ns_js_current_document(js.ptr()) })
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
    let url = unsafe { ns_js_current_url(js.ptr()) };
    if url.is_null() {
        return None;
    }
    let url = unsafe { CStr::from_ptr(url) }.to_bytes();
    let hash = url.iter().position(|&c| c == b'#')?;
    let fragment = &url[hash + 1..];
    (!fragment.is_empty()).then(|| fragment.to_vec())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    if !value.is_object() {
        return None;
    }
    node(unsafe { ns_unwrap_element(quickjs::raw(value)) })
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), Node::ptr_or_null(node)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> Option<Element> {
    node(unsafe { ns_document_root_for(quickjs::raw_context(scope), quickjs::raw(this)) })
}

pub(crate) fn selector_syntax_error(scope: &mut Scope<'_>, selector: &[u8]) -> Value {
    let mut message = Vec::with_capacity(selector.len() + 28);
    message.push(b'\'');
    message.extend(selector.iter().copied().filter(|&c| c != 0));
    message.extend_from_slice(b"' is not a valid selector\0");
    unsafe {
        ns_throw_dom_exception(
            quickjs::raw_context(scope),
            c"SyntaxError".as_ptr(),
            SYNTAX_ERR,
            message.as_ptr().cast(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn live_new_object(scope: &mut Scope<'_>, back: Box<Live>) -> Value {
    let back = Box::into_raw(back);
    let raw = unsafe { ns_live_object_new(quickjs::raw_context(scope), back.cast()) };
    if !quickjs::raw_is_object(raw) {
        drop(unsafe { Box::from_raw(back) });
    }
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn live_of<'a>(value: &Value) -> Option<&'a Live> {
    if !value.is_object() {
        return None;
    }
    unsafe { back(ns_live_back_of(quickjs::raw(value))) }
}

unsafe fn back<'a>(back: *const c_void) -> Option<&'a Live> {
    unsafe { back.cast::<Live>().as_ref() }
}

pub(crate) fn build_attributes(scope: &mut Scope<'_>, owner: &Value) -> Value {
    let raw = unsafe { ns_live_build_attributes(quickjs::raw_context(scope), quickjs::raw(owner)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn build_labels(scope: &mut Scope<'_>, control: Element) -> Value {
    let raw = unsafe { ns_live_build_labels(quickjs::raw_context(scope), control.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn form_elements_named(scope: &mut Scope<'_>, snapshot: &Value, name: &CStr) -> Value {
    let raw = unsafe {
        ns_form_elements_named_lookup(
            quickjs::raw_context(scope),
            quickjs::raw(snapshot),
            name.as_ptr(),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
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

pub(crate) fn form_owner(control: Element, doc: Element) -> Option<Element> {
    node(unsafe { ns_form_owner(control.as_ptr(), doc.as_ptr()) })
}

const MAX_ARGS: usize = 2;

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let count = if argv.is_null() {
                0
            } else {
                (argc.max(0) as usize).min(MAX_ARGS)
            };
            let arg = |scope: &Scope<'_>, i: usize| {
                if i < count {
                    quickjs::borrow_value(scope, *argv.add(i))
                } else {
                    Value::undefined()
                }
            };
            let args = [arg(scope, 0), arg(scope, 1)];
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this, &args[..count]);
            quickjs::result_raw(scope, result)
        })
    }
}

pub(crate) fn with_text<R>(
    scope: &mut Scope<'_>,
    value: &Value,
    f: impl FnOnce(&mut Scope<'_>, &[u8], &CStr) -> R,
) -> Result<R, Value> {
    let ctx = quickjs::raw_context(scope);
    let mut len = 0usize;
    let text = unsafe { JS_ToCStringLen2(ctx, &mut len, quickjs::raw(value), 0) };
    if text.is_null() {
        return Err(quickjs::take_exception(scope));
    }
    let bytes = unsafe { core::slice::from_raw_parts(text.cast::<u8>(), len) };
    let result = f(scope, bytes, unsafe { CStr::from_ptr(text) });
    unsafe { JS_FreeCString(ctx, text) };
    Ok(result)
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

natives! {
    ns_element_querySelector => query::element_query_selector,
    ns_element_querySelectorAll => query::element_query_selector_all,
    ns_document_querySelector => query::document_query_selector,
    ns_document_querySelectorAll => query::document_query_selector_all,
    ns_element_matches => query::matches,
    ns_element_closest => query::closest,
    ns_element_getElementById => query::element_get_element_by_id,
    ns_document_getElementById => query::document_get_element_by_id,
    ns_element_getElementsByTagName => live::element_by_tag,
    ns_document_getElementsByTagName => live::document_by_tag,
    ns_element_getElementsByTagNameNS => live::element_by_tag_ns,
    ns_document_getElementsByTagNameNS => live::document_by_tag_ns,
    ns_element_getElementsByClassName => live::element_by_class,
    ns_document_getElementsByClassName => live::document_by_class,
    ns_document_getElementsByName => live::document_by_name,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_length_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    _argc: c_int,
    _argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = match live_of(&this) {
                Some(back) => Ok(Value::int64(live::length(scope, back) as i64)),
                None => Err(scope.type_error("Illegal invocation")),
            };
            quickjs::result_raw(scope, result)
        })
    }
}

pub(crate) unsafe extern "C" fn live_item_native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let index = (argc > 0 && !argv.is_null()).then(|| quickjs::borrow_value(scope, *argv));
            let result = live::item(scope, &this, index.as_ref());
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_qcache_invalidate(js: *const NsJs) {
    page::invalidate(Js::of(js));
}

fn c_bytes<'a>(text: *const c_char) -> &'a [u8] {
    if text.is_null() {
        return &[];
    }
    unsafe { CStr::from_ptr(text) }.to_bytes()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_qcache_get(
    ctx: *mut JSContext,
    root: *const c_void,
    kind: c_char,
    key: *const c_char,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            page::qcache_get(js_of(scope), root as usize, kind as u8, c_bytes(key))
                .map_or(quickjs::UNDEFINED, quickjs::into_raw)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_qcache_put(
    ctx: *mut JSContext,
    root: *const c_void,
    kind: c_char,
    key: *const c_char,
    value: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            page::qcache_put(
                js_of(scope),
                root as usize,
                kind as u8,
                c_bytes(key),
                &value,
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_collections_teardown(js: *const NsJs) {
    page::teardown(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_nodelist_from_array(ctx: *mut JSContext, array: JSValue) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let array = quickjs::take_value(scope, array);
            quickjs::into_raw(nodelist::finish(scope, array))
        })
    }
}

fn opt_bytes<'a>(text: *const c_char) -> Option<&'a [u8]> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_live2(
    ctx: *mut JSContext,
    owner: JSValue,
    kind: c_int,
    param: *const c_char,
    param2: *const c_char,
) -> JSValue {
    let Some(kind) = LiveKind::from_raw(kind) else {
        return quickjs::UNDEFINED;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let owner = quickjs::borrow_value(scope, owner);
            quickjs::into_raw(live::make(
                scope,
                &owner,
                kind,
                opt_bytes(param),
                opt_bytes(param2),
            ))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_live(
    ctx: *mut JSContext,
    owner: JSValue,
    kind: c_int,
    param: *const c_char,
) -> JSValue {
    unsafe { ns_make_live2(ctx, owner, kind, param, ptr::null()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_snapshot(ctx: *mut JSContext, obj: JSValue) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            match live_of(&obj) {
                Some(back) => quickjs::into_raw(live::snapshot(scope, back)),
                None => quickjs::into_raw(obj),
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_collection_kind(obj: JSValue) -> c_int {
    let Some(back) = (unsafe { back(ns_live_back_of(obj)) }) else {
        return -1;
    };
    back.collection_kind()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_owner_node(obj: JSValue) -> *mut NsNode {
    unsafe { back(ns_live_back_of(obj)) }
        .and_then(|back| unwrap_node(back.owner()))
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_install_protos(ctx: *mut JSContext) {
    unsafe { quickjs::with_context(ctx, live::install_protos) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_proto(ctx: *mut JSContext, which: c_int) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            live::proto(js_of(scope), which).map_or(quickjs::UNDEFINED, quickjs::into_raw)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_wire_constructors(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            live::wire_constructors(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_back_free(back: *mut c_void) {
    if !back.is_null() {
        drop(unsafe { Box::from_raw(back.cast::<Live>()) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_back_owner(back: *const c_void) -> JSValue {
    unsafe { self::back(back) }.map_or(quickjs::UNDEFINED, |back| quickjs::raw(back.owner()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_back_cache(back: *const c_void) -> JSValue {
    unsafe { self::back(back) }.map_or(quickjs::UNDEFINED, Live::cache_raw)
}

fn key<'a>(is_index: GBoolean, index: u32, name: *const c_char) -> live::Key<'a> {
    if is_index != 0 {
        live::Key::Index(index)
    } else {
        live::Key::Name(if name.is_null() {
            c""
        } else {
            unsafe { CStr::from_ptr(name) }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_get_own(
    ctx: *mut JSContext,
    back: *const c_void,
    is_index: GBoolean,
    index: u32,
    name: *const c_char,
    out: *mut JSValue,
) -> c_int {
    let Some(back) = (unsafe { self::back(back) }) else {
        return 0;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            match live::get_own(scope, back, key(is_index, index, name)) {
                Some(value) => {
                    if let Some(out) = out.as_mut() {
                        *out = quickjs::into_raw(value);
                    }
                    1
                }
                None => 0,
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_named_access(back: *const c_void) -> GBoolean {
    unsafe { self::back(back) }.map_or(glib::FALSE, |back| glib::boolean(back.named_access()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_delete(
    ctx: *mut JSContext,
    back: *const c_void,
    is_index: GBoolean,
    index: u32,
    name: *const c_char,
) -> c_int {
    let Some(back) = (unsafe { self::back(back) }) else {
        return 1;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            live::deletable(scope, back, key(is_index, index, name)) as c_int
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_define_rejects(
    ctx: *mut JSContext,
    back: *const c_void,
    name: *const c_char,
) -> c_int {
    let Some(back) = (unsafe { self::back(back) }) else {
        return 0;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            live::named_exists(scope, back, key(glib::FALSE, 0, name)) as c_int
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_own_names(
    ctx: *mut JSContext,
    back: *const c_void,
    obj: JSValue,
    named: *mut GPtrArray,
) -> u32 {
    let Some(back) = (unsafe { self::back(back) }) else {
        return 0;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            live::own_names(scope, back, &obj, |name| {
                if !named.is_null() {
                    glib::g_ptr_array_add(named, glib::strdup(name).cast());
                }
            })
        })
    }
}

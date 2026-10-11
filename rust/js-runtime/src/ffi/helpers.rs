//! Southstar — small engine helpers other subsystems call through the C ABI: argument and property conversions, the illegal-constructor native, node predicates and tree splicing, src recording, attribute lists, frame purging and the engine version.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_dom::{Node, NsAttr, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};

use super::NsJs;

const FLAG_IMG_LOAD_FIRED: u32 = 1 << 3;
const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;

unsafe extern "C" {
    fn JS_GetPropertyStr(ctx: *mut JSContext, this_obj: JSValue, prop: *const c_char) -> JSValue;
    fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);
    fn JS_ToInt32(ctx: *mut JSContext, pres: *mut i32, val: JSValue) -> c_int;
    fn JS_ToBool(ctx: *mut JSContext, val: JSValue) -> c_int;
    fn JS_ToFloat64(ctx: *mut JSContext, pres: *mut f64, val: JSValue) -> c_int;
    fn JS_ThrowTypeError(ctx: *mut JSContext, fmt: *const c_char, ...) -> JSValue;
    fn JS_GetGlobalObject(ctx: *mut JSContext) -> JSValue;
    fn JS_SetPrototype(ctx: *mut JSContext, obj: JSValue, proto: JSValue) -> c_int;
    fn JS_NewArray(ctx: *mut JSContext) -> JSValue;
    fn JS_SetPropertyUint32(
        ctx: *mut JSContext,
        this_obj: JSValue,
        idx: u32,
        val: JSValue,
    ) -> c_int;
    fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        plen: *mut usize,
        val: JSValue,
        cesu8: c_int,
    ) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
    fn JS_GetException(ctx: *mut JSContext) -> JSValue;
    fn JS_GetVersion() -> *const c_char;

    fn ns_bind_fn(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
        func: JSCFunction,
        argc: c_int,
    );
    fn ns_target_addEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_target_removeEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_attr_cloneNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_isSameNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_element_get_attr_len(
        n: *const NsNode,
        name: *const c_char,
        out_len: *mut usize,
    ) -> *const c_char;
    fn ns_js_set_attr_recorded_paint_only(
        js: *mut NsJs,
        n: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_set_attr_recorded_len(
        js: *mut NsJs,
        n: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
        len: isize,
    );
    fn ns_js_start_image_load(js: *mut NsJs, el: *mut NsNode, src: *const c_char);
    fn ns_js_schedule_iframe_load_full(js: *mut NsJs, iframe: *mut NsNode, force: GBoolean);
    fn ns_node_iters_pre_remove(js: *mut NsJs, node: *mut NsNode);
    fn ns_node_remove(child: *mut NsNode);
    fn ns_js_unorphan_node(js: *mut NsJs, n: *mut NsNode);
    fn ns_live_owner_node(obj: JSValue) -> *mut NsNode;
    fn ns_attr_name_is_internal(name: *const c_char) -> GBoolean;
    fn ns_attr_to_js(
        ctx: *mut JSContext,
        owner: JSValue,
        attr: *const NsAttr,
        include_base: GBoolean,
    ) -> JSValue;
    fn ns_element_effectively_disabled(el: *const NsNode) -> GBoolean;
    fn ns_node_next_in_subtree(
        n: *const NsNode,
        root: *const NsNode,
        descend: GBoolean,
    ) -> *mut NsNode;
    fn ns_js_purge_frame_rafs(js: *mut NsJs, frame: *mut NsNode);
    fn ns_services_purge_frame_timers(js: *mut NsJs, frame: *mut NsNode);
    fn ns_js_scrub_iframe_globals(js: *mut NsJs, frame: *mut NsNode);
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_array_length(ctx: *mut JSContext, arr: JSValue) -> u32 {
    let mut len = 0i32;
    unsafe {
        let value = JS_GetPropertyStr(ctx, arr, c"length".as_ptr());
        JS_ToInt32(ctx, &mut len, value);
        JS_FreeValue(ctx, value);
    }
    len as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_get_bool_prop(
    ctx: *mut JSContext,
    obj: JSValue,
    key: *const c_char,
    was_set: *mut GBoolean,
) -> GBoolean {
    unsafe {
        let value = JS_GetPropertyStr(ctx, obj, key);
        let defined = quickjs::raw_tag(value) != quickjs::raw_tag(quickjs::UNDEFINED);
        let truthy = defined && JS_ToBool(ctx, value) > 0;
        JS_FreeValue(ctx, value);
        if let Some(was_set) = was_set.as_mut() {
            *was_set = GBoolean::from(defined);
        }
        GBoolean::from(truthy)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_arg_d(ctx: *mut JSContext, value: JSValue) -> f64 {
    let mut out = 0.0;
    unsafe { JS_ToFloat64(ctx, &mut out, value) };
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_illegal_constructor(
    ctx: *mut JSContext,
    _this_val: JSValue,
    _argc: c_int,
    _argv: *mut JSValue,
) -> JSValue {
    unsafe { JS_ThrowTypeError(ctx, c"Illegal constructor".as_ptr()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue) {
    unsafe {
        ns_bind_fn(
            ctx,
            obj,
            c"addEventListener".as_ptr(),
            ns_target_addEventListener,
            2,
        );
        ns_bind_fn(
            ctx,
            obj,
            c"removeEventListener".as_ptr(),
            ns_target_removeEventListener,
            2,
        );
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_apply_proto(ctx: *mut JSContext, obj: JSValue) {
    unsafe {
        let global = JS_GetGlobalObject(ctx);
        let ctor = JS_GetPropertyStr(ctx, global, c"Attr".as_ptr());
        let proto = JS_GetPropertyStr(ctx, ctor, c"prototype".as_ptr());
        if quickjs::raw_is_object(proto) {
            JS_SetPrototype(ctx, obj, proto);
        }
        JS_FreeValue(ctx, proto);
        JS_FreeValue(ctx, ctor);
        JS_FreeValue(ctx, global);
        ns_bind_fn(ctx, obj, c"cloneNode".as_ptr(), ns_attr_cloneNode, 0);
        ns_bind_fn(ctx, obj, c"isSameNode".as_ptr(), ns_element_isSameNode, 1);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_exception_message(ctx: *mut JSContext, ex: JSValue) -> *mut c_char {
    unsafe {
        let text = JS_ToCStringLen2(ctx, ptr::null_mut(), ex, 0);
        if text.is_null() {
            JS_FreeValue(ctx, JS_GetException(ctx));
            return glib::strdup(b"Script error.");
        }
        let out = glib::g_strdup(text);
        JS_FreeCString(ctx, text);
        out
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_engine_version() -> *const c_char {
    static VERSION: OnceLock<CString> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let version = unsafe { CStr::from_ptr(JS_GetVersion()) };
            let mut text = b"QuickJS ".to_vec();
            text.extend_from_slice(version.to_bytes());
            CString::new(text).unwrap_or_default()
        })
        .as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_in_template_content(n: *const NsNode) -> GBoolean {
    let mut cursor = node(n);
    while let Some(p) = cursor {
        if p.flags() & FLAG_TEMPLATE_CONTENT != 0 || p.element_name() == Some(b"template") {
            return 1;
        }
        cursor = p.parent();
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_insert_before_single(
    js: *mut NsJs,
    parent: *mut NsNode,
    newc: *mut NsNode,
    reference: *mut NsNode,
) {
    if newc == reference {
        return;
    }
    let (Some(parent_node), Some(child), Some(reference_node)) =
        (node(parent), node(newc), node(reference))
    else {
        return;
    };
    unsafe {
        if !js.is_null() && child.parent().is_some() {
            ns_node_iters_pre_remove(js, newc);
        }
        if child.parent().is_some() {
            ns_node_remove(newc);
        }
        if !js.is_null() {
            ns_js_unorphan_node(js, newc);
        }
    }
    parent_node.link_before(child, reference_node);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_disabled_form_control(el: *const NsNode) -> GBoolean {
    const CONTROLS: [&[u8]; 7] = [
        b"button",
        b"fieldset",
        b"input",
        b"optgroup",
        b"option",
        b"select",
        b"textarea",
    ];
    let Some(name) = node(el).and_then(Node::element_name) else {
        return 0;
    };
    if !CONTROLS.iter().any(|tag| name.eq_ignore_ascii_case(tag)) {
        return 0;
    }
    GBoolean::from(unsafe { ns_element_effectively_disabled(el) } != 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_img_src_layout_neutral(n: *const NsNode) -> GBoolean {
    let Some(n) = node(n) else {
        return 0;
    };
    if n.name().is_none_or(|name| name.to_bytes() != b"img") {
        return 0;
    }
    let present = |name: &CStr| n.attr(name).is_some_and(|value| !value.is_empty());
    GBoolean::from(present(c"width") && present(c"height"))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_src_recorded(
    js: *mut NsJs,
    n: *mut NsNode,
    s: *const c_char,
    slen: usize,
) {
    let Some(target) = node(n) else {
        return;
    };
    let mut old_len = 0usize;
    let old = unsafe { ns_element_get_attr_len(n, c"src".as_ptr(), &mut old_len) };
    let new = if s.is_null() || slen == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(s.cast::<u8>(), slen) }
    };
    let changed = old.is_null()
        || old_len != slen
        || unsafe { core::slice::from_raw_parts(old.cast::<u8>(), old_len) } != new;
    unsafe {
        if changed && ns_js_img_src_layout_neutral(n) != 0 {
            ns_js_set_attr_recorded_paint_only(js, n, c"src".as_ptr(), s);
        } else {
            ns_js_set_attr_recorded_len(js, n, c"src".as_ptr(), s, slen as isize);
        }
    }
    let name = target.name().map(CStr::to_bytes);
    if changed && name == Some(b"img") {
        target.remove_flags(FLAG_IMG_LOAD_FIRED);
        unsafe { ns_js_start_image_load(js, n, s) };
    }
    if name == Some(b"iframe") {
        unsafe { ns_js_schedule_iframe_load_full(js, n, 1) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_namedmap_owner(this_val: JSValue) -> *mut NsNode {
    unsafe { ns_live_owner_node(this_val) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_build_attributes(ctx: *mut JSContext, owner: JSValue) -> JSValue {
    unsafe {
        let array = JS_NewArray(ctx);
        let Some(root) = node(super::wrapper::ns_unwrap_element(owner)) else {
            return array;
        };
        let attrs: Vec<JSValue> = root
            .attrs()
            .filter(|attr| {
                ns_attr_name_is_internal(attr.name().map_or(ptr::null(), CStr::as_ptr)) == 0
            })
            .map(|attr| ns_attr_to_js(ctx, owner, attr.as_ptr(), 0))
            .collect();
        for (index, attr) in attrs.into_iter().enumerate() {
            JS_SetPropertyUint32(ctx, array, index as u32, attr);
        }
        array
    }
}

pub(crate) fn url_parses(js: *mut NsJs, url: *const c_char) -> GBoolean {
    if url.is_null() {
        return 0;
    }
    unsafe {
        let base = ns_js_doc_base_url(js);
        let resolved = if glib::bytes(base).is_some_and(|base| !base.is_empty()) {
            ns_url_resolve(base, url)
        } else {
            ns_url_resolve(ptr::null(), url)
        };
        let parsed = !resolved.is_null();
        glib::g_free(resolved.cast());
        glib::g_free(base.cast());
        GBoolean::from(parsed)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_subtree_rafs(js: *mut NsJs, root: *mut NsNode) {
    if js.is_null() {
        return;
    }
    let mut n = root;
    while !n.is_null() {
        if node(n).and_then(Node::element_name) == Some(b"iframe") {
            unsafe {
                ns_js_purge_frame_rafs(js, n);
                ns_services_purge_frame_timers(js, n);
                ns_js_scrub_iframe_globals(js, n);
            }
        }
        n = unsafe { ns_node_next_in_subtree(n, root, 1) };
    }
}

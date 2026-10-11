//! Southstar — the ns_js page object itself: its engine handles, page strings, partition and cookie jar, CSP, time origin, DOM prototypes and the accessors and teardown steps the rest of the engine reaches it through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::collections::HashMap;

use southstar_dom::{Kind, Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

use super::exports::ns_js_snapshot_globals;
use super::state::Slots;
use super::{NavigationTiming, NsJs};

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_SVG_NS: u32 = 1 << 7;
const FLAG_FOREIGN_NS: u32 = 1 << 9;
const FLAG_CDATA: u32 = 1 << 10;
const FLAG_PI: u32 = 1 << 11;
const CSP_CONNECT: c_int = 5;
const CSP_WORKER: c_int = 9;
const CSP_FORM_ACTION: c_int = 13;
const PROTO_COUNT: usize = 15;

type InterruptHandler = unsafe extern "C" fn(rt: *mut c_void, opaque: *mut c_void) -> c_int;
type RejectionTrackerFn = unsafe extern "C" fn(
    ctx: *mut JSContext,
    promise: JSValue,
    reason: JSValue,
    is_handled: bool,
    opaque: *mut c_void,
);
type ModuleNormalize = unsafe extern "C" fn(
    ctx: *mut JSContext,
    base: *const c_char,
    name: *const c_char,
    opaque: *mut c_void,
) -> *mut c_char;
type ModuleLoader = unsafe extern "C" fn(
    ctx: *mut JSContext,
    name: *const c_char,
    opaque: *mut c_void,
    attributes: JSValue,
) -> *mut c_void;

unsafe extern "C" {
    fn JS_NewRuntime() -> *mut c_void;
    fn JS_FreeRuntime(rt: *mut c_void);
    fn JS_SetInterruptHandler(rt: *mut c_void, handler: InterruptHandler, opaque: *mut c_void);
    fn JS_SetMemoryLimit(rt: *mut c_void, limit: usize);
    fn JS_SetHostPromiseRejectionTracker(
        rt: *mut c_void,
        tracker: RejectionTrackerFn,
        opaque: *mut c_void,
    );
    fn JS_SetMaxStackSize(rt: *mut c_void, stack_size: usize);
    fn JS_NewContext(rt: *mut c_void) -> *mut JSContext;
    fn JS_FreeContext(ctx: *mut JSContext);
    fn JS_SetContextOpaque(ctx: *mut JSContext, opaque: *mut c_void);
    fn JS_GetContextOpaque(ctx: *mut JSContext) -> *mut c_void;
    fn JS_SetRuntimeOpaque(rt: *mut c_void, opaque: *mut c_void);
    fn JS_SetModuleLoaderFunc2(
        rt: *mut c_void,
        normalize: ModuleNormalize,
        loader: ModuleLoader,
        check_attributes: *const c_void,
        opaque: *mut c_void,
    );
    fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);
    fn JS_DupValue(ctx: *mut JSContext, v: JSValue) -> JSValue;
    fn JS_SetPrototype(ctx: *mut JSContext, obj: JSValue, proto: JSValue) -> c_int;
    fn JS_GetGlobalObject(ctx: *mut JSContext) -> JSValue;
    fn JS_GetPropertyStr(ctx: *mut JSContext, this_obj: JSValue, prop: *const c_char) -> JSValue;
    fn JS_SetPropertyStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
        val: JSValue,
    ) -> c_int;
    fn JS_DefinePropertyValueStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
        val: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_IsFunction(ctx: *mut JSContext, val: JSValue) -> bool;
    fn JS_Call(
        ctx: *mut JSContext,
        func: JSValue,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn JS_GetException(ctx: *mut JSContext) -> JSValue;
    fn JS_ValueToAtom(ctx: *mut JSContext, val: JSValue) -> u32;
    fn JS_FreeAtom(ctx: *mut JSContext, atom: u32);
    fn JS_DeleteProperty(ctx: *mut JSContext, obj: JSValue, prop: u32, flags: c_int) -> c_int;
    fn JS_Eval(
        ctx: *mut JSContext,
        input: *const c_char,
        input_len: usize,
        filename: *const c_char,
        eval_flags: c_int,
    ) -> JSValue;
    fn JS_GetCallerRealm(ctx: *mut JSContext) -> *mut JSContext;

    fn ns_js_module_normalize(
        ctx: *mut JSContext,
        base: *const c_char,
        name: *const c_char,
        opaque: *mut c_void,
    ) -> *mut c_char;
    fn ns_js_module_loader(
        ctx: *mut JSContext,
        name: *const c_char,
        opaque: *mut c_void,
        attributes: JSValue,
    ) -> *mut c_void;
    fn ns_js_eval(js: *mut NsJs, src: *const c_char, len: usize, origin: *const c_char);
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_storage_switch_session(js: *mut NsJs, old: *const c_char, new: *const c_char);
    fn ns_storage_load_local(js: *mut NsJs, url: *const c_char);
    fn ns_ce_name_valid(name: *const c_char) -> GBoolean;
    fn ns_csp_parse(header: *const c_char) -> *mut c_void;
    fn ns_csp_free(csp: *mut c_void);
    fn ns_csp_merge(dst: *mut c_void, src: *mut c_void);
    fn ns_csp_allows(
        csp: *const c_void,
        kind: c_int,
        resource_url: *const c_char,
        document_url: *const c_char,
    ) -> GBoolean;
    fn ns_csp_inline_event_handler_allowed(csp: *const c_void) -> GBoolean;
    fn ns_hide_shared_array_buffer(ctx: *mut JSContext, global: JSValue);
    fn ns_install_pdf_plugins(ctx: *mut JSContext);
    fn ns_js_link_interface_ctors(ctx: *mut JSContext);
    fn ns_js_lock_global_prototypes(ctx: *mut JSContext);
    fn ns_js_brand_node_interfaces(ctx: *mut JSContext, element_cid: u32, attr_cid: u32);
    fn ns_attr_class_id() -> u32;
    fn ns_obj_has_defined_own_prop(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
    ) -> GBoolean;
    fn ns_make_performance_object(
        ctx: *mut JSContext,
        js: *mut NsJs,
        include_memory: GBoolean,
    ) -> JSValue;
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_js_update_focus_visible(js: *mut NsJs);
    fn ns_popover_attr_changed(
        js: *mut NsJs,
        node: *mut NsNode,
        attr: *const c_char,
        old_value: *const c_char,
        new_value: *const c_char,
    );
    fn ns_js_script_needs_prepare(js: *mut NsJs, script: *mut NsNode);
    fn ns_ce_attribute_changed(
        js: *mut NsJs,
        node: *mut NsNode,
        attr: *const c_char,
        old_value: *const c_char,
        new_value: *const c_char,
    );
    fn ns_node_new_element(name: *mut c_char) -> *mut NsNode;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
    fn ns_tag_owner_document(ctx: *mut JSContext, doc: JSValue, node: JSValue);
    fn ns_js_frame_context(js: *const NsJs, frame: *const NsNode) -> *mut JSContext;
    fn ns_js_reset_form(ctx: *mut JSContext, form: *mut NsNode) -> JSValue;
    fn ns_box_lookup_forget(js: *mut NsJs);
    fn ns_js_sync_window_metrics(js: *const NsJs);
    fn ns_js_promote_deferred_iframes(js: *mut NsJs);
    fn ns_intersection_observers_tick(js: *mut NsJs);
    fn ns_resize_observers_tick(js: *mut NsJs);
}

#[repr(C)]
pub(crate) struct DomProtos {
    protos: [JSValue; PROTO_COUNT],
}

const PROTO_NODE: usize = 0;
const PROTO_ELEMENT: usize = 1;
const PROTO_HTML_ELEMENT: usize = 2;
const PROTO_HTML_UNKNOWN_ELEMENT: usize = 3;
const PROTO_SVG_ELEMENT: usize = 4;
const PROTO_SVGA_ELEMENT: usize = 5;
const PROTO_MATHML_ELEMENT: usize = 6;
const PROTO_CHARDATA: usize = 7;
const PROTO_TEXT: usize = 8;
const PROTO_COMMENT: usize = 9;
const PROTO_CDATA: usize = 10;
const PROTO_PI: usize = 11;
const PROTO_DOCTYPE: usize = 12;
const PROTO_DOCFRAG: usize = 13;
const PROTO_DOCUMENT: usize = 14;
const FREE_ORDER: [usize; PROTO_COUNT] = [
    PROTO_NODE,
    PROTO_ELEMENT,
    PROTO_HTML_ELEMENT,
    PROTO_SVG_ELEMENT,
    PROTO_SVGA_ELEMENT,
    PROTO_MATHML_ELEMENT,
    PROTO_CHARDATA,
    PROTO_TEXT,
    PROTO_COMMENT,
    PROTO_CDATA,
    PROTO_PI,
    PROTO_DOCTYPE,
    PROTO_DOCFRAG,
    PROTO_DOCUMENT,
    PROTO_HTML_UNKNOWN_ELEMENT,
];

pub(crate) struct Core {
    rt: *mut c_void,
    ctx: *mut JSContext,
    main_realm_ctx: *mut JSContext,
    frame_ctxs: Option<Vec<*mut JSContext>>,
    current_url: *mut c_char,
    current_doc: *mut NsNode,
    raf_frame_ctx: *mut NsNode,
    pristine_promise: JSValue,
    layout_root: *const c_void,
    pinned_wrappers_set: *mut GHashTable,
    cookie_value: *mut c_char,
    cookie_buckets: Option<HashMap<Vec<u8>, Vec<u8>>>,
    partition_key: *mut c_char,
    opaque_counter: u64,
    navigator_brand: JSValue,
    halted: GBoolean,
    csp: *mut c_void,
    time_origin_us: i64,
    time_origin_real_ms: f64,
    navigation_timing: NavigationTiming,
    ce_main_doc: *const NsNode,
    platform_globals: *mut GHashTable,
    form_data_helper: Option<JSValue>,
    protos: [JSValue; PROTO_COUNT],
    per_tag_protos: Option<HashMap<Vec<u8>, JSValue>>,
    dom_protos_set: bool,
    runtime: *mut c_void,
}

thread_local! {
    static ACTIVE: Cell<*mut NsJs> = const { Cell::new(ptr::null_mut()) };
}

fn core<'a>(js: *const NsJs) -> Option<&'a mut Core> {
    unsafe { js.cast_mut().cast::<Core>().as_mut() }
}

fn bytes<'a>(text: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(text) }
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn replace_owned(slot: &mut *mut c_char, value: *mut c_char) {
    let previous = core::mem::replace(slot, value);
    unsafe { glib::g_free(previous.cast()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_active_js() -> *mut NsJs {
    ACTIVE.with(Cell::get)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_set_active_js(js: *mut NsJs) {
    ACTIVE.with(|active| active.set(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_alloc() -> *mut NsJs {
    let core = Box::new(Core {
        rt: ptr::null_mut(),
        ctx: ptr::null_mut(),
        main_realm_ctx: ptr::null_mut(),
        frame_ctxs: None,
        current_url: ptr::null_mut(),
        current_doc: ptr::null_mut(),
        raf_frame_ctx: ptr::null_mut(),
        pristine_promise: quickjs::UNDEFINED,
        layout_root: ptr::null(),
        pinned_wrappers_set: ptr::null_mut(),
        cookie_value: ptr::null_mut(),
        cookie_buckets: None,
        partition_key: ptr::null_mut(),
        opaque_counter: 0,
        navigator_brand: quickjs::UNDEFINED,
        halted: 0,
        csp: ptr::null_mut(),
        time_origin_us: 0,
        time_origin_real_ms: 0.0,
        navigation_timing: NavigationTiming::starting_at(0, 0.0),
        ce_main_doc: ptr::null(),
        platform_globals: ptr::null_mut(),
        form_data_helper: None,
        protos: [quickjs::UNDEFINED; PROTO_COUNT],
        per_tag_protos: None,
        dom_protos_set: false,
        runtime: ptr::null_mut(),
    });
    Box::into_raw(core).cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_time_origin(js: *mut NsJs, origin_us: i64, origin_real_ms: f64) {
    if let Some(js) = core(js) {
        js.time_origin_us = origin_us;
        js.time_origin_real_ms = origin_real_ms;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_create_runtime(
    js: *mut NsJs,
    memory_limit: usize,
    stack_size: usize,
    interrupt: InterruptHandler,
    rejection_tracker: RejectionTrackerFn,
) -> GBoolean {
    let Some(core) = core(js) else {
        return 0;
    };
    unsafe {
        core.rt = JS_NewRuntime();
        if core.rt.is_null() {
            return 0;
        }
        JS_SetInterruptHandler(core.rt, interrupt, js.cast());
        JS_SetMemoryLimit(core.rt, memory_limit);
        JS_SetHostPromiseRejectionTracker(core.rt, rejection_tracker, ptr::null_mut());
        JS_SetMaxStackSize(core.rt, stack_size);
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_create_context(js: *mut NsJs) -> GBoolean {
    let Some(core) = core(js) else {
        return 0;
    };
    unsafe {
        core.ctx = JS_NewContext(core.rt);
        if core.ctx.is_null() {
            return 0;
        }
        core.main_realm_ctx = core.ctx;
        JS_SetContextOpaque(core.ctx, js.cast());
        JS_SetRuntimeOpaque(core.rt, js.cast());
        JS_SetModuleLoaderFunc2(
            core.rt,
            ns_js_module_normalize,
            ns_js_module_loader,
            ptr::null(),
            js.cast(),
        );
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_init_tables(js: *mut NsJs, page: GBoolean) {
    let Some(core) = core(js) else {
        return;
    };
    core.pinned_wrappers_set =
        unsafe { glib::g_hash_table_new(Some(glib::g_direct_hash), Some(glib::g_direct_equal)) };
    if page == 0 {
        return;
    }
    core.frame_ctxs = Some(Vec::new());
    core.cookie_buckets = Some(HashMap::new());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_partition_key(js: *mut NsJs, partition_key: *const c_char) {
    if let Some(core) = core(js) {
        core.partition_key = unsafe { glib::g_strdup(partition_key) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_runtime_state(js: *const NsJs) -> *mut c_void {
    core(js).map_or(ptr::null_mut(), |core| core.runtime)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_runtime_state(js: *mut NsJs, state: *mut c_void) {
    if let Some(core) = core(js) {
        core.runtime = state;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_slots_of(js: *mut NsJs, out: *mut Slots) {
    let (Some(core), Some(out)) = (core(js), unsafe { out.as_mut() }) else {
        return;
    };
    out.rt = &raw mut core.rt;
    out.ctx = &raw mut core.ctx;
    out.main_realm_ctx = &raw mut core.main_realm_ctx;
    out.current_url = &raw mut core.current_url;
    out.current_doc = &raw mut core.current_doc;
    out.raf_frame_ctx = &raw mut core.raf_frame_ctx;
    out.ce_main_doc = &raw mut core.ce_main_doc;
    out.halted = &raw mut core.halted;
    out.pinned_wrappers_set = &raw mut core.pinned_wrappers_set;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_release_wrapper(js: *mut NsJs, node_ptr: *mut NsNode) {
    let (Some(core), Some(n)) = (core(js), node(node_ptr)) else {
        return;
    };
    unsafe { JS_FreeValue(core.ctx, quickjs::identity_raw(n.js_wrapper())) };
}

fn replace_value(ctx: *mut JSContext, slot: &mut JSValue, value: JSValue) {
    let previous = core::mem::replace(slot, value);
    unsafe { JS_FreeValue(ctx, previous) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_proto_document(js: *mut NsJs, proto: JSValue) {
    if let Some(core) = core(js) {
        let ctx = core.ctx;
        replace_value(ctx, &mut core.protos[PROTO_DOCUMENT], proto);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_navigator_brand(js: *mut NsJs, brand: JSValue) {
    if let Some(core) = core(js) {
        let ctx = core.ctx;
        replace_value(ctx, &mut core.navigator_brand, brand);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_pristine_promise(js: *mut NsJs, promise: JSValue) {
    if let Some(core) = core(js) {
        let ctx = core.ctx;
        replace_value(ctx, &mut core.pristine_promise, promise);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_navigator_brand(js: *const NsJs) -> JSValue {
    core(js).map_or(quickjs::UNDEFINED, |core| core.navigator_brand)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pristine_promise(js: *const NsJs) -> JSValue {
    core(js).map_or(quickjs::UNDEFINED, |core| core.pristine_promise)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pinned_wrappers(js: *const NsJs) -> *mut GHashTable {
    core(js).map_or(ptr::null_mut(), |core| core.pinned_wrappers_set)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_detach_wrapper(
    js: *mut NsJs,
    node_ptr: *mut NsNode,
    release: GBoolean,
) {
    let Some(n) = node(node_ptr) else {
        return;
    };
    let wrapper = n.js_wrapper();
    if wrapper.is_null() {
        return;
    }
    unsafe { quickjs::set_identity_opaque(wrapper, ptr::null_mut()) };
    n.set_js_wrapper(ptr::null_mut());
    n.set_js_invalidate(None);
    if release != 0
        && let Some(core) = core(js)
    {
        unsafe { JS_FreeValue(core.ctx, quickjs::identity_raw(wrapper)) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_frame_contexts(js: *mut NsJs) {
    let Some(frames) = core(js).and_then(|core| core.frame_ctxs.as_mut()) else {
        return;
    };
    for ctx in frames.drain(..) {
        unsafe { JS_FreeContext(ctx) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_context_count(js: *const NsJs) -> c_uint {
    core(js)
        .and_then(|core| core.frame_ctxs.as_ref())
        .map_or(0, |frames| frames.len() as c_uint)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_new_frame_context(js: *mut NsJs) -> *mut JSContext {
    let Some(core) = core(js) else {
        return ptr::null_mut();
    };
    unsafe {
        let fctx = JS_NewContext(core.rt);
        if fctx.is_null() {
            return ptr::null_mut();
        }
        JS_SetContextOpaque(fctx, js.cast());
        let global = JS_GetGlobalObject(fctx);
        ns_hide_shared_array_buffer(fctx, global);
        JS_FreeValue(fctx, global);
        if let Some(frames) = core.frame_ctxs.as_mut() {
            frames.push(fctx);
        }
        fctx
    }
}

const POLYFILLS: &[u8] = include_bytes!("../../../../data/js/polyfills.js");
const STREAMING: &[u8] = include_bytes!("../../../../data/js/streaming.js");

fn eval_source(js: *mut NsJs, source: &[u8], origin: &CStr) {
    unsafe { ns_js_eval(js, source.as_ptr().cast(), source.len(), origin.as_ptr()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_polyfills(js: *mut NsJs) {
    eval_source(js, POLYFILLS, c"<polyfills>");
    eval_source(js, STREAMING, c"<streaming>");
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_worker_polyfills(js: *mut NsJs) {
    eval_source(js, POLYFILLS, c"<worker-polyfills>");
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_snapshot_platform_globals(js: *mut NsJs) {
    let Some(core) = core(js) else {
        return;
    };
    if !core.platform_globals.is_null() {
        unsafe { glib::g_hash_table_destroy(core.platform_globals) };
    }
    core.platform_globals = unsafe { ns_js_snapshot_globals(js) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_platform_globals(js: *mut NsJs) -> *mut GHashTable {
    let Some(core) = core(js) else {
        return ptr::null_mut();
    };
    if !core.platform_globals.is_null() {
        return core.platform_globals;
    }
    let main = if core.main_realm_ctx.is_null() {
        core.ctx
    } else {
        core.main_realm_ctx
    };
    let saved = core::mem::replace(&mut core.ctx, main);
    let globals = unsafe { ns_js_snapshot_globals(js) };
    let Some(core) = self::core(js) else {
        return globals;
    };
    core.platform_globals = globals;
    core.ctx = saved;
    globals
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_page_strings(js: *mut NsJs) {
    let Some(core) = core(js) else {
        return;
    };
    if !core.csp.is_null() {
        unsafe { ns_csp_free(core.csp) };
        core.csp = ptr::null_mut();
    }
    replace_owned(&mut core.cookie_value, ptr::null_mut());
    replace_owned(&mut core.current_url, ptr::null_mut());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_pinned_set(js: *mut NsJs) {
    let Some(core) = core(js) else {
        return;
    };
    let set = core::mem::replace(&mut core.pinned_wrappers_set, ptr::null_mut());
    if !set.is_null() {
        unsafe { glib::g_hash_table_destroy(set) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_realm_values(js: *mut NsJs) {
    let Some(core) = core(js) else {
        return;
    };
    let ctx = core.ctx;
    replace_value(ctx, &mut core.pristine_promise, quickjs::UNDEFINED);
    if !core.dom_protos_set {
        return;
    }
    for index in FREE_ORDER {
        unsafe { JS_FreeValue(ctx, core.protos[index]) };
    }
    if let Some(protos) = core.per_tag_protos.take() {
        for proto in protos.into_values() {
            unsafe { JS_FreeValue(ctx, proto) };
        }
    }
    core.dom_protos_set = false;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_platform_globals(js: *mut NsJs) {
    let Some(core) = core(js) else {
        return;
    };
    let globals = core::mem::replace(&mut core.platform_globals, ptr::null_mut());
    if !globals.is_null() {
        unsafe { glib::g_hash_table_destroy(globals) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_partition(js: *mut NsJs) {
    if let Some(core) = core(js) {
        core.cookie_buckets = None;
        replace_owned(&mut core.partition_key, ptr::null_mut());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_lookup_caches(js: *mut NsJs) {
    if let Some(core) = core(js)
        && let Some(helper) = core.form_data_helper.take()
    {
        unsafe { JS_FreeValue(core.ctx, helper) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_window_values(js: *mut NsJs) {
    if let Some(core) = core(js) {
        let ctx = core.ctx;
        replace_value(ctx, &mut core.navigator_brand, quickjs::UNDEFINED);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_engine(js: *mut NsJs) {
    if js.is_null() {
        return;
    }
    let core = unsafe { Box::from_raw(js.cast::<Core>()) };
    unsafe {
        if let Some(frames) = core.frame_ctxs.as_ref() {
            for &ctx in frames {
                JS_FreeContext(ctx);
            }
        }
        if !core.ctx.is_null() {
            JS_FreeContext(core.ctx);
        }
        if !core.rt.is_null() {
            JS_FreeRuntime(core.rt);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_add_csp_header(js: *mut NsJs, header_value: *const c_char) {
    let Some(core) = core(js) else {
        return;
    };
    if bytes(header_value).is_none_or(<[u8]>::is_empty) {
        return;
    }
    let parsed = unsafe { ns_csp_parse(header_value) };
    if parsed.is_null() {
        return;
    }
    if core.csp.is_null() {
        core.csp = parsed;
    } else {
        unsafe {
            ns_csp_merge(core.csp, parsed);
            ns_csp_free(parsed);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_csp_form_action_allowed(
    js: *const NsJs,
    action_url: *const c_char,
) -> GBoolean {
    let Some(core) = core(js) else {
        return 1;
    };
    if core.csp.is_null() || action_url.is_null() {
        return 1;
    }
    unsafe { ns_csp_allows(core.csp, CSP_FORM_ACTION, action_url, core.current_url) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_csp_allows_worker(js: *mut NsJs, url: *const c_char) -> GBoolean {
    let Some(core) = core(js) else {
        return 1;
    };
    if core.csp.is_null() {
        return 1;
    }
    unsafe { ns_csp_allows(core.csp, CSP_WORKER, url, core.current_url) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_csp_allows_connect(
    js: *const NsJs,
    url: *const c_char,
    page: *const c_char,
) -> GBoolean {
    match core(js) {
        Some(core) if !core.csp.is_null() => unsafe {
            ns_csp_allows(core.csp, CSP_CONNECT, url, page)
        },
        _ => 1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_inline_handlers_allowed(js: *const NsJs) -> GBoolean {
    let csp = core(js).map_or(ptr::null_mut(), |core| core.csp);
    unsafe { ns_csp_inline_event_handler_allowed(csp) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_csp(js: *const NsJs) -> *const c_void {
    core(js).map_or(ptr::null(), |core| core.csp.cast_const())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_storage_partition(js: *const NsJs) -> *const c_char {
    core(js).map_or(ptr::null(), |core| core.partition_key.cast_const())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_partition_key(js: *const NsJs) -> *const c_char {
    core(js).map_or(ptr::null(), |core| core.partition_key.cast_const())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_us(js: *const NsJs) -> i64 {
    core(js).map_or(0, |core| core.time_origin_us)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_real_ms(js: *const NsJs) -> f64 {
    core(js).map_or(0.0, |core| core.time_origin_real_ms)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_navigation_timing(js: *const NsJs) -> *const NavigationTiming {
    core(js).map_or(ptr::null(), |core| &raw const core.navigation_timing)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_layout_root(js: *const NsJs) -> *const c_void {
    core(js).map_or(ptr::null(), |core| core.layout_root)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_layout_root(js: *mut NsJs, root: *const c_void) {
    let Some(core) = core(js) else {
        return;
    };
    if core.layout_root != root {
        unsafe { ns_box_lookup_forget(js) };
    }
    if let Some(core) = self::core(js) {
        core.layout_root = root;
    }
    unsafe {
        ns_js_sync_window_metrics(js);
        if !root.is_null() {
            ns_js_promote_deferred_iframes(js);
            ns_intersection_observers_tick(js);
            ns_resize_observers_tick(js);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_cookie_value(js: *const NsJs) -> *const c_char {
    core(js).map_or(ptr::null(), |core| core.cookie_value.cast_const())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_cookie_value(js: *mut NsJs, value: *const c_char) {
    if let Some(core) = core(js) {
        replace_owned(&mut core.cookie_value, unsafe { glib::g_strdup(value) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode {
    core(js).map_or(ptr::null(), |core| core.ce_main_doc)
}

fn partition_apply(js: *mut NsJs, new_url: *const c_char) {
    let Some(core) = core(js) else {
        return;
    };
    let origin = unsafe { ns_url_origin_from(new_url) };
    let new_key = match bytes(origin) {
        Some(text) if !text.is_empty() => origin,
        _ => {
            unsafe { glib::g_free(origin.cast()) };
            core.opaque_counter += 1;
            glib::strdup(format!("opaque://{}", core.opaque_counter).as_bytes())
        }
    };
    let new_bytes = bytes(new_key).unwrap_or_default().to_vec();
    if bytes(core.partition_key) == Some(&new_bytes[..]) {
        unsafe { glib::g_free(new_key.cast()) };
        return;
    }
    if let (Some(old), Some(buckets)) = (bytes(core.partition_key), core.cookie_buckets.as_mut()) {
        let cookie = bytes(core.cookie_value).unwrap_or_default().to_vec();
        buckets.insert(old.to_vec(), cookie);
    }
    let old_key = core.partition_key;
    unsafe { ns_storage_switch_session(js, old_key, new_key) };
    let Some(core) = self::core(js) else {
        return;
    };
    replace_owned(&mut core.cookie_value, ptr::null_mut());
    if let Some(cookie) = core
        .cookie_buckets
        .as_ref()
        .and_then(|buckets| buckets.get(&new_bytes))
    {
        core.cookie_value = glib::strdup(cookie);
    }
    replace_owned(&mut core.partition_key, new_key);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_load_for(js: *mut NsJs, new_url: *const c_char) {
    if js.is_null() {
        return;
    }
    partition_apply(js, new_url);
    unsafe { ns_storage_load_local(js, new_url) };
}

const PLAIN_INTERFACE_TAGS: [&[u8]; 51] = [
    b"abbr",
    b"acronym",
    b"address",
    b"article",
    b"aside",
    b"b",
    b"basefont",
    b"bdi",
    b"bdo",
    b"big",
    b"center",
    b"cite",
    b"code",
    b"dd",
    b"dfn",
    b"dt",
    b"em",
    b"figcaption",
    b"figure",
    b"footer",
    b"header",
    b"hgroup",
    b"i",
    b"kbd",
    b"main",
    b"mark",
    b"nav",
    b"nobr",
    b"noembed",
    b"noframes",
    b"noscript",
    b"plaintext",
    b"rb",
    b"rp",
    b"rt",
    b"rtc",
    b"ruby",
    b"s",
    b"samp",
    b"search",
    b"section",
    b"small",
    b"strike",
    b"strong",
    b"sub",
    b"summary",
    b"sup",
    b"tt",
    b"u",
    b"var",
    b"wbr",
];

fn has_plain_interface(name: &[u8]) -> bool {
    PLAIN_INTERFACE_TAGS.contains(&name)
}

fn is_object(value: JSValue) -> bool {
    quickjs::raw_is_object(value)
}

fn html_kind_proto(core: &Core, n: Node<'_>) -> JSValue {
    let (Some(name), Some(per_tag)) = (n.name(), core.per_tag_protos.as_ref()) else {
        return core.protos[PROTO_HTML_ELEMENT];
    };
    let name_bytes = name.to_bytes();
    if !name_bytes.iter().any(u8::is_ascii_uppercase) {
        if let Some(&proto) = per_tag.get(name_bytes) {
            return proto;
        }
        if has_plain_interface(name_bytes) {
            return core.protos[PROTO_HTML_ELEMENT];
        }
    }
    if unsafe { ns_ce_name_valid(name.as_ptr()) } == 0
        && is_object(core.protos[PROTO_HTML_UNKNOWN_ELEMENT])
    {
        return core.protos[PROTO_HTML_UNKNOWN_ELEMENT];
    }
    core.protos[PROTO_HTML_ELEMENT]
}

fn foreign_kind_proto(core: &Core, n: Node<'_>) -> JSValue {
    let mathml = n
        .attr(c"data-nd-ns-uri")
        .is_some_and(|ns| ns.to_bytes() == b"http://www.w3.org/1998/Math/MathML");
    if mathml && is_object(core.protos[PROTO_MATHML_ELEMENT]) {
        core.protos[PROTO_MATHML_ELEMENT]
    } else {
        core.protos[PROTO_ELEMENT]
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_kind_proto(js: *const NsJs, node_ptr: *const NsNode) -> JSValue {
    let (Some(core), Some(n)) = (core(js), node(node_ptr)) else {
        return quickjs::UNDEFINED;
    };
    if !core.dom_protos_set {
        return quickjs::UNDEFINED;
    }
    let flags = n.flags();
    match n.kind() {
        Kind::Element => {
            if flags & FLAG_SVG_NS != 0 {
                if n.name()
                    .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"a"))
                    && is_object(core.protos[PROTO_SVGA_ELEMENT])
                {
                    return core.protos[PROTO_SVGA_ELEMENT];
                }
                return if is_object(core.protos[PROTO_SVG_ELEMENT]) {
                    core.protos[PROTO_SVG_ELEMENT]
                } else {
                    core.protos[PROTO_ELEMENT]
                };
            }
            if flags & FLAG_FOREIGN_NS != 0 {
                return foreign_kind_proto(core, n);
            }
            html_kind_proto(core, n)
        }
        Kind::Text => {
            if flags & FLAG_CDATA != 0 {
                core.protos[PROTO_CDATA]
            } else {
                core.protos[PROTO_TEXT]
            }
        }
        Kind::Comment => {
            if flags & FLAG_PI != 0 {
                core.protos[PROTO_PI]
            } else {
                core.protos[PROTO_COMMENT]
            }
        }
        Kind::Doctype => core.protos[PROTO_DOCTYPE],
        Kind::Document => {
            if flags & FLAG_FRAGMENT != 0 {
                core.protos[PROTO_DOCFRAG]
            } else {
                core.protos[PROTO_DOCUMENT]
            }
        }
        Kind::Other => quickjs::UNDEFINED,
    }
}

fn js_of_context(ctx: *mut JSContext) -> *mut NsJs {
    if ctx.is_null() {
        ptr::null_mut()
    } else {
        unsafe { JS_GetContextOpaque(ctx) }.cast()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_register_tag_proto(
    ctx: *mut JSContext,
    tag: *const c_char,
    proto: JSValue,
) {
    let js = js_of_context(ctx);
    let (Some(core), Some(tag)) = (core(js), bytes(tag)) else {
        return;
    };
    let protos = core.per_tag_protos.get_or_insert_with(HashMap::new);
    if let Some(old) = protos.remove(tag) {
        unsafe { JS_FreeValue(ctx, old) };
    }
    protos.insert(tag.to_vec(), unsafe { JS_DupValue(ctx, proto) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_adopt_dom_protos(ctx: *mut JSContext, protos: *const DomProtos) {
    let js = js_of_context(ctx);
    let (Some(core), Some(protos)) = (core(js), unsafe { protos.as_ref() }) else {
        return;
    };
    core.protos = protos.protos;
    core.per_tag_protos.get_or_insert_with(HashMap::new);
    core.dom_protos_set = true;
    let pinned = core.pinned_wrappers_set;
    if pinned.is_null() {
        return;
    }
    let mut len = 0u32;
    let keys = unsafe { glib::g_hash_table_get_keys_as_array(pinned, &mut len) };
    let nodes: Vec<*mut NsNode> = (0..len as usize)
        .map(|i| unsafe { *keys.add(i) }.cast())
        .collect();
    unsafe { glib::g_free(keys.cast()) };
    for n in nodes.into_iter().filter_map(|n| node(n)) {
        let wrapper = n.js_wrapper();
        if wrapper.is_null() {
            continue;
        }
        let proto = unsafe { ns_js_kind_proto(js, n.as_ptr()) };
        if is_object(proto) {
            unsafe { JS_SetPrototype(ctx, quickjs::identity_raw(wrapper), proto) };
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_root_for(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> *mut NsNode {
    let root = unsafe { super::wrapper::ns_unwrap_element(this_val) };
    if let Some(root) = node(root)
        && matches!(root.kind(), Kind::Document)
    {
        return root.as_mut_ptr();
    }
    core(js_of_context(ctx)).map_or(ptr::null_mut(), |core| core.current_doc)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_wrapper(
    ctx: *mut JSContext,
    node_ptr: *const NsNode,
) -> JSValue {
    let Some(n) = node(node_ptr) else {
        return quickjs::UNDEFINED;
    };
    if ctx.is_null() || n.js_wrapper().is_null() {
        return quickjs::UNDEFINED;
    }
    unsafe { JS_DupValue(ctx, quickjs::identity_raw(n.js_wrapper())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_unpinned(js: *const NsJs, node_ptr: *const NsNode) -> GBoolean {
    match core(js) {
        Some(core) if !core.pinned_wrappers_set.is_null() => GBoolean::from(
            unsafe { glib::g_hash_table_contains(core.pinned_wrappers_set, node_ptr.cast()) } == 0,
        ),
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_wrapper_pinned(
    js: *const NsJs,
    node_ptr: *const NsNode,
) -> GBoolean {
    let (Some(core), Some(n)) = (core(js), node(node_ptr)) else {
        return 0;
    };
    if n.js_wrapper().is_null() || core.pinned_wrappers_set.is_null() {
        return 0;
    }
    unsafe { glib::g_hash_table_contains(core.pinned_wrappers_set, node_ptr.cast()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tag_caller_document(ctx: *mut JSContext, node_val: JSValue) {
    let Some(core) = core(js_of_context(ctx)) else {
        return;
    };
    if !is_object(node_val) {
        return;
    }
    unsafe {
        let rctx = JS_GetCallerRealm(ctx);
        let main = if core.main_realm_ctx.is_null() {
            core.ctx
        } else {
            core.main_realm_ctx
        };
        let global = JS_GetGlobalObject(rctx);
        let doc = JS_GetPropertyStr(rctx, global, c"document".as_ptr());
        if rctx != main && is_object(doc) {
            JS_DefinePropertyValueStr(
                ctx,
                node_val,
                c"__ndOwnerDoc".as_ptr(),
                JS_DupValue(ctx, doc),
                0,
            );
        } else {
            ns_tag_owner_document(ctx, doc, node_val);
        }
        JS_FreeValue(rctx, doc);
        JS_FreeValue(rctx, global);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_new_orphan_element(
    ctx: *mut JSContext,
    name: *const c_char,
) -> JSValue {
    let js = js_of_context(ctx);
    let n = unsafe { ns_node_new_element(glib::g_strdup(name)) };
    if n.is_null() {
        return quickjs::UNDEFINED;
    }
    unsafe {
        super::wrapper::ns_node_arm_js_invalidate(n);
        ns_js_track_orphan(js, n);
        let element = super::wrapper::ns_make_element(ctx, n);
        ns_tag_caller_document(ctx, element);
        element
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_attr_changed(
    js: *mut NsJs,
    node_ptr: *mut NsNode,
    attr: *const c_char,
    old_value: *const c_char,
    new_value: *const c_char,
) {
    let live = core(js).is_some_and(|core| !core.ctx.is_null() && core.halted == 0);
    let attr_bytes = bytes(attr);
    if live && !node_ptr.is_null() && attr_bytes.is_some() {
        unsafe {
            if ptr::eq(node_ptr, ns_js_focused_node(js))
                && attr_bytes.is_some_and(|attr| attr.eq_ignore_ascii_case(b"type"))
            {
                ns_js_update_focus_visible(js);
            }
            ns_popover_attr_changed(js, node_ptr, attr, old_value, new_value);
        }
    }
    if !js.is_null()
        && attr_bytes.is_some_and(|attr| attr.eq_ignore_ascii_case(b"src"))
        && old_value.is_null()
        && !new_value.is_null()
        && let Some(n) = node(node_ptr)
        && n.element_name() == Some(b"script")
        && n.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0
    {
        unsafe { ns_js_script_needs_prepare(js, node_ptr) };
        return;
    }
    if !live || node_ptr.is_null() || attr_bytes.is_none() {
        return;
    }
    unsafe { ns_ce_attribute_changed(js, node_ptr, attr, old_value, new_value) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_form_reset(js: *mut NsJs, form: *mut NsNode) {
    let Some(core) = core(js) else {
        return;
    };
    if core.ctx.is_null() || form.is_null() {
        return;
    }
    unsafe { ns_js_reset_form(core.ctx, form) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_load_realm(js: *mut NsJs) -> *mut JSContext {
    let Some(core) = core(js) else {
        return ptr::null_mut();
    };
    let realm = if core.raf_frame_ctx.is_null() {
        ptr::null_mut()
    } else {
        unsafe { ns_js_frame_context(js, core.raf_frame_ctx) }
    };
    if realm.is_null() { core.ctx } else { realm }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_doc_base(js: *mut NsJs, el: *const NsNode) -> *const c_char {
    let frames = node(el)
        .and_then(Node::parent)
        .into_iter()
        .flat_map(|first| core::iter::successors(Some(first), |n| n.parent()));
    for p in frames {
        let is_frame = matches!(
            p.element_name(),
            Some(b"iframe") | Some(b"frame") | Some(b"object")
        );
        if is_frame
            && let Some(url) = p.attr(c"data-nd-frame-url")
            && !url.is_empty()
        {
            return url.as_ptr();
        }
    }
    core(js).map_or(ptr::null(), |core| core.current_url.cast_const())
}

const FRAME_WINDOW_CHAIN: &str = concat!(
    "(function(G){",
    "  var gp = Object.getPrototypeOf, sp = Object.setPrototypeOf;",
    "  var W = G.Window && G.Window.prototype, ET = G.EventTarget && G.EventTarget.prototype;",
    "  if (!W || !ET) return;",
    "  var named = gp(W);",
    "  if (named && named !== ET && named !== Object.prototype && gp(named) !== ET)",
    "    try { sp(named, ET); } catch (e) {}",
    "  if (gp(G) !== W) try { sp(G, W); } catch (e) {}",
    "})(globalThis)",
);

fn adopt_frame_window_events(core: &Core, fctx: *mut JSContext, fg: JSValue) {
    const EVAL_FLAGS: c_int = 1 << 8;
    let mut source = FRAME_WINDOW_CHAIN.as_bytes().to_vec();
    source.push(0);
    unsafe {
        let chain = JS_Eval(
            fctx,
            source.as_ptr().cast(),
            source.len() - 1,
            c"<frame-window-chain>".as_ptr(),
            EVAL_FLAGS,
        );
        if quickjs::raw_tag(chain) == quickjs::raw_tag(quickjs::EXCEPTION) {
            JS_FreeValue(fctx, JS_GetException(fctx));
        }
        JS_FreeValue(fctx, chain);
        let main = if core.main_realm_ctx.is_null() {
            core.ctx
        } else {
            core.main_realm_ctx
        };
        let main_global = JS_GetGlobalObject(main);
        let adopt = JS_GetPropertyStr(main, main_global, c"__ndAdoptWindowEventOps".as_ptr());
        if JS_IsFunction(main, adopt) {
            let mut args = [fg];
            let result = JS_Call(main, adopt, quickjs::UNDEFINED, 1, args.as_mut_ptr());
            if quickjs::raw_tag(result) == quickjs::raw_tag(quickjs::EXCEPTION) {
                JS_FreeValue(main, JS_GetException(main));
            }
            JS_FreeValue(main, result);
        }
        JS_FreeValue(main, adopt);
        JS_FreeValue(main, main_global);
        let symbol = JS_GetPropertyStr(fctx, fg, c"Symbol".as_ptr());
        let tag_symbol = JS_GetPropertyStr(fctx, symbol, c"toStringTag".as_ptr());
        let atom = JS_ValueToAtom(fctx, tag_symbol);
        if atom != 0 {
            JS_DeleteProperty(fctx, fg, atom, 0);
            JS_FreeAtom(fctx, atom);
        }
        JS_FreeValue(fctx, tag_symbol);
        JS_FreeValue(fctx, symbol);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_realm_finish(
    js: *mut NsJs,
    fctx: *mut JSContext,
    fg: JSValue,
    parent_global: JSValue,
) {
    let Some(core) = core(js) else {
        return;
    };
    unsafe {
        ns_install_pdf_plugins(fctx);
        adopt_frame_window_events(core, fctx, fg);
        ns_js_link_interface_ctors(fctx);
        ns_js_lock_global_prototypes(fctx);
        ns_js_brand_node_interfaces(
            fctx,
            super::wrapper::ns_element_class_id(),
            ns_attr_class_id(),
        );
        let performance = JS_GetPropertyStr(fctx, parent_global, c"performance".as_ptr());
        let include_memory = is_object(performance)
            && ns_obj_has_defined_own_prop(fctx, performance, c"memory".as_ptr()) != 0;
        JS_FreeValue(fctx, performance);
        JS_SetPropertyStr(
            fctx,
            fg,
            c"performance".as_ptr(),
            ns_make_performance_object(fctx, js, GBoolean::from(include_memory)),
        );
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_url_parses(js: *mut NsJs, url: *const c_char) -> GBoolean {
    super::helpers::url_parses(js, url)
}

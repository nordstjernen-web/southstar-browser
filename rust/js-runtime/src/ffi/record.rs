//! Southstar — the page's JavaScript record (ns_js): its runtime and realms, DOM prototypes, storage partition, cookies and CSP, and the node and realm helpers the bindings reach through it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_dom::{Kind, Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable, GStr};
use southstar_js_bind::bind_native;
use southstar_js_engine::{
    Attributes, ContextHandle, RuntimeHandle, RuntimeOptions, Scope, ScriptOptions, Value,
};

use super::NavigationTiming;
use super::imports;
use super::state::Slots;

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_IMG_LOAD_FIRED: u32 = 1 << 3;
const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;
const FLAG_SVG_NS: u32 = 1 << 7;
const FLAG_FOREIGN_NS: u32 = 1 << 9;
const FLAG_CDATA: u32 = 1 << 10;
const FLAG_PI: u32 = 1 << 11;
const CSP_CONNECT: c_int = 5;
const CSP_WORKER: c_int = 9;
const CSP_FORM_ACTION: c_int = 13;
const MATHML_NS: &CStr = c"http://www.w3.org/1998/Math/MathML";

static POLYFILLS: &str = concat!(include_str!("../../../../data/js/polyfills.js"), "\0");
static STREAMING: &str = concat!(include_str!("../../../../data/js/streaming.js"), "\0");

const FRAME_WINDOW_CHAIN: &[u8] = b"(function(G){\
  var gp = Object.getPrototypeOf, sp = Object.setPrototypeOf;\
  var W = G.Window && G.Window.prototype, ET = G.EventTarget && G.EventTarget.prototype;\
  if (!W || !ET) return;\
  var named = gp(W);\
  if (named && named !== ET && named !== Object.prototype && gp(named) !== ET)\
    try { sp(named, ET); } catch (e) {}\
  if (gp(G) !== W) try { sp(G, W); } catch (e) {}\
})(globalThis)";

const SITE_QUIRKS: &[u8] = b"(function(){\
try{\
var l=location&&location.hostname||'';\
if(l!=='freecivweb.com'&&l!=='www.freecivweb.com'&&l!=='fcw.movingborders.es')return;\
if(typeof tile_types_setup!=='object'||!tile_types_setup)return;\
if(typeof MATCH_NONE!=='number'||typeof MATCH_SAME!=='number'||\
typeof MATCH_PAIR!=='number'||typeof MATCH_FULL!=='number'||\
typeof CELL_WHOLE!=='number'||typeof CELL_CORNER!=='number')return;\
var r=typeof MATCH_RANDOM==='number'?MATCH_RANDOM:4,t=tile_types_setup;\
function s(k,m,c){var o=t[k];if(!o)return;if(o.match_style===void 0)o.match_style=m;if(o.sprite_type===void 0)o.sprite_type=c;}\
s('l0.lake',MATCH_PAIR,CELL_CORNER);\
s('l0.coast',MATCH_FULL,CELL_CORNER);\
s('l1.coast',MATCH_PAIR,CELL_CORNER);\
s('l0.floor',MATCH_FULL,CELL_CORNER);\
s('l1.floor',MATCH_PAIR,CELL_CORNER);\
s('l0.arctic',MATCH_NONE,CELL_WHOLE);\
s('l0.desert',MATCH_NONE,CELL_WHOLE);\
s('l1.desert',r,CELL_WHOLE);\
s('l0.forest',MATCH_NONE,CELL_WHOLE);\
s('l1.forest',MATCH_SAME,CELL_WHOLE);\
s('l0.grassland',MATCH_NONE,CELL_WHOLE);\
s('l1.grassland',r,CELL_WHOLE);\
s('l0.hills',MATCH_NONE,CELL_WHOLE);\
s('l1.hills',MATCH_SAME,CELL_WHOLE);\
s('l0.jungle',MATCH_NONE,CELL_WHOLE);\
s('l1.jungle',MATCH_SAME,CELL_WHOLE);\
s('l0.mountains',MATCH_NONE,CELL_WHOLE);\
s('l1.mountains',MATCH_SAME,CELL_WHOLE);\
s('l0.plains',MATCH_NONE,CELL_WHOLE);\
s('l1.plains',r,CELL_WHOLE);\
s('l0.swamp',MATCH_NONE,CELL_WHOLE);\
s('l1.swamp',r,CELL_WHOLE);\
s('l0.tundra',MATCH_NONE,CELL_WHOLE);\
s('l0.inaccessible',MATCH_NONE,CELL_WHOLE);\
}catch(e){}\
})()";

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

const FORM_CONTROLS: [&CStr; 7] = [
    c"button",
    c"fieldset",
    c"input",
    c"optgroup",
    c"option",
    c"select",
    c"textarea",
];

#[repr(C)]
pub(crate) struct NsCsp {
    _private: [u8; 0],
}

pub struct AdoptedProtos {
    pub node: Value,
    pub element: Value,
    pub html_element: Value,
    pub html_unknown_element: Value,
    pub svg_element: Value,
    pub svga_element: Value,
    pub mathml_element: Value,
    pub chardata: Value,
    pub text: Value,
    pub comment: Value,
    pub cdata: Value,
    pub pi: Value,
    pub doctype: Value,
    pub docfrag: Value,
    pub document: Value,
}

struct DomProtos {
    element: Value,
    html_element: Value,
    html_unknown_element: Value,
    svg_element: Value,
    svga_element: Value,
    mathml_element: Value,
    text: Value,
    comment: Value,
    cdata: Value,
    pi: Value,
    doctype: Value,
    docfrag: Value,
}

#[repr(C)]
pub struct NsJs {
    _private: [u8; 0],
}

struct Record {
    rt: RuntimeHandle,
    ctx: ContextHandle,
    main_realm_ctx: ContextHandle,
    frame_ctxs: Option<Vec<ContextHandle>>,
    current_url: *mut c_char,
    current_doc: *mut NsNode,
    raf_frame_ctx: *mut NsNode,
    pristine_promise: Value,
    layout_root: *const c_void,
    pinned_wrappers_set: *mut GHashTable,
    cookie_value: Option<CString>,
    cookie_buckets: Option<HashMap<CString, CString>>,
    partition_key: Option<CString>,
    opaque_counter: u64,
    navigator_brand: Value,
    halted: GBoolean,
    csp: *mut NsCsp,
    time_origin_us: i64,
    time_origin_real_ms: f64,
    navigation_timing: NavigationTiming,
    ce_main_doc: *const NsNode,
    platform_globals: *mut GHashTable,
    protos: Option<DomProtos>,
    proto_document: Value,
    per_tag_protos: HashMap<Vec<u8>, Value>,
    runtime: *mut c_void,
}

unsafe extern "C" {
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_storage_switch_session(
        js: *mut NsJs,
        old_partition: *const c_char,
        new_partition: *const c_char,
    );
    fn ns_storage_load_local(js: *mut NsJs, url: *const c_char);
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_element_get_attr_len(
        node: *const NsNode,
        name: *const c_char,
        len: *mut usize,
    ) -> *const c_char;
    fn ns_ce_name_valid(name: *const c_char) -> GBoolean;
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
    fn ns_node_is_element_named(n: *const NsNode, tag: *const c_char) -> GBoolean;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_csp_allows(
        csp: *const NsCsp,
        kind: c_int,
        url: *const c_char,
        page: *const c_char,
    ) -> GBoolean;
    fn ns_csp_parse(header: *const c_char) -> *mut NsCsp;
    fn ns_csp_merge(dst: *mut NsCsp, src: *mut NsCsp);
    fn ns_csp_free(csp: *mut NsCsp);
    fn ns_csp_inline_event_handler_allowed(csp: *const NsCsp) -> GBoolean;
    fn ns_node_next_in_subtree(
        n: *const NsNode,
        root: *const NsNode,
        descend: GBoolean,
    ) -> *mut NsNode;
    fn ns_js_purge_frame_rafs(js: *mut NsJs, frame: *const NsNode);
    fn ns_services_purge_frame_timers(js: *mut NsJs, frame: *const NsNode);
    fn ns_js_scrub_iframe_globals(js: *mut NsJs, iframe: *const NsNode);
    fn ns_node_iters_pre_remove(js: *mut NsJs, removed: *mut NsNode);
    fn ns_node_remove(child: *mut NsNode);
    fn ns_js_unorphan_node(js: *mut NsJs, n: *mut NsNode);
    fn ns_box_lookup_forget(js: *mut NsJs);
    fn ns_js_sync_window_metrics(js: *const NsJs);
    fn ns_js_promote_deferred_iframes(js: *mut NsJs);
    fn ns_intersection_observers_tick(js: *mut NsJs);
    fn ns_resize_observers_tick(js: *mut NsJs);
    fn ns_attr_name_is_internal(name: *const c_char) -> GBoolean;
    fn ns_element_effectively_disabled(el: *const NsNode) -> GBoolean;
    fn ns_js_frame_context(js: *mut NsJs, frame: *const NsNode) -> ContextHandle;
    fn ns_install_pdf_plugins(ctx: ContextHandle);
    fn ns_js_link_interface_ctors(ctx: ContextHandle);
    fn ns_js_lock_global_prototypes(ctx: ContextHandle);
    fn ns_js_brand_node_interfaces(ctx: ContextHandle, element_cid: u32, attr_cid: u32);
    fn ns_attr_class_id() -> u32;
    fn ns_node_new_element(name: *mut c_char) -> *mut NsNode;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_js_update_focus_visible(js: *const NsJs);
    fn ns_popover_attr_changed(
        js: *const NsJs,
        el: *mut NsNode,
        attr: *const c_char,
        old_value: *const c_char,
        new_value: *const c_char,
    );
    fn ns_js_script_needs_prepare(js: *mut NsJs, script: *mut NsNode);
    fn ns_ce_attribute_changed(
        js: *const NsJs,
        node: *const NsNode,
        attr: *const c_char,
        old_value: *const c_char,
        new_value: *const c_char,
    );
    fn ns_js_eval(js: *mut NsJs, src: *const c_char, len: usize, origin: *const c_char);
}

thread_local! {
    static ACTIVE: Cell<*mut NsJs> = const { Cell::new(ptr::null_mut()) };
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn is_named(n: Node<'_>, tag: &CStr) -> bool {
    unsafe { ns_node_is_element_named(n.as_ptr(), tag.as_ptr()) != 0 }
}

fn attr<'a>(n: Node<'a>, name: &CStr) -> Option<&'a CStr> {
    let value = unsafe { ns_element_get_attr(n.as_ptr(), name.as_ptr()) };
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) })
}

fn rec(js: *const NsJs) -> *mut Record {
    js.cast_mut().cast()
}

fn record<'a>(js: *const NsJs) -> Option<&'a Record> {
    unsafe { rec(js).as_ref() }
}

fn main_context(js: &Record) -> ContextHandle {
    js.main_realm_ctx.or(js.ctx)
}

fn page_of(scope: &Scope<'_>) -> *mut NsJs {
    scope.context_opaque().cast()
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn opt_ptr(text: &Option<CString>) -> *const c_char {
    text.as_ref().map_or(ptr::null(), |text| text.as_ptr())
}

pub(crate) fn alloc() -> *mut NsJs {
    Box::into_raw(Box::new(Record {
        rt: RuntimeHandle::NULL,
        ctx: ContextHandle::NULL,
        main_realm_ctx: ContextHandle::NULL,
        frame_ctxs: None,
        current_url: ptr::null_mut(),
        current_doc: ptr::null_mut(),
        raf_frame_ctx: ptr::null_mut(),
        pristine_promise: Value::undefined(),
        layout_root: ptr::null(),
        pinned_wrappers_set: ptr::null_mut(),
        cookie_value: None,
        cookie_buckets: None,
        partition_key: None,
        opaque_counter: 0,
        navigator_brand: Value::undefined(),
        halted: glib::FALSE,
        csp: ptr::null_mut(),
        time_origin_us: 0,
        time_origin_real_ms: 0.0,
        navigation_timing: NavigationTiming::starting_at(0, 0.0),
        ce_main_doc: ptr::null(),
        platform_globals: ptr::null_mut(),
        protos: None,
        proto_document: Value::undefined(),
        per_tag_protos: HashMap::new(),
        runtime: ptr::null_mut(),
    }))
    .cast()
}

pub(crate) unsafe fn slots_of(js: *mut NsJs) -> Slots {
    unsafe {
        Slots {
            rt: &raw mut (*rec(js)).rt,
            ctx: &raw mut (*rec(js)).ctx,
            main_realm_ctx: &raw mut (*rec(js)).main_realm_ctx,
            current_url: &raw mut (*rec(js)).current_url,
            current_doc: &raw mut (*rec(js)).current_doc,
            raf_frame_ctx: &raw mut (*rec(js)).raf_frame_ctx,
            ce_main_doc: &raw mut (*rec(js)).ce_main_doc,
            halted: &raw mut (*rec(js)).halted,
            pinned_wrappers_set: &raw mut (*rec(js)).pinned_wrappers_set,
        }
    }
}

pub(crate) unsafe fn runtime_state(js: *const NsJs) -> *mut c_void {
    record(js).map_or(ptr::null_mut(), |js| js.runtime)
}

pub(crate) unsafe fn set_runtime_state(js: *mut NsJs, state: *mut c_void) {
    unsafe { (*rec(js)).runtime = state };
}

pub(crate) unsafe fn set_time_origin(js: *mut NsJs, origin_us: i64, origin_real_ms: f64) {
    unsafe {
        (*rec(js)).time_origin_us = origin_us;
        (*rec(js)).time_origin_real_ms = origin_real_ms;
    }
}

pub(crate) unsafe fn set_navigation_timing(js: *mut NsJs, timing: NavigationTiming) {
    unsafe { (*rec(js)).navigation_timing = timing };
}

pub(crate) unsafe fn create_runtime(js: *mut NsJs, options: &RuntimeOptions) -> bool {
    let rt = RuntimeHandle::new(options);
    unsafe { (*rec(js)).rt = rt };
    !rt.is_null()
}

pub(crate) unsafe fn create_context(js: *mut NsJs) -> bool {
    let rt = unsafe { (*rec(js)).rt };
    let ctx = rt.new_context(js.cast());
    if ctx.is_null() {
        return false;
    }
    unsafe {
        (*rec(js)).ctx = ctx;
        (*rec(js)).main_realm_ctx = ctx;
    }
    rt.set_opaque(js.cast());
    true
}

pub(crate) unsafe fn init_tables(js: *mut NsJs, page: bool) {
    unsafe {
        (*rec(js)).pinned_wrappers_set =
            glib::g_hash_table_new(Some(glib::g_direct_hash), Some(glib::g_direct_equal));
        if page {
            (*rec(js)).frame_ctxs = Some(Vec::new());
            (*rec(js)).cookie_buckets = Some(HashMap::new());
        }
    }
}

pub(crate) unsafe fn set_partition_key(js: *mut NsJs, partition_key: *const c_char) {
    let key = unsafe { glib::bytes(partition_key) }.map(cstring);
    unsafe { (*rec(js)).partition_key = key };
}

pub(crate) unsafe fn pinned_wrappers(js: *const NsJs) -> *mut GHashTable {
    record(js).map_or(ptr::null_mut(), |js| js.pinned_wrappers_set)
}

pub(crate) unsafe fn set_proto_document(js: *mut NsJs, proto: Value) {
    unsafe { (*rec(js)).proto_document = proto };
}

pub(crate) unsafe fn set_navigator_brand(js: *mut NsJs, brand: Value) {
    unsafe { (*rec(js)).navigator_brand = brand };
}

pub(crate) unsafe fn set_pristine_promise(js: *mut NsJs, promise: Value) {
    unsafe { (*rec(js)).pristine_promise = promise };
}

pub(crate) unsafe fn release_wrapper(js: *mut NsJs, n: *mut NsNode) {
    let Some(n) = node(n) else {
        return;
    };
    let wrapper = n.js_wrapper();
    if wrapper.is_null() {
        return;
    }
    let ctx = unsafe { (*rec(js)).ctx };
    ctx.with(|scope| unsafe { scope.release_identity(wrapper) });
}

pub(crate) unsafe fn detach_wrapper(js: *mut NsJs, n: *mut NsNode, release: bool) {
    let Some(n) = node(n) else {
        return;
    };
    let wrapper = n.js_wrapper();
    if wrapper.is_null() {
        return;
    }
    unsafe { southstar_js_engine::clear_identity_instance(wrapper) };
    n.set_js_wrapper(ptr::null_mut());
    n.set_js_invalidate(None);
    if release {
        let ctx = unsafe { (*rec(js)).ctx };
        ctx.with(|scope| unsafe { scope.release_identity(wrapper) });
    }
}

pub(crate) unsafe fn eval_worker_polyfills(js: *mut NsJs) {
    unsafe {
        ns_js_eval(
            js,
            POLYFILLS.as_ptr().cast(),
            POLYFILLS.len() - 1,
            c"<worker-polyfills>".as_ptr(),
        )
    };
}

pub(crate) unsafe fn free_page_strings(js: *mut NsJs) {
    unsafe {
        if !(*rec(js)).csp.is_null() {
            ns_csp_free((*rec(js)).csp);
            (*rec(js)).csp = ptr::null_mut();
        }
        (*rec(js)).cookie_value = None;
        glib::g_free((*rec(js)).current_url.cast());
        (*rec(js)).current_url = ptr::null_mut();
    }
}

pub(crate) unsafe fn free_pinned_set(js: *mut NsJs) {
    unsafe {
        let set = (*rec(js)).pinned_wrappers_set;
        if !set.is_null() {
            glib::g_hash_table_destroy(set);
            (*rec(js)).pinned_wrappers_set = ptr::null_mut();
        }
    }
}

pub(crate) unsafe fn free_realm_values(js: *mut NsJs) {
    let (pristine, protos, document, per_tag) = unsafe {
        (
            core::mem::replace(&mut (*rec(js)).pristine_promise, Value::undefined()),
            (*rec(js)).protos.take(),
            core::mem::replace(&mut (*rec(js)).proto_document, Value::undefined()),
            core::mem::take(&mut (*rec(js)).per_tag_protos),
        )
    };
    drop((pristine, protos, document, per_tag));
}

pub(crate) unsafe fn free_platform_globals(js: *mut NsJs) {
    unsafe {
        let globals = (*rec(js)).platform_globals;
        if !globals.is_null() {
            glib::g_hash_table_destroy(globals);
            (*rec(js)).platform_globals = ptr::null_mut();
        }
    }
}

pub(crate) unsafe fn free_partition(js: *mut NsJs) {
    unsafe {
        (*rec(js)).cookie_buckets = None;
        (*rec(js)).partition_key = None;
    }
}

pub(crate) unsafe fn free_window_values(js: *mut NsJs) {
    let brand = unsafe { core::mem::replace(&mut (*rec(js)).navigator_brand, Value::undefined()) };
    drop(brand);
}

pub(crate) unsafe fn free_engine(js: *mut NsJs) {
    unsafe {
        free_realm_values(js);
        free_window_values(js);
        for ctx in (*rec(js)).frame_ctxs.take().unwrap_or_default() {
            ctx.free();
        }
        (*rec(js)).ctx.free();
        (*rec(js)).rt.free();
        drop(Box::from_raw(rec(js)));
    }
}

pub(crate) unsafe fn worker_compile(
    scope: &mut Scope<'_>,
    source: &CStr,
    len: usize,
    url: &CStr,
    module: bool,
) -> (Result<Value, Value>, bool) {
    let bytes = &source.to_bytes()[..len.min(source.to_bytes().len())];
    if !module {
        let options = ScriptOptions {
            name: Some(url.to_bytes()),
            line: 1,
            ..ScriptOptions::default()
        };
        let compiled = scope.compile_script(bytes, &options);
        let parse_error = compiled.is_err();
        return (compiled, parse_error);
    }
    let compiled = match imports::nsr_js_compile_module_cached(scope, bytes, url.to_bytes()) {
        Ok(compiled) => compiled,
        Err(error) => return (Err(error), true),
    };
    if let Err(error) = imports::nsr_js_module_set_import_meta(scope, &compiled, true) {
        return (Err(error), false);
    }
    (Ok(compiled), false)
}

fn partition_apply(js: *mut NsJs, new_url: *const c_char) {
    let origin = unsafe { GStr::take(ns_url_origin_from(new_url)) }
        .filter(|origin| !origin.is_empty())
        .map(|origin| CString::from(&*origin));
    let new_key = origin.unwrap_or_else(|| {
        let counter = unsafe {
            (*rec(js)).opaque_counter += 1;
            (*rec(js)).opaque_counter
        };
        cstring(format!("opaque://{counter}").as_bytes())
    });
    if unsafe { (*rec(js)).partition_key.as_ref() } == Some(&new_key) {
        return;
    }
    unsafe {
        if let (Some(key), Some(buckets)) = (
            (*rec(js)).partition_key.clone(),
            (*rec(js)).cookie_buckets.as_mut(),
        ) {
            let value = (*rec(js)).cookie_value.clone().unwrap_or_default();
            buckets.insert(key, value);
        }
        let old_key = opt_ptr(&(*rec(js)).partition_key);
        ns_storage_switch_session(js, old_key, new_key.as_ptr());
        (*rec(js)).cookie_value = (*rec(js))
            .cookie_buckets
            .as_ref()
            .and_then(|buckets| buckets.get(&new_key).cloned());
        (*rec(js)).partition_key = Some(new_key);
    }
}

fn foreign_kind_proto<'a>(protos: &'a DomProtos, n: Node<'_>) -> &'a Value {
    let mathml = attr(n, c"data-nd-ns-uri").is_some_and(|ns| ns == MATHML_NS);
    if mathml && protos.mathml_element.is_object() {
        &protos.mathml_element
    } else {
        &protos.element
    }
}

fn html_kind_proto<'a>(js: &'a Record, protos: &'a DomProtos, n: Node<'_>) -> &'a Value {
    let Some(name) = n.name() else {
        return &protos.html_element;
    };
    let name = name.to_bytes();
    if !name.iter().any(u8::is_ascii_uppercase) {
        if let Some(proto) = js.per_tag_protos.get(name) {
            return proto;
        }
        if PLAIN_INTERFACE_TAGS.contains(&name) {
            return &protos.html_element;
        }
    }
    let valid_custom = n
        .name()
        .is_some_and(|name| unsafe { ns_ce_name_valid(name.as_ptr()) } != 0);
    if !valid_custom && protos.html_unknown_element.is_object() {
        return &protos.html_unknown_element;
    }
    &protos.html_element
}

fn kind_proto<'a>(js: &'a Record, n: Node<'_>) -> Option<&'a Value> {
    let protos = js.protos.as_ref()?;
    let flags = n.flags();
    Some(match n.kind() {
        Kind::Element => {
            if flags & FLAG_SVG_NS != 0 {
                if n.name()
                    .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"a"))
                    && protos.svga_element.is_object()
                {
                    &protos.svga_element
                } else if protos.svg_element.is_object() {
                    &protos.svg_element
                } else {
                    &protos.element
                }
            } else if flags & FLAG_FOREIGN_NS != 0 {
                foreign_kind_proto(protos, n)
            } else {
                html_kind_proto(js, protos, n)
            }
        }
        Kind::Text if flags & FLAG_CDATA != 0 => &protos.cdata,
        Kind::Text => &protos.text,
        Kind::Comment if flags & FLAG_PI != 0 => &protos.pi,
        Kind::Comment => &protos.comment,
        Kind::Doctype => &protos.doctype,
        Kind::Document if flags & FLAG_FRAGMENT != 0 => &protos.docfrag,
        Kind::Document => &js.proto_document,
        Kind::Other => return None,
    })
}

pub(crate) unsafe fn kind_proto_value(js: *const NsJs, n: *const NsNode) -> Option<Value> {
    match (record(js), node(n)) {
        (Some(js), Some(n)) => kind_proto(js, n).cloned(),
        _ => None,
    }
}

fn adopt_frame_window_events(js: &Record, fctx: ContextHandle, frame_global: &Value) {
    let options = ScriptOptions {
        name: Some(b"<frame-window-chain>"),
        line: 1,
        hide_source: true,
        ..ScriptOptions::default()
    };
    fctx.with(|scope| drop(scope.eval_script_bytes(FRAME_WINDOW_CHAIN, &options)));
    main_context(js).with(|scope| {
        let global = scope.global();
        let Ok(adopt) = scope.get(&global, "__ndAdoptWindowEventOps") else {
            return;
        };
        if scope.is_function(&adopt) {
            drop(scope.call(
                &adopt,
                &Value::undefined(),
                core::slice::from_ref(frame_global),
            ));
        }
    });
    fctx.with(|scope| {
        let tag = scope
            .get(frame_global, "Symbol")
            .and_then(|symbol| scope.get(&symbol, "toStringTag"));
        if let Ok(tag) = tag {
            drop(scope.delete_key(frame_global, &tag));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_active_js() -> *mut NsJs {
    ACTIVE.with(Cell::get)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_set_active_js(js: *mut NsJs) {
    ACTIVE.with(|active| active.set(js));
}

pub(crate) fn array_length(scope: &mut Scope<'_>, arr: &Value) -> u32 {
    scope
        .get(arr, "length")
        .and_then(|length| scope.to_int64(&length))
        .map_or(0, |length| length as u32)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_load_for(js: *mut NsJs, new_url: *const c_char) {
    if js.is_null() {
        return;
    }
    partition_apply(js, new_url);
    unsafe { ns_storage_load_local(js, new_url) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_img_src_layout_neutral(n: *const NsNode) -> GBoolean {
    let neutral = node(n).is_some_and(|n| {
        n.name().is_some_and(|name| name.to_bytes() == b"img")
            && attr(n, c"width").is_some_and(|w| !w.is_empty())
            && attr(n, c"height").is_some_and(|h| !h.is_empty())
    });
    glib::boolean(neutral)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_src_recorded(
    js: *mut NsJs,
    n: *mut NsNode,
    s: *const c_char,
    slen: usize,
) {
    let Some(el) = node(n) else {
        return;
    };
    let new = unsafe { glib::slice(s.cast(), slen) };
    let mut old_len = 0usize;
    let old = unsafe { ns_element_get_attr_len(n, c"src".as_ptr(), &mut old_len) };
    let changed = old.is_null() || unsafe { glib::slice(old.cast(), old_len) } != new;
    unsafe {
        if changed && ns_js_img_src_layout_neutral(n) != 0 {
            ns_js_set_attr_recorded_paint_only(js, n, c"src".as_ptr(), s);
        } else {
            ns_js_set_attr_recorded_len(js, n, c"src".as_ptr(), s, slen as isize);
        }
    }
    let name = el.name().map(CStr::to_bytes);
    if changed && name == Some(b"img") {
        el.remove_flags(FLAG_IMG_LOAD_FIRED);
        unsafe { ns_js_start_image_load(js, n, s) };
    }
    if name == Some(b"iframe") {
        unsafe { ns_js_schedule_iframe_load_full(js, n, glib::TRUE) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_in_template_content(n: *const NsNode) -> GBoolean {
    let inside = node(n).is_some_and(|n| {
        southstar_dom::ancestors_and_self(n)
            .any(|p| p.flags() & FLAG_TEMPLATE_CONTENT != 0 || is_named(p, c"template"))
    });
    glib::boolean(inside)
}

pub(crate) fn bind_event_target_listeners(scope: &mut Scope<'_>, obj: &Value) {
    bind_native(
        scope,
        obj,
        "addEventListener",
        2,
        imports::nsr_target_addEventListener,
    );
    bind_native(
        scope,
        obj,
        "removeEventListener",
        2,
        imports::nsr_target_removeEventListener,
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_media_native_formats() -> c_uint {
    let mut formats = 0;
    if cfg!(feature = "libav") {
        formats |= 1;
    }
    if cfg!(feature = "audio-native-vorbis") {
        formats |= 2;
    }
    if cfg!(feature = "audio-native-opus") {
        formats |= 4;
    }
    formats
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_url_parses(js: *mut NsJs, url: *const c_char) -> GBoolean {
    if url.is_null() {
        return glib::FALSE;
    }
    let base = unsafe { GStr::take(ns_js_doc_base_url(js)) };
    let base = base
        .as_deref()
        .filter(|base| !base.is_empty())
        .map_or(ptr::null(), CStr::as_ptr);
    let resolved = unsafe { GStr::take(ns_url_resolve(base, url)) };
    glib::boolean(resolved.is_some())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_csp_allows_worker(js: *mut NsJs, url: *const c_char) -> GBoolean {
    let Some(js) = record(js) else {
        return glib::TRUE;
    };
    glib::boolean(
        js.csp.is_null() || unsafe { ns_csp_allows(js.csp, CSP_WORKER, url, js.current_url) } != 0,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_engine_version() -> *const c_char {
    static VERSION: OnceLock<CString> = OnceLock::new();
    VERSION
        .get_or_init(|| cstring(southstar_js_engine::engine_version().as_bytes()))
        .as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_subtree_rafs(js: *mut NsJs, root: *mut NsNode) {
    if js.is_null() {
        return;
    }
    let mut n = root;
    while !n.is_null() {
        if node(n).is_some_and(|el| is_named(el, c"iframe")) {
            unsafe {
                ns_js_purge_frame_rafs(js, n);
                ns_services_purge_frame_timers(js, n);
                ns_js_scrub_iframe_globals(js, n);
            }
        }
        n = unsafe { ns_node_next_in_subtree(n, root, glib::TRUE) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_insert_before_single(
    js: *mut NsJs,
    parent: *mut NsNode,
    newc: *mut NsNode,
    reference: *mut NsNode,
) {
    let (Some(_), Some(child), Some(reference_node)) = (node(parent), node(newc), node(reference))
    else {
        return;
    };
    if newc == reference {
        return;
    }
    if child.parent().is_some() {
        unsafe {
            if !js.is_null() {
                ns_node_iters_pre_remove(js, newc);
            }
            ns_node_remove(newc);
        }
    }
    if !js.is_null() {
        unsafe { ns_js_unorphan_node(js, newc) };
    }
    reference_node.insert_before(child);
}

pub(crate) fn attr_apply_proto(scope: &mut Scope<'_>, obj: &Value) {
    let global = scope.global();
    let proto = scope
        .get(&global, "Attr")
        .and_then(|ctor| scope.get(&ctor, "prototype"));
    if let Ok(proto) = proto
        && proto.is_object()
    {
        drop(scope.set_prototype(obj, &proto));
    }
    bind_native(scope, obj, "cloneNode", 0, imports::nsr_attr_cloneNode);
    bind_native(scope, obj, "isSameNode", 1, imports::nsr_element_isSameNode);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_layout_root(js: *mut NsJs, root: *const c_void) {
    if js.is_null() {
        return;
    }
    unsafe {
        if (*rec(js)).layout_root != root {
            ns_box_lookup_forget(js);
        }
        (*rec(js)).layout_root = root;
        ns_js_sync_window_metrics(js);
        if !root.is_null() {
            ns_js_promote_deferred_iframes(js);
            ns_intersection_observers_tick(js);
            ns_resize_observers_tick(js);
        }
    }
}

pub(crate) fn build_attributes(scope: &mut Scope<'_>, owner: &Value) -> Value {
    let array = scope.new_array();
    let Some(root) = super::wrapper::unwrap(owner) else {
        return array;
    };
    let attrs: Vec<Value> = root
        .attrs()
        .filter(|a| {
            a.name()
                .is_some_and(|name| unsafe { ns_attr_name_is_internal(name.as_ptr()) } == 0)
        })
        .map(|a| {
            unsafe { imports::nsr_attr_to_js(scope, owner, a.as_ptr(), false) }
                .unwrap_or_else(|_| Value::undefined())
        })
        .collect();
    for (index, attr) in attrs.into_iter().enumerate() {
        drop(scope.set_index(&array, index as u32, attr));
    }
    array
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_disabled_form_control(el: *const NsNode) -> GBoolean {
    let disabled = node(el).is_some_and(|el| {
        el.is_element()
            && el.name().is_some_and(|name| {
                FORM_CONTROLS
                    .iter()
                    .any(|control| name.to_bytes().eq_ignore_ascii_case(control.to_bytes()))
            })
            && unsafe { ns_element_effectively_disabled(el.as_ptr()) } != 0
    });
    glib::boolean(disabled)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_form_reset(js: *mut NsJs, form: *mut NsNode) {
    let Some(ctx) = record(js).map(|js| js.ctx) else {
        return;
    };
    if form.is_null() {
        return;
    }
    ctx.with(|scope| unsafe { imports::nsr_js_reset_form(scope, form) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_load_realm(js: *mut NsJs) -> ContextHandle {
    let Some(record) = record(js) else {
        return ContextHandle::NULL;
    };
    let realm = if record.raf_frame_ctx.is_null() {
        ContextHandle::NULL
    } else {
        unsafe { ns_js_frame_context(js, record.raf_frame_ctx) }
    };
    realm.or(record.ctx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_doc_base(js: *mut NsJs, el: *const NsNode) -> *const c_char {
    if let Some(el) = node(el) {
        for p in southstar_dom::ancestors(el) {
            if (is_named(p, c"iframe") || is_named(p, c"frame") || is_named(p, c"object"))
                && let Some(url) = attr(p, c"data-nd-frame-url").filter(|url| !url.is_empty())
            {
                return url.as_ptr();
            }
        }
    }
    record(js).map_or(ptr::null(), |js| js.current_url.cast_const())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_platform_globals(js: *mut NsJs) -> *mut GHashTable {
    unsafe {
        if (*rec(js)).platform_globals.is_null() {
            let saved = (*rec(js)).ctx;
            (*rec(js)).ctx = main_context(&*rec(js));
            let globals = super::exports::ns_js_snapshot_globals(js);
            (*rec(js)).ctx = saved;
            (*rec(js)).platform_globals = globals;
        }
        (*rec(js)).platform_globals
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_new_frame_context(js: *mut NsJs) -> ContextHandle {
    let fctx = unsafe { (*rec(js)).rt }.new_context(js.cast());
    if fctx.is_null() {
        return fctx;
    }
    fctx.with(|scope| {
        let global = scope.global();
        southstar_js_realm::hide_shared_array_buffer(scope, &global);
    });
    if let Some(frames) = unsafe { (*rec(js)).frame_ctxs.as_mut() } {
        frames.push(fctx);
    }
    fctx
}

pub(crate) unsafe fn frame_realm_finish(
    js: *mut NsJs,
    fctx: ContextHandle,
    frame_global: &Value,
    parent_global: &Value,
) {
    unsafe {
        ns_install_pdf_plugins(fctx);
        adopt_frame_window_events(&*rec(js), fctx, frame_global);
        ns_js_link_interface_ctors(fctx);
        ns_js_lock_global_prototypes(fctx);
        ns_js_brand_node_interfaces(fctx, super::wrapper::ELEMENT_CLASS.id(), ns_attr_class_id());
    }
    fctx.with(|scope| {
        let include_memory = scope
            .get(parent_global, "performance")
            .is_ok_and(|performance| {
                performance.is_object()
                    && southstar_js_bind::has_defined_own(scope, &performance, "memory")
            });
        let performance =
            unsafe { imports::nsr_make_performance_object(scope, js, include_memory) };
        drop(scope.set(frame_global, "performance", performance));
    });
}

pub(crate) fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> *mut NsNode {
    if let Some(root) = super::wrapper::unwrap(this).filter(|root| root.kind() == Kind::Document) {
        return root.as_mut_ptr();
    }
    record(page_of(scope)).map_or(ptr::null_mut(), |js| js.current_doc)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode {
    record(js).map_or(ptr::null(), |js| js.ce_main_doc)
}

pub(crate) fn node_wrapper(scope: &mut Scope<'_>, n: *const NsNode) -> Option<Value> {
    let wrapper = node(n).map(Node::js_wrapper).filter(|w| !w.is_null())?;
    Some(scope.object_from_identity(wrapper))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_unpinned(js: *const NsJs, n: *const NsNode) -> GBoolean {
    let unpinned = record(js).is_some_and(|js| {
        !js.pinned_wrappers_set.is_null()
            && unsafe { glib::g_hash_table_contains(js.pinned_wrappers_set, n.cast()) } == 0
    });
    glib::boolean(unpinned)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_wrapper_pinned(js: *const NsJs, n: *const NsNode) -> GBoolean {
    let pinned = record(js).is_some_and(|js| {
        node(n).is_some_and(Node::has_js_wrapper)
            && !js.pinned_wrappers_set.is_null()
            && unsafe { glib::g_hash_table_contains(js.pinned_wrappers_set, n.cast()) } != 0
    });
    glib::boolean(pinned)
}

pub(crate) fn new_orphan_element(scope: &mut Scope<'_>, name: &[u8]) -> Result<Value, Value> {
    let js = page_of(scope);
    let name = cstring(name);
    let n = unsafe { ns_node_new_element(glib::g_strdup(name.as_ptr())) };
    let Some(element_node) = node(n) else {
        return Ok(Value::undefined());
    };
    unsafe {
        super::wrapper::ns_node_arm_js_invalidate(n);
        ns_js_track_orphan(js, n);
    }
    let element = super::wrapper::make(scope, element_node)?;
    tag_caller_document(scope, &element);
    Ok(element)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_attr_changed(
    js: *mut NsJs,
    n: *mut NsNode,
    attr_name: *const c_char,
    old_value: *const c_char,
    new_value: *const c_char,
) {
    let live = |js: *mut NsJs| record(js).is_some_and(|js| !js.ctx.is_null() && js.halted == 0);
    let attr_bytes = unsafe { glib::bytes(attr_name) };
    if !n.is_null() && attr_bytes.is_some() && live(js) {
        unsafe {
            if n.cast_const() == ns_js_focused_node(js)
                && attr_bytes.is_some_and(|a| a.eq_ignore_ascii_case(b"type"))
            {
                ns_js_update_focus_visible(js);
            }
            ns_popover_attr_changed(js, n, attr_name, old_value, new_value);
        }
    }
    let is_plain_script = node(n).is_some_and(|el| {
        is_named(el, c"script") && el.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0
    });
    if !js.is_null()
        && attr_bytes.is_some_and(|a| a.eq_ignore_ascii_case(b"src"))
        && old_value.is_null()
        && !new_value.is_null()
        && is_plain_script
    {
        unsafe { ns_js_script_needs_prepare(js, n) };
        return;
    }
    if n.is_null() || attr_bytes.is_none() || !live(js) {
        return;
    }
    unsafe { ns_ce_attribute_changed(js, n, attr_name, old_value, new_value) };
}

pub(crate) fn illegal_constructor(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    Err(scope.type_error("Illegal constructor"))
}

pub(crate) fn register_tag_proto(scope: &mut Scope<'_>, tag: &[u8], proto: Value) {
    let js = page_of(scope);
    if js.is_null() {
        return;
    }
    let previous = unsafe { (*rec(js)).per_tag_protos.insert(tag.to_vec(), proto) };
    drop(previous);
}

pub(crate) fn adopt_dom_protos(scope: &mut Scope<'_>, protos: AdoptedProtos) {
    let js = page_of(scope);
    if js.is_null() {
        return;
    }
    let AdoptedProtos {
        node: _,
        element,
        html_element,
        html_unknown_element,
        svg_element,
        svga_element,
        mathml_element,
        chardata: _,
        text,
        comment,
        cdata,
        pi,
        doctype,
        docfrag,
        document,
    } = protos;
    let adopted = DomProtos {
        element,
        html_element,
        html_unknown_element,
        svg_element,
        svga_element,
        mathml_element,
        text,
        comment,
        cdata,
        pi,
        doctype,
        docfrag,
    };
    let previous = unsafe { (*rec(js)).protos.replace(adopted) };
    let previous_document = unsafe { core::mem::replace(&mut (*rec(js)).proto_document, document) };
    drop((previous, previous_document));
    let set = unsafe { (*rec(js)).pinned_wrappers_set };
    if set.is_null() {
        return;
    }
    let keys: Vec<*mut c_void> = unsafe { glib::hash_table_entries(set) }
        .map(|(k, _)| k)
        .collect();
    for key in keys {
        let Some(n) = node(key.cast()) else {
            continue;
        };
        let wrapper = n.js_wrapper();
        if wrapper.is_null() {
            continue;
        }
        let Some(proto) = kind_proto(unsafe { &*rec(js) }, n)
            .filter(|proto| proto.is_object())
            .cloned()
        else {
            continue;
        };
        let object = scope.object_from_identity(wrapper);
        drop(scope.set_prototype(&object, &proto));
    }
}

pub(crate) fn tag_caller_document(scope: &mut Scope<'_>, node_val: &Value) {
    let Some(record) = record(page_of(scope)) else {
        return;
    };
    if !node_val.is_object() {
        return;
    }
    let rctx = scope.caller_handle();
    let main_ctx = main_context(record);
    let doc = rctx
        .with(|scope| {
            let global = scope.global();
            scope
                .get(&global, "document")
                .unwrap_or_else(|_| Value::undefined())
        })
        .unwrap_or_else(Value::undefined);
    if rctx != main_ctx && doc.is_object() {
        let hidden = Attributes {
            writable: false,
            enumerable: false,
            configurable: false,
        };
        drop(scope.define(node_val, "__ndOwnerDoc", doc, hidden));
    } else {
        imports::nsr_tag_owner_document(scope, &doc, node_val);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_free_frame_contexts(js: *mut NsJs) {
    let frames = unsafe { (*rec(js)).frame_ctxs.as_mut() };
    if let Some(frames) = frames {
        for ctx in frames.drain(..) {
            unsafe { ctx.free() };
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_context_count(js: *const NsJs) -> c_uint {
    record(js)
        .and_then(|js| js.frame_ctxs.as_ref())
        .map_or(0, |frames| frames.len() as c_uint)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_eval_polyfills(js: *mut NsJs) {
    unsafe {
        ns_js_eval(
            js,
            POLYFILLS.as_ptr().cast(),
            POLYFILLS.len() - 1,
            c"<polyfills>".as_ptr(),
        );
        ns_js_eval(
            js,
            STREAMING.as_ptr().cast(),
            STREAMING.len() - 1,
            c"<streaming>".as_ptr(),
        );
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_snapshot_platform_globals(js: *mut NsJs) {
    unsafe {
        free_platform_globals(js);
        (*rec(js)).platform_globals = super::exports::ns_js_snapshot_globals(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_apply_site_quirks(js: *mut NsJs) {
    let Some(record) = record(js) else {
        return;
    };
    let url = unsafe { glib::bytes(record.current_url) };
    let relevant = url.is_some_and(|url| {
        let contains = |needle: &[u8]| url.windows(needle.len()).any(|w| w == needle);
        contains(b"freecivweb.com") || contains(b"fcw.movingborders.es")
    });
    if record.halted != 0 || !relevant || record.ctx.is_null() {
        return;
    }
    let options = ScriptOptions {
        name: Some(b"site-quirks"),
        line: 1,
        ..ScriptOptions::default()
    };
    record
        .ctx
        .with(|scope| drop(scope.eval_script_bytes(SITE_QUIRKS, &options)));
}

pub(crate) fn exception_message(scope: &mut Scope<'_>, ex: &Value) -> Vec<u8> {
    scope
        .to_bytes(ex)
        .unwrap_or_else(|_| b"Script error.".to_vec())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_add_csp_header(js: *mut NsJs, header_value: *const c_char) {
    if js.is_null() || unsafe { glib::bytes(header_value) }.is_none_or(<[u8]>::is_empty) {
        return;
    }
    let parsed = unsafe { ns_csp_parse(header_value) };
    if parsed.is_null() {
        return;
    }
    unsafe {
        if (*rec(js)).csp.is_null() {
            (*rec(js)).csp = parsed;
        } else {
            ns_csp_merge((*rec(js)).csp, parsed);
            ns_csp_free(parsed);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_csp_form_action_allowed(
    js: *const NsJs,
    action_url: *const c_char,
) -> GBoolean {
    let Some(js) = record(js) else {
        return glib::TRUE;
    };
    if js.csp.is_null() || action_url.is_null() {
        return glib::TRUE;
    }
    unsafe { ns_csp_allows(js.csp, CSP_FORM_ACTION, action_url, js.current_url) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_storage_partition(js: *const NsJs) -> *const c_char {
    record(js).map_or(ptr::null(), |js| opt_ptr(&js.partition_key))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_inline_handlers_allowed(js: *const NsJs) -> GBoolean {
    let csp = record(js).map_or(ptr::null_mut(), |js| js.csp);
    unsafe { ns_csp_inline_event_handler_allowed(csp) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_us(js: *const NsJs) -> i64 {
    record(js).map_or(0, |js| js.time_origin_us)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_real_ms(js: *const NsJs) -> f64 {
    record(js).map_or(0.0, |js| js.time_origin_real_ms)
}

pub(crate) unsafe fn navigator_brand_ref<'a>(js: *const NsJs) -> Option<&'a Value> {
    record(js).map(|js| &js.navigator_brand)
}

pub(crate) unsafe fn pristine_promise_ref<'a>(js: *const NsJs) -> Option<&'a Value> {
    record(js).map(|js| &js.pristine_promise)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_layout_root(js: *const NsJs) -> *const c_void {
    record(js).map_or(ptr::null(), |js| js.layout_root)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_cookie_value(js: *const NsJs) -> *const c_char {
    record(js).map_or(ptr::null(), |js| opt_ptr(&js.cookie_value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_cookie_value(js: *mut NsJs, value: *const c_char) {
    if js.is_null() {
        return;
    }
    let value = unsafe { glib::bytes(value) }.map(cstring);
    unsafe { (*rec(js)).cookie_value = value };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_partition_key(js: *const NsJs) -> *const c_char {
    record(js).map_or(ptr::null(), |js| opt_ptr(&js.partition_key))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_navigation_timing(js: *const NsJs) -> *const NavigationTiming {
    record(js).map_or(ptr::null(), |js| &raw const js.navigation_timing)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_csp(js: *const NsJs) -> *const NsCsp {
    record(js).map_or(ptr::null(), |js| js.csp.cast_const())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_csp_allows_connect(
    js: *const NsJs,
    url: *const c_char,
    page: *const c_char,
) -> GBoolean {
    let Some(js) = record(js) else {
        return glib::TRUE;
    };
    glib::boolean(js.csp.is_null() || unsafe { ns_csp_allows(js.csp, CSP_CONNECT, url, page) } != 0)
}

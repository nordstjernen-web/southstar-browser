//! Southstar — the C ABI of page installation as declared in src/js.h, and the js.c accessors and binding installers it calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GPtrArray};
use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::{Element, run, stats};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct GString {
    _private: [u8; 0],
}

#[repr(C)]
struct NsResponse {
    _private: [u8; 0],
}

#[repr(C)]
struct ResourceTiming {
    top_url: *mut c_char,
    url: *mut c_char,
    initiator: *const c_char,
    start_us: i64,
    end_us: i64,
    resp: *mut NsResponse,
    render_blocking: GBoolean,
    in_frame: GBoolean,
}

#[repr(C)]
struct PerfInfo {
    timeline: *const c_void,
    document_url: *const c_char,
    render_blocking: GBoolean,
    cors_mode: GBoolean,
    next_hop_protocol: *const c_char,
    timing_allow_origin: *const c_char,
    status: c_long,
    body_size: i64,
}

#[repr(C)]
struct ScriptTask {
    node: *mut NsNode,
    schedule: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<ResourceTiming>() == 56);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<PerfInfo>() == 56);

const DLOG_JS: c_uint = 5;

#[derive(Clone, Copy)]
pub(crate) enum ProtoTable {
    DocumentMethods,
    DocumentAccessors,
    NamedNodeMap,
    Src,
    Image,
    Source,
    Media,
    Video,
    Iframe,
    Anchor,
    Href,
    DomImplementation,
    Validity,
}

#[derive(Clone, Copy)]
pub(crate) enum HostProto {
    Style,
    Dataset,
    TokenList,
}

#[derive(Clone, Copy)]
pub(crate) enum Native {
    IllegalConstructor,
    EventConstructor,
    FontFaceConstructor,
    XmlSerializerConstructor,
    Range,
    LiveLength,
    DocumentOpen,
    DocumentClose,
    DocumentWrite,
    DocumentWriteln,
}

impl Native {
    fn function(self) -> JSCFunction {
        match self {
            Native::IllegalConstructor => ns_illegal_constructor,
            Native::EventConstructor => ns_window_event_ctor,
            Native::FontFaceConstructor => ns_window_fontface_ctor,
            Native::XmlSerializerConstructor => ns_xml_serializer_ctor,
            Native::Range => ns_native_range,
            Native::LiveLength => ns_live_length_get,
            Native::DocumentOpen => ns_document_open,
            Native::DocumentClose => ns_document_close,
            Native::DocumentWrite => ns_document_write,
            Native::DocumentWriteln => ns_document_writeln,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Installer {
    DomHierarchy,
    HasInstance,
    ToStringTag,
    Compression,
    IndexedDb,
    NetInterfaces,
    PerfEntryList,
    LiveConstructors,
    EventAttributeGetters,
}

#[derive(Clone, Copy)]
pub(crate) enum Shaper {
    WebApiShapes,
    NavigatorShape,
    PdfPlugins,
    LinkInterfaces,
    NameEngineMembers,
    ShapeWindowGlobal,
    LinkInterfaceConstructors,
    LockGlobalPrototypes,
    NodeShapes,
    ElementShapes,
    BrandNodeClasses,
}

#[derive(Clone, Copy)]
pub(crate) enum ValueInstaller {
    DoctypeGetter,
    DocumentFunctions,
    LegacyNamed,
    TreeWalkerProto,
    EventHandlerAccessors,
    LiftDocumentMethods,
}

unsafe extern "C" {
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_set_current_url(js: *mut NsJs, url: *const c_char);
    fn ns_js_set_document(js: *mut NsJs, doc: *mut NsNode, origin: *mut c_char);
    fn ns_js_new_document_object(js: *mut NsJs) -> JSValue;
    fn ns_js_install_proto_table(ctx: *mut JSContext, proto: JSValue, table: c_int);
    fn ns_js_host_class_proto(ctx: *mut JSContext, which: c_int) -> JSValue;
    fn ns_js_brand_node_classes(ctx: *mut JSContext);
    fn ns_js_release_pinned_wrappers(js: *mut NsJs);
    fn ns_js_free_frame_contexts(js: *mut NsJs);
    fn ns_js_frame_context_count(js: *const NsJs) -> c_uint;
    fn ns_js_eval_polyfills(js: *mut NsJs);
    fn ns_js_link_interfaces(ctx: *mut JSContext);
    fn ns_js_snapshot_platform_globals(js: *mut NsJs);
    fn ns_js_clear_doc_ready_states(js: *mut NsJs);
    fn ns_js_set_ready_state(js: *mut NsJs, state: c_int);
    fn ns_js_early_inject_src(js: *const NsJs) -> *const c_char;
    fn ns_js_mark_navigation(js: *mut NsJs, legacy_key: *const c_char);
    fn ns_js_drop_message_tasks(js: *mut NsJs);
    fn ns_storage_load_for(js: *mut NsJs, new_url: *const c_char);
    fn ns_drain_microtasks(js: *mut NsJs);
    fn ns_js_profile_enabled() -> GBoolean;
    fn ns_js_eval_source(js: *mut NsJs, src: *const c_char, origin: *const c_char) -> *mut c_char;

    fn ns_top_layer_clear(js: *mut NsJs);
    fn ns_focus_reset(js: *mut NsJs);
    fn ns_js_forget_pending_change(js: *mut NsJs);
    fn ns_storage_free_deferred_events(js: *mut NsJs);
    fn ns_services_reset(js: *mut NsJs);
    fn ns_lifecycle_reset(js: *mut NsJs);
    fn ns_dispatch_reset(js: *mut NsJs);
    fn ns_js_net_reset(js: *mut NsJs);
    fn ns_ce_reset(js: *mut NsJs);
    fn ns_css_clear_defined_elements();
    fn ns_css_clear_registered_properties();
    fn ns_js_cancel_async_scripts(js: *mut NsJs);
    fn ns_attr_detach_all(js: *mut NsJs);
    fn ns_attribute_maps_release_all(js: *mut NsJs);
    fn ns_observers_reset(js: *mut NsJs);
    fn ns_perf_reset_observers(js: *mut NsJs);
    fn ns_js_orphans_take_all(js: *mut NsJs) -> *mut GPtrArray;
    fn ns_node_free(node: *mut NsNode);
    fn ns_js_frames_reset(js: *mut NsJs);
    fn ns_window_links_clear(js: *mut NsJs, destroy: GBoolean);
    fn ns_js_clear_frame_clocks(js: *mut NsJs, destroy: GBoolean);
    fn ns_js_drop_pending_rejections(js: *mut NsJs);

    fn ns_dom_set_active_modal(modal: *const NsNode);
    fn ns_doc_id_index_build(doc: *mut NsNode);
    fn ns_doc_class_index_build(doc: *mut NsNode);
    fn ns_doc_tag_index_build(doc: *mut NsNode);
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_url_host_from(url: *const c_char) -> *mut c_char;
    fn ns_debug_log_emit_take(level: c_uint, category: *const c_char, message: *mut c_char);
    fn ns_js_seed_cookies_from_jar(js: *mut NsJs);

    fn ns_document_define_doctype_getter(ctx: *mut JSContext, doc: JSValue);
    fn ns_document_install_funcs(ctx: *mut JSContext, doc: JSValue);
    fn ns_document_expose_legacy_named(ctx: *mut JSContext, root: *const NsNode, document: JSValue);
    fn ns_document_lift_methods_to_proto(ctx: *mut JSContext, document: JSValue);
    fn ns_window_make_location(ctx: *mut JSContext) -> JSValue;
    fn ns_tree_walker_install_proto(ctx: *mut JSContext, proto: JSValue);
    fn ns_install_event_handler_accessors(ctx: *mut JSContext, proto: JSValue);

    fn ns_install_dom_hierarchy(ctx: *mut JSContext, global: JSValue);
    fn ns_install_hasinstance(ctx: *mut JSContext, global: JSValue);
    fn ns_install_tostringtag(ctx: *mut JSContext, global: JSValue);
    fn ns_compression_install(ctx: *mut JSContext, global: JSValue);
    fn ns_idb_install(ctx: *mut JSContext, global: JSValue);
    fn ns_net_install_interfaces(ctx: *mut JSContext, global: JSValue);
    fn ns_perf_install_entry_list(ctx: *mut JSContext, global: JSValue);
    fn ns_live_wire_constructors(ctx: *mut JSContext, global: JSValue);
    fn ns_install_event_attribute_getters(ctx: *mut JSContext, global: JSValue);

    fn ns_install_web_api_shapes(ctx: *mut JSContext);
    fn ns_install_navigator_shape(ctx: *mut JSContext);
    fn ns_install_pdf_plugins(ctx: *mut JSContext);
    fn ns_js_name_engine_members(ctx: *mut JSContext);
    fn ns_js_shape_window_global(ctx: *mut JSContext);
    fn ns_js_link_interface_ctors(ctx: *mut JSContext);
    fn ns_js_lock_global_prototypes(ctx: *mut JSContext);
    fn ns_install_node_shapes(ctx: *mut JSContext);
    fn ns_install_element_shapes(ctx: *mut JSContext);

    fn ns_illegal_constructor(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_event_ctor(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_fontface_ctor(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_xml_serializer_ctor(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_native_range(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_live_length_get(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_document_open(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_document_close(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_document_write(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_document_writeln(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;

    fn ns_js_lifecycle_clear(js: *mut NsJs);
    fn ns_js_frames_clear_initial_blank(js: *mut NsJs);
    fn ns_js_run_content_scripts(js: *mut NsJs, url: *const c_char, document_start: GBoolean);
    fn ns_js_schedule_static_iframes(js: *mut NsJs, n: *mut NsNode);
    fn ns_engine_take_resource_timings(top_url: *const c_char) -> *mut GPtrArray;
    fn ns_js_element_perf_info(js: *mut NsJs, el: *const NsNode, info: *mut PerfInfo);
    fn ns_perf_add_resource_timed(
        js: *mut NsJs,
        info: *const PerfInfo,
        url: *const c_char,
        initiator: *const c_char,
        start_us: i64,
        end_us: i64,
        resp: *mut NsResponse,
    );
    fn ns_js_register_import_maps(js: *mut NsJs, root: *mut NsNode);
    fn ns_js_collect_script_tasks(n: *mut NsNode, tasks: *mut GArray);
    fn ns_js_run_parser_blocking_scripts(js: *mut NsJs, tasks: *mut GArray, origin: *const c_char);
    fn ns_js_lifecycle_start(
        js: *mut NsJs,
        doc: *mut NsNode,
        tasks: *mut GArray,
        origin: *const c_char,
        start_us: i64,
    );
    fn ns_js_image_load_tracked(js: *mut NsJs, el: *const NsNode) -> GBoolean;
    fn ns_js_start_image_load(js: *mut NsJs, el: *mut NsNode, src: *const c_char);
    fn ns_node_is_embedded_doc(n: *const NsNode) -> GBoolean;
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;

    fn ns_services_timer_count(js: *const NsJs) -> c_uint;
    fn ns_js_raf_count(js: *const NsJs) -> c_uint;
    fn ns_js_net_pending_fetches(js: *const NsJs) -> c_uint;
    fn ns_js_net_pending_xhrs(js: *const NsJs) -> c_uint;
    fn ns_js_net_pending_sockets(js: *const NsJs) -> c_uint;
    fn ns_dispatch_listener_count(js: *const NsJs) -> c_uint;
    fn g_string_append_len(s: *mut GString, val: *const c_char, len: isize) -> *mut GString;
    fn g_get_monotonic_time() -> i64;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    fn ctx(self) -> *mut JSContext {
        unsafe { ns_js_main_context(self.0) }
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        let ctx = self.ctx();
        if ctx.is_null() {
            return None;
        }
        Some(unsafe { quickjs::with_context(ctx, f) })
    }

    pub fn halted(self) -> bool {
        unsafe { ns_js_halted(self.0) != 0 }
    }

    pub fn in_pump(self) -> bool {
        unsafe { ns_js_in_pump(self.0) != 0 }
    }

    pub fn current_url(self) -> *const c_char {
        unsafe { ns_js_current_url(self.0) }
    }

    pub fn current_url_text(self) -> &'static CStr {
        let url = self.current_url();
        if url.is_null() {
            c""
        } else {
            unsafe { CStr::from_ptr(url) }
        }
    }

    pub fn current_document(self) -> *const NsNode {
        unsafe { ns_js_current_document(self.0) }
    }

    pub fn set_document(self, doc: Option<Element>, url: &CStr) {
        let origin = if url.is_empty() {
            ptr::null_mut()
        } else {
            unsafe { ns_url_origin_from(url.as_ptr()) }
        };
        unsafe {
            ns_js_set_document(self.0, Node::ptr_or_null(doc).cast_mut(), origin);
            ns_dom_set_active_modal(ptr::null());
            ns_js_set_current_url(self.0, url.as_ptr());
        }
    }

    pub fn load_storage_and_cookies(self) {
        unsafe {
            ns_storage_load_for(self.0, self.current_url());
            ns_js_seed_cookies_from_jar(self.0);
        }
    }

    pub fn reset_page_state(self) {
        unsafe {
            ns_top_layer_clear(self.0);
            ns_focus_reset(self.0);
            ns_js_forget_pending_change(self.0);
            ns_storage_free_deferred_events(self.0);
            ns_services_reset(self.0);
            ns_lifecycle_reset(self.0);
            ns_dispatch_reset(self.0);
            ns_js_net_reset(self.0);
            ns_ce_reset(self.0);
            ns_css_clear_defined_elements();
            ns_css_clear_registered_properties();
            ns_js_cancel_async_scripts(self.0);
            ns_attr_detach_all(self.0);
            ns_attribute_maps_release_all(self.0);
            ns_js_release_pinned_wrappers(self.0);
            ns_observers_reset(self.0);
            ns_perf_reset_observers(self.0);
        }
    }

    pub fn free_orphans(self) {
        let orphans = unsafe { ns_js_orphans_take_all(self.0) };
        if orphans.is_null() {
            return;
        }
        let len = unsafe { (*orphans).len } as usize;
        for i in 0..len {
            unsafe { ns_node_free((*(*orphans).pdata.add(i)).cast()) };
        }
        unsafe { glib::g_ptr_array_free(orphans, glib::TRUE) };
    }

    pub fn reset_frames_and_tasks(self) {
        unsafe {
            ns_js_frames_reset(self.0);
            ns_window_links_clear(self.0, glib::FALSE);
            ns_js_drop_message_tasks(self.0);
            ns_js_clear_frame_clocks(self.0, glib::FALSE);
            ns_js_drop_pending_rejections(self.0);
            ns_js_free_frame_contexts(self.0);
        }
    }

    pub fn drain_microtasks(self) {
        unsafe { ns_drain_microtasks(self.0) };
    }

    pub fn eval_polyfills(self) {
        unsafe { ns_js_eval_polyfills(self.0) };
    }

    pub fn eval_source(self, source: &CStr, origin: &CStr) {
        let result = unsafe { ns_js_eval_source(self.0, source.as_ptr(), origin.as_ptr()) };
        unsafe { glib::g_free(result.cast()) };
    }

    pub fn early_inject_src(self) -> Option<&'static CStr> {
        let src = unsafe { ns_js_early_inject_src(self.0) };
        (!src.is_null()).then(|| unsafe { CStr::from_ptr(src) })
    }

    pub fn new_document_object(self, scope: &mut Scope<'_>) -> Value {
        unsafe { quickjs::take_value(scope, ns_js_new_document_object(self.0)) }
    }

    pub fn snapshot_platform_globals(self) {
        unsafe { ns_js_snapshot_platform_globals(self.0) };
    }

    pub fn clear_lifecycle(self) {
        unsafe {
            ns_js_lifecycle_clear(self.0);
            ns_js_set_ready_state(self.0, 0);
            ns_js_clear_doc_ready_states(self.0);
            ns_js_frames_clear_initial_blank(self.0);
            ns_js_mark_navigation(self.0, c"domLoading".as_ptr());
        }
    }

    pub fn run_content_scripts(self, url: Option<&CStr>) {
        unsafe { ns_js_run_content_scripts(self.0, c_ptr(url), glib::TRUE) };
    }

    pub fn schedule_static_iframes(self, doc: Element) {
        unsafe { ns_js_schedule_static_iframes(self.0, doc.as_mut_ptr()) };
    }

    pub fn record_stylesheet_timings(self, doc: Element, base_url: Option<&CStr>) {
        let sheets = unsafe { ns_engine_take_resource_timings(c_ptr(base_url)) };
        if sheets.is_null() {
            return;
        }
        let len = unsafe { (*sheets).len } as usize;
        for i in 0..len {
            let timing = unsafe { &*(*(*sheets).pdata.add(i)).cast::<ResourceTiming>() };
            if timing.in_frame != 0 {
                continue;
            }
            let mut info = PerfInfo {
                timeline: ptr::null(),
                document_url: ptr::null(),
                render_blocking: glib::FALSE,
                cors_mode: glib::FALSE,
                next_hop_protocol: ptr::null(),
                timing_allow_origin: ptr::null(),
                status: 0,
                body_size: 0,
            };
            unsafe { ns_js_element_perf_info(self.0, doc.as_ptr(), &mut info) };
            info.render_blocking = timing.render_blocking;
            unsafe {
                ns_perf_add_resource_timed(
                    self.0,
                    &info,
                    timing.url,
                    timing.initiator,
                    timing.start_us,
                    timing.end_us,
                    timing.resp,
                )
            };
        }
        unsafe { glib::g_ptr_array_free(sheets, glib::TRUE) };
    }

    pub fn run_document_scripts(self, doc: Element, origin: &CStr, start_us: i64) {
        let tasks = unsafe {
            glib::g_array_new(
                glib::FALSE,
                glib::FALSE,
                core::mem::size_of::<ScriptTask>() as c_uint,
            )
        };
        unsafe {
            ns_js_register_import_maps(self.0, doc.as_mut_ptr());
            ns_js_collect_script_tasks(doc.as_mut_ptr(), tasks);
            ns_js_run_parser_blocking_scripts(self.0, tasks, origin.as_ptr());
            ns_js_lifecycle_start(self.0, doc.as_mut_ptr(), tasks, origin.as_ptr(), start_us);
        }
    }

    pub fn image_load_tracked(self, el: Element) -> bool {
        unsafe { ns_js_image_load_tracked(self.0, el.as_ptr()) != 0 }
    }

    pub fn start_image_load(self, el: Element, src: &CStr) {
        unsafe { ns_js_start_image_load(self.0, el.as_mut_ptr(), src.as_ptr()) };
    }

    pub fn frame_context_count(self) -> u32 {
        unsafe { ns_js_frame_context_count(self.0) }
    }

    pub fn event_loop_counts(self) -> [u32; 6] {
        unsafe {
            [
                ns_services_timer_count(self.0),
                ns_js_raf_count(self.0),
                ns_js_net_pending_fetches(self.0),
                ns_js_net_pending_xhrs(self.0),
                ns_js_net_pending_sockets(self.0),
                ns_dispatch_listener_count(self.0),
            ]
        }
    }
}

fn c_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(ptr::null(), CStr::as_ptr)
}

pub(crate) fn build_document_indexes(doc: Element) {
    unsafe {
        ns_doc_id_index_build(doc.as_mut_ptr());
        ns_doc_class_index_build(doc.as_mut_ptr());
        ns_doc_tag_index_build(doc.as_mut_ptr());
    }
}

pub(crate) fn debug_js(category: &CStr, message: &[u8]) {
    unsafe { ns_debug_log_emit_take(DLOG_JS, category.as_ptr(), glib::strdup(message)) };
}

pub(crate) fn profile_enabled() -> bool {
    unsafe { ns_js_profile_enabled() != 0 }
}

pub(crate) fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn url_host(url: &CStr) -> Option<Vec<u8>> {
    take_string(unsafe { ns_url_host_from(url.as_ptr()) })
}

pub(crate) fn url_origin(url: &CStr) -> Option<Vec<u8>> {
    take_string(unsafe { ns_url_origin_from(url.as_ptr()) })
}

fn take_string(raw: *mut c_char) -> Option<Vec<u8>> {
    if raw.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec();
    unsafe { glib::g_free(raw.cast()) };
    Some(bytes)
}

pub(crate) fn hidden_child(node: Element) -> bool {
    unsafe {
        ns_node_is_embedded_doc(node.as_ptr()) != 0 || ns_node_is_shadow_root(node.as_ptr()) != 0
    }
}

pub(crate) fn native(scope: &mut Scope<'_>, name: &str, arity: u32, native: Native) -> Value {
    quickjs::c_function(scope, name, arity, native.function())
}

pub(crate) fn native_constructor(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    native: Native,
) -> Value {
    quickjs::c_constructor_or_function(scope, name, arity, native.function())
}

pub(crate) fn install_proto_table(scope: &mut Scope<'_>, proto: &Value, table: ProtoTable) {
    unsafe {
        ns_js_install_proto_table(
            quickjs::raw_context(scope),
            quickjs::raw(proto),
            table as c_int,
        )
    };
}

pub(crate) fn host_class_proto(scope: &mut Scope<'_>, which: HostProto) -> Value {
    let raw = unsafe { ns_js_host_class_proto(quickjs::raw_context(scope), which as c_int) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn make_location(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_window_make_location(quickjs::raw_context(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn install_on(scope: &mut Scope<'_>, installer: Installer, global: &Value) {
    let ctx = quickjs::raw_context(scope);
    let global = quickjs::raw(global);
    unsafe {
        match installer {
            Installer::DomHierarchy => ns_install_dom_hierarchy(ctx, global),
            Installer::HasInstance => ns_install_hasinstance(ctx, global),
            Installer::ToStringTag => ns_install_tostringtag(ctx, global),
            Installer::Compression => ns_compression_install(ctx, global),
            Installer::IndexedDb => ns_idb_install(ctx, global),
            Installer::NetInterfaces => ns_net_install_interfaces(ctx, global),
            Installer::PerfEntryList => ns_perf_install_entry_list(ctx, global),
            Installer::LiveConstructors => ns_live_wire_constructors(ctx, global),
            Installer::EventAttributeGetters => ns_install_event_attribute_getters(ctx, global),
        }
    }
}

pub(crate) fn install_value(
    scope: &mut Scope<'_>,
    installer: ValueInstaller,
    value: &Value,
    root: *const NsNode,
) {
    let ctx = quickjs::raw_context(scope);
    let value = quickjs::raw(value);
    unsafe {
        match installer {
            ValueInstaller::DoctypeGetter => ns_document_define_doctype_getter(ctx, value),
            ValueInstaller::DocumentFunctions => ns_document_install_funcs(ctx, value),
            ValueInstaller::LegacyNamed => ns_document_expose_legacy_named(ctx, root, value),
            ValueInstaller::TreeWalkerProto => ns_tree_walker_install_proto(ctx, value),
            ValueInstaller::EventHandlerAccessors => ns_install_event_handler_accessors(ctx, value),
            ValueInstaller::LiftDocumentMethods => ns_document_lift_methods_to_proto(ctx, value),
        }
    }
}

pub(crate) fn shape(scope: &mut Scope<'_>, shaper: Shaper) {
    let ctx = quickjs::raw_context(scope);
    unsafe {
        match shaper {
            Shaper::WebApiShapes => ns_install_web_api_shapes(ctx),
            Shaper::NavigatorShape => ns_install_navigator_shape(ctx),
            Shaper::PdfPlugins => ns_install_pdf_plugins(ctx),
            Shaper::LinkInterfaces => ns_js_link_interfaces(ctx),
            Shaper::NameEngineMembers => ns_js_name_engine_members(ctx),
            Shaper::ShapeWindowGlobal => ns_js_shape_window_global(ctx),
            Shaper::LinkInterfaceConstructors => ns_js_link_interface_ctors(ctx),
            Shaper::LockGlobalPrototypes => ns_js_lock_global_prototypes(ctx),
            Shaper::NodeShapes => ns_install_node_shapes(ctx),
            Shaper::ElementShapes => ns_install_element_shapes(ctx),
            Shaper::BrandNodeClasses => ns_js_brand_node_classes(ctx),
        }
    }
}

pub(crate) fn memory_usage(scope: &Scope<'_>) -> quickjs::MemoryUsage {
    quickjs::memory_usage(scope)
}

unsafe fn c_text<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_run_scripts_in_doc(
    js: *mut NsJs,
    doc: *mut NsNode,
    base_url: *const c_char,
) {
    let Some(doc) = (unsafe { Node::from_ptr(doc) }) else {
        return;
    };
    if js.is_null() {
        return;
    }
    let base_url = unsafe { c_text(base_url) }.map(CStr::to_owned);
    run::run_scripts_in_doc(Js(js), doc, base_url.as_deref());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dump_stats(js: *mut NsJs, out: *mut GString) {
    if js.is_null() || out.is_null() {
        return;
    }
    let text = stats::dump(Js(js));
    unsafe { g_string_append_len(out, text.as_ptr().cast(), text.len() as isize) };
}

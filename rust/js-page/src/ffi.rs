//! Southstar — the C ABI of page installation, and the realm accessors and binding installers it calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GPtrArray};
use southstar_js_engine::{ContextHandle, MemoryUsage, NativeFn, Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, run, stats};

pub(crate) use southstar_js_runtime::api::NsJs;

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
    fn function(self) -> NativeFn {
        match self {
            Native::IllegalConstructor => api::illegal_constructor,
            Native::EventConstructor => nsr_window_event_ctor,
            Native::FontFaceConstructor => nsr_window_fontface_ctor,
            Native::XmlSerializerConstructor => nsr_xml_serializer_ctor,
            Native::Range => nsr_native_range,
            Native::LiveLength => nsr_live_length_get,
            Native::DocumentOpen => nsr_document_open,
            Native::DocumentClose => nsr_document_close,
            Native::DocumentWrite => nsr_document_write,
            Native::DocumentWriteln => nsr_document_writeln,
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
    fn ns_js_brand_node_classes(ctx: ContextHandle);
    fn ns_js_link_interfaces(ctx: ContextHandle);
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
    fn ns_js_cancel_async_scripts(js: *mut NsJs);
    fn ns_attr_detach_all(js: *mut NsJs);
    fn ns_attribute_maps_release_all(js: *mut NsJs);
    fn ns_observers_reset(js: *mut NsJs);
    fn ns_js_orphans_take_all(js: *mut NsJs) -> *mut GPtrArray;
    fn ns_js_frames_reset(js: *mut NsJs);
    fn ns_window_links_clear(js: *mut NsJs, destroy: GBoolean);
    fn ns_js_drop_pending_rejections(js: *mut NsJs);

    fn ns_debug_log_emit_take(level: c_uint, category: *const c_char, message: *mut c_char);
    fn ns_js_seed_cookies_from_jar(js: *mut NsJs);

    fn ns_install_web_api_shapes(ctx: ContextHandle);
    fn ns_install_navigator_shape(ctx: ContextHandle);
    fn ns_install_pdf_plugins(ctx: ContextHandle);
    fn ns_js_name_engine_members(ctx: ContextHandle);
    fn ns_js_shape_window_global(ctx: ContextHandle);
    fn ns_js_link_interface_ctors(ctx: ContextHandle);
    fn ns_js_lock_global_prototypes(ctx: ContextHandle);
    fn ns_install_node_shapes(ctx: ContextHandle);
    fn ns_install_element_shapes(ctx: ContextHandle);

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

unsafe extern "Rust" {
    safe fn nsr_window_event_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_window_fontface_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_xml_serializer_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_native_range(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_live_length_get(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_document_open(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_document_close(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_document_write(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_document_writeln(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_install_dom_hierarchy(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_install_hasinstance(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_install_tostringtag(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_compression_install(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_idb_install(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_net_install_interfaces(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_live_wire_constructors(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_install_event_attribute_getters(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_document_define_doctype_getter(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_document_install_funcs(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_document_lift_methods_to_proto(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_tree_walker_install_proto(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_install_event_handler_accessors(scope: &mut Scope<'_>, target: &Value);
    safe fn nsr_js_install_proto_table(scope: &mut Scope<'_>, proto: &Value, table: c_int);
    safe fn nsr_js_host_class_proto(scope: &mut Scope<'_>, which: c_int) -> Value;
    safe fn nsr_window_make_location(scope: &mut Scope<'_>) -> Value;
    unsafe fn nsr_document_expose_legacy_named(
        scope: &mut Scope<'_>,
        root: *const NsNode,
        document: &Value,
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        api::main_context(self.0).non_null()?.with(f)
    }

    pub fn halted(self) -> bool {
        api::halted(self.0)
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.0)
    }

    pub fn current_url_text(self) -> CString {
        api::current_url(self.0)
    }

    pub fn current_document(self) -> *const NsNode {
        Node::ptr_or_null(api::current_document(self.0))
    }

    pub fn set_document(self, doc: Option<Element>, url: &CStr) {
        let origin = if url.is_empty() {
            None
        } else {
            southstar_url::bytes::origin_from(url.to_bytes()).map(|mut origin| {
                origin.truncate(origin.iter().position(|&b| b == 0).unwrap_or(origin.len()));
                CString::new(origin).unwrap_or_default()
            })
        };
        api::set_document(self.0, doc, origin.as_deref());
        southstar_dom::tree::set_active_modal(None);
        api::set_current_url(self.0, Some(url));
    }

    pub fn load_storage_and_cookies(self) {
        api::storage_load_for(self.0, Some(&api::current_url(self.0)));
        unsafe { ns_js_seed_cookies_from_jar(self.0) };
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
            southstar_css::clear_defined_elements();
            southstar_css::clear_registered_properties();
            ns_js_cancel_async_scripts(self.0);
            ns_attr_detach_all(self.0);
            ns_attribute_maps_release_all(self.0);
            api::release_pinned_wrappers(self.0);
            ns_observers_reset(self.0);
        }
        southstar_js_perf::reset_observers(self.0);
    }

    pub fn free_orphans(self) {
        let orphans = unsafe { ns_js_orphans_take_all(self.0) };
        if orphans.is_null() {
            return;
        }
        let len = unsafe { (*orphans).len } as usize;
        for i in 0..len {
            // SAFETY: the taken array holds `len` orphaned nodes of this thread's store.
            if let Some(node) = unsafe { Node::from_ptr((*(*orphans).pdata.add(i)).cast()) } {
                node.free_tree();
            }
        }
        unsafe { glib::g_ptr_array_free(orphans, glib::TRUE) };
    }

    pub fn reset_frames_and_tasks(self) {
        unsafe {
            ns_js_frames_reset(self.0);
            ns_window_links_clear(self.0, glib::FALSE);
            api::drop_message_tasks(self.0);
            southstar_js_perf::clear_frame_clocks(self.0, false);
            ns_js_drop_pending_rejections(self.0);
        }
        api::free_frame_contexts(self.0);
    }

    pub fn drain_microtasks(self) {
        api::drain_microtasks(self.0);
    }

    pub fn eval_polyfills(self) {
        api::eval_polyfills(self.0);
    }

    pub fn eval_source(self, source: &CStr, origin: &CStr) {
        let result = unsafe { ns_js_eval_source(self.0, source.as_ptr(), origin.as_ptr()) };
        unsafe { glib::g_free(result.cast()) };
    }

    pub fn early_inject_src(self) -> Option<CString> {
        api::early_inject_src(self.0)
    }

    pub fn new_document_object(self, scope: &mut Scope<'_>) -> Value {
        api::new_document_object(self.0, scope).unwrap_or_else(|_| Value::undefined())
    }

    pub fn snapshot_platform_globals(self) {
        api::snapshot_platform_globals(self.0);
    }

    pub fn clear_lifecycle(self) {
        unsafe { ns_js_lifecycle_clear(self.0) };
        api::set_ready_state(self.0, 0);
        api::clear_doc_ready_states(self.0);
        unsafe { ns_js_frames_clear_initial_blank(self.0) };
        southstar_js_perf::mark_navigation(self.0, "domLoading");
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
        southstar_js_images::image_load_tracked(self.0, el)
    }

    pub fn start_image_load(self, el: Element, src: &CStr) {
        southstar_js_images::start_image_load(self.0, el, Some(src));
    }

    pub fn frame_context_count(self) -> u32 {
        api::frame_context_count(self.0)
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
    southstar_dom::index::id_build(doc);
    southstar_dom::index::class_build(doc);
    southstar_dom::index::tag_build(doc);
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
    southstar_url::bytes::host_from(url.to_bytes())
}

pub(crate) fn url_origin(url: &CStr) -> Option<Vec<u8>> {
    southstar_url::bytes::origin_from(url.to_bytes())
}

pub(crate) fn hidden_child(node: Element) -> bool {
    southstar_dom::serialize::is_embedded_doc(node)
        || unsafe { ns_node_is_shadow_root(node.as_ptr()) != 0 }
}

pub(crate) fn native(scope: &mut Scope<'_>, name: &str, arity: u32, native: Native) -> Value {
    scope.function(name, arity, native.function())
}

pub(crate) fn native_constructor(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    native: Native,
) -> Value {
    scope.constructor_or_function(name, arity, native.function())
}

pub(crate) fn install_proto_table(scope: &mut Scope<'_>, proto: &Value, table: ProtoTable) {
    nsr_js_install_proto_table(scope, proto, table as c_int);
}

pub(crate) fn host_class_proto(scope: &mut Scope<'_>, which: HostProto) -> Value {
    nsr_js_host_class_proto(scope, which as c_int)
}

pub(crate) fn make_location(scope: &mut Scope<'_>) -> Value {
    nsr_window_make_location(scope)
}

pub(crate) fn install_on(scope: &mut Scope<'_>, installer: Installer, global: &Value) {
    let install = match installer {
        Installer::DomHierarchy => nsr_install_dom_hierarchy,
        Installer::HasInstance => nsr_install_hasinstance,
        Installer::ToStringTag => nsr_install_tostringtag,
        Installer::Compression => nsr_compression_install,
        Installer::IndexedDb => nsr_idb_install,
        Installer::NetInterfaces => nsr_net_install_interfaces,
        Installer::PerfEntryList => southstar_js_perf::install_entry_list,
        Installer::LiveConstructors => nsr_live_wire_constructors,
        Installer::EventAttributeGetters => nsr_install_event_attribute_getters,
    };
    install(scope, global);
}

pub(crate) fn install_value(
    scope: &mut Scope<'_>,
    installer: ValueInstaller,
    value: &Value,
    root: *const NsNode,
) {
    match installer {
        ValueInstaller::DoctypeGetter => nsr_document_define_doctype_getter(scope, value),
        ValueInstaller::DocumentFunctions => nsr_document_install_funcs(scope, value),
        ValueInstaller::LegacyNamed => unsafe {
            nsr_document_expose_legacy_named(scope, root, value)
        },
        ValueInstaller::TreeWalkerProto => nsr_tree_walker_install_proto(scope, value),
        ValueInstaller::EventHandlerAccessors => nsr_install_event_handler_accessors(scope, value),
        ValueInstaller::LiftDocumentMethods => nsr_document_lift_methods_to_proto(scope, value),
    }
}

pub(crate) fn shape(scope: &mut Scope<'_>, shaper: Shaper) {
    let ctx = scope.handle();
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

pub(crate) fn memory_usage(scope: &mut Scope<'_>) -> MemoryUsage {
    scope.memory_usage()
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

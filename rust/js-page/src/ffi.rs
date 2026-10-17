//! Southstar — the C ABI of page installation, and the realm accessors and binding installers it calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_js_engine::{ContextHandle, MemoryUsage, NativeFn, Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, run, stats};

pub(crate) use southstar_js_runtime::api::NsJs;

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

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<ResourceTiming>() == 56);

const DLOG_JS: c_int = 5;

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
pub(crate) enum ValueInstaller {
    DoctypeGetter,
    DocumentFunctions,
    LegacyNamed,
    TreeWalkerProto,
    EventHandlerAccessors,
    LiftDocumentMethods,
}

unsafe extern "C" {

    fn ns_focus_reset(js: *mut NsJs);
    fn ns_storage_free_deferred_events(js: *mut NsJs);
    fn ns_observers_reset(js: *mut NsJs);

    fn ns_js_name_engine_members(ctx: ContextHandle);

    fn ns_engine_take_resource_timings(top_url: *const c_char) -> *mut GPtrArray;

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
        southstar_js_document::seed_cookies_from_jar(self.0);
    }

    pub fn reset_page_state(self) {
        // SAFETY: a Js handle names the page's live script engine, which these resets accept.
        unsafe {
            southstar_js_top_layer::clear_top_layer(self.0);
            ns_focus_reset(self.0);
        }
        southstar_js_mutation::forget_pending_change(self.0);
        // SAFETY: a Js handle names the page's live script engine, which these resets accept.
        unsafe {
            ns_storage_free_deferred_events(self.0);
            southstar_js_services::reset_page(self.0);
            southstar_js_lifecycle::reset_lifecycle(self.0);
            southstar_js_dispatch::reset(self.0);
            southstar_js_net::reset_page(self.0);
        }
        southstar_js_custom_elements::reset(self.0);
        southstar_css::clear_defined_elements();
        southstar_css::clear_registered_properties();
        southstar_js_loader::cancel_async_scripts(self.0);
        southstar_js_attrs::detach_all(self.0);
        southstar_js_mutation::attribute_maps_release_all(self.0);
        api::release_pinned_wrappers(self.0);
        // SAFETY: a Js handle names the page's live script engine, which these resets accept.
        unsafe { ns_observers_reset(self.0) };
        southstar_js_perf::reset_observers(self.0);
    }

    pub fn free_orphans(self) {
        for node in southstar_js_mutation::orphans_take_all(self.0) {
            node.free_tree();
        }
    }

    pub fn reset_frames_and_tasks(self) {
        southstar_js_frames::reset_frames(self.0);
        southstar_js_window::clear_window_links(self.0);
        api::drop_message_tasks(self.0);
        southstar_js_perf::clear_frame_clocks(self.0, false);
        southstar_js_dispatch::drop_pending_rejections(self.0);
        api::free_frame_contexts(self.0);
    }

    pub fn drain_microtasks(self) {
        api::drain_microtasks(self.0);
    }

    pub fn eval_polyfills(self) {
        api::eval_polyfills(self.0);
    }

    pub fn eval_source(self, source: &CStr, origin: &CStr) {
        let _ = southstar_js_loader::eval_source(self.0, source, Some(origin));
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
        southstar_js_lifecycle::clear_lifecycle(self.0);
        api::set_ready_state(self.0, 0);
        api::clear_doc_ready_states(self.0);
        southstar_js_frames::clear_initial_blank_frames(self.0);
        southstar_js_perf::mark_navigation(self.0, "domLoading");
    }

    pub fn run_content_scripts(self, url: Option<&CStr>) {
        southstar_js_loader::run_content_scripts(self.0, url, true);
    }

    pub fn schedule_static_iframes(self, doc: Element) {
        southstar_js_frames::schedule_static_iframes(self.0, doc);
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
            let mut info = southstar_js_loader::element_perf_info(self.0, Some(doc));
            info.render_blocking = timing.render_blocking;
            // SAFETY: the engine's timing record keeps its URL alive until the array is freed below.
            let Some(url) = (unsafe { c_text(timing.url) }) else {
                continue;
            };
            // SAFETY: the initiator is null or a static string the engine recorded.
            let initiator = unsafe { c_text(timing.initiator) };
            let span = (timing.start_us, timing.end_us);
            // SAFETY: the element's perf strings and the timing's response stay live for the call.
            unsafe {
                southstar_js_perf::add_resource_timed(
                    self.0,
                    Some(&info),
                    url,
                    initiator,
                    span,
                    timing.resp.cast(),
                )
            };
        }
        unsafe { glib::g_ptr_array_free(sheets, glib::TRUE) };
    }

    pub fn run_document_scripts(self, doc: Element, origin: &CStr, start_us: i64) {
        southstar_js_loader::register_import_maps(self.0, doc);
        let tasks = southstar_js_loader::collect_script_tasks(doc);
        southstar_js_loader::run_parser_blocking_scripts(self.0, &tasks, Some(origin));
        southstar_js_lifecycle::start_lifecycle(self.0, doc, tasks, Some(origin), start_us);
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
        let count = |n: usize| n as u32;
        [
            southstar_js_services::timer_count(self.0),
            count(southstar_js_lifecycle::raf_count(self.0)),
            count(southstar_js_net::pending_fetch_count(self.0)),
            count(southstar_js_net::pending_xhr_count(self.0)),
            count(southstar_js_net::pending_socket_count(self.0)),
            count(southstar_js_dispatch::listener_count(self.0)),
        ]
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
    southstar_debuglog::emit(DLOG_JS, category.to_bytes(), message);
}

pub(crate) fn profile_enabled() -> bool {
    southstar_js_loader::profile_enabled()
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
    southstar_dom::serialize::is_embedded_doc(node) || southstar_js_shadow::is_shadow_root(node)
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

pub(crate) fn name_engine_members(scope: &mut Scope<'_>) {
    unsafe { ns_js_name_engine_members(scope.handle()) };
}

pub(crate) fn memory_usage(scope: &mut Scope<'_>) -> MemoryUsage {
    scope.memory_usage()
}

unsafe fn c_text<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub fn run_scripts_in_doc(js: *const NsJs, doc: Node<'_>, base_url: Option<&CStr>) {
    if let (false, Some(doc)) = (js.is_null(), doc.downgrade().get()) {
        run::run_scripts_in_doc(Js(js.cast_mut()), doc, base_url);
    }
}

pub fn dump_stats(js: *const NsJs) -> String {
    if js.is_null() {
        return String::new();
    }
    stats::dump(Js(js.cast_mut()))
}

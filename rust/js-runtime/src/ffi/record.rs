//! Southstar — the page's JavaScript record (ns_js): its runtime and realms, DOM prototypes, storage partition, cookies and CSP, and the node and realm helpers the bindings reach through it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, OnceCell, RefCell};
use core::ffi::CStr;
use core::ptr::{self, NonNull};
use std::collections::HashMap;
use std::ffi::CString;

use southstar_csp::Csp;
use southstar_dom::{Kind, Node, NsNode};
use southstar_js_bind::bind_native;
use southstar_js_engine::{
    Attributes, ContextHandle, RuntimeHandle, RuntimeOptions, Scope, ScriptOptions, Value,
};
use southstar_layout::BoxRef;

use super::NavigationTiming;
use super::imports;
use super::state::PageState;
use crate::api::Page;

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_IMG_LOAD_FIRED: u32 = 1 << 3;
const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;
const FLAG_SVG_NS: u32 = 1 << 7;
const FLAG_FOREIGN_NS: u32 = 1 << 9;
const FLAG_CDATA: u32 = 1 << 10;
const FLAG_PI: u32 = 1 << 11;
const MATHML_NS: &CStr = c"http://www.w3.org/1998/Math/MathML";

static POLYFILLS: &str = concat!(include_str!("../../js/polyfills.js"), "\0");
static STREAMING: &str = concat!(include_str!("../../js/streaming.js"), "\0");

const FRAME_WINDOW_CHAIN: &[u8] = b"(function(G){\
  var gp = Object.getPrototypeOf, sp = Object.setPrototypeOf;\
  var W = G.Window && G.Window.prototype, ET = G.EventTarget && G.EventTarget.prototype;\
  if (!W || !ET) return;\
  var named = gp(W);\
  if (named && named !== ET && named !== Object.prototype && gp(named) !== ET)\
    try { sp(named, ET); } catch (e) {}\
  if (gp(G) !== W) try { sp(G, W); } catch (e) {}\
})(globalThis)";

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

pub(crate) struct Record {
    pub rt: Cell<RuntimeHandle>,
    pub ctx: Cell<ContextHandle>,
    pub main_realm_ctx: Cell<ContextHandle>,
    frame_ctxs: RefCell<Option<Vec<ContextHandle>>>,
    pub current_doc: Cell<*mut NsNode>,
    pub raf_frame_ctx: Cell<*mut NsNode>,
    pristine_promise: RefCell<Value>,
    layout_root: Cell<Option<BoxRef<'static>>>,
    pinned_wrappers_set: RefCell<Option<HashMap<usize, Value>>>,
    cookie_value: RefCell<Option<CString>>,
    cookie_version: Cell<u64>,
    cookie_buckets: RefCell<Option<HashMap<CString, CString>>>,
    partition_key: RefCell<Option<CString>>,
    opaque_counter: Cell<u64>,
    navigator_brand: RefCell<Value>,
    pub halted: Cell<bool>,
    csp: RefCell<Option<Csp>>,
    time_origin_us: Cell<i64>,
    time_origin_real_ms: Cell<f64>,
    navigation_timing: Cell<NavigationTiming>,
    pub ce_main_doc: Cell<*const NsNode>,
    platform_globals: RefCell<Option<Vec<Vec<u8>>>>,
    protos: RefCell<Option<DomProtos>>,
    proto_document: RefCell<Value>,
    per_tag_protos: RefCell<HashMap<Vec<u8>, Value>>,
    state: OnceCell<Box<PageState>>,
}

thread_local! {
    static ACTIVE: Cell<Option<Page>> = const { Cell::new(None) };
    static LIVE: RefCell<HashMap<usize, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn live_page(addr: usize) -> Option<Page> {
    LIVE.with_borrow(|live| live.get(&addr).copied())
}

pub(crate) fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    // SAFETY: the bindings pass null or a node of this thread's store, and from_ptr returns None once that node is freed.
    unsafe { Node::from_ptr(n) }
}

fn is_named(n: Node<'_>, tag: &CStr) -> bool {
    n.element_name() == Some(tag.to_bytes())
}

fn attr<'a>(n: Node<'a>, name: &CStr) -> Option<&'a CStr> {
    n.attr(name)
}

pub(crate) fn record<'a>(js: Option<Page>) -> Option<&'a Record> {
    let js = js?.as_ptr().cast::<Record>();
    // SAFETY: a page names a record alloc leaked, which stays live until free_engine reclaims it.
    Some(unsafe { &*js })
}

fn main_context(js: &Record) -> ContextHandle {
    js.main_realm_ctx.get().or(js.ctx.get())
}

fn page_of(scope: &Scope<'_>) -> Option<Page> {
    Page::current(scope)
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn alloc() -> Page {
    let record = Box::new(Record {
        rt: Cell::new(RuntimeHandle::NULL),
        ctx: Cell::new(ContextHandle::NULL),
        main_realm_ctx: Cell::new(ContextHandle::NULL),
        frame_ctxs: RefCell::new(None),
        current_doc: Cell::new(ptr::null_mut()),
        raf_frame_ctx: Cell::new(ptr::null_mut()),
        pristine_promise: RefCell::new(Value::undefined()),
        layout_root: Cell::new(None),
        pinned_wrappers_set: RefCell::new(None),
        cookie_value: RefCell::new(None),
        cookie_version: Cell::new(0),
        cookie_buckets: RefCell::new(None),
        partition_key: RefCell::new(None),
        opaque_counter: Cell::new(0),
        navigator_brand: RefCell::new(Value::undefined()),
        halted: Cell::new(false),
        csp: RefCell::new(None),
        time_origin_us: Cell::new(0),
        time_origin_real_ms: Cell::new(0.0),
        navigation_timing: Cell::new(NavigationTiming::starting_at(0, 0.0)),
        ce_main_doc: Cell::new(ptr::null()),
        platform_globals: RefCell::new(None),
        protos: RefCell::new(None),
        proto_document: RefCell::new(Value::undefined()),
        per_tag_protos: RefCell::new(HashMap::new()),
        state: OnceCell::new(),
    });
    drop(record.state.set(Box::new(PageState::new())));
    let js = Page::from_record(NonNull::from(Box::leak(record)).cast());
    LIVE.with_borrow_mut(|live| live.insert(js.addr(), js));
    js
}

impl Record {
    pub(crate) fn state(&self) -> &PageState {
        self.state.get().expect("alloc sets the page state")
    }
}

pub(crate) fn set_time_origin(js: &Record, origin_us: i64, origin_real_ms: f64) {
    js.time_origin_us.set(origin_us);
    js.time_origin_real_ms.set(origin_real_ms);
}

pub(crate) fn set_navigation_timing(js: Option<Page>, timing: NavigationTiming) {
    if let Some(js) = record(js) {
        js.navigation_timing.set(timing);
    }
}

pub(crate) fn navigation_timing(js: Option<Page>) -> Option<NavigationTiming> {
    record(js).map(|js| js.navigation_timing.get())
}

pub(crate) fn create_runtime(js: &Record, options: &RuntimeOptions) -> bool {
    let rt = RuntimeHandle::new(options);
    js.rt.set(rt);
    !rt.is_null()
}

pub(crate) fn create_context(page: Page, js: &Record) -> bool {
    let rt = js.rt.get();
    let ctx = rt.new_context(page.as_ptr().cast_mut().cast());
    if ctx.is_null() {
        return false;
    }
    js.ctx.set(ctx);
    js.main_realm_ctx.set(ctx);
    rt.set_opaque(page.as_ptr().cast_mut().cast());
    true
}

pub(crate) fn init_tables(js: &Record, page: bool) {
    js.pinned_wrappers_set.replace(Some(HashMap::new()));
    if page {
        js.frame_ctxs.replace(Some(Vec::new()));
        js.cookie_buckets.replace(Some(HashMap::new()));
    }
}

pub(crate) fn set_partition_key(js: &Record, partition_key: Option<&CStr>) {
    js.partition_key
        .replace(partition_key.map(|key| cstring(key.to_bytes())));
}

pub(crate) fn has_pinned_set(js: Option<Page>) -> bool {
    record(js).is_some_and(|js| js.pinned_wrappers_set.borrow().is_some())
}

pub(crate) fn pin_wrapper(js: Option<Page>, n: *const NsNode, wrapper: &Value) -> bool {
    let Some(js) = record(js) else {
        return false;
    };
    let previous = js
        .pinned_wrappers_set
        .borrow_mut()
        .as_mut()
        .map(|set| set.insert(n as usize, wrapper.clone()));
    previous.is_some()
}

pub(crate) fn unpin_wrapper(js: Option<Page>, n: *const NsNode) -> bool {
    let Some(js) = record(js) else {
        return false;
    };
    let pinned = js
        .pinned_wrappers_set
        .borrow_mut()
        .as_mut()
        .and_then(|set| set.remove(&(n as usize)));
    pinned.is_some()
}

pub(crate) fn take_pinned_wrappers(js: Option<Page>) -> Vec<(usize, Value)> {
    record(js).map_or_else(Vec::new, |js| {
        js.pinned_wrappers_set
            .borrow_mut()
            .as_mut()
            .map_or_else(Vec::new, |set| set.drain().collect())
    })
}

fn pinned_wrappers(js: Option<Page>) -> Vec<(usize, Value)> {
    record(js).map_or_else(Vec::new, |js| {
        js.pinned_wrappers_set
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, |set| {
                set.iter()
                    .map(|(key, wrapper)| (*key, wrapper.clone()))
                    .collect()
            })
    })
}

pub(crate) fn set_proto_document(js: &Record, proto: Value) {
    drop(js.proto_document.replace(proto));
}

pub(crate) fn set_navigator_brand(js: &Record, brand: Value) {
    drop(js.navigator_brand.replace(brand));
}

pub(crate) fn set_pristine_promise(js: &Record, promise: Value) {
    drop(js.pristine_promise.replace(promise));
}

pub(crate) fn release_wrapper(_js: &Record, _n: Node<'_>) {}

pub(crate) fn detach_wrapper(js: &Record, n: Node<'_>, pinned: Option<Value>) {
    let wrapper = n.js_wrapper();
    if wrapper.is_null() {
        return;
    }
    let object = pinned.or_else(|| {
        js.ctx
            .get()
            .with(|scope| scope.object_from_identity(wrapper))
    });
    if let Some(object) = object {
        object.with_instance(
            &super::wrapper::ELEMENT_CLASS,
            super::wrapper::ElementData::unbind,
        );
    }
    n.set_js_wrapper(ptr::null_mut());
    n.set_js_invalidate(None);
}

fn eval_bundled(js: Option<Page>, source: &str, origin: &CStr) {
    let source = source.strip_suffix('\0').unwrap_or(source);
    (crate::api::loader_hooks().eval)(js, source.as_bytes(), Some(origin));
}

pub(crate) fn eval_worker_polyfills(js: Option<Page>) {
    eval_bundled(js, POLYFILLS, c"<worker-polyfills>");
}

pub(crate) fn free_page_strings(js: &Record) {
    js.csp.replace(None);
    js.cookie_value.replace(None);
    js.cookie_version.set(0);
}

pub(crate) fn free_pinned_set(js: &Record) {
    js.pinned_wrappers_set.replace(None);
}

pub(crate) fn free_realm_values(js: &Record) {
    let pristine = js.pristine_promise.replace(Value::undefined());
    let protos = js.protos.take();
    let document = js.proto_document.replace(Value::undefined());
    let per_tag = js.per_tag_protos.take();
    drop((pristine, protos, document, per_tag));
}

pub(crate) fn free_platform_globals(js: &Record) {
    js.platform_globals.replace(None);
}

pub(crate) fn free_partition(js: &Record) {
    js.cookie_buckets.replace(None);
    js.partition_key.replace(None);
}

pub(crate) fn free_window_values(js: &Record) {
    drop(js.navigator_brand.replace(Value::undefined()));
}

/// # Safety
///
/// `js` is a page alloc returned that nothing uses afterwards.
pub(crate) unsafe fn free_engine(js: Page) {
    let Some(record) = record(Some(js)) else {
        return;
    };
    LIVE.with_borrow_mut(|live| live.remove(&js.addr()));
    free_realm_values(record);
    free_window_values(record);
    for ctx in record.frame_ctxs.take().unwrap_or_default() {
        ctx.free();
    }
    record.ctx.get().free();
    record.rt.get().free();
    // SAFETY: the record came from Box::leak in alloc, and the caller guarantees it is not used again.
    drop(unsafe { Box::from_raw(js.as_ptr().cast_mut().cast::<Record>()) });
}

pub(crate) fn worker_compile(
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

fn partition_apply(page: Option<Page>, js: &Record, new_url: Option<&CStr>) {
    let origin = new_url
        .and_then(|url| southstar_url::bytes::origin_from(url.to_bytes()))
        .filter(|origin| !origin.is_empty())
        .map(|origin| cstring(&origin));
    let new_key = origin.unwrap_or_else(|| {
        let counter = js.opaque_counter.get() + 1;
        js.opaque_counter.set(counter);
        cstring(format!("opaque://{counter}").as_bytes())
    });
    if js.partition_key.borrow().as_ref() == Some(&new_key) {
        return;
    }
    let old_key = js.partition_key.borrow().clone();
    if let (Some(key), Some(buckets)) = (old_key.clone(), js.cookie_buckets.borrow_mut().as_mut()) {
        let value = js.cookie_value.borrow().clone().unwrap_or_default();
        buckets.insert(key, value);
    }
    (crate::api::storage_hooks().switch_session)(page, old_key.as_deref(), &new_key);
    let value = js
        .cookie_buckets
        .borrow()
        .as_ref()
        .and_then(|buckets| buckets.get(&new_key).cloned());
    js.cookie_value.replace(value);
    js.cookie_version.set(0);
    js.partition_key.replace(Some(new_key));
}

fn foreign_kind_proto<'a>(protos: &'a DomProtos, n: Node<'_>) -> &'a Value {
    let mathml = attr(n, c"data-nd-ns-uri").is_some_and(|ns| ns == MATHML_NS);
    if mathml && protos.mathml_element.is_object() {
        &protos.mathml_element
    } else {
        &protos.element
    }
}

fn html_kind_proto<'a>(js: &Record, protos: &'a DomProtos, n: Node<'_>) -> Option<&'a Value> {
    let Some(name) = n.name() else {
        return Some(&protos.html_element);
    };
    let name = name.to_bytes();
    if !name.iter().any(u8::is_ascii_uppercase) {
        if js.per_tag_protos.borrow().contains_key(name) {
            return None;
        }
        if PLAIN_INTERFACE_TAGS.contains(&name) {
            return Some(&protos.html_element);
        }
    }
    let valid_custom = n
        .name()
        .is_some_and(|name| (crate::api::custom_elements_hooks().name_valid)(name.to_bytes()));
    if !valid_custom && protos.html_unknown_element.is_object() {
        return Some(&protos.html_unknown_element);
    }
    Some(&protos.html_element)
}

fn per_tag_proto(js: &Record, n: Node<'_>) -> Option<Value> {
    let name = n.name()?.to_bytes();
    js.per_tag_protos.borrow().get(name).cloned()
}

fn kind_proto(js: &Record, n: Node<'_>) -> Option<Value> {
    let protos = js.protos.borrow();
    let protos = protos.as_ref()?;
    let document = js.proto_document.borrow();
    let flags = n.flags();
    Some(
        match n.kind() {
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
                    match html_kind_proto(js, protos, n) {
                        Some(proto) => proto,
                        None => return per_tag_proto(js, n),
                    }
                }
            }
            Kind::Text if flags & FLAG_CDATA != 0 => &protos.cdata,
            Kind::Text => &protos.text,
            Kind::Comment if flags & FLAG_PI != 0 => &protos.pi,
            Kind::Comment => &protos.comment,
            Kind::Doctype => &protos.doctype,
            Kind::Document if flags & FLAG_FRAGMENT != 0 => &protos.docfrag,
            Kind::Document => &*document,
            Kind::Other => return None,
        }
        .clone(),
    )
}

pub(crate) fn kind_proto_value(js: Option<Page>, n: *const NsNode) -> Option<Value> {
    match (record(js), node(n)) {
        (Some(js), Some(n)) => kind_proto(js, n),
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

pub(crate) fn active() -> Option<Page> {
    ACTIVE.with(Cell::get)
}

pub(crate) fn set_active(js: Option<Page>) {
    ACTIVE.with(|active| active.set(js));
}

pub(crate) fn array_length(scope: &mut Scope<'_>, arr: &Value) -> u32 {
    scope
        .get(arr, "length")
        .and_then(|length| scope.to_int64(&length))
        .map_or(0, |length| length as u32)
}

pub(crate) fn storage_load_for(js: Option<Page>, new_url: Option<&CStr>) {
    let Some(record) = record(js) else {
        return;
    };
    partition_apply(js, record, new_url);
    (crate::api::storage_hooks().load_local)(js, new_url);
}

pub(crate) fn img_src_layout_neutral(n: Node<'_>) -> bool {
    n.name().is_some_and(|name| name.to_bytes() == b"img")
        && attr(n, c"width").is_some_and(|w| !w.is_empty())
        && attr(n, c"height").is_some_and(|h| !h.is_empty())
}

pub(crate) fn set_src_recorded(js: Option<Page>, el: Node<'_>, src: &[u8]) {
    let mut terminated = Vec::with_capacity(src.len() + 1);
    terminated.extend_from_slice(src);
    terminated.push(0);
    let old = southstar_dom::attrs::find(el, c"src").and_then(southstar_dom::Attr::value_bytes);
    let changed = old != Some(src);
    let mutation = crate::api::mutation_hooks();
    if changed && img_src_layout_neutral(el) {
        let value = CStr::from_bytes_until_nul(&terminated).unwrap_or(c"");
        (mutation.set_attr_recorded_paint_only)(js, el, c"src", value);
    } else {
        (mutation.set_attr_recorded)(js, el, c"src", src);
    }
    let name = el.name().map(CStr::to_bytes);
    if changed && name == Some(b"img") {
        el.remove_flags(FLAG_IMG_LOAD_FIRED);
        let src = CStr::from_bytes_until_nul(&terminated).ok();
        (crate::api::images_hooks().start_image_load)(js, el, src);
    }
    if name == Some(b"iframe") {
        (crate::api::frames_hooks().schedule_load)(js, el, true);
    }
}

pub(crate) fn in_template_content(n: Node<'_>) -> bool {
    southstar_dom::ancestors_and_self(n)
        .any(|p| p.flags() & FLAG_TEMPLATE_CONTENT != 0 || is_named(p, c"template"))
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

pub(crate) fn media_native_formats() -> u32 {
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

pub(crate) fn url_parses(js: Option<Page>, url: &CStr) -> bool {
    let base = (crate::api::document_hooks().base_url)(js);
    let base = base.as_deref().filter(|base| !base.is_empty());
    southstar_url::bytes::resolve(base, url.to_bytes()).is_some()
}

pub(crate) fn csp_allows_worker(js: Option<Page>, realm: ContextHandle, url: &CStr) -> bool {
    let page = crate::api::realm_url(js, realm).or_else(|| crate::api::page_url(js));
    with_realm_csp(js, realm, |csp| {
        csp.is_none_or(|csp| csp.allows(southstar_csp::WORKER, url, page.as_deref(), None, true))
    })
}

pub(crate) fn purge_subtree_rafs(js: Option<Page>, root: *mut NsNode) {
    if js.is_none() {
        return;
    }
    let mut n = root;
    while !n.is_null() {
        if let Some(frame) = node(n).filter(|&el| is_named(el, c"iframe")) {
            (crate::api::lifecycle_hooks().purge_frame_rafs)(js, frame);
            (crate::api::services_hooks().purge_frame_timers)(js, frame);
            (crate::api::frames_hooks().scrub_globals)(js, frame);
        }
        n = node(n)
            .and_then(|current| southstar_dom::index::next_in_subtree(current, node(root), true))
            .map_or(ptr::null_mut(), Node::as_mut_ptr);
    }
}

pub(crate) fn insert_before_single(js: Option<Page>, child: Node<'_>, reference_node: Node<'_>) {
    if child == reference_node {
        return;
    }
    if child.parent().is_some() {
        if js.is_some() {
            (crate::api::traversal_hooks().pre_remove)(js, child);
        }
        child.detach();
    }
    if let Some(js) = js {
        (crate::api::mutation_hooks().unorphan_node)(js, child);
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

pub(crate) fn set_layout_root(js: Option<Page>, root: Option<BoxRef<'_>>) {
    let Some(page) = super::Js::of(js) else {
        return;
    };
    let record = page.rec();
    let root = root.map(BoxRef::into_static);
    if record.layout_root.get().map(BoxRef::as_ptr) != root.map(BoxRef::as_ptr) {
        (crate::api::geometry_hooks().forget_box_lookup)(page.page());
    }
    record.layout_root.set(root);
    (crate::api::window_hooks().sync_metrics)(page.page());
    if root.is_some() {
        (crate::api::frames_hooks().promote_deferred)(js);
        if super::glue::is_running_script(page.ctx()) {
            crate::api::observer_schedule_tick(js);
        } else {
            super::glue::observers_tick(page);
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
                .is_some_and(|name| !southstar_dom::attrs::is_internal(name.to_bytes()))
        })
        .map(|a| {
            imports::nsr_attr_to_js(scope, owner, a.view(), false)
                .unwrap_or_else(|_| Value::undefined())
        })
        .collect();
    for (index, attr) in attrs.into_iter().enumerate() {
        drop(scope.set_index(&array, index as u32, attr));
    }
    array
}

pub(crate) fn is_disabled_form_control(el: Node<'_>) -> bool {
    el.is_element()
        && el.name().is_some_and(|name| {
            FORM_CONTROLS
                .iter()
                .any(|control| name.to_bytes().eq_ignore_ascii_case(control.to_bytes()))
        })
        && southstar_dom::controls::effectively_disabled(el)
}

pub(crate) fn form_reset(js: Option<Page>, form: Node<'_>) {
    let Some(ctx) = record(js).map(|js| js.ctx.get()) else {
        return;
    };
    ctx.with(|scope| (crate::api::forms_hooks().reset_form)(scope, form));
}

pub(crate) fn image_load_realm(js: Option<Page>) -> ContextHandle {
    let Some(record) = record(js) else {
        return ContextHandle::NULL;
    };
    let realm = node(record.raf_frame_ctx.get()).map_or(ContextHandle::NULL, |frame| {
        (crate::api::frames_hooks().frame_context)(js, frame)
    });
    realm.or(record.ctx.get())
}

pub(crate) fn frame_url(js: super::Js, frame: *const NsNode) -> Option<CString> {
    (crate::api::frames_hooks().frame_url)(js.page(), node(frame))
}

pub(crate) fn node_frame_url<'a>(el: Node<'a>) -> Option<&'a CStr> {
    southstar_dom::ancestors(el)
        .filter(|&p| is_named(p, c"iframe") || is_named(p, c"frame") || is_named(p, c"object"))
        .find_map(|p| attr(p, c"data-nd-frame-url").filter(|url| !url.is_empty()))
}

pub(crate) fn platform_global_names(js: Option<Page>) -> Vec<Vec<u8>> {
    let Some(record) = record(js) else {
        return Vec::new();
    };
    if record.platform_globals.borrow().is_none() {
        let saved = record.ctx.replace(main_context(record));
        let globals = super::Js::of(js).map(super::exports::snapshot_globals);
        record.ctx.set(saved);
        record.platform_globals.replace(globals);
    }
    record.platform_globals.borrow().clone().unwrap_or_default()
}

pub(crate) fn new_frame_context(js: Option<Page>) -> ContextHandle {
    let Some(record) = record(js) else {
        return ContextHandle::NULL;
    };
    let fctx = record
        .rt
        .get()
        .new_context(super::Js::of(js).map_or(ptr::null_mut(), super::Js::raw));
    if fctx.is_null() {
        return fctx;
    }
    fctx.with(|scope| {
        let global = scope.global();
        southstar_js_realm::hide_shared_array_buffer(scope, &global);
    });
    if let Some(frames) = record.frame_ctxs.borrow_mut().as_mut() {
        frames.push(fctx);
    }
    fctx
}

pub(crate) fn frame_realm_finish(
    js: Page,
    fctx: ContextHandle,
    frame_global: &Value,
    parent_global: &Value,
) {
    let interfaces = crate::api::interfaces_hooks();
    (interfaces.install_pdf_plugins)(fctx);
    if let Some(record) = record(Some(js)) {
        adopt_frame_window_events(record, fctx, frame_global);
    }
    (interfaces.link_interface_ctors)(fctx);
    (interfaces.lock_global_prototypes)(fctx);
    let attr_class = (crate::api::attrs_hooks().class_id)();
    fctx.with(|scope| {
        southstar_js_brand::brand_node_interfaces(
            scope,
            super::wrapper::ELEMENT_CLASS.id(),
            attr_class,
        )
    });
    fctx.with(|scope| {
        let include_memory = scope
            .get(parent_global, "performance")
            .is_ok_and(|performance| {
                performance.is_object()
                    && southstar_js_bind::has_defined_own(scope, &performance, "memory")
            });
        let performance =
            (crate::api::perf_hooks().make_performance_object)(scope, js, include_memory);
        drop(scope.set(frame_global, "performance", performance));
    });
}

pub(crate) fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> Option<Node<'static>> {
    if let Some(root) = super::wrapper::unwrap(this).filter(|root| root.kind() == Kind::Document) {
        return Some(root);
    }
    node(record(page_of(scope))?.current_doc.get())
}

pub(crate) fn ce_main_doc(js: Option<Page>) -> Option<Node<'static>> {
    node(record(js)?.ce_main_doc.get())
}

pub(crate) fn node_wrapper(scope: &mut Scope<'_>, n: *const NsNode) -> Option<Value> {
    let wrapper = node(n).map(Node::js_wrapper).filter(|w| !w.is_null())?;
    Some(scope.object_from_identity(wrapper))
}

pub(crate) fn node_unpinned(js: Option<Page>, n: Node<'_>) -> bool {
    record(js).is_some_and(|js| {
        js.pinned_wrappers_set
            .borrow()
            .as_ref()
            .is_some_and(|set| !set.contains_key(&(n.as_ptr() as usize)))
    })
}

pub(crate) fn wrapper_pinned(js: Option<Page>, n: Node<'_>) -> bool {
    record(js).is_some_and(|js| {
        n.has_js_wrapper()
            && js
                .pinned_wrappers_set
                .borrow()
                .as_ref()
                .is_some_and(|set| set.contains_key(&(n.as_ptr() as usize)))
    })
}

pub(crate) fn new_orphan_element(scope: &mut Scope<'_>, name: &[u8]) -> Result<Value, Value> {
    let js = page_of(scope);
    let name = cstring(name);
    let element_node = southstar_dom::node::new_element(name.to_bytes());
    super::wrapper::arm_js_invalidate(element_node);
    (crate::api::mutation_hooks().track_orphan)(js, element_node);
    let element = super::wrapper::make(scope, element_node)?;
    tag_caller_document(scope, &element);
    Ok(element)
}

pub(crate) fn ce_attr_changed(
    js: Option<Page>,
    el: Node<'_>,
    attr_name: &CStr,
    old: Option<&CStr>,
    new: Option<&CStr>,
) {
    let live = || record(js).is_some_and(|js| !js.ctx.get().is_null() && !js.halted.get());
    let attr_bytes = attr_name.to_bytes();
    if live() {
        let focus = crate::api::focus_hooks();
        if (focus.focused_node)(js) == Some(el) && attr_bytes.eq_ignore_ascii_case(b"type") {
            (focus.update_focus_visible)(js);
        }
        (crate::api::top_layer_hooks().popover_attr_changed)(js, el, attr_name, old, new);
    }
    let is_plain_script =
        is_named(el, c"script") && el.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0;
    if js.is_some()
        && attr_bytes.eq_ignore_ascii_case(b"src")
        && old.is_none()
        && new.is_some()
        && is_plain_script
    {
        (crate::api::loader_hooks().script_needs_prepare)(js, el);
        return;
    }
    if !live() {
        return;
    }
    (crate::api::custom_elements_hooks().attribute_changed)(js, el, attr_name, old, new);
}

pub(crate) fn illegal_constructor(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    Err(scope.type_error("Illegal constructor"))
}

pub(crate) fn register_tag_proto(scope: &mut Scope<'_>, tag: &[u8], proto: Value) {
    let Some(js) = record(page_of(scope)) else {
        return;
    };
    let previous = js.per_tag_protos.borrow_mut().insert(tag.to_vec(), proto);
    drop(previous);
}

pub(crate) fn adopt_dom_protos(scope: &mut Scope<'_>, protos: AdoptedProtos) {
    let js = page_of(scope);
    let Some(record) = record(js) else {
        return;
    };
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
    let previous = record.protos.replace(Some(adopted));
    let previous_document = record.proto_document.replace(document);
    drop((previous, previous_document));
    for (key, object) in pinned_wrappers(js) {
        let Some(n) = node(key as *const NsNode).filter(|n| n.has_js_wrapper()) else {
            continue;
        };
        let Some(proto) = kind_proto(record, n).filter(Value::is_object) else {
            continue;
        };
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

pub(crate) fn free_frame_contexts(js: Option<Page>) {
    let Some(record) = record(js) else {
        return;
    };
    let frames = record.frame_ctxs.borrow_mut().as_mut().map(core::mem::take);
    for ctx in frames.unwrap_or_default() {
        ctx.free();
    }
}

pub(crate) fn frame_context_count(js: Option<Page>) -> u32 {
    record(js)
        .and_then(|js| js.frame_ctxs.borrow().as_ref().map(Vec::len))
        .map_or(0, |frames| frames as u32)
}

pub(crate) fn eval_polyfills(js: Option<Page>) {
    if js.is_none() {
        return;
    }
    eval_bundled(js, POLYFILLS, c"<polyfills>");
    eval_bundled(js, STREAMING, c"<streaming>");
}

pub(crate) fn snapshot_platform_globals(js: Option<Page>) {
    let Some(record) = record(js) else {
        return;
    };
    let globals = super::Js::of(js).map(super::exports::snapshot_globals);
    record.platform_globals.replace(globals);
}

pub(crate) fn exception_message(scope: &mut Scope<'_>, ex: &Value) -> Vec<u8> {
    scope
        .to_bytes(ex)
        .unwrap_or_else(|_| b"Script error.".to_vec())
}

pub(crate) fn add_csp_header(js: Option<Page>, header_value: Option<&CStr>) {
    let Some(header_value) = header_value.filter(|header| !header.is_empty()) else {
        return;
    };
    let Some(record) = record(js) else {
        return;
    };
    let Some(mut parsed) = Csp::parse(header_value.to_bytes()) else {
        return;
    };
    match &mut *record.csp.borrow_mut() {
        Some(csp) => csp.merge(&mut parsed),
        slot @ None => *slot = Some(parsed),
    }
}

pub(crate) fn csp_form_action_allowed(
    js: Option<Page>,
    form: Option<Node<'_>>,
    action_url: Option<&CStr>,
) -> bool {
    let Some(action_url) = action_url else {
        return true;
    };
    let page = form
        .and_then(node_frame_url)
        .map(CStr::to_owned)
        .or_else(|| super::Js::of(js).and_then(super::Js::current_url));
    with_node_csp(js, form, |csp| {
        csp.is_none_or(|csp| {
            csp.allows(
                southstar_csp::FORM_ACTION,
                action_url,
                page.as_deref(),
                None,
                true,
            )
        })
    })
}

pub(crate) fn inline_handlers_allowed(js: Option<Page>, target: Option<Node<'_>>) -> bool {
    with_node_csp(js, target, |csp| {
        csp.is_none_or(Csp::inline_event_handler_allowed)
    })
}

pub(crate) fn time_origin_us(js: Option<Page>) -> i64 {
    record(js).map_or(0, |js| js.time_origin_us.get())
}

pub(crate) fn time_origin_real_ms(js: Option<Page>) -> f64 {
    record(js).map_or(0.0, |js| js.time_origin_real_ms.get())
}

pub(crate) fn navigator_brand(js: Option<Page>) -> Value {
    record(js).map_or_else(Value::undefined, |js| js.navigator_brand.borrow().clone())
}

pub(crate) fn pristine_promise(js: Option<Page>) -> Value {
    record(js).map_or_else(Value::undefined, |js| js.pristine_promise.borrow().clone())
}

pub(crate) fn layout_root(js: Option<Page>) -> Option<BoxRef<'static>> {
    record(js)?.layout_root.get()
}

pub(crate) fn cookie_value(js: Option<Page>) -> Option<CString> {
    record(js)?.cookie_value.borrow().clone()
}

pub(crate) fn set_cookie_value(js: Option<Page>, value: Option<&CStr>) {
    let Some(record) = record(js) else {
        return;
    };
    let value = value.map(|value| cstring(value.to_bytes()));
    record.cookie_value.replace(value);
}

pub(crate) fn cookie_version(js: Option<Page>) -> u64 {
    record(js).map_or(0, |js| js.cookie_version.get())
}

pub(crate) fn set_cookie_version(js: Option<Page>, version: u64) {
    if let Some(js) = record(js) {
        js.cookie_version.set(version);
    }
}

pub(crate) fn partition_key(js: Option<Page>) -> Option<CString> {
    record(js)?.partition_key.borrow().clone()
}

pub(crate) fn with_page_csp<R>(js: Option<Page>, f: impl FnOnce(Option<&Csp>) -> R) -> R {
    match record(js) {
        Some(js) => f(js.csp.borrow().as_ref()),
        None => f(None),
    }
}

fn with_frame_csp<R>(
    js: Option<Page>,
    frame: Option<Node<'_>>,
    f: impl FnOnce(Option<&Csp>) -> R,
) -> R {
    let own = js
        .zip(frame)
        .and_then(|(page, frame)| (crate::api::frames_hooks().frame_csp)(page, frame));
    match own {
        Some(own) => f(own.as_deref()),
        None => with_page_csp(js, f),
    }
}

pub(crate) fn calling_realm(js: Option<Page>, caller: ContextHandle) -> ContextHandle {
    match super::Js::of(js) {
        Some(js) if caller == js.main_or_current() && !js.ctx().is_null() => js.ctx(),
        _ => caller,
    }
}

fn realm_frame(js: Option<Page>, realm: ContextHandle) -> Option<Node<'static>> {
    let page = js?;
    let main = super::Js::of(js)?.main_or_current();
    if realm.is_null() || realm == main {
        return None;
    }
    (crate::api::frames_hooks().frame_of_realm)(page, realm)
}

fn containing_frame(node: Node<'_>) -> Option<Node<'_>> {
    southstar_dom::ancestors_and_self(node)
        .find(|n| n.kind() == Kind::Document)
        .and_then(|doc| doc.parent())
}

pub(crate) fn with_realm_csp<R>(
    js: Option<Page>,
    realm: ContextHandle,
    f: impl FnOnce(Option<&Csp>) -> R,
) -> R {
    with_frame_csp(js, realm_frame(js, realm), f)
}

pub(crate) fn with_node_csp<R>(
    js: Option<Page>,
    node: Option<Node<'_>>,
    f: impl FnOnce(Option<&Csp>) -> R,
) -> R {
    with_frame_csp(js, node.and_then(containing_frame), f)
}

pub(crate) fn csp_allows_connect(
    js: Option<Page>,
    realm: ContextHandle,
    url: &CStr,
    page: Option<&CStr>,
) -> bool {
    with_realm_csp(js, realm, |csp| {
        csp.is_none_or(|csp| csp.allows(southstar_csp::CONNECT, url, page, None, true))
    })
}

pub(crate) fn csp_allows_media(
    js: Option<Page>,
    element: Option<Node<'_>>,
    url: &CStr,
    page: Option<&CStr>,
) -> bool {
    with_node_csp(js, element, |csp| {
        csp.is_none_or(|csp| csp.allows(southstar_csp::MEDIA, url, page, None, true))
    })
}

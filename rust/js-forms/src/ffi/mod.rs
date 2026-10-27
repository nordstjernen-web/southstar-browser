//! Southstar — the forms bindings' exports (C for node-only calls, Rust ABI for natives) and the DOM and engine calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean, GError};
use southstar_js_bind::host::{self, HostKind};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use self::imports::*;
use crate::checkable::ClickState;
use crate::{Element, JsResult};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Page(*mut NsJs);

unsafe extern "C" {
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_uint,
        match_options: c_uint,
        error: *mut *mut GError,
    ) -> *mut c_void;
    fn g_regex_match(
        regex: *const c_void,
        string: *const c_char,
        match_options: c_uint,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut c_void);

    fn g_content_type_guess(
        filename: *const c_char,
        data: *const u8,
        data_size: usize,
        result_uncertain: *mut GBoolean,
    ) -> *mut c_char;
    fn g_content_type_get_mime_type(content_type: *const c_char) -> *mut c_char;
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes.to_vec())).unwrap_or_default()
}

fn live(n: Node<'_>) -> Option<Element> {
    n.downgrade().get()
}

pub(crate) fn element(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    api::make_element(scope, Node::ptr_or_null(node)).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn shadow_root(scope: &mut Scope<'_>, element: &Value) -> Value {
    nsr_element_get_shadowRoot(scope, element).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn not_found(scope: &mut Scope<'_>, message: &str) -> Value {
    dom_exception(scope, c"NotFoundError", 8, message)
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: c_int,
    message: &str,
) -> Value {
    let message = crate::until_nul(message.as_bytes().to_vec());
    nsr_dom_exception(scope, name.to_bytes(), code, &message)
}

pub(crate) fn mark_restyle_dirty(node: Element) {
    southstar_css::mark_restyle_dirty(node);
}

pub(crate) fn parse_color(text: &[u8]) -> Option<(u8, u8, u8, u8)> {
    southstar_css::parse_color(&cstring(text)).map(|[r, g, b, a]| (r, g, b, a))
}

pub(crate) fn utf8_casefold(text: &[u8]) -> Vec<u8> {
    let text = cstring(text);
    southstar_unicode::fold_string(&text.to_string_lossy()).into_bytes()
}

pub(crate) fn node_remove(node: Element) {
    node.detach();
}

pub(crate) fn append_child(parent: Element, child: Element) {
    parent.append(child);
}

pub(crate) fn insert_before_single(page: Option<Page>, child: Element, reference: Element) {
    api::insert_before_single(Page::raw(page), child, reference);
}

pub(crate) fn array_item_function(scope: &mut Scope<'_>) -> Value {
    scope.function("item", 1, crate::collection::array_item)
}

pub(crate) fn array_named_item_function(scope: &mut Scope<'_>) -> Value {
    scope.function("namedItem", 1, crate::collection::array_named_item)
}

pub(crate) fn named_map_item_function(scope: &mut Scope<'_>) -> Value {
    scope.function("item", 1, nsr_namedmap_item)
}

pub(crate) fn make_live(
    scope: &mut Scope<'_>,
    owner: &Value,
    kind: c_int,
    param: Option<&[u8]>,
) -> JsResult {
    let param = param.map(|p| crate::until_nul(p.to_vec()));
    Ok(nsr_make_live(scope, owner, kind, param.as_deref(), None))
}

pub(crate) fn live_snapshot(scope: &mut Scope<'_>, live: &Value) -> Value {
    nsr_live_snapshot(scope, live)
}

pub(crate) fn live_owner(_scope: &mut Scope<'_>, live: &Value) -> Value {
    nsr_live_owner(live)
}

pub(crate) fn qcache_get(scope: &mut Scope<'_>, root: Option<Element>, kind: u8) -> Value {
    nsr_qcache_get(scope, Node::ptr_or_null(root).cast(), kind, b"")
}

pub(crate) fn qcache_put(scope: &mut Scope<'_>, root: Option<Element>, kind: u8, value: &Value) {
    nsr_qcache_put(scope, Node::ptr_or_null(root).cast(), kind, b"", value);
}

pub(crate) struct FileBytes {
    pub contents: Vec<u8>,
    pub mime: Option<String>,
}

pub(crate) fn read_file(path: &CStr) -> Option<FileBytes> {
    let bytes = southstar_glib::file_get_contents(path.to_bytes())?;
    let content_type = unsafe {
        g_content_type_guess(
            path.as_ptr(),
            bytes.as_ptr(),
            bytes.len().min(4096),
            ptr::null_mut(),
        )
    };
    let mime = if content_type.is_null() {
        None
    } else {
        let mime = unsafe { g_content_type_get_mime_type(content_type) };
        let text = (!mime.is_null()).then(|| {
            unsafe { CStr::from_ptr(mime) }
                .to_string_lossy()
                .into_owned()
        });
        unsafe {
            glib::g_free(mime.cast());
            glib::g_free(content_type.cast());
        }
        text
    };
    Some(FileBytes {
        contents: bytes,
        mime,
    })
}

pub(crate) fn is_form_data(_scope: &mut Scope<'_>, value: &Value) -> bool {
    host::is(value, HostKind::FormData)
}

pub(crate) fn construct_form_data(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    host::construct(scope, new_target, HostKind::FormData)
}

pub(crate) fn blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Vec<u8> {
    nsr_blob_bytes(scope, blob)
}

pub(crate) fn multipart_boundary() -> Vec<u8> {
    southstar_net::multipart_boundary()
}

pub(crate) fn with_pattern_scope<R>(f: impl FnOnce(&mut Scope<'_>) -> Option<R>) -> Option<R> {
    api::pattern_context().with(f).flatten()
}

pub(crate) fn regex_matches(pattern: &[u8], value: &[u8]) -> Option<bool> {
    let pattern = cstring(pattern);
    let value = cstring(value);
    let mut error = ptr::null_mut();
    let regex = unsafe { g_regex_new(pattern.as_ptr(), 0, 0, &mut error) };
    if regex.is_null() {
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        return None;
    }
    let matched = unsafe { g_regex_match(regex, value.as_ptr(), 0, ptr::null_mut()) != 0 };
    unsafe { g_regex_unref(regex) };
    Some(matched)
}

pub(crate) fn url_is_valid_absolute(url: &[u8]) -> bool {
    southstar_url::bytes::is_valid_absolute(&crate::until_nul(url.to_vec()))
}

pub(crate) fn sandbox_blocks_forms(node: Element) -> bool {
    southstar_js_frames::sandbox_blocks_forms(node)
}

pub(crate) fn set_attr(node: Element, name: &CStr, value: &[u8]) {
    let value = cstring(value);
    southstar_dom::attrs::set_attribute(node, name, Some(value.to_bytes()));
}

pub(crate) fn remove_attr(node: Element, name: &CStr) {
    southstar_dom::attrs::remove_attribute(node, name);
}

pub(crate) fn append_text(parent: Element, text: &[u8]) {
    let text = cstring(text);
    parent.append(southstar_dom::node::new_text(
        southstar_dom::Kind::Text,
        text.to_bytes(),
    ));
}

pub(crate) fn event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    nsr_event_ctor(scope, this, args)
}

impl Page {
    pub(crate) fn of(scope: &Scope<'_>) -> Option<Page> {
        let js = scope.context_opaque().cast::<NsJs>();
        (!js.is_null()).then_some(Page(js))
    }

    fn raw(page: Option<Page>) -> *mut NsJs {
        page.map_or(ptr::null_mut(), |p| p.0)
    }

    pub(crate) fn current_document(self) -> Option<Element> {
        api::current_document(self.0)
    }

    pub(crate) fn node_in_page(self, node: Element) -> bool {
        api::node_in_page(self.0, Some(node))
    }

    pub(crate) fn log_line(self, line: &CStr) {
        api::log_line(self.0, line);
    }

    pub(crate) fn submit_form(self, form: Element, submitter: Option<Element>) {
        api::submit_form(self.0, form, submitter);
    }

    pub(crate) fn close_dialog(page: Option<Page>, dialog: Element, return_value: Option<&CStr>) {
        (api::top_layer_hooks().dialog_close)(Page::raw(page), dialog, return_value);
    }

    pub(crate) fn set_attr_recorded(page: Option<Page>, node: Element, name: &CStr, value: &[u8]) {
        let value = cstring(value);
        southstar_js_mutation::set_attr_recorded(Page::raw(page), node, name, value.to_bytes());
    }

    pub(crate) fn set_attr_recorded_len(
        page: Option<Page>,
        node: Element,
        name: &CStr,
        value: &[u8],
    ) {
        southstar_js_mutation::set_attr_recorded(Page::raw(page), node, name, value);
    }

    pub(crate) fn remove_attr_recorded(page: Option<Page>, node: Element, name: &CStr) {
        southstar_js_mutation::remove_attr_recorded(Page::raw(page), node, name);
    }

    pub(crate) fn has_transient_activation(self) -> bool {
        api::has_transient_activation(self.0)
    }

    pub(crate) fn consume_user_activation(self) {
        api::consume_user_activation(self.0);
    }

    pub(crate) fn halted(self) -> bool {
        api::halted(self.0)
    }

    pub(crate) fn in_pump(self) -> bool {
        api::in_pump(self.0)
    }

    pub(crate) fn dispatch_select(self, scope: &mut Scope<'_>, target: Element) {
        let event = southstar_js_runtime::events::make_event(scope, "select", Some(target));
        let _ = scope.set(&event, "cancelable", Value::boolean(false));
        southstar_js_runtime::events::dispatch_built_event(self.0, target, "select", event);
    }

    pub(crate) fn mark_mutated(page: Option<Page>) {
        api::mark_mutated(Page::raw(page));
    }

    pub(crate) fn clear_children(page: Option<Page>, node: Element) {
        southstar_js_mutation::clear_children(Page::raw(page), node);
    }

    pub(crate) fn dispatch_event(self, target: Element, kind: &CStr) -> bool {
        southstar_js_runtime::events::dispatch_event(self.0, target, &kind.to_string_lossy()).1
    }

    pub(crate) fn dispatch_submit(self, form: Element, submitter: Option<Element>) -> (bool, bool) {
        if api::events_suspended(self.0) {
            return (false, false);
        }
        let main = api::main_context(self.0);
        let event = main.with(|scope| {
            let event = southstar_js_runtime::events::make_event(scope, "submit", Some(form));
            let submitter = wrap(scope, submitter);
            let _ = scope.set(&event, "submitter", submitter);
            event
        });
        let Some(event) = event else {
            return (false, false);
        };
        southstar_js_runtime::events::dispatch_built_event(self.0, form, "submit", event)
    }
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value) -> JsResult {
                $f(scope, this, &[])
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
                $f(scope, this, core::slice::from_ref(value)).map(drop)
            }
        )*
    };
}

methods! {
    nsr_window_form_data_ctor => crate::form_data::construct,
    nsr_custom_state_set_ctor => crate::internals::custom_state_set,
    nsr_submit_event_ctor => crate::submit::submit_event_ctor,
    nsr_radio_node_list_get_value => crate::collection::radio_list_value,
    nsr_radio_node_list_set_value => crate::collection::set_radio_list_value,
}

getters! {}

setters! {}

pub fn nsr_net_install_form_data(scope: &mut Scope<'_>, global: &Value) {
    crate::form_data::install(scope, global);
}

pub fn nsr_js_form_data_serialize(scope: &mut Scope<'_>, fd: &Value) -> (Vec<u8>, Vec<u8>) {
    crate::form_data::serialize(scope, fd)
}

pub fn nsr_form_elements_named_lookup(
    scope: &mut Scope<'_>,
    elements: &Value,
    name: &[u8],
) -> JsResult {
    crate::collection::named_lookup(scope, elements, name)
}

pub fn nsr_form_indexed_control(scope: &mut Scope<'_>, form: &Value, index: u32) -> JsResult {
    crate::collection::indexed_control(scope, form, index)
}

pub fn nsr_form_named_control(scope: &mut Scope<'_>, form: &Value, name: &[u8]) -> JsResult {
    crate::collection::named_control(scope, form, name)
}

pub fn nsr_form_controls_snapshot(scope: &mut Scope<'_>, form: &Value) -> JsResult {
    crate::collection::controls_snapshot(scope, form)
}

fn page_of(js: *mut NsJs) -> Option<Page> {
    (!js.is_null()).then_some(Page(js))
}

pub fn is_submit_trigger(el: Node<'_>) -> bool {
    live(el).is_some_and(crate::submit::is_submit_trigger)
}

pub fn is_reset_trigger(el: Node<'_>) -> bool {
    live(el).is_some_and(crate::submit::is_reset_trigger)
}

pub fn resanitize_value(el: Node<'_>) {
    if let Some(el) = live(el) {
        crate::value::resanitize(el);
    }
}

pub fn is_labelable(n: Node<'_>) -> bool {
    live(n).is_some_and(crate::labels::is_labelable)
}

pub fn first_labelable_descendant(n: Node<'_>) -> Option<Element> {
    live(n).and_then(|n| crate::labels::first_labelable_descendant(n, 0))
}

pub fn label_associated_control(label: Node<'_>) -> Option<Element> {
    live(label).and_then(crate::labels::associated_control)
}

pub fn checkable_input_kind(el: Node<'_>) -> i32 {
    live(el).map_or(0, crate::checkable::kind)
}

pub fn checkable_pre_click(js: *const NsJs, el: Node<'_>, kind: i32) -> ClickState {
    live(el).map_or_else(ClickState::default, |el| {
        crate::checkable::pre_click(page_of(js.cast_mut()), el, kind)
    })
}

pub fn checkable_post_click(
    js: *const NsJs,
    el: Node<'_>,
    kind: i32,
    state: &ClickState,
    prevented: bool,
) {
    if let Some(el) = live(el) {
        crate::checkable::post_click(page_of(js.cast_mut()), el, kind, state, prevented);
    }
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn dispatch_submit_event(
    js: *mut c_void,
    form: Node<'_>,
    submitter: Option<Node<'_>>,
) -> (bool, bool) {
    let (Some(page), Some(form)) = (page_of(js.cast()), live(form)) else {
        return (false, false);
    };
    page.dispatch_submit(form, submitter.and_then(live))
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn click_activate(js: *mut c_void, n: Node<'_>) -> bool {
    let (Some(page), Some(n)) = (page_of(js.cast()), live(n)) else {
        return false;
    };
    crate::checkable::click_activate(page, n)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn select_choose_option(js: *mut c_void, option: Node<'_>) -> bool {
    let (Some(page), Some(option)) = (page_of(js.cast()), live(option)) else {
        return false;
    };
    crate::select::choose_option(page, option)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn select_toggle_option(js: *mut c_void, option: Node<'_>) -> bool {
    let (Some(page), Some(option)) = (page_of(js.cast()), live(option)) else {
        return false;
    };
    crate::select::toggle_option(page, option)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn select_step(js: *mut c_void, select: Node<'_>, dir: c_int) -> bool {
    let (Some(page), Some(select)) = (page_of(js.cast()), live(select)) else {
        return false;
    };
    crate::select::step(page, select, dir)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn select_edge(js: *mut c_void, select: Node<'_>, last: bool) -> bool {
    let (Some(page), Some(select)) = (page_of(js.cast()), live(select)) else {
        return false;
    };
    crate::select::edge(page, select, last)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn select_typeahead(js: *mut c_void, select: Node<'_>, key: &[u8]) -> bool {
    let (Some(page), Some(select)) = (page_of(js.cast()), live(select)) else {
        return false;
    };
    crate::select::typeahead(page, select, key)
}

pub fn request_submit_form(scope: &mut Scope<'_>, form: Node<'_>, submitter: Option<Node<'_>>) {
    crate::submit::request_submit(
        scope,
        form.downgrade().get(),
        submitter.and_then(|submitter| submitter.downgrade().get()),
    );
}

pub fn reset_form(scope: &mut Scope<'_>, form: Node<'_>) {
    crate::submit::reset_form(scope, form.downgrade().get());
}

pub(crate) fn listed_controls(form: Node<'_>, include_image: bool) -> Vec<Node<'static>> {
    form.downgrade()
        .get()
        .map_or_else(Vec::new, |form| crate::listed_controls(form, include_image))
}

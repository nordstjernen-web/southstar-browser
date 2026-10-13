//! Southstar — the forms bindings' exports (C for node-only calls, Rust ABI for natives) and the DOM and engine calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;
#[cfg(feature = "quickjs")]
mod legacy;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GPtrArray};
use southstar_js_bind::host::{self, HostKind};
use southstar_js_engine::{ContextHandle, Scope, Value};

use self::imports::*;
use crate::{Element, JsResult};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Page(*mut NsJs);

unsafe extern "C" {
    fn ns_multipart_boundary() -> *mut c_char;
    fn ns_js_pattern_context() -> ContextHandle;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_has_transient_activation(js: *mut NsJs) -> GBoolean;
    fn ns_js_consume_user_activation(js: *mut NsJs);
    fn ns_js_main_context(js: *const NsJs) -> ContextHandle;
    fn ns_js_events_suspended(js: *const NsJs) -> GBoolean;
    fn ns_js_node_in_page(js: *mut NsJs, node: *const NsNode) -> GBoolean;
    fn ns_node_sandbox_blocks_forms(node: *const NsNode) -> GBoolean;
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_submit_form(js: *mut NsJs, form: *const NsNode, submitter: *const NsNode);
    fn ns_js_dialog_close(js: *mut NsJs, dialog: *mut NsNode, return_value: *const c_char);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_clear_children(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_node_new_text(text: *mut c_char) -> *mut NsNode;
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_url_is_valid_absolute(url: *const c_char) -> GBoolean;
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
    fn ns_js_set_attr_recorded_len(
        js: *mut NsJs,
        n: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
        len: isize,
    );
    fn ns_js_remove_attr_recorded(js: *mut NsJs, n: *mut NsNode, name: *const c_char);
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_css_mark_restyle_dirty(parent: *mut NsNode);
    fn ns_css_parse_color(
        s: *const c_char,
        r: *mut u8,
        g: *mut u8,
        b: *mut u8,
        a: *mut u8,
    ) -> GBoolean;
    fn ns_node_remove(child: *mut NsNode);
    fn ns_element_insert_before_single(
        js: *mut NsJs,
        parent: *mut NsNode,
        newc: *mut NsNode,
        reference: *mut NsNode,
    );
    fn g_file_get_contents(
        filename: *const c_char,
        contents: *mut *mut c_char,
        length: *mut usize,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_content_type_guess(
        filename: *const c_char,
        data: *const u8,
        data_size: usize,
        result_uncertain: *mut GBoolean,
    ) -> *mut c_char;
    fn g_content_type_get_mime_type(content_type: *const c_char) -> *mut c_char;
    fn g_ascii_formatd(
        buffer: *mut c_char,
        buf_len: c_int,
        format: *const c_char,
        d: f64,
    ) -> *mut c_char;
    fn g_utf8_casefold(s: *const c_char, len: isize) -> *mut c_char;
}

#[repr(C)]
pub struct NsCheckableClickState {
    checked: GBoolean,
    indeterminate: GBoolean,
    checked_radio: *mut NsNode,
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes.to_vec())).unwrap_or_default()
}

fn node(ptr: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(ptr) }
}

fn mut_ptr(node: Element) -> *mut NsNode {
    node.as_ptr().cast_mut()
}

pub(crate) fn element(value: &Value) -> Option<Element> {
    node(nsr_unwrap_element(value))
}

pub(crate) fn wrap(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    nsr_make_element(scope, Node::ptr_or_null(node)).unwrap_or_else(|_| Value::undefined())
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
    unsafe { ns_css_mark_restyle_dirty(mut_ptr(node)) };
}

pub(crate) fn parse_color(text: &[u8]) -> Option<(u8, u8, u8, u8)> {
    let text = cstring(text);
    let (mut r, mut g, mut b, mut a) = (0u8, 0u8, 0u8, 0u8);
    let ok = unsafe { ns_css_parse_color(text.as_ptr(), &mut r, &mut g, &mut b, &mut a) };
    (ok != 0).then_some((r, g, b, a))
}

pub(crate) fn format_double(format: &CStr, value: f64) -> Vec<u8> {
    let mut buffer = [0 as c_char; 64];
    unsafe {
        g_ascii_formatd(
            buffer.as_mut_ptr(),
            buffer.len() as c_int,
            format.as_ptr(),
            value,
        );
        CStr::from_ptr(buffer.as_ptr()).to_bytes().to_vec()
    }
}

pub(crate) fn utf8_casefold(text: &[u8]) -> Vec<u8> {
    let text = cstring(text);
    let folded = unsafe { g_utf8_casefold(text.as_ptr(), -1) };
    let bytes = unsafe { glib::bytes(folded) }.unwrap_or_default().to_vec();
    unsafe { glib::g_free(folded.cast()) };
    bytes
}

pub(crate) fn node_remove(node: Element) {
    unsafe { ns_node_remove(mut_ptr(node)) };
}

pub(crate) fn append_child(parent: Element, child: Element) {
    unsafe { ns_node_append_child(mut_ptr(parent), mut_ptr(child)) };
}

pub(crate) fn insert_before_single(
    page: Option<Page>,
    parent: Element,
    child: Element,
    reference: Element,
) {
    unsafe {
        ns_element_insert_before_single(
            Page::raw(page),
            mut_ptr(parent),
            mut_ptr(child),
            mut_ptr(reference),
        )
    };
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
    let mut contents: *mut c_char = ptr::null_mut();
    let mut len: usize = 0;
    let ok =
        unsafe { g_file_get_contents(path.as_ptr(), &mut contents, &mut len, ptr::null_mut()) };
    if ok == 0 || contents.is_null() {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(contents.cast::<u8>(), len) }.to_vec();
    unsafe { glib::g_free(contents.cast()) };
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
    let raw = unsafe { ns_multipart_boundary() };
    let bytes = unsafe { glib::bytes(raw) }.unwrap_or_default().to_vec();
    unsafe { glib::g_free(raw.cast()) };
    bytes
}

pub(crate) fn with_pattern_scope<R>(f: impl FnOnce(&mut Scope<'_>) -> Option<R>) -> Option<R> {
    unsafe { ns_js_pattern_context() }.with(f).flatten()
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
    let url = cstring(url);
    unsafe { ns_url_is_valid_absolute(url.as_ptr()) != 0 }
}

pub(crate) fn sandbox_blocks_forms(node: Element) -> bool {
    unsafe { ns_node_sandbox_blocks_forms(node.as_ptr()) != 0 }
}

pub(crate) fn set_attr(node: Element, name: &CStr, value: &[u8]) {
    let value = cstring(value);
    unsafe { ns_element_set_attr(mut_ptr(node), name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn remove_attr(node: Element, name: &CStr) {
    unsafe { ns_element_remove_attr(mut_ptr(node), name.as_ptr()) };
}

pub(crate) fn append_text(parent: Element, text: &[u8]) {
    let text = cstring(text);
    let child = unsafe { ns_node_new_text(glib::g_strdup(text.as_ptr())) };
    unsafe { ns_node_append_child(mut_ptr(parent), child) };
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
        node(unsafe { ns_js_current_document(self.0) })
    }

    pub(crate) fn node_in_page(self, node: Element) -> bool {
        unsafe { ns_js_node_in_page(self.0, node.as_ptr()) != 0 }
    }

    pub(crate) fn log_line(self, line: &CStr) {
        unsafe { ns_js_log_line(self.0, line.as_ptr()) };
    }

    pub(crate) fn submit_form(self, form: Element, submitter: Option<Element>) {
        unsafe { ns_js_submit_form(self.0, form.as_ptr(), Node::ptr_or_null(submitter)) };
    }

    pub(crate) fn close_dialog(page: Option<Page>, dialog: Element, return_value: Option<&CStr>) {
        let value = return_value.map_or(ptr::null(), CStr::as_ptr);
        unsafe { ns_js_dialog_close(Page::raw(page), mut_ptr(dialog), value) };
    }

    pub(crate) fn set_attr_recorded(page: Option<Page>, node: Element, name: &CStr, value: &[u8]) {
        let value = cstring(value);
        unsafe {
            ns_js_set_attr_recorded_len(
                Page::raw(page),
                mut_ptr(node),
                name.as_ptr(),
                value.as_ptr(),
                -1,
            )
        };
    }

    pub(crate) fn set_attr_recorded_len(
        page: Option<Page>,
        node: Element,
        name: &CStr,
        value: &[u8],
    ) {
        let mut owned = value.to_vec();
        owned.push(0);
        unsafe {
            ns_js_set_attr_recorded_len(
                Page::raw(page),
                mut_ptr(node),
                name.as_ptr(),
                owned.as_ptr().cast(),
                value.len() as isize,
            )
        };
    }

    pub(crate) fn remove_attr_recorded(page: Option<Page>, node: Element, name: &CStr) {
        unsafe { ns_js_remove_attr_recorded(Page::raw(page), mut_ptr(node), name.as_ptr()) };
    }

    pub(crate) fn has_transient_activation(self) -> bool {
        unsafe { ns_js_has_transient_activation(self.0) != 0 }
    }

    pub(crate) fn consume_user_activation(self) {
        unsafe { ns_js_consume_user_activation(self.0) };
    }

    pub(crate) fn halted(self) -> bool {
        unsafe { ns_js_halted(self.0) != 0 }
    }

    pub(crate) fn in_pump(self) -> bool {
        unsafe { ns_js_in_pump(self.0) != 0 }
    }

    pub(crate) fn dispatch_select(self, scope: &mut Scope<'_>, target: Element) {
        let event = unsafe { nsr_make_event(scope, "select", target.as_ptr()) };
        let _ = scope.set(&event, "cancelable", Value::boolean(false));
        unsafe { nsr_js_dispatch_built_event(self.0, target.as_ptr(), "select", event) };
    }

    pub(crate) fn mark_mutated(page: Option<Page>) {
        unsafe { ns_js_mark_mutated(Page::raw(page)) };
    }

    pub(crate) fn clear_children(page: Option<Page>, node: Element) {
        unsafe { ns_js_clear_children(Page::raw(page), mut_ptr(node)) };
    }

    pub(crate) fn dispatch_event(self, target: Element, kind: &CStr) -> bool {
        let mut prevented: GBoolean = 0;
        unsafe { ns_js_dispatch_event(self.0, target.as_ptr(), kind.as_ptr(), &mut prevented) };
        prevented != 0
    }

    pub(crate) fn dispatch_submit(self, form: Element, submitter: Option<Element>) -> (bool, bool) {
        if unsafe { ns_js_events_suspended(self.0) } != 0 {
            return (false, false);
        }
        let main = unsafe { ns_js_main_context(self.0) };
        let event = main.with(|scope| {
            let event = unsafe { nsr_make_event(scope, "submit", form.as_ptr()) };
            let submitter = wrap(scope, submitter);
            let _ = scope.set(&event, "submitter", submitter);
            event
        });
        let Some(event) = event else {
            return (false, false);
        };
        unsafe { nsr_js_dispatch_built_event(self.0, form.as_ptr(), "submit", event) }
    }
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
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
            #[unsafe(no_mangle)]
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
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
                $f(scope, this, core::slice::from_ref(value)).map(drop)
            }
        )*
    };
}

methods! {
    nsr_window_form_data_ctor => crate::form_data::construct,
    nsr_element_attachInternals => crate::internals::attach_internals,
    nsr_custom_state_set_ctor => crate::internals::custom_state_set,
    nsr_element_check_validity => crate::validity::check_validity,
    nsr_element_setCustomValidity => crate::validity::set_custom_validity,
    nsr_element_form_requestSubmit => crate::submit::request_submit_method,
    nsr_element_form_submit => crate::submit::submit_method,
    nsr_element_form_reset => crate::submit::reset_method,
    nsr_submit_event_ctor => crate::submit::submit_event_ctor,
    nsr_input_select => crate::selection::select,
    nsr_input_setSelectionRange => crate::selection::set_selection_range,
    nsr_input_setRangeText => crate::selection::set_range_text,
    nsr_input_stepUp => crate::selection::step_up,
    nsr_input_stepDown => crate::selection::step_down,
    nsr_options_item => crate::select::item,
    nsr_options_namedItem => crate::select::named_item,
    nsr_select_add => crate::select::add,
    nsr_radio_node_list_get_value => crate::collection::radio_list_value,
    nsr_radio_node_list_set_value => crate::collection::set_radio_list_value,
    nsr_element_show_picker => crate::picker::show_picker,
}

getters! {
    nsr_element_get_internals => crate::internals::internals,
    nsr_validity_get_valid => crate::validity::valid,
    nsr_element_get_validity => crate::validity::validity,
    nsr_element_get_validation_message => crate::validity::validation_message,
    nsr_element_get_will_validate => crate::validity::will_validate_getter,
    nsr_element_get_label_control => crate::labels::control,
    nsr_element_get_selection_dir => crate::selection::selection_direction,
    nsr_element_get_default_value => crate::value::default_value,
    nsr_element_get_default_checked => crate::checkable::default_checked,
    nsr_element_get_default_selected => crate::select::default_selected,
    nsr_element_get_selected => crate::select::selected,
    nsr_element_get_value_as_number => crate::value::value_as_number,
    nsr_element_get_value_as_date => crate::value::value_as_date,
    nsr_element_get_checked => crate::checkable::checked,
    nsr_element_get_indeterminate => crate::checkable::indeterminate,
    nsr_element_get_progress_position => crate::value::progress_position,
    nsr_element_get_value_prop => crate::value::get_value,
    nsr_element_get_label_prop => crate::value::label,
    nsr_element_get_selectedIndex => crate::select::selected_index,
    nsr_element_get_options => crate::select::options_getter,
    nsr_element_get_selectedOptions => crate::select::selected_options,
    nsr_element_get_selection_start => crate::selection::selection_start,
    nsr_element_get_selection_end => crate::selection::selection_end,
    nsr_text_control_get_text_length => crate::selection::text_length,
    nsr_input_get_files => crate::files::get_files,
    nsr_element_get_form_elements => crate::collection::form_elements,
}

setters! {
    nsr_element_set_default_value => crate::value::set_default_value,
    nsr_element_set_default_checked => crate::checkable::set_default_checked,
    nsr_element_set_default_selected => crate::select::set_default_selected,
    nsr_element_set_selected => crate::select::set_selected,
    nsr_element_set_value_as_number => crate::value::set_value_as_number,
    nsr_element_set_value_as_date => crate::value::set_value_as_date,
    nsr_element_set_checked => crate::checkable::set_checked,
    nsr_element_set_indeterminate => crate::checkable::set_indeterminate_prop,
    nsr_element_set_value_prop => crate::value::set_value,
    nsr_element_set_label_prop => crate::value::set_label,
    nsr_element_set_selectedIndex => crate::select::set_selected_index,
    nsr_element_set_selection_start => crate::selection::set_selection_start,
    nsr_element_set_selection_end => crate::selection::set_selection_end,
    nsr_element_set_selection_dir => crate::selection::set_selection_direction,
}

#[unsafe(no_mangle)]
pub fn nsr_element_range_number_getter(
    scope: &mut Scope<'_>,
    this: &Value,
    magic: i32,
) -> JsResult {
    crate::value::range_number(scope, this, magic)
}

#[unsafe(no_mangle)]
pub fn nsr_element_range_number_setter(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    magic: i32,
) -> JsResult<()> {
    crate::value::set_range_number(scope, this, value, magic).map(drop)
}

#[unsafe(no_mangle)]
pub fn nsr_net_install_form_data(scope: &mut Scope<'_>, global: &Value) {
    crate::form_data::install(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_js_form_data_serialize(scope: &mut Scope<'_>, fd: &Value) -> (Vec<u8>, Vec<u8>) {
    crate::form_data::serialize(scope, fd)
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_request_submit_form(
    scope: &mut Scope<'_>,
    form: *const NsNode,
    submitter: *const NsNode,
) {
    crate::submit::request_submit(scope, node(form), node(submitter));
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_reset_form(scope: &mut Scope<'_>, form: *mut NsNode) {
    crate::submit::reset_form(scope, node(form));
}

#[unsafe(no_mangle)]
pub fn nsr_form_elements_named_lookup(
    scope: &mut Scope<'_>,
    elements: &Value,
    name: &[u8],
) -> JsResult {
    crate::collection::named_lookup(scope, elements, name)
}

#[unsafe(no_mangle)]
pub fn nsr_form_indexed_control(scope: &mut Scope<'_>, form: &Value, index: u32) -> JsResult {
    crate::collection::indexed_control(scope, form, index)
}

#[unsafe(no_mangle)]
pub fn nsr_form_named_control(scope: &mut Scope<'_>, form: &Value, name: &[u8]) -> JsResult {
    crate::collection::named_control(scope, form, name)
}

#[unsafe(no_mangle)]
pub fn nsr_form_controls_snapshot(scope: &mut Scope<'_>, form: &Value) -> JsResult {
    crate::collection::controls_snapshot(scope, form)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_listed_controls(
    form: *const NsNode,
    include_image: GBoolean,
    out: *mut GPtrArray,
) {
    let (Some(form), false) = (node(form), out.is_null()) else {
        return;
    };
    for control in crate::listed_controls(form, include_image != 0) {
        unsafe { glib::g_ptr_array_add(out, mut_ptr(control).cast()) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_submit_trigger(el: *const NsNode) -> GBoolean {
    glib::boolean(node(el).is_some_and(crate::submit::is_submit_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_reset_trigger(el: *const NsNode) -> GBoolean {
    glib::boolean(node(el).is_some_and(crate::submit::is_reset_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_submit_event(
    js: *mut NsJs,
    form: *const NsNode,
    submitter: *const NsNode,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    if !default_prevented.is_null() {
        unsafe { *default_prevented = 0 };
    }
    let (false, Some(form)) = (js.is_null(), node(form)) else {
        return glib::FALSE;
    };
    let (dispatched, prevented) = Page(js).dispatch_submit(form, node(submitter));
    if !default_prevented.is_null() {
        unsafe { *default_prevented = glib::boolean(prevented) };
    }
    glib::boolean(dispatched)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_resanitize_value(el: *mut NsNode) {
    if let Some(el) = node(el) {
        crate::value::resanitize(el);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_is_labelable(n: *const NsNode) -> GBoolean {
    glib::boolean(node(n).is_some_and(crate::labels::is_labelable))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_first_labelable_descendant(
    n: *const NsNode,
    depth: c_int,
) -> *const NsNode {
    Node::ptr_or_null(node(n).and_then(|n| crate::labels::first_labelable_descendant(n, depth)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_label_associated_control(label: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(node(label).and_then(crate::labels::associated_control))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_checkable_input_kind(el: *const NsNode) -> c_int {
    node(el).map_or(0, crate::checkable::kind)
}

fn page_of(js: *mut NsJs) -> Option<Page> {
    (!js.is_null()).then_some(Page(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_checkable_pre_click(
    js: *mut NsJs,
    el: *mut NsNode,
    kind: c_int,
    state: *mut NsCheckableClickState,
) {
    let (Some(el), false) = (node(el), state.is_null()) else {
        return;
    };
    let result = crate::checkable::pre_click(page_of(js), el, kind);
    unsafe {
        *state = NsCheckableClickState {
            checked: glib::boolean(result.checked),
            indeterminate: glib::boolean(result.indeterminate),
            checked_radio: result.checked_radio.map_or(ptr::null_mut(), mut_ptr),
        }
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_checkable_post_click(
    js: *mut NsJs,
    el: *mut NsNode,
    kind: c_int,
    state: *const NsCheckableClickState,
    prevented: GBoolean,
) {
    let (Some(el), Some(state)) = (node(el), unsafe { state.as_ref() }) else {
        return;
    };
    let state = crate::checkable::ClickState {
        checked: state.checked != 0,
        indeterminate: state.indeterminate != 0,
        checked_radio: node(state.checked_radio),
    };
    crate::checkable::post_click(page_of(js), el, kind, &state, prevented != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_checkedness(js: *mut NsJs, n: *mut NsNode, checked: GBoolean) {
    if let Some(n) = node(n) {
        crate::checkable::set_checkedness(page_of(js), n, checked != 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_click_activate(js: *mut NsJs, n: *const NsNode) -> GBoolean {
    let (Some(page), Some(n)) = (page_of(js), node(n)) else {
        return glib::FALSE;
    };
    glib::boolean(crate::checkable::click_activate(page, n))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_select_choose_option(
    js: *mut NsJs,
    option: *mut NsNode,
) -> GBoolean {
    let (Some(page), Some(option)) = (page_of(js), node(option)) else {
        return glib::FALSE;
    };
    glib::boolean(crate::select::choose_option(page, option))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_select_toggle_option(
    js: *mut NsJs,
    option: *mut NsNode,
) -> GBoolean {
    let (Some(page), Some(option)) = (page_of(js), node(option)) else {
        return glib::FALSE;
    };
    glib::boolean(crate::select::toggle_option(page, option))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_select_step(
    js: *mut NsJs,
    select: *mut NsNode,
    dir: c_int,
) -> GBoolean {
    let (Some(page), Some(select)) = (page_of(js), node(select)) else {
        return glib::FALSE;
    };
    glib::boolean(crate::select::step(page, select, dir))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_select_edge(
    js: *mut NsJs,
    select: *mut NsNode,
    last: GBoolean,
) -> GBoolean {
    let (Some(page), Some(select)) = (page_of(js), node(select)) else {
        return glib::FALSE;
    };
    glib::boolean(crate::select::edge(page, select, last != 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_select_typeahead(
    js: *mut NsJs,
    select: *mut NsNode,
    key: *const c_char,
) -> GBoolean {
    let (Some(page), Some(select), Some(key)) =
        (page_of(js), node(select), unsafe { glib::bytes(key) })
    else {
        return glib::FALSE;
    };
    glib::boolean(crate::select::typeahead(page, select, key))
}

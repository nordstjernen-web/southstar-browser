//! Southstar — the C ABI of the page's scheduling, realm scopes and runtime state, over the typed API binding crates call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GHashTable};
use southstar_js_engine::ContextHandle;

use super::glue::{self, SourceFn};
use super::record;
use super::state::{
    DataFn, DelayFn, DownloadFn, FormSubmitFn, NavigateFn, NodeFn, TextQueryFn, UrlFn,
    ViewportScrollFn,
};
use super::{Js, NsJs};
use crate::api;
use crate::scheduling::{self, RunTasks};

const SOURCE_REMOVE: GBoolean = 0;
const SOURCE_CONTINUE: GBoolean = 1;

fn flag(value: bool) -> GBoolean {
    GBoolean::from(value)
}

fn text<'a>(raw: *const c_char) -> Option<&'a CStr> {
    glue::text(raw)
}

fn node<'a>(raw: *const NsNode) -> Option<Node<'a>> {
    record::node(raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_in_pump(js: *const NsJs) -> GBoolean {
    flag(api::in_pump(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_source_remove(js: *mut NsJs, id: c_uint) {
    api::source_remove(js, id);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_attach_timeout(
    js: *mut NsJs,
    ms: c_uint,
    func: SourceFn,
    data: *mut c_void,
) -> c_uint {
    api::attach_timeout(js, ms, func, data)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_attach_idle(
    js: *mut NsJs,
    func: SourceFn,
    data: *mut c_void,
) -> c_uint {
    api::attach_idle(js, func, data)
}

pub(crate) unsafe extern "C" fn run_message_tasks(data: *mut c_void) -> GBoolean {
    let js = Js(data.cast());
    match scheduling::run_message_tasks(js) {
        RunTasks::Done => SOURCE_REMOVE,
        RunTasks::Continue => SOURCE_CONTINUE,
        RunTasks::Retry => {
            let source = glue::attach_timeout(Some(js), 4, run_message_tasks, data);
            js.st().message_task_source.set(source);
            SOURCE_REMOVE
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_in_page(js: *mut NsJs, n: *const NsNode) -> GBoolean {
    flag(api::node_in_page(js, node(n)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_drain_microtasks(js: *mut NsJs) {
    api::drain_microtasks(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_observer_schedule_tick(js: *mut NsJs) {
    api::observer_schedule_tick(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mark_mutated(js: *mut NsJs) {
    api::mark_mutated(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_consume_mutated(js: *mut NsJs) -> GBoolean {
    flag(api::consume_mutated(js))
}

pub(crate) fn snapshot_globals(js: Js) -> *mut GHashTable {
    let names = js
        .scope(|scope| {
            let global = scope.global();
            let keys = scope.own_property_keys(&global, false).unwrap_or_default();
            keys.iter()
                .filter_map(|key| scope.to_bytes(key).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    glue::names_table(names)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_style_table(js: *mut NsJs, styles: *mut GHashTable) {
    if let Some(js) = Js::of(js) {
        js.st().style_table.set(styles);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_style_table(js: *const NsJs) -> *mut GHashTable {
    api::style_table(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_anim(js: *mut NsJs, anim: *mut c_void) {
    if let Some(js) = Js::of(js) {
        js.st().anim.set(anim);
        glue::raf_set_host_driven(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_anim(js: *const NsJs) -> *mut c_void {
    Js::of(js).map_or(ptr::null_mut(), |js| js.st().anim.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_image_cache(js: *mut NsJs, cache: *mut c_void) {
    if let Some(js) = Js::of(js) {
        js.st().image_cache.set(cache);
        glue::blob_registry(js, !cache.is_null());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_request_repaint(js: *mut NsJs) {
    api::request_repaint(js);
}

macro_rules! hook_setter {
    ($name:ident, $field:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(js: *mut NsJs, cb: Option<$ty>, user_data: *mut c_void) {
            if let Some(js) = Js::of(js) {
                js.st().$field.set(cb, user_data);
            }
        }
    };
}

hook_setter!(ns_js_set_form_submit_cb, form_submit, FormSubmitFn);
hook_setter!(ns_js_set_download_cb, download, DownloadFn);
hook_setter!(ns_js_set_clipboard_write_cb, clipboard_write, TextQueryFn);
hook_setter!(ns_js_set_selection_cmd_cb, selection_cmd, TextQueryFn);
hook_setter!(ns_js_set_scroll_to_cb, scroll_to, NodeFn);
hook_setter!(ns_js_set_fragment_nav_cb, fragment_nav, UrlFn);
hook_setter!(ns_js_set_soft_nav_cb, soft_nav, NavigateFn);
hook_setter!(ns_js_set_window_action_cb, window_action, UrlFn);
hook_setter!(
    ns_js_set_viewport_scroll_cb,
    viewport_scroll,
    ViewportScrollFn
);
hook_setter!(ns_js_set_layout_flush_cb, layout_flush, DataFn);
hook_setter!(ns_js_set_load_delay_cb, load_delay, DelayFn);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_selection(
    js: *mut NsJs,
    selection: *const c_char,
    has_range: GBoolean,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    api::set_selection(js, text(selection), has_range != 0, [x, y, w, h]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_selection_state(
    js: *const NsJs,
    text_out: *mut *const c_char,
    rect: *mut f64,
) -> GBoolean {
    let state = Js(js.cast_mut()).st();
    unsafe {
        *text_out = state.selection_text.get();
        for (i, value) in state.selection_rect.get().into_iter().enumerate() {
            *rect.add(i) = value;
        }
    }
    flag(state.selection_has_range.get())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_early_inject_src(js: *mut NsJs, src: *const c_char) {
    if let Some(js) = Js::of(js) {
        let src = text(src).map_or(ptr::null_mut(), |src| glib::strdup(src.to_bytes()));
        super::state::PageState::replace_owned(&js.st().early_inject_src, src);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_layout(js: *mut NsJs) {
    api::flush_layout(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_style(js: *mut NsJs) {
    api::flush_layout(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_url(js: *const NsJs) -> *const c_char {
    let url = api::page_url_ptr(js);
    if url.is_null() { c"".as_ptr() } else { url }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_submit_form(
    js: *mut NsJs,
    form: *const NsNode,
    submitter: *const NsNode,
) {
    if let Some(form) = node(form) {
        api::submit_form(js, form, node(submitter));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_realm(js: *const NsJs) -> ContextHandle {
    api::main_realm(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_main_context(js: *const NsJs) -> ContextHandle {
    api::main_context(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_is_worker(js: *const NsJs) -> GBoolean {
    flag(api::is_worker(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_current_document(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(api::current_document(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_log_enabled(js: *const NsJs) -> GBoolean {
    flag(api::log_enabled(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_log_line(js: *mut NsJs, line: *const c_char) {
    if let Some(line) = text(line) {
        api::log_line(js, line);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_img_src_layout_neutral(n: *const NsNode) -> GBoolean {
    flag(node(n).is_some_and(api::img_src_layout_neutral))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_src_recorded(
    js: *mut NsJs,
    n: *mut NsNode,
    s: *const c_char,
    slen: usize,
) {
    if let Some(el) = node(n) {
        api::set_src_recorded(js, el, unsafe { glib::slice(s.cast(), slen) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_in_template_content(n: *const NsNode) -> GBoolean {
    flag(node(n).is_some_and(api::in_template_content))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_media_native_formats() -> c_uint {
    api::media_native_formats()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_subtree_rafs(js: *mut NsJs, root: *mut NsNode) {
    if let Some(root) = node(root) {
        api::purge_subtree_rafs(js, root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_insert_before_single(
    js: *mut NsJs,
    parent: *mut NsNode,
    newc: *mut NsNode,
    reference: *mut NsNode,
) {
    if let (Some(_), Some(child), Some(reference)) = (node(parent), node(newc), node(reference)) {
        api::insert_before_single(js, child, reference);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_layout_root(js: *mut NsJs, root: *const c_void) {
    api::set_layout_root(js, root);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_form_reset(js: *mut NsJs, form: *mut NsNode) {
    if let Some(form) = node(form) {
        api::form_reset(js, form);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_doc_base(js: *mut NsJs, el: *const NsNode) -> *const c_char {
    if let Some(url) = node(el).and_then(record::node_frame_url) {
        return url.as_ptr();
    }
    api::page_url_ptr(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(api::ce_main_doc(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_wrapper_pinned(js: *const NsJs, n: *const NsNode) -> GBoolean {
    flag(node(n).is_some_and(|n| api::wrapper_pinned(js, n)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_attr_changed(
    js: *mut NsJs,
    n: *mut NsNode,
    attr_name: *const c_char,
    old_value: *const c_char,
    new_value: *const c_char,
) {
    if let (Some(el), Some(name)) = (node(n), text(attr_name)) {
        api::ce_attr_changed(js, el, name, text(old_value), text(new_value));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_add_csp_header(js: *mut NsJs, header_value: *const c_char) {
    api::add_csp_header(js, text(header_value));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_csp_form_action_allowed(
    js: *const NsJs,
    action_url: *const c_char,
) -> GBoolean {
    flag(api::csp_form_action_allowed(js, text(action_url)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_storage_partition(js: *const NsJs) -> *const c_char {
    record::partition_key_ptr(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_us(js: *const NsJs) -> i64 {
    api::time_origin_us(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_page_time_origin_real_ms(js: *const NsJs) -> f64 {
    api::time_origin_real_ms(js)
}

#[unsafe(no_mangle)]
pub fn nsr_make_element(
    scope: &mut southstar_js_engine::Scope<'_>,
    node: *const NsNode,
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    crate::api::make_element(scope, node)
}

#[unsafe(no_mangle)]
pub fn nsr_unwrap_element(value: &southstar_js_engine::Value) -> *const NsNode {
    Node::ptr_or_null(crate::api::unwrap_element(value))
}

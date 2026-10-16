//! Southstar — the C ABI of the page's scheduling, realm scopes and runtime state, over the typed API binding crates call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable};

use super::glue;
use super::record;
use super::state::{DataFn, FormSubmitFn};
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
    api::set_style_table(js, styles);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_style_table(js: *const NsJs) -> *mut GHashTable {
    api::style_table(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_anim(js: *mut NsJs, anim: *mut c_void) {
    api::set_anim(js, anim);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_anim(js: *const NsJs) -> *mut c_void {
    api::anim(js)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_image_cache(js: *mut NsJs, cache: *mut c_void) {
    api::set_image_cache(js, cache);
}

macro_rules! hook_setter {
    ($name:ident, $setter:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(js: *mut NsJs, cb: Option<$ty>, user_data: *mut c_void) {
            api::$setter(js, cb, user_data);
        }
    };
}

hook_setter!(ns_js_set_form_submit_cb, set_form_submit_hook, FormSubmitFn);
hook_setter!(ns_js_set_layout_flush_cb, set_layout_flush_hook, DataFn);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_early_inject_src(js: *mut NsJs, src: *const c_char) {
    api::set_early_inject_src(js, text(src));
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
pub unsafe extern "C" fn ns_js_set_layout_root(js: *mut NsJs, root: *const c_void) {
    api::set_layout_root(js, root);
}

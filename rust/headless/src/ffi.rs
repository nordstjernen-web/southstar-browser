//! Southstar — the C ABI of the headless driver, the debug-log listener and console setup, and the main-loop runs of the driver.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod css;
mod dom_api;
mod engine;
mod js;
mod rproc;
mod stdio;
mod util;

use core::cell::Cell;
use core::ffi::c_int;
#[cfg(windows)]
use core::ffi::{c_uint, c_void};
use core::ops::ControlFlow;

use southstar_glib as glib;
use southstar_layout::{BoxRef, NsBox};

pub use css::{Selectors, StyleRef, StyleTable, Value, box_dom, box_style, hit_test};
pub use dom_api::{
    c_atoi, c_atol, details_fragment_needs_open, editable_value, effectively_disabled, find_by_id,
    first_element, flatten_editable, form_owner, fragment_target, hidden_until_found,
    image_map_resolve, input_is_checked, is_contenteditable_host, is_editable, is_named,
    is_numeric_input, length_limits_apply, numeric_filter_insert, remove_attr, root, set_attr,
    set_editable_value, set_submission_charset, utf8_strlen,
};
pub use engine::{
    Anim, CssCache, Document, ImageCache, PrintSetup, Relayout, Response, Timing, Video,
    VideoCache, anim_observe, apply_render_page_rule, collect_videos, compute_cascade,
    css_set_active_node, css_set_doc_language, css_set_print_media, css_set_target_fragment,
    css_set_viewport, decode_body, dump_layout, dump_text, error_page, fetch, fetch_images,
    free_layout, hit_form_dom, hit_inline_dom, hit_link, image_document, json_document,
    load_keyframes, navigate, node_dump, paint_set_anim, paint_set_js, print_setup_default,
    relayout, suffix_before_ext, url_resolve, video_sources, write_pdf, write_pdf_paged, write_png,
    xml_document,
};
pub use js::{
    DragSession, Js, NavigationTiming, bind_video_events, consume_mutated, eval_source, js_page,
    new_js, note_pointer_input, note_viewport_scroll,
};
pub use rproc::{encode_png, frame_size};
pub use stdio::{
    err, flush, out, scan_drag, scan_hold, scan_pair, scan_point, scan_select, scan_size,
};
pub use util::{
    MainLoop, ascii_strtoll, dlog_level_name, file_contents, monotonic_us, strcompress, usleep,
};

use crate::inproc::{Settle, WptWait};

#[cfg(windows)]
unsafe extern "system" {
    fn SetConsoleOutputCP(code_page: c_uint) -> c_int;
}

#[cfg(windows)]
unsafe extern "C" {
    fn __acrt_iob_func(index: c_uint) -> *mut c_void;
    fn _fileno(stream: *mut c_void) -> c_int;
    fn _setmode(fd: c_int, mode: c_int) -> c_int;
}

#[cfg(windows)]
pub fn console_setup() {
    const CP_UTF8: c_uint = 65001;
    const O_BINARY: c_int = 0x8000;
    unsafe {
        SetConsoleOutputCP(CP_UTF8);
        _setmode(_fileno(__acrt_iob_func(1)), O_BINARY);
        _setmode(_fileno(__acrt_iob_func(2)), O_BINARY);
    }
}

#[cfg(not(windows))]
pub fn console_setup() {}

pub fn box_from_ptr<'a>(p: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(p) }
}

pub fn javascript_enabled() -> bool {
    southstar_config::get().is_none_or(|cfg| cfg.javascript_enabled != 0)
}

fn on_dlog(mask: u32, e: &southstar_debuglog::Entry) {
    let level = e.level as u32;
    if mask & 1u32.checked_shl(level).unwrap_or(0) == 0 {
        return;
    }
    let name = dlog_level_name(level).unwrap_or(b"(null)");
    err(&[b"[", name, b" ", &e.category, b"] ", &e.message, b"\n"].concat());
}

pub fn subscribe_dlog(mask: u32) -> Option<southstar_debuglog::Subscription> {
    (mask != 0).then(|| southstar_debuglog::subscribe(move |e| on_dlog(mask, e)))
}

pub fn run_loop_for(ms: c_int, state: &Settle) {
    let main_loop = glib::MainLoop::new();
    glib::scope(|scope| {
        let quit = main_loop.clone();
        scope.timeout_add_once(ms.max(0).unsigned_abs(), move || quit.quit());
        scope.timeout_add(16, || {
            state.tick();
            ControlFlow::Continue(())
        });
        main_loop.run();
    });
}

pub fn run_wpt_wait(timeout_ms: c_int, wait: &WptWait) -> bool {
    let main_loop = glib::MainLoop::new();
    let done = Cell::new(false);
    glib::scope(|scope| {
        scope.timeout_add(16, || {
            wait.settle.tick();
            ControlFlow::Continue(())
        });
        scope.timeout_add(50, || {
            if !wait.ready() {
                return ControlFlow::Continue(());
            }
            done.set(true);
            main_loop.quit();
            ControlFlow::Break(())
        });
        let quit = main_loop.clone();
        scope.timeout_add_once(timeout_ms.max(0).unsigned_abs(), move || quit.quit());
        main_loop.run();
    });
    done.get()
}

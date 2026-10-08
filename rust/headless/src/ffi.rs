//! Southstar — the C ABI of src/headless.h and the entry points headless.c calls, with the C calls the driver makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod css;
mod rproc;
mod stdio;
mod util;

use core::ffi::{CStr, c_char, c_int, c_uint};

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable};
use southstar_layout::{BoxRef, NsBox};

pub use css::{Selectors, StyleRef, StyleTable, Value, box_dom, box_style, hit_test};
pub use rproc::{Renderer, single_process_enable};
pub use stdio::{err, flush, fmt_g, out, scan_point, scan_select, scan_size};
pub use util::{
    MainLoop, ascii_strtoll, base64, dlog_level_name, file_contents, monotonic_us, strcompress,
    usleep,
};

use crate::{Dump, Opts};

#[repr(C)]
pub struct NsHeadlessOpts {
    url: *const c_char,
    dump: c_uint,
    out_path: *const c_char,
    viewport_width: c_int,
    viewport_height: c_int,
    settle_ms: c_int,
    time_ms: c_int,
    debug_levels: c_uint,
    actions: *const c_char,
    eval: *const c_char,
    inspect: *const c_char,
    inspect_at: *const c_char,
    wpt: GBoolean,
    wpt_timeout_ms: c_int,
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn dump_kind(raw: c_uint) -> Dump {
    match raw {
        0 => Dump::Text,
        1 => Dump::Dom,
        2 => Dump::Layout,
        3 => Dump::Png,
        4 => Dump::Pdf,
        5 => Dump::Print,
        6 => Dump::None,
        _ => Dump::Unknown,
    }
}

unsafe fn opts<'a>(raw: *const NsHeadlessOpts) -> Option<Opts<'a>> {
    let o = unsafe { raw.as_ref() }?;
    Some(Opts {
        url: c_str(o.url),
        dump: dump_kind(o.dump),
        viewport_width: o.viewport_width,
        viewport_height: o.viewport_height,
        settle_ms: o.settle_ms,
        actions: c_str(o.actions),
        eval: c_str(o.eval),
        inspect: c_str(o.inspect),
        inspect_at: c_str(o.inspect_at),
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_headless_debug_mask(spec: *const c_char) -> c_uint {
    crate::debug_mask(c_str(spec))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_headless_run_via_renderer(raw: *const NsHeadlessOpts) -> c_int {
    match unsafe { opts(raw) } {
        Some(o) => crate::renderer::run(&o),
        None => 2,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_headless_inspect_report(
    layout: *const NsBox,
    doc: *const NsNode,
    styles: *mut GHashTable,
    raw: *const NsHeadlessOpts,
) {
    let Some(o) = (unsafe { opts(raw) }) else {
        return;
    };
    let report = crate::inspect::report(
        unsafe { BoxRef::from_ptr(layout) },
        unsafe { Node::from_ptr(doc) },
        unsafe { StyleTable::from_ptr(styles) },
        o.inspect,
        o.inspect_at,
    );
    if let Some(report) = report {
        out(&report);
    }
}

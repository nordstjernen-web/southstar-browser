//! Southstar — the binding crates' entry points the page record calls back into, as typed tables each crate installs once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::sync::OnceLock;

use southstar_dom::Node;
use southstar_js_engine::{NativeFn, Scope, Value};

use super::{NavigationTiming, NsJs, ResourceInfo, Response};

pub type AddResourceTimedFn = unsafe fn(
    *const NsJs,
    Option<&ResourceInfo>,
    &CStr,
    Option<&CStr>,
    (i64, i64),
    *const Response,
);

pub struct PerfHooks {
    pub init: fn(*const NsJs),
    pub teardown: fn(*const NsJs),
    pub clear_frame_clocks: fn(*const NsJs, bool),
    pub coarsen_navigation_timing: fn(&mut NavigationTiming),
    pub make_performance_object: fn(&mut Scope<'_>, *const NsJs, bool) -> Value,
    pub supported_entry_types: fn(&mut Scope<'_>) -> Value,
    pub extend_event_target: fn(&mut Scope<'_>, &Value, &Value),
    pub install_entry_list: fn(&mut Scope<'_>, &Value),
    pub observer_ctor: NativeFn,
    pub observer_observe: NativeFn,
    pub observer_disconnect: NativeFn,
    pub observer_take_records: NativeFn,
    pub add_resource_timed: AddResourceTimedFn,
}

static PERF: OnceLock<PerfHooks> = OnceLock::new();

pub fn set_perf_hooks(hooks: PerfHooks) {
    let _ = PERF.set(hooks);
}

pub(crate) fn perf_hooks() -> &'static PerfHooks {
    PERF.get()
        .expect("the performance hooks are installed before the first page")
}

pub struct ImagesHooks {
    pub start_image_load: fn(*const NsJs, Node<'_>, Option<&CStr>),
    pub forget_node: fn(*const NsJs, Node<'_>),
    pub teardown: fn(*const NsJs),
}

static IMAGES: OnceLock<ImagesHooks> = OnceLock::new();

pub fn set_images_hooks(hooks: ImagesHooks) {
    let _ = IMAGES.set(hooks);
}

pub(crate) fn images_hooks() -> &'static ImagesHooks {
    IMAGES
        .get()
        .expect("the image hooks are installed before the first page")
}

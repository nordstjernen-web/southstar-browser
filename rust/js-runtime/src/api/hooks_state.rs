//! Southstar — the typed tables through which the page record reaches the binding crates above it that keep per-page state: storage, blob URLs, the window, timers, workers, animation frames, media and the per-page caches torn down with the page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};
use southstar_layout::BoxRef;

use super::Page;
use super::hooks_tree::hook_table;

pub type PageFn = fn(Page);

pub struct StorageHooks {
    pub init: PageFn,
    pub teardown: PageFn,
    pub flush: PageFn,
    pub schedule_flush: PageFn,
    pub drain_deferred_events: PageFn,
    pub free_deferred_events: PageFn,
    pub switch_session: fn(Option<Page>, Option<&CStr>, &CStr),
    pub load_local: fn(Option<Page>, Option<&CStr>),
}

impl StorageHooks {
    const NONE: StorageHooks = StorageHooks {
        init: |_| {},
        teardown: |_| {},
        flush: |_| {},
        schedule_flush: |_| {},
        drain_deferred_events: |_| {},
        free_deferred_events: |_| {},
        switch_session: |_, _, _| {},
        load_local: |_, _| {},
    };
}

hook_table!(StorageHooks, STORAGE, set_storage_hooks, storage_hooks);

pub struct UrlHooks {
    pub blob_registry_add: PageFn,
    pub blob_registry_remove: PageFn,
    pub blob_urls_free: PageFn,
    pub file_reader_teardown: PageFn,
    pub teardown: PageFn,
}

impl UrlHooks {
    const NONE: UrlHooks = UrlHooks {
        blob_registry_add: |_| {},
        blob_registry_remove: |_| {},
        blob_urls_free: |_| {},
        file_reader_teardown: |_| {},
        teardown: |_| {},
    };
}

hook_table!(UrlHooks, URL, set_url_hooks, url_hooks);

pub struct WindowHooks {
    pub note_viewport_scroll: fn(Page, f64, f64),
    pub sync_metrics: PageFn,
    pub history_teardown: PageFn,
    pub links_clear: PageFn,
    pub link_outward: fn(Option<Page>, &Value, &Value),
    pub child_frame: fn(Node<'_>, u32, Option<&CStr>) -> Option<Node<'static>>,
    pub fire_page_transition: fn(Option<Page>, &CStr, bool),
    pub install_state: fn(Page, &mut Scope<'_>, &Value),
}

impl WindowHooks {
    const NONE: WindowHooks = WindowHooks {
        note_viewport_scroll: |_, _, _| {},
        sync_metrics: |_| {},
        history_teardown: |_| {},
        links_clear: |_| {},
        link_outward: |_, _, _| {},
        child_frame: |_, _, _| None,
        fire_page_transition: |_, _, _| {},
        install_state: |_, _, _| {},
    };
}

hook_table!(WindowHooks, WINDOW, set_window_hooks, window_hooks);

pub struct ServicesHooks {
    pub chrome_compat: fn() -> bool,
    pub run_due_timers: PageFn,
    pub purge_frame_timers: fn(Option<Page>, Node<'_>),
    pub teardown: PageFn,
}

impl ServicesHooks {
    const NONE: ServicesHooks = ServicesHooks {
        chrome_compat: || false,
        run_due_timers: |_| {},
        purge_frame_timers: |_, _| {},
        teardown: |_| {},
    };
}

hook_table!(ServicesHooks, SERVICES, set_services_hooks, services_hooks);

pub type WorkerHost = std::sync::Arc<dyn core::any::Any + Send + Sync>;
pub type WorkerHostClosingFn = fn(&WorkerHost) -> bool;
pub type WorkerLogFn = fn(&WorkerHost, &CStr);

pub struct ServiceWorkerController(pub Box<dyn core::any::Any>);

pub struct ServiceWorkerFetch<'a> {
    pub id: u32,
    pub url: &'a str,
    pub method: &'a str,
    pub headers: &'a [String],
    pub body: &'a [u8],
}

pub struct WorkersHooks {
    pub host_closing: WorkerHostClosingFn,
    pub log_line: WorkerLogFn,
    pub shape_global: fn(&mut Scope<'_>, bool),
    pub teardown: PageFn,
    pub sw_controller_for: fn(Option<Page>, &str) -> Option<ServiceWorkerController>,
    pub sw_post_fetch: fn(&ServiceWorkerController, &ServiceWorkerFetch<'_>),
    pub host_base_url: fn(Option<Page>) -> Option<CString>,
    pub pending: fn(Option<Page>) -> bool,
}

impl WorkersHooks {
    const NONE: WorkersHooks = WorkersHooks {
        host_closing: |_| false,
        log_line: |_, _| {},
        shape_global: |_, _| {},
        teardown: |_| {},
        sw_controller_for: |_, _| None,
        sw_post_fetch: |_, _| {},
        host_base_url: |_| None,
        pending: |_| false,
    };
}

hook_table!(WorkersHooks, WORKERS, set_workers_hooks, workers_hooks);

pub struct LifecycleHooks {
    pub raf_set_host_driven: PageFn,
    pub purge_frame_rafs: fn(Option<Page>, Node<'_>),
    pub teardown: PageFn,
    pub idle_frame_end: fn(Option<Page>, i64, i64) -> i64,
    pub purge_media_rafs: fn(Option<Page>, Node<'_>),
    pub queue_scrollend: fn(Option<Page>, Node<'_>),
    pub queue_scrollend_document: fn(Option<Page>),
}

impl LifecycleHooks {
    const NONE: LifecycleHooks = LifecycleHooks {
        raf_set_host_driven: |_| {},
        purge_frame_rafs: |_, _| {},
        teardown: |_| {},
        idle_frame_end: |_, _, end| end,
        purge_media_rafs: |_, _| {},
        queue_scrollend: |_, _| {},
        queue_scrollend_document: |_| {},
    };
}

hook_table!(
    LifecycleHooks,
    LIFECYCLE,
    set_lifecycle_hooks,
    lifecycle_hooks
);

pub struct PageStateHooks {
    pub init: PageFn,
    pub teardown: PageFn,
}

impl PageStateHooks {
    const NONE: PageStateHooks = PageStateHooks {
        init: |_| {},
        teardown: |_| {},
    };
}

hook_table!(PageStateHooks, MEDIA, set_media_hooks, media_hooks);
hook_table!(PageStateHooks, FONTS, set_fonts_hooks, fonts_hooks);
hook_table!(PageStateHooks, CANVAS, set_canvas_hooks, canvas_hooks);
hook_table!(PageStateHooks, NET, set_net_hooks, net_hooks);
hook_table!(PageStateHooks, INPUT, set_input_hooks, input_hooks);
hook_table!(
    PageStateHooks,
    COLLECTIONS,
    set_collections_hooks,
    collections_hooks
);
hook_table!(PageStateHooks, CSSOM, set_cssom_hooks, cssom_hooks);

pub struct GeometryHooks {
    pub forget_box_lookup: PageFn,
    pub find_box_by_dom: for<'a, 'b> fn(BoxRef<'a>, Node<'b>) -> Option<BoxRef<'a>>,
}

impl GeometryHooks {
    const NONE: GeometryHooks = GeometryHooks {
        forget_box_lookup: |_| {},
        find_box_by_dom: |_, _| None,
    };
}

hook_table!(GeometryHooks, GEOMETRY, set_geometry_hooks, geometry_hooks);

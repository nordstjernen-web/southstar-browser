//! Southstar — a page's image sessions: starting them after layout, counting what is outstanding, batching arrivals into relayouts and media load events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use crate::ffi::{self, NsBrowser};
use crate::page;

const IMAGE_RELAYOUT_BATCH: u32 = 8;
const WAIT_IMAGES_US: i64 = 15 * 1_000_000;
const DISPATCH_BATCH: u32 = 64;

pub fn fire_media_events(b: &NsBrowser) {
    b.media_events_source.set(0);
    if let (Some(js), Some(layout)) = (b.js(), b.layout()) {
        js.fire_media_load_events(layout);
    }
}

pub fn load_waits_for_images(b: &NsBrowser) -> bool {
    if ffi::monotonic_us() >= b.load_delay_deadline_us.get() {
        return false;
    }
    b.media_events_source.get() != 0
        || b.images_arrived_since_layout.get()
        || images_outstanding(b) > 0
}

pub fn image_arrived(b: &NsBrowser) {
    b.images_arrived_since_layout.set(true);
    b.image_arrivals_since_layout
        .set(b.image_arrivals_since_layout.get().wrapping_add(1));
    if b.image_arrivals_since_layout.get() >= IMAGE_RELAYOUT_BATCH || images_outstanding(b) == 0 {
        b.image_arrivals_since_layout.set(0);
        b.dirty.set(true);
    }
}

pub fn images_outstanding(b: &NsBrowser) -> c_int {
    let sessions = b.sessions();
    if !sessions.exists() {
        return 0;
    }
    let mut total = 0;
    let mut i = 0;
    while i < sessions.len() {
        let session = sessions.get(i);
        let outstanding = session.outstanding();
        if outstanding == 0 {
            session.close();
            sessions.remove_fast(i);
            continue;
        }
        total += outstanding;
        i += 1;
    }
    total
}

pub fn ensure_images(b: &NsBrowser) {
    if b.images_fetched.get() && !b.has_deferred_lazy.get() {
        return;
    }
    b.requested_images();
    b.sessions().ensure();
    let viewport_h = if b.cur_viewport_h.get() > 0.0 {
        b.cur_viewport_h.get()
    } else {
        b.vh.get()
    };
    let (session, deferred) = ffi::start_image_session(b, viewport_h);
    if let Some(session) = session {
        b.sessions().push(session);
    }
    b.images_fetched.set(true);
    b.has_deferred_lazy.set(deferred);
}

pub fn wait_images(b: &NsBrowser) {
    ensure_images(b);
    let deadline = ffi::monotonic_us() + WAIT_IMAGES_US;
    while images_outstanding(b) > 0 && ffi::monotonic_us() < deadline {
        let mut dispatched = 0;
        while ffi::main_context_pending() {
            if dispatched >= DISPATCH_BATCH {
                break;
            }
            dispatched += 1;
            ffi::main_context_iteration();
        }
        if dispatched == 0 {
            ffi::usleep(1000);
        }
    }
    if b.dirty.get() {
        page::relayout(b);
        b.dirty.set(false);
    }
}

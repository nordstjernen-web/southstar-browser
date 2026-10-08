//! Southstar — advancing a page: the settle loop after load, the per-frame tick and whether anything is still animating.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use crate::ffi::{self, NsBrowser};
use crate::{images, page, scroll};

const SETTLE_QUIET_TICKS: c_int = 3;
const HOVER_RELAYOUT_MIN_US: i64 = 60000;
const MAIN_LOOP_BATCH: c_int = 64;
const TICK_GUARD: c_int = 4096;

pub struct SettleState {
    quiet_ticks: c_int,
    deadline_us: i64,
}

impl SettleState {
    pub fn new(deadline_us: i64) -> SettleState {
        SettleState {
            quiet_ticks: 0,
            deadline_us,
        }
    }
}

fn settle_quiet(b: &NsBrowser) -> bool {
    if b.dirty.get() {
        return false;
    }
    if b.anim().is_some_and(ffi::Anim::has_active) {
        return false;
    }
    if b.js().is_some_and(ffi::Js::has_pending_work) {
        return false;
    }
    if b.js().is_some_and(ffi::Js::has_pending_animation_frame) {
        return false;
    }
    if b.images().is_some_and(ffi::Images::has_pending) {
        return false;
    }
    if b.videos().is_some_and(ffi::Videos::has_pending) {
        return false;
    }
    !ffi::main_context_pending()
}

fn note_videos(b: &NsBrowser, now: i64) {
    if let (Some(videos), Some(layout)) = (b.videos(), b.layout()) {
        videos.discover(layout, b.doc(), now);
        if let Some(layout) = b.layout() {
            videos.note_layout(
                layout,
                b.cur_scroll_x.get(),
                b.cur_scroll_y.get(),
                b.cur_scale.get(),
            );
        }
    }
}

pub fn settle_tick(b: &NsBrowser, state: &mut SettleState) -> bool {
    let now = ffi::monotonic_us();
    if now >= state.deadline_us {
        return true;
    }
    if let Some(images) = b.images() {
        images.tick(now);
    }
    if let (Some(videos), Some(_)) = (b.videos(), b.layout()) {
        note_videos(b, now);
        videos.tick(now);
    }
    if let Some(anim) = b.anim() {
        if anim.tick(now) {
            b.cascade_dirty.set(true);
            if anim.needs_layout() {
                b.dirty.set(true);
            }
        }
    }
    if let (Some(anim), Some(js)) = (b.anim(), b.js()) {
        js.dispatch_anim_events(anim);
    }
    if let Some(js) = b.js() {
        js.run_animation_frame();
    }
    if b.js().is_some_and(ffi::Js::consume_mutated) {
        b.dirty.set(true);
    }
    if b.dirty.get() && page::mutation_relayout_due(b) && page::relayout_from_mutation(b) {
        b.dirty.set(false);
    }
    if settle_quiet(b) {
        state.quiet_ticks += 1;
        if state.quiet_ticks >= SETTLE_QUIET_TICKS {
            return true;
        }
    } else {
        state.quiet_ticks = 0;
    }
    false
}

pub fn settle(b: &NsBrowser, settle_ms: c_int) {
    if settle_ms <= 0 {
        return;
    }
    if b.videos().is_some() && b.layout().is_some() {
        note_videos(b, ffi::monotonic_us());
    }
    if settle_quiet(b) {
        return;
    }
    ffi::run_settle_loop(b, settle_ms);
}

fn hover_relayout_gap(b: &NsBrowser) -> i64 {
    (b.relayout_cost_us.get() * 2).max(HOVER_RELAYOUT_MIN_US)
}

fn pump_main_loop(deadline: i64) -> bool {
    let mut did_iter = false;
    let mut it = 0;
    while ffi::main_context_pending() {
        if it >= MAIN_LOOP_BATCH {
            break;
        }
        it += 1;
        if ffi::monotonic_us() >= deadline {
            break;
        }
        ffi::main_context_iteration();
        did_iter = true;
    }
    did_iter
}

pub fn tick(b: &NsBrowser, budget_ms: c_int) -> bool {
    let budget_ms = budget_ms.max(0);
    if b.refresh_due_us.get() != 0
        && !b.pending_nav.is_set()
        && ffi::monotonic_us() >= b.refresh_due_us.get()
    {
        b.refresh_due_us.set(0);
        b.pending_nav.adopt(b.refresh_url.take());
    }
    let deadline = ffi::monotonic_us() + i64::from(budget_ms) * 1000;
    let mut changed = false;
    if let Some(js) = b.js() {
        let (x, y) = (b.cur_scroll_x.get(), b.cur_scroll_y.get());
        if !b.pending_scroll.get()
            && ((x - b.js_scroll_x.get()).abs() > 0.5 || (y - b.js_scroll_y.get()).abs() > 0.5)
        {
            b.js_scroll_x.set(x);
            b.js_scroll_y.set(y);
            js.note_viewport_scroll(x, y);
        }
    }
    if b.hover_restyle_pending.get() {
        let now = ffi::monotonic_us();
        if now - b.hover_relayout_us.get() >= hover_relayout_gap(b) {
            b.hover_restyle_pending.set(false);
            b.hover_relayout_us.set(now);
            page::relayout(b);
            b.dirty.set(false);
            changed = true;
        }
    }
    let mut guard = 0;
    loop {
        let now = ffi::monotonic_us();
        if b.images().is_some_and(|images| images.tick(now)) {
            changed = true;
        }
        if let (Some(videos), Some(_)) = (b.videos(), b.layout()) {
            note_videos(b, now);
            if videos.tick(now) {
                changed = true;
            }
        }
        if let Some(anim) = b.anim() {
            if anim.tick(now) {
                changed = true;
                b.cascade_dirty.set(true);
                if anim.needs_layout() {
                    b.dirty.set(true);
                }
            }
        }
        if let (Some(anim), Some(js)) = (b.anim(), b.js()) {
            js.dispatch_anim_events(anim);
        }
        if b.js().is_some_and(ffi::Js::run_animation_frame) {
            changed = true;
        }
        if !pump_main_loop(deadline) {
            break;
        }
        guard += 1;
        if guard >= TICK_GUARD || ffi::monotonic_us() >= deadline {
            break;
        }
    }
    if b.js().is_some_and(ffi::Js::consume_mutated) {
        b.dirty.set(true);
    }
    if b.dirty.get() && page::mutation_relayout_due(b) && page::relayout_from_mutation(b) {
        changed = true;
        b.dirty.set(false);
        if b.videos().is_some() && b.layout().is_some() {
            note_videos(b, ffi::monotonic_us());
        }
    }
    scroll::follow_scroll_anchor(b);
    if b.pending_scroll.get() {
        changed = true;
    }
    if !changed && b.videos().is_some_and(ffi::Videos::waiting_growth) {
        changed = true;
    }
    changed
}

pub fn animating(b: &NsBrowser) -> bool {
    b.dirty.get()
        || b.hover_restyle_pending.get()
        || b.refresh_due_us.get() != 0
        || b.refresh_url.is_set()
        || images::images_outstanding(b) > 0
        || b.js().is_some_and(ffi::Js::has_pending_animation_frame)
        || b.js().is_some_and(ffi::Js::needs_tick)
        || b.anim().is_some_and(ffi::Anim::has_active)
        || b.images().is_some_and(ffi::Images::animating)
        || b.videos().is_some_and(ffi::Videos::animating)
}

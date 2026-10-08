//! Southstar — fragment targets and scroll requests: revealing a target, queuing the scroll to it, keeping it anchored and the script engine's scroll and navigation hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::Node;
use southstar_glib::GStr;
use southstar_layout::{BoxRef, children};

use crate::ffi::{self, NsBrowser};
use crate::{page, settle};

pub struct Fragment {
    pub present: bool,
    pub text: Option<GStr>,
}

impl Fragment {
    pub fn target(&self) -> Option<&CStr> {
        self.text.as_deref().filter(|f| !f.is_empty())
    }
}

pub fn url_fragment(url: Option<&CStr>) -> Fragment {
    let Some(hash) = url.and_then(|u| u.to_bytes().iter().position(|&c| c == b'#').map(|i| (u, i)))
    else {
        return Fragment {
            present: false,
            text: None,
        };
    };
    let (url, at) = hash;
    let rest = &url.to_bytes_with_nul()[at + 1..];
    let rest = CStr::from_bytes_with_nul(rest).unwrap_or_default();
    let text = ffi::uri_unescape(rest).or_else(|| ffi::dup_text(rest));
    Fragment {
        present: true,
        text,
    }
}

fn reveal_fragment_target(b: &NsBrowser, target: Node<'_>) -> bool {
    let mut changed = false;
    let mut cur = Some(target);
    while let Some(node) = cur {
        if ffi::hidden_until_found(node) {
            ffi::remove_attr(node, c"hidden");
            changed = true;
        }
        if let Some(parent) = node.parent() {
            if ffi::details_fragment_needs_open(parent, node) && parent.attr(c"open").is_none() {
                ffi::set_attr(parent, c"open", c"");
                changed = true;
            }
        }
        if b.doc()
            .is_some_and(|doc| core::ptr::eq(doc.as_ptr(), node.as_ptr()))
        {
            break;
        }
        cur = node.parent();
    }
    changed
}

fn box_for_node<'a>(b: BoxRef<'a>, target: Node<'_>) -> Option<BoxRef<'a>> {
    if core::ptr::eq(b.dom_ptr(), target.as_ptr().cast()) {
        return Some(b);
    }
    children(b)
        .find_map(|c| box_for_node(c, target))
        .or_else(|| {
            b.inline_atomic_boxes()
                .into_iter()
                .find_map(|a| box_for_node(a, target))
        })
}

pub fn target_scroll_y(b: &NsBrowser, target: Node<'_>) -> Option<c_int> {
    let layout = b.layout()?;
    let y = match box_for_node(layout, target) {
        Some(found) => found.y() + found.margin().top,
        None => ffi::inline_rect_y(layout, target)?,
    };
    let y = if 0.0 > y { 0.0 } else { y };
    Some(y.floor() as c_int)
}

pub fn queue_scroll_to(b: &NsBrowser, target: Node<'_>, reveal: bool) {
    if reveal && reveal_fragment_target(b, target) {
        b.dirty.set(true);
    }
    page::flush(b);
    let Some(y) = target_scroll_y(b, target) else {
        return;
    };
    b.pending_scroll_y.set(y);
    b.pending_scroll.set(true);
    if reveal {
        b.scroll_anchor.set(Some(target));
        b.scroll_anchor_y.set(y);
    }
}

pub fn follow_scroll_anchor(b: &NsBrowser) {
    let Some(anchor) = b.scroll_anchor.get() else {
        return;
    };
    if (b.cur_scroll_y.get() - f64::from(b.scroll_anchor_y.get())).abs() > 1.0
        && !b.pending_scroll.get()
    {
        b.scroll_anchor.set(None);
        return;
    }
    if let Some(y) = target_scroll_y(b, anchor) {
        if y != b.scroll_anchor_y.get() {
            b.scroll_anchor_y.set(y);
            b.pending_scroll_y.set(y);
            b.pending_scroll.set(true);
        }
    }
    if !settle::animating(b) && !b.pending_scroll.get() {
        b.scroll_anchor.set(None);
    }
}

fn clamp(v: f64, low: f64, high: f64) -> f64 {
    if v > high {
        high
    } else if v < low {
        low
    } else {
        v
    }
}

fn at_least_zero(v: f64) -> f64 {
    if v > 0.0 { v } else { 0.0 }
}

pub fn js_viewport_scroll(b: &NsBrowser, x: f64, y: f64) -> (f64, f64) {
    page::flush(b);
    let (page_w, page_h) = page::page_size(b).unwrap_or((0, 0));
    let view_h = if b.cur_viewport_h.get() > 0.0 {
        b.cur_viewport_h.get()
    } else {
        b.vh.get()
    };
    let max_x = at_least_zero(f64::from(page_w) - f64::from(b.vw.get()));
    let max_y = at_least_zero(f64::from(page_h) - view_h);
    let x = clamp(x, 0.0, max_x).round();
    let y = clamp(y, 0.0, max_y).round();
    b.pending_scroll_x.set(x as c_int);
    b.pending_scroll_y.set(y as c_int);
    b.pending_scroll.set(true);
    b.js_scroll_x.set(x);
    b.js_scroll_y.set(y);
    b.scroll_anchor.set(None);
    (x, y)
}

pub fn js_scroll_to(b: &NsBrowser, target: Option<Node<'_>>) {
    b.scroll_anchor.set(None);
    if let Some(target) = target {
        queue_scroll_to(b, target, false);
    }
}

pub fn js_soft_navigate(b: &NsBrowser, url: &CStr, replace: bool) {
    if !replace {
        b.soft_nav_pushed.set(true);
    }
    let fragment = url_fragment(Some(url));
    ffi::css_set_target_fragment(if fragment.present {
        fragment.target()
    } else {
        None
    });
    b.base_url.set(Some(url));
    b.dirty.set(true);
}

pub fn js_fragment_navigate(b: &NsBrowser, url: &CStr) {
    let fragment = url_fragment(Some(url));
    if !fragment.present {
        return;
    }
    ffi::css_set_target_fragment(fragment.target());
    b.dirty.set(true);
    b.scroll_anchor.set(None);
    let Some(target_id) = fragment.target() else {
        b.pending_scroll_y.set(0);
        b.pending_scroll.set(true);
        return;
    };
    if let Some(target) = ffi::find_fragment_target(b.doc(), target_id) {
        queue_scroll_to(b, target, true);
    }
}

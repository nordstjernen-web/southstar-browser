//! Southstar — collecting from a laid-out page: the boxes that paint images and videos, and the content-box size of every iframe and frame, kept until the next layout for the frame documents' media queries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::BTreeMap;
use std::sync::Mutex;

use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxRef};

use crate::ffi;

fn with_atomics<'a>(b: BoxRef<'a>, mut visit: impl FnMut(BoxRef<'a>)) {
    for c in southstar_layout::children(b) {
        visit(c);
    }
    for atomic in &b.inline_atomics().unwrap_or_default() {
        if let Some(ab) = atomic.box_ref() {
            visit(ab);
        }
    }
}

pub fn images<'a>(b: BoxRef<'a>, out: &mut Vec<BoxRef<'a>>) {
    if b.kind() == BoxKind::Image {
        out.push(b);
    }
    if b.media().is_some_and(|m| {
        m.bg_image_src().is_some()
            || m.marker_image_src().is_some()
            || m.border_image_src().is_some()
    }) {
        out.push(b);
    }
    with_atomics(b, |c| images(c, out));
}

pub fn videos<'a>(b: BoxRef<'a>, out: &mut Vec<BoxRef<'a>>) {
    if b.kind() == BoxKind::Video {
        out.push(b);
    }
    with_atomics(b, |c| videos(c, out));
}

static FRAME_VIEWPORTS: Mutex<BTreeMap<usize, (f64, f64)>> = Mutex::new(BTreeMap::new());

fn is_frame(n: Node<'_>) -> bool {
    n.is_element()
        && n.name()
            .is_some_and(|name| name == c"iframe" || name == c"frame")
}

fn frames_walk(b: BoxRef<'_>, frames: &mut BTreeMap<usize, (f64, f64)>) {
    if let Some(dom) = ffi::dom_of(b).filter(|n| is_frame(*n)) {
        let w = if b.content_width() > 0.0 {
            b.content_width()
        } else {
            0.0
        };
        let h = if b.content_height() > 0.0 {
            b.content_height()
        } else {
            0.0
        };
        frames.insert(dom.as_ptr() as usize, (w, h));
    }
    for c in southstar_layout::children(b) {
        frames_walk(c, frames);
    }
}

pub fn record_frame_viewports(root: Option<BoxRef<'_>>) {
    let mut frames = FRAME_VIEWPORTS.lock().unwrap_or_else(|e| e.into_inner());
    frames.clear();
    if let Some(root) = root {
        frames_walk(root, &mut frames);
    }
}

pub fn frame_viewport(frame: Node<'_>) -> Option<(f64, f64)> {
    let frames = FRAME_VIEWPORTS.lock().unwrap_or_else(|e| e.into_inner());
    frames.get(&(frame.as_ptr() as usize)).copied()
}

pub fn kind_name(kind: u32) -> &'static core::ffi::CStr {
    match kind {
        0 => c"block",
        1 => c"inline",
        2 => c"text",
        3 => c"image",
        4 => c"table",
        5 => c"caption",
        6 => c"row",
        7 => c"cell",
        8 => c"video",
        9 => c"math",
        10 => c"svg",
        _ => c"?",
    }
}

//! Southstar — the layout entry points the box tree and the formatting-context crates recurse through, installed once by the crate that links them all.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_css::StyleData;

use crate::BoxRef;

pub struct Dispatch {
    pub layout_box: fn(BoxRef<'_>, f64, Option<&StyleData>),
    pub layout_block: fn(BoxRef<'_>, f64, Option<&StyleData>),
    pub shift_box_tree: fn(BoxRef<'_>, f64, f64),
    pub legacy_align_block_child: fn(BoxRef<'_>, f64, f64, Option<&StyleData>),
    pub max_bottom: fn(BoxRef<'_>, f64) -> f64,
    pub clips_out_point: fn(BoxRef<'_>, f64, f64) -> bool,
}

static DISPATCH: OnceLock<Dispatch> = OnceLock::new();

pub fn install(dispatch: Dispatch) {
    let _ = DISPATCH.set(dispatch);
}

pub(crate) fn get() -> &'static Dispatch {
    DISPATCH
        .get()
        .expect("the layout entry points are installed before the first layout")
}

pub fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: Option<&StyleData>) {
    (get().layout_box)(b, parent_content_width, inherited);
}

pub fn layout_block(b: BoxRef<'_>, parent_content_width: f64, inherited: Option<&StyleData>) {
    (get().layout_block)(b, parent_content_width, inherited);
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    (get().shift_box_tree)(b, dx, dy);
}

pub fn legacy_align_block_child(
    c: BoxRef<'_>,
    avail_x: f64,
    avail_w: f64,
    inherited: Option<&StyleData>,
) {
    (get().legacy_align_block_child)(c, avail_x, avail_w, inherited);
}

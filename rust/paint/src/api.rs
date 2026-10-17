//! Southstar — the Rust API a page paints through: whole documents, layer plans and their viewport layers, the canvas colour and the paint state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;

use southstar_anim::AnimHandle;
use southstar_cairo::ContextRef;
use southstar_dom::Node;
use southstar_layout::{BoxRef, NsBox};
use southstar_selection::NsSelection;
use southstar_style::StyleRef;

use crate::document::{self, DocLayers, LayerPlan};
use crate::ffi::pango::Context;
use crate::walk::VpCapture;
use crate::{inline, marker, state, text, three_d};

fn selection_ptr(sel: Option<&NsSelection>) -> *const c_void {
    sel.map_or(ptr::null(), |s| ptr::from_ref(s).cast())
}

pub fn paint(
    cr: ContextRef,
    root: BoxRef<'_>,
    highlight: Option<&CStr>,
    sel: Option<&NsSelection>,
) {
    document::paint_document(cr, root, highlight, selection_ptr(sel));
}

pub fn plan_layers(cr: ContextRef, root: BoxRef<'_>, plan: &mut LayerPlan) {
    document::plan_layers(cr, Some(root), plan);
}

type Upper<'u> = &'u mut dyn FnMut(i32) -> Option<ContextRef<'u>>;

unsafe extern "C" fn upper_trampoline(index: i32, data: *mut c_void) -> *mut c_void {
    // SAFETY: doc_layers passes a pointer to its own Upper closure reference, which outlives the walk that calls back.
    let upper = unsafe { &mut *data.cast::<Upper<'_>>() };
    upper(index).map_or(ptr::null_mut(), |cr| cr.as_raw())
}

pub fn doc_layers(
    cr: ContextRef,
    mut upper: Upper<'_>,
    root: BoxRef<'_>,
    highlight: Option<&CStr>,
    sel: Option<&NsSelection>,
    plan: &LayerPlan,
) -> bool {
    let up = DocLayers {
        upper: Some(upper_trampoline),
        upper_data: (&raw mut upper).cast(),
    };
    document::doc_layers(cr, up, root, highlight, selection_ptr(sel), plan)
}

pub fn vp_layer(
    cr: ContextRef,
    root: BoxRef<'_>,
    layer: &VpCapture,
    vp: (f64, f64),
    highlight: Option<&CStr>,
    sel: Option<&NsSelection>,
) {
    document::vp_layer(cr, root, layer, vp, highlight, selection_ptr(sel));
}

pub fn canvas_color(root: BoxRef<'_>) -> (bool, [f64; 4]) {
    let (found, bg) = document::canvas_color(Some(root));
    (found, [bg.r, bg.g, bg.b, bg.a])
}

pub fn set_search(case_sensitive: bool, active: *const NsBox) {
    state::set_search(case_sensitive, active);
}

pub fn set_caret_visible(visible: bool) {
    state::set_caret_visible(visible);
}

pub fn set_anim(anim: Option<AnimHandle>) {
    state::set_anim(anim);
}

/// # Safety
///
/// `js` is null or a live script engine that stays alive until it is replaced here.
pub unsafe fn set_js(js: *mut c_void) {
    state::set_js(js);
}

pub fn invalidate_3d() {
    three_d::invalidate();
}

impl LayerPlan {
    pub fn new() -> LayerPlan {
        let mut plan = LayerPlan {
            dynamic: 0,
            kinds: ptr::null_mut(),
            vp: ptr::null_mut(),
        };
        document::plan_init(&mut plan);
        plan
    }

    fn captures(&self) -> &[VpCapture] {
        // SAFETY: a plan's layer array is null or the GArray plan_init created, alive until the plan drops.
        let Some(vp) = (unsafe { self.vp.as_ref() }) else {
            return &[];
        };
        if vp.len == 0 || vp.data.is_null() {
            return &[];
        }
        // SAFETY: the planner fills the array with `len` VpCapture records at `data`.
        unsafe { core::slice::from_raw_parts(vp.data.cast::<VpCapture>(), vp.len as usize) }
    }

    pub fn viewport_layers(&self) -> usize {
        self.captures().len()
    }

    pub fn dynamic(&self) -> bool {
        self.dynamic != 0
    }

    pub fn capture(&self, index: usize) -> Option<&VpCapture> {
        self.captures().get(index)
    }
}

impl Default for LayerPlan {
    fn default() -> LayerPlan {
        LayerPlan::new()
    }
}

impl Drop for LayerPlan {
    fn drop(&mut self) {
        document::plan_clear(self);
    }
}

impl VpCapture {
    pub fn kind(&self) -> i32 {
        self.kind
    }

    pub fn box_ref(&self) -> Option<BoxRef<'_>> {
        // SAFETY: captures are recorded by the planner from boxes of the tree the plan was made over, which the page keeps until it plans again.
        unsafe { BoxRef::from_ptr(self.b) }
    }
}

pub fn text_context() -> Context {
    text::context()
}

pub fn pango_font_size(size_px: f64) -> i32 {
    text::pango_font_size(size_px)
}

pub fn css_line_height_px(s: Option<StyleRef<'_>>) -> f64 {
    text::css_line_height_px(s)
}

pub fn normal_line_height_px(s: Option<StyleRef<'_>>) -> f64 {
    text::normal_line_height_px(s)
}

pub fn list_ordinals_begin() {
    marker::ordinals_begin();
}

pub fn list_ordinals_end() {
    marker::ordinals_end();
}

pub fn inside_list_marker(
    li: Node<'_>,
    s: Option<StyleRef<'_>>,
    max_len: usize,
) -> Option<Vec<u8>> {
    if !marker::li_is_inside(s) {
        return None;
    }
    marker::li_marker_text(Some(li), s, max_len)
}

pub fn sync_inline_atomic_offsets(root: BoxRef<'_>) {
    inline::sync_inline_atomic_offsets(root);
}

pub fn inline_xy_to_byte(b: BoxRef<'_>, rel_x: f64, rel_y: f64) -> Option<usize> {
    inline::inline_xy_to_byte(b, rel_x, rel_y)
}

pub fn registered_3d(b: BoxRef<'_>) -> bool {
    three_d::registered(b)
}

pub fn pick_3d<'a>(root3d: BoxRef<'a>, x: f64, y: f64) -> Option<BoxRef<'a>> {
    // SAFETY: pick returns null or a box of root3d's tree, which the caller's borrow keeps alive.
    unsafe { BoxRef::from_ptr(three_d::pick(root3d, x, y)) }
}

//! Southstar — the Rust API a page paints through: whole documents, layer plans and their viewport layers, the canvas colour and the paint state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;
use std::rc::Rc;

use southstar_anim::AnimHandle;
use southstar_cairo::ContextRef;
use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr, NsBox};
use southstar_selection::NsSelection;
use southstar_style::StyleRef;

use crate::document::{self, DocLayers, LayerPlan};
use crate::text_layout::{AttrList, Attribute, Context, Layout};
use crate::walk::VpCapture;
use crate::{inline, marker, state, text, three_d};

pub fn paint(
    cr: ContextRef,
    root: BoxRef<'_>,
    highlight: Option<&CStr>,
    sel: Option<&NsSelection>,
) {
    document::paint_document(cr, root, highlight, sel);
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
    document::doc_layers(cr, up, root, highlight, sel, plan)
}

pub fn vp_layer(
    cr: ContextRef,
    root: BoxRef<'_>,
    layer: &VpCapture,
    vp: (f64, f64),
    highlight: Option<&CStr>,
    sel: Option<&NsSelection>,
) {
    document::vp_layer(cr, root, layer, vp, highlight, sel);
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
        LayerPlan {
            dynamic: false,
            kinds: Rc::default(),
            vp: Vec::new(),
        }
    }

    fn captures(&self) -> &[VpCapture] {
        &self.vp
    }

    pub fn viewport_layers(&self) -> usize {
        self.captures().len()
    }

    pub fn dynamic(&self) -> bool {
        self.dynamic
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

pub fn register_font_oracle() {
    southstar_css::set_font_oracle(southstar_css::FontOracle {
        available: text::font_available,
        generation: text::font_generation,
        metrics: text::font_metrics,
    });
    southstar_css::set_font_alias(text::font_alias);
}

pub fn drop_box_cache(b: BoxRef<'_>) {
    inline::drop_box_cache(b);
}

pub fn inline_word_range(b: BoxRef<'_>, byte: usize) -> Option<(usize, usize)> {
    inline::inline_word_range(b, byte)
}

pub fn inline_range_extents(
    b: BoxRef<'_>,
    start: usize,
    len: usize,
    element: Option<&InlineAttr>,
) -> Option<(f64, f64, f64, f64)> {
    inline::inline_range_extents(b, start, len, element)
}

pub fn wrap_mode_for(s: Option<StyleRef<'_>>) -> i32 {
    text::wrap_mode_for(s)
}

pub fn apply_inline_font(layout: &Layout, s: Option<StyleRef<'_>>) {
    text::apply_inline_font(layout, s);
}

pub fn apply_css_line_spacing(layout: &Layout, s: Option<StyleRef<'_>>) {
    text::apply_css_line_spacing(Some(layout), s);
}

pub fn apply_i18n(layout: &Layout, attrs: &AttrList, b: BoxRef<'_>) {
    text::apply_i18n(Some(layout), Some(attrs), Some(b));
}

pub fn apply_font_features(attrs: &AttrList, s: Option<StyleRef<'_>>) {
    if let Some(s) = s {
        text::apply_font_features(attrs, s, 0, u32::MAX);
    }
}

pub fn start_align_overflow(layout: &Layout) {
    text::start_align_overflow(layout);
}

pub fn font_features_attr(
    kerning: i32,
    ligatures: Option<&CStr>,
    settings: Option<&CStr>,
) -> Option<Attribute> {
    text::font_features_attr(kerning, ligatures, settings)
}

pub fn font_variations_attr(settings: Option<&CStr>) -> Option<Attribute> {
    text::font_variations_attr(settings)
}

pub fn build_inline_layout(b: BoxRef<'_>) -> Option<Layout> {
    inline::build_inline_layout(b)
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

pub fn pick_3d<'a>(
    root3d: BoxRef<'a>,
    x: f64,
    y: f64,
    hit_test: three_d::HitTest,
) -> Option<BoxRef<'a>> {
    // SAFETY: pick returns null or a box of root3d's tree, which the caller's borrow keeps alive.
    unsafe { BoxRef::from_ptr(three_d::pick(root3d, x, y, hit_test)) }
}

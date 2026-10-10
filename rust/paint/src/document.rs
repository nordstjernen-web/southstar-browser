//! Southstar — painting a document: the canvas background, the top layer and its backdrop, selection runs, and the layer planner that splits fixed and sticky subtrees into viewport layers the embedder composites.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;

use southstar_dom::{Kind as NodeKind, Node};
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable};
use southstar_layout::{BoxKind, BoxRef};
use southstar_style::{PropId as P, ValueRef, layer, layer_count};

use crate::ffi::cairo::{Cr, Matrix};
use crate::ffi::engine::{self, Video};
use crate::marker;
use crate::util::{Rgba, get, keyword_is, rgba_of, style_of};
use crate::walk::{
    self, LAYERS_DOC, LAYERS_PLAN, Layers, UpperFn, VpCapture, box_clip_hides, box_is_hidden,
    box_skips_contents, box_z_index, paint_cache_clip, paint_walk,
};

pub const VP_FIXED: i32 = 1;
pub const VP_STICKY: i32 = 2;

#[repr(C)]
pub struct LayerPlan {
    pub dynamic: GBoolean,
    pub kinds: *mut GHashTable,
    pub vp: *mut GArray,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<LayerPlan>() == 24);

unsafe extern "C" {
    fn g_array_set_size(array: *mut GArray, length: u32) -> *mut GArray;
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut u8;
}

fn node_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

fn find_element_box_named<'a>(b: BoxRef<'a>, name: &[u8]) -> Option<BoxRef<'a>> {
    if node_of(b).is_some_and(|n| {
        n.kind() == NodeKind::Element && n.name().is_some_and(|nm| nm.to_bytes() == name)
    }) {
        return Some(b);
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        if let Some(hit) = find_element_box_named(child, name) {
            return Some(hit);
        }
        c = child.next_sibling();
    }
    None
}

fn box_solid_background(b: Option<BoxRef<'_>>) -> Option<Rgba> {
    let v = b.and_then(|b| get(style_of(b), P::BackgroundColor));
    v.and_then(ValueRef::color)
        .filter(|c| c[3] > 0)
        .map(|_| rgba_of(v, Rgba::new(1.0, 1.0, 1.0, 1.0)))
}

fn canvas_background_of(root: BoxRef<'_>) -> Option<Rgba> {
    box_solid_background(find_element_box_named(root, b"html"))
        .or_else(|| box_solid_background(find_element_box_named(root, b"body")))
}

fn find_box_for_node<'a>(b: BoxRef<'a>, node: Node<'_>) -> Option<BoxRef<'a>> {
    if b.dom_ptr() == node.as_ptr().cast() {
        return Some(b);
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        if let Some(hit) = find_box_for_node(child, node) {
            return Some(hit);
        }
        c = child.next_sibling();
    }
    None
}

fn top_layer_box(root: BoxRef<'_>) -> Option<BoxRef<'_>> {
    let modal = engine::active_modal()?;
    find_box_for_node(root, modal)
}

fn paint_top_layer(cr: Cr, root: BoxRef<'_>, highlight: Option<&CStr>) {
    let Some(top) = top_layer_box(root) else {
        return;
    };
    let bd = style_of(top).and_then(|s| s.backdrop());
    if let Some(bg) = bd.and_then(|s| s.get(P::BackgroundColor)) {
        let c = rgba_of(Some(bg), Rgba::default());
        let (x1, y1, x2, y2) = cr.clip_extents();
        cr.save();
        c.set_source(cr);
        cr.rectangle(x1, y1, x2 - x1, y2 - y1);
        cr.fill();
        cr.restore();
    }
    let (saved_skip, saved_flush) = walk::with(|w| {
        (
            w.skip_box.replace(ptr::null()),
            w.flush_box.replace(top.as_ptr()),
        )
    });
    paint_walk(cr, top, highlight);
    walk::with(|w| {
        w.flush_box.set(saved_flush);
        w.skip_box.set(saved_skip);
    });
}

fn begin_selection(root: BoxRef<'_>, sel: *const c_void) {
    if !sel.is_null() {
        let runs = unsafe { engine::selection_ranges(root, sel) };
        walk::with(|w| w.sel_runs.set(runs));
    }
}

fn end_selection() {
    let runs = walk::with(|w| w.sel_runs.replace(ptr::null_mut()));
    if !runs.is_null() {
        unsafe { glib::g_hash_table_destroy(runs) };
    }
}

fn end_pass() {
    end_selection();
    walk::with(|w| w.have_clip.set(false));
    walk::clear_video_holes();
    walk::with(|w| w.have_viewport.set(false));
    marker::ordinals_end();
}

pub fn paint_document(cr: Cr, root: BoxRef<'_>, highlight: Option<&CStr>, sel: *const c_void) {
    marker::ordinals_begin();
    walk::clear_video_holes();
    let bg = canvas_background_of(root).unwrap_or(Rgba::new(
        254.0 / 255.0,
        254.0 / 255.0,
        254.0 / 255.0,
        1.0,
    ));
    cr.save();
    bg.set_source(cr);
    cr.paint();
    cr.restore();
    paint_cache_clip(cr);
    begin_selection(root, sel);
    let top = top_layer_box(root).map_or(ptr::null(), BoxRef::as_ptr);
    walk::with(|w| w.skip_box.set(top));
    paint_walk(cr, root, highlight);
    walk::with(|w| w.skip_box.set(ptr::null()));
    paint_top_layer(cr, root, highlight);
    end_pass();
}

pub fn canvas_color(root: Option<BoxRef<'_>>) -> (bool, Rgba) {
    let found = root.and_then(canvas_background_of);
    (
        found.is_some(),
        found.unwrap_or(Rgba::new(254.0 / 255.0, 254.0 / 255.0, 254.0 / 255.0, 1.0)),
    )
}

fn layers_box_bg_fixed(b: BoxRef<'_>) -> bool {
    let s = style_of(b);
    let att = get(s, P::BackgroundAttachment);
    let img = get(s, P::BackgroundImage);
    if att.is_none() || img.is_none() || keyword_is(img, c"none") {
        return false;
    }
    let n = layer_count(att).max(1);
    (0..n).any(|i| keyword_is(layer(att, i), c"fixed"))
}

fn layers_box_video_composited(b: BoxRef<'_>) -> bool {
    b.kind() == BoxKind::Video
        && b.media()
            .and_then(|m| unsafe { Video::from_ptr(m.video()) })
            .is_some_and(|v| engine::video_helper_composited(Some(v)))
}

fn layers_vp_kind(b: BoxRef<'_>) -> i32 {
    let pos = get(style_of(b), P::Position);
    if keyword_is(pos, c"fixed") {
        return if engine::box_is_fixed(b) { VP_FIXED } else { 0 };
    }
    if keyword_is(pos, c"sticky") && !engine::box_in_scroller(b) {
        return VP_STICKY;
    }
    0
}

fn layers_box_needs_frames(b: BoxRef<'_>, under: i32) -> bool {
    (under != 0 && layers_box_video_composited(b)) || layers_box_bg_fixed(b)
}

fn layers_scan_kind(b: BoxRef<'_>, under: i32, in_atomic: bool) -> i32 {
    if layers_box_needs_frames(b, under) {
        return -1;
    }
    let kind = layers_vp_kind(b);
    if kind == 0 {
        return 0;
    }
    let nested_sticky = under == VP_STICKY || (under != 0 && kind == VP_STICKY);
    if in_atomic || nested_sticky || box_z_index(b) < 0 {
        return -1;
    }
    if under != 0 { 0 } else { kind }
}

fn layers_scan_children(
    b: BoxRef<'_>,
    under: i32,
    in_atomic: bool,
    kinds: *mut GHashTable,
    roots: &mut i32,
) -> bool {
    let mut c = b.first_child();
    while let Some(child) = c {
        if !layers_scan(child, under, in_atomic, kinds, roots) {
            return false;
        }
        c = child.next_sibling();
    }
    for a in b.inline_atomics().unwrap_or(&[]) {
        if let Some(ab) = a.box_ref()
            && !layers_scan(ab, under, true, kinds, roots)
        {
            return false;
        }
    }
    true
}

fn layers_scan(
    b: BoxRef<'_>,
    under: i32,
    in_atomic: bool,
    kinds: *mut GHashTable,
    roots: &mut i32,
) -> bool {
    if box_is_hidden(b) || box_clip_hides(b) {
        return true;
    }
    let kind = layers_scan_kind(b, under, in_atomic);
    if kind < 0 {
        return false;
    }
    let mut under = under;
    if kind > 0 {
        unsafe {
            glib::g_hash_table_insert(
                kinds,
                b.as_ptr().cast_mut().cast(),
                kind as isize as *mut c_void,
            )
        };
        *roots += 1;
        under = kind;
    }
    box_skips_contents(b) || layers_scan_children(b, under, in_atomic, kinds, roots)
}

pub fn plan_init(plan: &mut LayerPlan) {
    plan.dynamic = 0;
    plan.kinds =
        unsafe { glib::g_hash_table_new(Some(glib::g_direct_hash), Some(glib::g_direct_equal)) };
    plan.vp = unsafe { glib::g_array_new(0, 1, core::mem::size_of::<VpCapture>() as u32) };
}

pub fn plan_clear(plan: &mut LayerPlan) {
    if !plan.kinds.is_null() {
        unsafe { glib::g_hash_table_destroy(plan.kinds) };
        plan.kinds = ptr::null_mut();
    }
    if !plan.vp.is_null() {
        unsafe { g_array_free(plan.vp, 1) };
    }
    plan.vp = ptr::null_mut();
}

fn set_layers(layers: Layers) {
    walk::with(|w| *w.layers.borrow_mut() = layers);
}

pub fn plan_layers(cr: Cr, root: Option<BoxRef<'_>>, plan: &mut LayerPlan) {
    plan.dynamic = 0;
    unsafe {
        glib::g_hash_table_remove_all(plan.kinds);
        g_array_set_size(plan.vp, 0);
    }
    let Some(root) = root else {
        return;
    };
    let mut roots = 0;
    if top_layer_box(root).is_some() || !layers_scan(root, 0, false, plan.kinds, &mut roots) {
        plan.dynamic = 1;
        return;
    }
    if roots == 0 {
        return;
    }
    let mut layers = Layers::off();
    layers.mode = LAYERS_PLAN;
    layers.root = root.as_ptr();
    layers.kinds = plan.kinds;
    layers.found = plan.vp;
    layers.base = cr.matrix();
    set_layers(layers);
    paint_document(cr, root, None, ptr::null());
    set_layers(Layers::off());
    if unsafe { (*plan.vp).len } as i32 != roots {
        plan.dynamic = 1;
    }
}

pub struct DocLayers {
    pub upper: Option<UpperFn>,
    pub upper_data: *mut c_void,
}

pub fn doc_layers(
    cr: Cr,
    up: DocLayers,
    root: BoxRef<'_>,
    highlight: Option<&CStr>,
    sel: *const c_void,
    plan: &LayerPlan,
) -> bool {
    let mut layers = Layers::off();
    layers.mode = LAYERS_DOC;
    layers.root = root.as_ptr();
    layers.doc = cr.raw();
    layers.kinds = plan.kinds;
    layers.upper = up.upper;
    layers.upper_data = up.upper_data;
    layers.n_upper = if plan.vp.is_null() {
        0
    } else {
        unsafe { (*plan.vp).len as i32 }
    };
    set_layers(layers);
    paint_document(cr, root, highlight, sel);
    let ok = !walk::with(|w| w.layers.borrow().video_above);
    set_layers(Layers::off());
    ok
}

pub fn vp_layer(
    cr: Cr,
    root: BoxRef<'_>,
    layer: &VpCapture,
    vp: (f64, f64),
    highlight: Option<&CStr>,
    sel: *const c_void,
) {
    marker::ordinals_begin();
    walk::clear_video_holes();
    paint_cache_clip(cr);
    walk::with(|w| {
        w.vp_x0.set(vp.0);
        w.vp_y0.set(vp.1);
        w.have_viewport.set(vp.0 != 0.0 || vp.1 != 0.0);
    });
    begin_selection(root, sel);
    if layer.kind == VP_STICKY {
        walk::with(|w| w.sticky_static.set(layer.b));
    }
    let saved_flush = walk::with(|w| w.flush_box.get());
    cr.save();
    let base = cr.matrix();
    let m = Matrix::multiply(&layer.rel, &base);
    cr.set_matrix(&m);
    if let Some(lb) = unsafe { BoxRef::from_ptr(layer.b) } {
        walk::with(|w| w.flush_box.set(lb.as_ptr()));
        paint_walk(cr, lb, highlight);
    }
    walk::with(|w| w.flush_box.set(saved_flush));
    cr.restore();
    walk::with(|w| w.sticky_static.set(ptr::null()));
    end_pass();
}

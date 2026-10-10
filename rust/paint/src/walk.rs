//! Southstar — walking the box tree: visibility, culling, opacity groups, blend modes, masks, transforms, sticky and fixed offsets, overflow clips with scrollbars, column rules, z-ordered children and the deferred positioned layers flushed in stacking order.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Kind as NodeKind, Node};
use southstar_glib::{GArray, GHashTable};
use southstar_layout::{BoxKind, BoxRef, NsBox};
use southstar_style::{Kind, PropId as P, StyleRef, ValueRef, display_of, parse_color};

use crate::decor::paint_block;
use crate::ffi::cairo::{self, Cr, Matrix, Pattern};
use crate::ffi::engine;
use crate::inline::paint_inline;
use crate::marker::paint_marker;
use crate::mask::{mask_layers_paintable, mask_layers_pattern};
use crate::media::{apply_box_content_clip, paint_image, paint_math, paint_svg, paint_video};
use crate::radii::{border_box_size, box_border_radii, rounded_rect_path};
use crate::three_d;
use crate::util::{Rgba, get, keyword, keyword_is, length_or, rgba_of, style_keyword, style_of};
use crate::{decor, state};

pub const LAYERS_OFF: i32 = 0;
pub const LAYERS_PLAN: i32 = 1;
pub const LAYERS_DOC: i32 = 2;

const CULL_MARGIN: f64 = 400.0;

pub type UpperFn = unsafe extern "C" fn(index: i32, data: *mut c_void) -> *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VpCapture {
    pub b: *const NsBox,
    pub kind: i32,
    pub rel: Matrix,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<VpCapture>() == 64);

#[derive(Clone, Copy)]
struct Hole {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

#[derive(Clone, Copy)]
struct Capture {
    b: *const NsBox,
    dev_x: f64,
    dev_y: f64,
    seq: u32,
}

pub struct Layers {
    pub mode: i32,
    pub root: *const NsBox,
    pub owner: *const NsBox,
    pub flush_layered: bool,
    pub vp_seen: i32,
    pub base: Matrix,
    pub doc: *mut c_void,
    pub kinds: *mut GHashTable,
    pub found: *mut GArray,
    pub upper: Option<UpperFn>,
    pub upper_data: *mut c_void,
    pub n_upper: i32,
    pub video_above: bool,
}

impl Layers {
    pub fn off() -> Layers {
        Layers {
            mode: LAYERS_OFF,
            root: ptr::null(),
            owner: ptr::null(),
            flush_layered: false,
            vp_seen: 0,
            base: Matrix::zero(),
            doc: ptr::null_mut(),
            kinds: ptr::null_mut(),
            found: ptr::null_mut(),
            upper: None,
            upper_data: ptr::null_mut(),
            n_upper: 0,
            video_above: false,
        }
    }
}

pub struct Walk {
    pub no_cull: Cell<i32>,
    deferred: RefCell<Option<Vec<Capture>>>,
    defer_depth: Cell<i32>,
    pub flush_box: Cell<*const NsBox>,
    pub layers: RefCell<Layers>,
    pub have_viewport: Cell<bool>,
    pub vp_x0: Cell<f64>,
    pub vp_y0: Cell<f64>,
    pub sel_runs: Cell<*mut GHashTable>,
    pub sticky_static: Cell<*const NsBox>,
    holes: RefCell<Vec<Hole>>,
    capture_seq: Cell<u32>,
    anchor_dx: Cell<f64>,
    anchor_dy: Cell<f64>,
    pub skip_box: Cell<*const NsBox>,
    pub have_clip: Cell<bool>,
    clip_y0: Cell<f64>,
    clip_y1: Cell<f64>,
    pub tex_root: Cell<*const NsBox>,
}

impl Walk {
    fn new() -> Walk {
        Walk {
            no_cull: Cell::new(0),
            deferred: RefCell::new(None),
            defer_depth: Cell::new(0),
            flush_box: Cell::new(ptr::null()),
            layers: RefCell::new(Layers::off()),
            have_viewport: Cell::new(false),
            vp_x0: Cell::new(0.0),
            vp_y0: Cell::new(0.0),
            sel_runs: Cell::new(ptr::null_mut()),
            sticky_static: Cell::new(ptr::null()),
            holes: RefCell::new(Vec::new()),
            capture_seq: Cell::new(0),
            anchor_dx: Cell::new(0.0),
            anchor_dy: Cell::new(0.0),
            skip_box: Cell::new(ptr::null()),
            have_clip: Cell::new(false),
            clip_y0: Cell::new(0.0),
            clip_y1: Cell::new(0.0),
            tex_root: Cell::new(ptr::null()),
        }
    }
}

thread_local! {
    static WALK: Walk = Walk::new();
}

pub fn with<R>(f: impl FnOnce(&Walk) -> R) -> R {
    WALK.with(f)
}

pub fn video_hole_record(cr: Cr, x: f64, y: f64, w: f64, h: f64) {
    let (mut x0, mut y0) = cr.user_to_device(x, y);
    let (mut x1, mut y1) = cr.user_to_device(x + w, y + h);
    if x0 > x1 {
        core::mem::swap(&mut x0, &mut x1);
    }
    if y0 > y1 {
        core::mem::swap(&mut y0, &mut y1);
    }
    with(|w| w.holes.borrow_mut().push(Hole { x0, y0, x1, y1 }));
}

pub fn clear_video_holes() {
    with(|w| w.holes.borrow_mut().clear());
}

fn paint_group_video_holes(
    cr: Cr,
    source: &Pattern,
    mask: Option<&Pattern>,
    opacity: f64,
    first_hole: usize,
) {
    let holes: Vec<Hole> = with(|w| {
        let holes = w.holes.borrow();
        holes
            .get(first_hole..)
            .map(<[Hole]>::to_vec)
            .unwrap_or_default()
    });
    if holes.is_empty() {
        return;
    }
    cr.save();
    cr.new_path();
    for h in holes {
        let (x0, y0) = cr.device_to_user(h.x0, h.y0);
        let (x1, y1) = cr.device_to_user(h.x1, h.y1);
        cr.rectangle(x0, y0, x1 - x0, y1 - y0);
    }
    cr.clip();
    cr.set_source(source);
    cr.set_operator(cairo::OPERATOR_SOURCE);
    if let Some(mask) = mask {
        cr.mask(mask);
    } else if opacity < 0.999 {
        cr.paint_with_alpha(opacity);
    } else {
        cr.paint();
    }
    cr.restore();
}

pub fn layers_mode() -> i32 {
    with(|w| w.layers.borrow().mode)
}

pub fn layers_note_video(cr: Cr) {
    with(|w| {
        let mut l = w.layers.borrow_mut();
        if l.mode == LAYERS_DOC && cr.raw() != l.doc {
            l.video_above = true;
        }
    });
}

pub fn viewport_origin() -> (bool, f64, f64) {
    with(|w| (w.have_viewport.get(), w.vp_x0.get(), w.vp_y0.get()))
}

pub fn selection_runs() -> *mut GHashTable {
    with(|w| w.sel_runs.get())
}

pub use crate::ffi::engine::SelectionRun;

pub fn selection_run(b: BoxRef<'_>) -> Option<SelectionRun> {
    let runs = selection_runs();
    if runs.is_null() {
        return None;
    }
    let run = unsafe { southstar_glib::g_hash_table_lookup(runs, b.as_ptr().cast()) };
    unsafe { run.cast::<SelectionRun>().as_ref() }.copied()
}

pub fn paint_walk_atomic(cr: Cr, b: BoxRef<'_>, highlight: Option<&CStr>) {
    let saved = with(|w| {
        w.no_cull.set(w.no_cull.get() + 1);
        w.flush_box.replace(b.as_ptr())
    });
    paint_walk(cr, b, highlight);
    with(|w| {
        w.flush_box.set(saved);
        w.no_cull.set(w.no_cull.get() - 1);
    });
}

pub fn box_is_hidden(b: BoxRef<'_>) -> bool {
    keyword(get(style_of(b), P::Visibility))
        .is_some_and(|v| matches!(v.to_bytes(), b"hidden" | b"collapse"))
}

pub fn box_skips_contents(b: BoxRef<'_>) -> bool {
    get(style_of(b), P::ContentVisibility)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .is_some_and(|k| k.to_bytes() == b"hidden")
}

fn box_opacity(b: BoxRef<'_>) -> f64 {
    if let Some(o) = engine::anim_opacity(b) {
        return o.clamp(0.0, 1.0);
    }
    match get(style_of(b), P::Opacity).and_then(ValueRef::length) {
        Some((o, _)) => o.clamp(0.0, 1.0),
        None => 1.0,
    }
}

fn position_keyword(b: BoxRef<'_>) -> Option<&[u8]> {
    get(style_of(b), P::Position)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes)
}

fn box_is_positioned(b: BoxRef<'_>) -> bool {
    matches!(
        position_keyword(b),
        Some(b"relative" | b"absolute" | b"fixed" | b"sticky")
    )
}

pub fn box_clip_hides(b: BoxRef<'_>) -> bool {
    let Some(rect) = get(style_of(b), P::Clip).and_then(ValueRef::rect) else {
        return false;
    };
    if !matches!(position_keyword(b), Some(b"absolute" | b"fixed")) {
        return false;
    }
    let (bw, bh) = border_box_size(b);
    let top = if rect.is_auto[0] { 0.0 } else { rect.v[0] };
    let right = if rect.is_auto[1] { bw } else { rect.v[1] };
    let bottom = if rect.is_auto[2] { bh } else { rect.v[2] };
    let left = if rect.is_auto[3] { 0.0 } else { rect.v[3] };
    (right - left) <= 0.0 || (bottom - top) <= 0.0
}

pub fn box_z_index(b: BoxRef<'_>) -> i32 {
    get(style_of(b), P::ZIndex)
        .and_then(ValueRef::length)
        .map_or(0, |(v, _)| v as i32)
}

fn box_z_index_is_auto(b: BoxRef<'_>) -> bool {
    get(style_of(b), P::ZIndex).is_none_or(|v| v.kind() != Kind::Length)
}

pub fn box_is_flex_or_grid_item(b: BoxRef<'_>) -> bool {
    let mut p = b.parent();
    while let Some(parent) = p {
        if let Some(s) = style_of(parent) {
            let d = display_of(Some(s));
            return d.box_ == 0 && d.internal == 0 && (d.inner == 3 || d.inner == 4);
        }
        p = parent.parent();
    }
    false
}

fn box_defers_to_positioned_layer(b: BoxRef<'_>) -> bool {
    if box_z_index(b) < 0 {
        return false;
    }
    box_is_positioned(b) || (!box_z_index_is_auto(b) && box_is_flex_or_grid_item(b))
}

fn box_isolates_positioned_descendants(b: BoxRef<'_>) -> bool {
    if !box_z_index_is_auto(b) {
        return true;
    }
    let Some(s) = style_of(b) else {
        return false;
    };
    let pos = s.get(P::Position);
    if keyword_is(pos, c"fixed") || keyword_is(pos, c"sticky") {
        return true;
    }
    let filter = s.get(P::Filter);
    filter.is_some() && !keyword_is(filter, c"none")
}

fn node_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

fn dom_tree_order_cmp(a: Option<Node<'_>>, b: Option<Node<'_>>) -> i32 {
    let (Some(a), Some(b)) = (a, b) else {
        return 0;
    };
    if a == b {
        return 0;
    }
    let chain = |n: Node<'_>| {
        let mut out = Vec::new();
        let mut p = Some(n);
        while let Some(node) = p {
            if out.len() >= 128 {
                break;
            }
            out.push(node.as_ptr());
            p = node.parent();
        }
        out
    };
    let pa = chain(a);
    let pb = chain(b);
    if pa.len() >= 128 || pb.len() >= 128 {
        return 0;
    }
    let mut ia = pa.len() as isize - 1;
    let mut ib = pb.len() as isize - 1;
    while ia >= 0 && ib >= 0 && pa[ia as usize] == pb[ib as usize] {
        ia -= 1;
        ib -= 1;
    }
    if ia < 0 {
        return -1;
    }
    if ib < 0 {
        return 1;
    }
    let na = unsafe { Node::from_ptr(pa[ia as usize]) };
    let nb = unsafe { Node::from_ptr(pb[ib as usize]) };
    let (Some(na), Some(nb)) = (na, nb) else {
        return 0;
    };
    if na.parent() != nb.parent() {
        return 0;
    }
    let mut s = na.next_sibling();
    while let Some(sib) = s {
        if sib == nb {
            return -1;
        }
        s = sib.next_sibling();
    }
    1
}

fn capture_cmp(a: &Capture, b: &Capture) -> i32 {
    let ab = unsafe { BoxRef::from_ptr(a.b) };
    let bb = unsafe { BoxRef::from_ptr(b.b) };
    let (Some(ab), Some(bb)) = (ab, bb) else {
        return match (ab.is_some(), bb.is_some()) {
            (true, false) => 1,
            (false, true) => -1,
            _ => 0,
        };
    };
    let za = box_z_index(ab);
    let zb = box_z_index(bb);
    if za != zb {
        return if za < zb { -1 } else { 1 };
    }
    let c = dom_tree_order_cmp(node_of(ab), node_of(bb));
    if c != 0 {
        return c;
    }
    a.seq.cmp(&b.seq) as i32
}

pub fn merge_sort_by<T: Copy>(items: &mut [T], cmp: &impl Fn(&T, &T) -> i32) {
    if items.len() < 2 {
        return;
    }
    let mid = items.len() / 2;
    merge_sort_by(&mut items[..mid], cmp);
    merge_sort_by(&mut items[mid..], cmp);
    let mut merged = Vec::with_capacity(items.len());
    let (mut i, mut j) = (0, mid);
    while i < mid && j < items.len() {
        if cmp(&items[j], &items[i]) < 0 {
            merged.push(items[j]);
            j += 1;
        } else {
            merged.push(items[i]);
            i += 1;
        }
    }
    merged.extend_from_slice(&items[i..mid]);
    merged.extend_from_slice(&items[j..]);
    items.copy_from_slice(&merged);
}

fn layers_record(cr: Cr, b: *const NsBox, kind: i32, dx: f64, dy: f64) {
    with(|w| {
        let l = w.layers.borrow();
        let mut m = cr.matrix();
        m.translate(dx, dy);
        let mut inv = l.base;
        if !inv.invert() {
            inv = Matrix::identity();
        }
        let layer = VpCapture {
            b,
            kind,
            rel: Matrix::multiply(&m, &inv),
        };
        unsafe { southstar_glib::g_array_append_vals(l.found, ptr::from_ref(&layer).cast(), 1) };
    });
}

fn layers_target(cr: Cr, cap: &Capture, dx: f64, dy: f64) -> Option<Cr> {
    let (kinds, mode) = with(|w| {
        let l = w.layers.borrow();
        (l.kinds, l.mode)
    });
    let kind = unsafe { southstar_glib::g_hash_table_lookup(kinds, cap.b.cast()) } as isize as i32;
    if kind != 0 {
        if mode == LAYERS_PLAN {
            layers_record(cr, cap.b, kind, dx, dy);
        }
        with(|w| w.layers.borrow_mut().vp_seen += 1);
        return None;
    }
    if mode == LAYERS_PLAN {
        return Some(cr);
    }
    let (vp_seen, n_upper, doc, upper, data) = with(|w| {
        let l = w.layers.borrow();
        (l.vp_seen, l.n_upper, l.doc, l.upper, l.upper_data)
    });
    let doc = unsafe { Cr::from_raw(doc) };
    if vp_seen == 0 || n_upper == 0 {
        return Some(doc);
    }
    let i = vp_seen.min(n_upper) - 1;
    let Some(upper) = upper else {
        return Some(doc);
    };
    let target = unsafe { upper(i, data) };
    if target.is_null() {
        Some(doc)
    } else {
        Some(unsafe { Cr::from_raw(target) })
    }
}

fn debug_on() -> bool {
    state::debug_point().is_some()
}

fn printerr(text: &str) {
    southstar_glib::stderr_write(text.as_bytes());
}

fn dom_name(b: BoxRef<'_>) -> String {
    node_of(b)
        .and_then(Node::name)
        .map_or_else(|| "?".to_string(), |n| n.to_string_lossy().into_owned())
}

fn dom_attr(b: BoxRef<'_>, name: &CStr) -> String {
    node_of(b)
        .filter(|n| n.kind() == NodeKind::Element)
        .and_then(|n| n.attr(name))
        .map_or_else(String::new, |v| v.to_string_lossy().into_owned())
}

fn truncated(s: &str, max: usize) -> String {
    let bytes = s.as_bytes();
    String::from_utf8_lossy(&bytes[..bytes.len().min(max)]).into_owned()
}

fn paint_flush_deferred(cr: Cr, list: Vec<Capture>, highlight: Option<&CStr>) {
    let layered = with(|w| {
        let mut l = w.layers.borrow_mut();
        let layered = l.mode != LAYERS_OFF && l.flush_layered;
        l.flush_layered = false;
        layered
    });
    if list.is_empty() {
        return;
    }
    let mut queue = list;
    merge_sort_by(&mut queue, &capture_cmp);
    let saved_flush = with(|w| w.flush_box.get());
    let mut i = 0;
    while i < queue.len() {
        let cap = queue[i];
        let (cur_x, cur_y) = cr.user_to_device(0.0, 0.0);
        let mut dx = cap.dev_x - cur_x;
        let mut dy = cap.dev_y - cur_y;
        if dx.is_nan() || dy.is_nan() {
            dx = 0.0;
            dy = 0.0;
        }
        let target = if layered {
            layers_target(cr, &cap, dx, dy)
        } else {
            Some(cr)
        };
        let Some(target) = target else {
            i += 1;
            continue;
        };
        if layered {
            with(|w| w.layers.borrow_mut().owner = cap.b);
        }
        target.save();
        if target != cr {
            target.set_matrix(&cr.matrix());
        }
        if dx != 0.0 || dy != 0.0 {
            target.translate(dx, dy);
        }
        with(|w| w.flush_box.set(cap.b));
        let Some(cb) = (unsafe { BoxRef::from_ptr(cap.b) }) else {
            target.restore();
            i += 1;
            continue;
        };
        if debug_on() && !cb.dom_ptr().is_null() {
            let (gx0, gy0, gx1, gy1) = cr.clip_extents();
            printerr(&format!(
                "[flush-one] <{}#{} y={:.0} h={:.0}> d={:.0},{:.0} clip={:.0},{:.0}..{:.0},{:.0}\n",
                dom_name(cb),
                dom_attr(cb, c"id"),
                cb.y(),
                cb.content_height(),
                dx,
                dy,
                gx0,
                gy0,
                gx1,
                gy1
            ));
        }
        let flat = !box_isolates_positioned_descendants(cb);
        let saved_list = if flat {
            with(|w| {
                w.defer_depth.set(w.defer_depth.get() + 1);
                w.deferred.borrow_mut().take()
            })
        } else {
            None
        };
        paint_walk(target, cb, highlight);
        if flat {
            let found = with(|w| {
                w.defer_depth.set(w.defer_depth.get() - 1);
                core::mem::replace(&mut *w.deferred.borrow_mut(), saved_list)
            });
            if let Some(found) = found {
                queue.extend(found);
                merge_sort_by(&mut queue[i + 1..], &capture_cmp);
            }
        }
        target.restore();
        i += 1;
    }
    with(|w| w.flush_box.set(saved_flush));
}

fn paint_anchor_leave(cr: Cr, saved: (f64, f64)) {
    cr.restore();
    with(|w| {
        w.anchor_dx.set(saved.0);
        w.anchor_dy.set(saved.1);
    });
}

fn compute_sticky_offset(b: BoxRef<'_>, cr: Cr) -> (f64, f64) {
    let Some(s) = style_of(b) else {
        return (0.0, 0.0);
    };
    if with(|w| w.sticky_static.get()) == b.as_ptr() {
        return (0.0, 0.0);
    }
    if engine::box_is_fixed(b) {
        let (have, x, y) = viewport_origin();
        return if have { (x, y) } else { (0.0, 0.0) };
    }
    if !keyword_is(s.get(P::Position), c"sticky") {
        return (0.0, 0.0);
    }
    engine::box_sticky_offset(b, cr.clip_extents())
}

pub fn blend_mode_operator(s: Option<StyleRef<'_>>) -> i32 {
    let Some(k) = get(s, P::MixBlendMode)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
    else {
        return cairo::OPERATOR_OVER;
    };
    match k.to_bytes() {
        b"multiply" => cairo::OPERATOR_MULTIPLY,
        b"screen" => cairo::OPERATOR_SCREEN,
        b"overlay" => cairo::OPERATOR_OVERLAY,
        b"darken" => cairo::OPERATOR_DARKEN,
        b"lighten" => cairo::OPERATOR_LIGHTEN,
        b"color-dodge" => cairo::OPERATOR_COLOR_DODGE,
        b"color-burn" => cairo::OPERATOR_COLOR_BURN,
        b"hard-light" => cairo::OPERATOR_HARD_LIGHT,
        b"soft-light" => cairo::OPERATOR_SOFT_LIGHT,
        b"difference" => cairo::OPERATOR_DIFFERENCE,
        b"exclusion" => cairo::OPERATOR_EXCLUSION,
        b"hue" => cairo::OPERATOR_HSL_HUE,
        b"saturation" => cairo::OPERATOR_HSL_SATURATION,
        b"color" => cairo::OPERATOR_HSL_COLOR,
        b"luminosity" => cairo::OPERATOR_HSL_LUMINOSITY,
        _ => cairo::OPERATOR_OVER,
    }
}

pub fn paint_cache_clip(cr: Cr) {
    let (x0, y0, _x1, y1) = cr.clip_extents();
    with(|w| {
        w.clip_y0.set(y0);
        w.clip_y1.set(y1);
        w.have_clip.set(true);
        w.vp_x0.set(x0);
        w.vp_y0.set(y0);
        w.have_viewport
            .set(x0.is_finite() && y0.is_finite() && (x0 != 0.0 || y0 != 0.0));
        w.anchor_dx.set(0.0);
        w.anchor_dy.set(0.0);
    });
}

fn ancestor_chain(b: BoxRef<'_>, item: impl Fn(BoxRef<'_>) -> String) -> String {
    let mut out = String::new();
    let mut p = Some(b);
    while let Some(cur) = p {
        out.push_str(&item(cur));
        p = cur.parent();
    }
    out
}

fn dbg_paint_probe(cr: Cr, b: BoxRef<'_>) {
    let Some((px, py)) = state::debug_point() else {
        return;
    };
    if b.x().is_nan() || b.y().is_nan() || b.content_width().is_nan() || b.content_height().is_nan()
    {
        let chain = ancestor_chain(b, |p2| {
            format!(
                " <{}#{}{}>",
                dom_name(p2),
                dom_attr(p2, c"id"),
                if p2.x().is_nan() { " NAN" } else { "" }
            )
        });
        printerr(&format!("[paint-NAN]{chain}\n"));
    }
    let (x0, y0) = cr.user_to_device(b.x(), b.y());
    let (x1, y1) = cr.user_to_device(b.x() + b.content_width(), b.y() + b.content_height());
    if x0.is_nan() && !b.x().is_nan() {
        let chain = ancestor_chain(b, |p2| {
            format!(
                " <{}#{} sx={:.0} sy={:.0}>",
                dom_name(p2),
                dom_attr(p2, c"id"),
                p2.scroll_x(),
                p2.scroll_y()
            )
        });
        printerr(&format!("[paint-CTM-NAN]{chain}\n"));
    }
    let (px, py) = (f64::from(px), f64::from(py));
    if px < x0 || px > x1 || py < y0 || py > y1 {
        return;
    }
    let bg = get(style_of(b), P::BackgroundColor)
        .and_then(ValueRef::color)
        .map_or_else(
            || "-".to_string(),
            |c| format!("rgba({},{},{},{})", c[0], c[1], c[2], c[3]),
        );
    let (kx0, ky0, kx1, ky1) = cr.clip_extents();
    printerr(&format!(
        "[paint-at] <{}> {:.0},{:.0} {:.0}x{:.0} bg={} clip={:.0},{:.0}..{:.0},{:.0}\n",
        dom_name(b),
        x0,
        y0,
        x1 - x0,
        y1 - y0,
        bg,
        kx0,
        ky0,
        kx1,
        ky1
    ));
}

fn clip_moved(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() > 0.5
        || (a.1 - b.1).abs() > 0.5
        || (a.2 - b.2).abs() > 0.5
        || (a.3 - b.3).abs() > 0.5
}

fn overflow_kw_clips(ov: Option<&CStr>) -> bool {
    ov.is_some_and(|ov| {
        let ov = ov.to_bytes();
        [&b"hidden"[..], b"clip", b"auto", b"scroll"]
            .iter()
            .any(|k| ov.eq_ignore_ascii_case(k))
    })
}

fn is_block_like(kind: BoxKind) -> bool {
    matches!(
        kind,
        BoxKind::Block | BoxKind::Table | BoxKind::TableCaption | BoxKind::TableCell
    )
}

fn has_length_or_calc(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

fn scrollbar_colors(s: Option<StyleRef<'_>>) -> (Rgba, Rgba) {
    let mut thumb = Rgba::new(0.0, 0.0, 0.0, 0.40);
    let mut track = Rgba::new(0.0, 0.0, 0.0, 0.06);
    let kw = get(s, P::ScrollbarColor)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text);
    if let Some(kw) = kw.filter(|k| !k.to_bytes().eq_ignore_ascii_case(b"auto")) {
        let mut idx = 0;
        for token in kw.to_bytes().split(|&c| c == b' ' || c == b'\t') {
            if idx >= 2 {
                break;
            }
            let t = strip(token);
            if t.is_empty() {
                continue;
            }
            let Some(c) = CString::new(t).ok().and_then(|t| parse_color(&t)) else {
                continue;
            };
            if idx == 0 {
                thumb = Rgba::from_bytes(c);
            } else {
                track = Rgba::from_bytes(c);
            }
            idx += 1;
        }
    }
    (thumb, track)
}

fn strip(token: &[u8]) -> &[u8] {
    let is_space = crate::util::is_space;
    let start = token
        .iter()
        .position(|&c| !is_space(c))
        .unwrap_or(token.len());
    let end = token
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(start, |e| e + 1);
    &token[start..end.max(start)]
}

fn paint_scrollbars(cr: Cr, b: BoxRef<'_>) {
    let s = style_of(b);
    let sbw_kw = get(s, P::ScrollbarWidth)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes);
    let sb_hidden = sbw_kw == Some(b"none");
    let sb_size = if sbw_kw == Some(b"thin") { 5.0 } else { 8.0 };
    let (thumb, track) = scrollbar_colors(s);
    if !b.scrolls() || sb_hidden || !(b.scroll_max_x() > 0.0 || b.scroll_max_y() > 0.0) {
        return;
    }
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    let px = b.x() + m.left + bd.left;
    let py = b.y() + m.top + bd.top;
    let pw = b.content_width() + p.left + p.right;
    let ph = b.content_height() + p.top + p.bottom;
    if b.scroll_x() != 0.0 || b.scroll_y() != 0.0 {
        cr.translate(b.scroll_x(), b.scroll_y());
    }
    if b.scroll_max_y() > 0.0 && ph > 16.0 {
        let track_w = sb_size;
        let track_x = px + pw - track_w - 1.0;
        let track_y = py + 1.0;
        let track_h = ph - 2.0;
        let total_h = ph + b.scroll_max_y();
        let mut thumb_h = track_h * (ph / total_h);
        if thumb_h < 16.0 {
            thumb_h = 16.0;
        }
        if thumb_h > track_h {
            thumb_h = track_h;
        }
        let thumb_y = track_y + (track_h - thumb_h) * (b.scroll_y() / b.scroll_max_y());
        cr.save();
        track.set_source(cr);
        cr.rectangle(track_x, track_y, track_w, track_h);
        cr.fill();
        thumb.set_source(cr);
        cr.rectangle(track_x + 1.0, thumb_y, track_w - 2.0, thumb_h);
        cr.fill();
        cr.restore();
    }
    if b.scroll_max_x() > 0.0 && pw > 16.0 {
        let track_h = sb_size;
        let track_x = px + 1.0;
        let track_y = py + ph - track_h - 1.0;
        let track_w = pw - 2.0 - if b.scroll_max_y() > 0.0 { sb_size } else { 0.0 };
        let total_w = pw + b.scroll_max_x();
        let mut thumb_w = track_w * (pw / total_w);
        if thumb_w < 16.0 {
            thumb_w = 16.0;
        }
        if thumb_w > track_w {
            thumb_w = track_w;
        }
        let thumb_x = track_x + (track_w - thumb_w) * (b.scroll_x() / b.scroll_max_x());
        cr.save();
        track.set_source(cr);
        cr.rectangle(track_x, track_y, track_w, track_h);
        cr.fill();
        thumb.set_source(cr);
        cr.rectangle(thumb_x, track_y + 1.0, thumb_w, track_h - 2.0);
        cr.fill();
        cr.restore();
    }
}

fn paint_column_rules(cr: Cr, b: BoxRef<'_>, s: StyleRef<'_>) {
    let mut col_gap = 16.0;
    let n_cols = b.columns();
    engine::used_column_count(s, b.content_width(), &mut col_gap);
    let rule_w = length_or(s.get(P::ColumnRuleWidth), 0.0);
    let Some(rstyle) = s
        .get(P::ColumnRuleStyle)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes)
        .filter(|k| *k != b"none" && *k != b"hidden")
    else {
        return;
    };
    if rule_w <= 0.0 {
        return;
    }
    let rc = rgba_of(s.get(P::ColumnRuleColor), Rgba::new(0.50, 0.50, 0.50, 1.0));
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    let inx = b.x() + m.left + bd.left + p.left;
    let iny = b.y() + m.top + bd.top + p.top;
    let cw = b.content_width();
    let nf = f64::from(n_cols);
    let col_w = (cw - col_gap * (nf - 1.0)) / nf;
    cr.save();
    rc.set_source(cr);
    cr.set_line_width(rule_w);
    if rstyle == b"dashed" {
        cr.set_dash(&[rule_w * 3.0, rule_w * 2.0]);
    } else if rstyle == b"dotted" {
        cr.set_dash(&[rule_w, rule_w]);
    }
    for i in 0..n_cols - 1 {
        let fi = f64::from(i);
        let rx = inx + col_w * (fi + 1.0) + col_gap * fi + col_gap / 2.0;
        cr.move_to(rx, iny);
        cr.line_to(rx, iny + b.content_height());
        cr.stroke();
    }
    cr.restore();
}

fn paint_canvas_element(cr: Cr, b: BoxRef<'_>) {
    let Some(surf) = engine::canvas_surface(b) else {
        return;
    };
    let sw = surf.width();
    let sh = surf.height();
    if sw <= 0 || sh <= 0 {
        return;
    }
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    let dx = b.x() + m.left + bd.left + p.left;
    let dy = b.y() + m.top + bd.top + p.top;
    let dw = if b.content_width() > 0.0 {
        b.content_width()
    } else {
        f64::from(sw)
    };
    let dh = if b.content_height() > 0.0 {
        b.content_height()
    } else {
        f64::from(sh)
    };
    cr.save();
    cr.translate(dx, dy);
    cr.scale(dw / f64::from(sw), dh / f64::from(sh));
    cr.set_source_surface(surf, 0.0, 0.0);
    cr.paint();
    cr.restore();
}

pub fn paint_walk(cr: Cr, b: BoxRef<'_>, highlight: Option<&CStr>) {
    if box_is_hidden(b) {
        return;
    }
    dbg_paint_probe(cr, b);
    if box_clip_hides(b) {
        return;
    }
    if with(|w| w.skip_box.get()) == b.as_ptr() {
        return;
    }
    let deferring = with(|w| w.defer_depth.get() > 0 && w.flush_box.get() != b.as_ptr());
    if deferring && box_defers_to_positioned_layer(b) {
        let (dev_x, dev_y) = cr.user_to_device(0.0, 0.0);
        with(|w| {
            let seq = w.capture_seq.get();
            w.capture_seq.set(seq.wrapping_add(1));
            w.deferred
                .borrow_mut()
                .get_or_insert_with(Vec::new)
                .push(Capture {
                    b: b.as_ptr(),
                    dev_x,
                    dev_y,
                    seq,
                });
        });
        if debug_on() && node_of(b).and_then(Node::name).is_some() {
            printerr(&format!(
                "[paint-defer] <{}#{}> y={:.0} h={:.0}\n",
                dom_name(b),
                dom_attr(b, c"id"),
                b.y(),
                b.content_height()
            ));
        }
        return;
    }
    let (no_cull, have_clip, clip_y0, clip_y1) = with(|w| {
        (
            w.no_cull.get(),
            w.have_clip.get(),
            w.clip_y0.get(),
            w.clip_y1.get(),
        )
    });
    if no_cull == 0 && have_clip && b.paint_bottom() > b.paint_top() {
        let anchor_dy = with(|w| w.anchor_dy.get());
        let top = b.paint_top() + anchor_dy;
        let bottom = b.paint_bottom() + anchor_dy;
        if bottom < clip_y0 - CULL_MARGIN || top > clip_y1 + CULL_MARGIN {
            return;
        }
    }
    let dbg_e = if debug_on() {
        cr.clip_extents()
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    let style = style_of(b);
    let skip_contents = box_skips_contents(b);
    let op = box_opacity(b);
    let blend = blend_mode_operator(style);
    let mask_grad = mask_layers_paintable(style);
    let grouped = op < 0.999 || blend != cairo::OPERATOR_OVER || mask_grad;
    let (mut sticky_dx, mut sticky_dy) = compute_sticky_offset(b, cr);
    if sticky_dx.is_nan() || sticky_dy.is_nan() {
        if debug_on() {
            printerr(&format!("[paint-nan-guard] sticky <{}>\n", dom_name(b)));
        }
        sticky_dx = 0.0;
        sticky_dy = 0.0;
    }
    let has_sticky = sticky_dx != 0.0 || sticky_dy != 0.0;
    let saved_anchor = with(|w| (w.anchor_dx.get(), w.anchor_dy.get()));
    if has_sticky {
        cr.save();
        cr.translate(sticky_dx, sticky_dy);
        with(|w| {
            w.anchor_dx.set(w.anchor_dx.get() + sticky_dx);
            w.anchor_dy.set(w.anchor_dy.get() + sticky_dy);
        });
    }
    let eff_tf = engine::effective_transform(b, style);
    let mut has_transform = eff_tf.n_ops > 0;
    if with(|w| w.tex_root.get()) == b.as_ptr() {
        has_transform = false;
    } else if three_d::box_establishes_3d(b) || (has_transform && engine::transform_is_3d(&eff_tf))
    {
        let u = if debug_on() {
            cr.clip_extents()
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        three_d::paint_3d_root(cr, b, highlight);
        if debug_on() && clip_moved(cr.clip_extents(), u) {
            printerr(&format!(
                "[3droot-LEAK] <{} class={}>\n",
                dom_name(b),
                truncated(&dom_attr(b, c"class"), 60)
            ));
        }
        if has_sticky {
            paint_anchor_leave(cr, saved_anchor);
        }
        return;
    }

    let mut box_offscreen = false;
    if !has_transform && !has_sticky && no_cull == 0 {
        let is_fixed = position_keyword(b) == Some(b"fixed");
        if !is_fixed {
            let anchor_dy = with(|w| w.anchor_dy.get());
            let (m, p, bd) = (b.margin(), b.padding(), b.border());
            let by = b.y() + m.top + anchor_dy;
            let bh = b.content_height() + p.top + p.bottom + bd.top + bd.bottom;
            if have_clip && (by + bh < clip_y0 - CULL_MARGIN || by > clip_y1 + CULL_MARGIN) {
                box_offscreen = true;
            }
        }
    }

    let first_group_hole = with(|w| w.holes.borrow().len());
    if grouped {
        cr.push_group();
    }
    if has_transform {
        cr.save();
        let (bx, by, bw, bh) = three_d::box_border_rect(b);
        let (ox, oy, _oz) = three_d::box_transform_origin(b, (bx, by, bw, bh), P::TransformOrigin);
        let m = engine::transform_to_mat4(&eff_tf, bw, bh).m;
        let cm = Matrix::new(m[0], m[4], m[1], m[5], m[3], m[7]);
        if m[0].is_nan()
            || m[4].is_nan()
            || m[1].is_nan()
            || m[5].is_nan()
            || m[3].is_nan()
            || m[7].is_nan()
            || ox.is_nan()
            || oy.is_nan()
        {
            if debug_on() {
                printerr(&format!("[paint-nan-guard] transform <{}>\n", dom_name(b)));
            }
        } else if (m[0] * m[5] - m[1] * m[4]).abs() < 1e-6 {
            cr.restore();
            if grouped {
                drop(cr.pop_group());
            }
            if has_sticky {
                paint_anchor_leave(cr, saved_anchor);
            }
            return;
        } else {
            cr.translate(ox, oy);
            cr.transform(&cm);
            cr.translate(-ox, -oy);
        }
    }
    let mut has_path_clip = false;
    if is_block_like(b.kind())
        && get(style, P::ClipPath)
            .filter(|v| v.kind() == Kind::Keyword)
            .and_then(ValueRef::keyword_text)
            .is_some_and(|k| k.to_bytes() != b"none")
    {
        cr.save();
        if apply_box_content_clip(cr, b) {
            has_path_clip = true;
        } else {
            cr.restore();
        }
    }
    if !box_offscreen {
        let s0 = if debug_on() {
            cr.clip_extents()
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        let kind = b.kind();
        if matches!(
            kind,
            BoxKind::Block
                | BoxKind::Table
                | BoxKind::TableCaption
                | BoxKind::TableRow
                | BoxKind::TableCell
                | BoxKind::Image
                | BoxKind::Video
                | BoxKind::Math
                | BoxKind::Svg
        ) {
            paint_block(cr, b);
            if debug_on() && clip_moved(cr.clip_extents(), s0) {
                printerr(&format!(
                    "[block-LEAK] <{} class={}>\n",
                    dom_name(b),
                    truncated(&dom_attr(b, c"class"), 60)
                ));
            }
        }
        if kind == BoxKind::Block {
            paint_marker(cr, b);
            decor::paint_hr(cr, b);
        }
        if kind == BoxKind::Inline && !skip_contents {
            paint_inline(cr, b, highlight);
            if debug_on() && clip_moved(cr.clip_extents(), s0) {
                let text = b
                    .text()
                    .map_or_else(String::new, |t| t.to_string_lossy().into_owned());
                printerr(&format!(
                    "[inline-LEAK] <{}> text={}\n",
                    dom_name(b),
                    truncated(&text, 30)
                ));
            }
        }
        if kind == BoxKind::Image && !skip_contents {
            paint_image(cr, b);
        }
        if kind == BoxKind::Video && !skip_contents {
            paint_video(cr, b);
        }
        if kind == BoxKind::Math && !skip_contents {
            paint_math(cr, b);
        }
        if kind == BoxKind::Svg {
            paint_svg(cr, b);
        }
    }
    if !skip_contents && node_of(b).and_then(Node::element_name) == Some(b"canvas") {
        paint_canvas_element(cr, b);
    }

    let mut entries: Vec<(i32, u32, BoxRef<'_>)> = Vec::new();
    let mut any_z = false;
    if !skip_contents {
        let mut c = b.first_child();
        let mut order = 0u32;
        while let Some(child) = c {
            let key = if box_is_positioned(child) {
                let z = box_z_index(child);
                if z != 0 {
                    any_z = true;
                }
                z
            } else {
                0
            };
            entries.push((key, order, child));
            order += 1;
            c = child.next_sibling();
        }
    }
    if any_z {
        entries.sort_by_key(|&(key, order, _)| (key, order));
    }
    let mut ovx = style_keyword(style, P::OverflowX);
    let mut ovy = style_keyword(style, P::OverflowY);
    let ovs = style_keyword(style, P::Overflow);
    if ovx.is_none() {
        ovx = ovs;
    }
    if ovy.is_none() {
        ovy = ovs;
    }
    let is_root = b.parent().is_none()
        || node_of(b)
            .and_then(Node::name)
            .is_some_and(|n| matches!(n.to_bytes(), b"html" | b"body"));
    let mut clip_overflow = !is_root && (overflow_kw_clips(ovx) || overflow_kw_clips(ovy));
    if clip_overflow
        && matches!(
            b.kind(),
            BoxKind::Block | BoxKind::TableCaption | BoxKind::TableCell
        )
    {
        let (m, bd, p) = (b.margin(), b.border(), b.padding());
        let mut px = b.x() + m.left + bd.left;
        let mut py = b.y() + m.top + bd.top;
        let mut pw = b.content_width() + p.left + p.right;
        let mut ph = b.content_height() + p.top + p.bottom;
        if pw < 0.0 {
            pw = 0.0;
        }
        if ph < 0.0 {
            ph = 0.0;
        }
        if px.is_nan() || py.is_nan() || pw.is_nan() || ph.is_nan() {
            if debug_on() {
                printerr(&format!(
                    "[paint-nan-guard] overflow-clip <{}>\n",
                    dom_name(b)
                ));
            }
            px = 0.0;
            py = 0.0;
            pw = 0.0;
            ph = 0.0;
        }
        let explicit_h = style.is_some()
            && (has_length_or_calc(get(style, P::MaxHeight))
                || has_length_or_calc(get(style, P::Height)));
        let explicit_w = style.is_some()
            && (has_length_or_calc(get(style, P::MaxWidth))
                || has_length_or_calc(get(style, P::Width)));
        let sized_by_container = box_is_flex_or_grid_item(b);
        if (pw > 0.0 || explicit_w || sized_by_container)
            && (ph > 0.0 || explicit_h || sized_by_container)
        {
            cr.save();
            let ov_radii = box_border_radii(Some(b));
            if ov_radii.is_zero() {
                cr.rectangle(px, py, pw, ph);
            } else {
                rounded_rect_path(cr, px, py, pw, ph, ov_radii);
            }
            cr.clip();
            if debug_on() {
                let (ex0, ey0, ex1, ey1) = cr.clip_extents();
                printerr(&format!(
                    "[paint-clip{}] <{}#{}> rect {:.0},{:.0} {:.0}x{:.0} -> clip {:.0},{:.0}..{:.0},{:.0}\n",
                    if ey1 - ey0 < 1.0 || ex1 - ex0 < 1.0 {
                        "-EMPTY"
                    } else {
                        ""
                    },
                    dom_name(b),
                    dom_attr(b, c"id"),
                    px,
                    py,
                    pw,
                    ph,
                    ex0,
                    ey0,
                    ex1,
                    ey1
                ));
            }
            if (b.scroll_x() != 0.0 || b.scroll_y() != 0.0)
                && !b.scroll_x().is_nan()
                && !b.scroll_y().is_nan()
            {
                cr.translate(-b.scroll_x(), -b.scroll_y());
            }
        } else {
            clip_overflow = false;
        }
    } else {
        clip_overflow = false;
    }
    let tex_root = with(|w| w.tex_root.get());
    let flush_box = with(|w| w.flush_box.get());
    let own_layer_scope = b.parent().is_none()
        || grouped
        || has_transform
        || clip_overflow
        || has_path_clip
        || b.as_ptr() == tex_root
        || (b.as_ptr() == flush_box && box_isolates_positioned_descendants(b));
    let mut saved_layer_list = None;
    if own_layer_scope {
        saved_layer_list = with(|w| {
            w.defer_depth.set(w.defer_depth.get() + 1);
            w.deferred.borrow_mut().take()
        });
    }
    let bump_cull = has_transform || has_sticky;
    if bump_cull {
        with(|w| w.no_cull.set(w.no_cull.get() + 1));
    }
    for &(_, _, child) in &entries {
        paint_walk(cr, child, highlight);
    }
    if bump_cull {
        with(|w| w.no_cull.set(w.no_cull.get() - 1));
    }
    if own_layer_scope {
        let mine = with(|w| {
            w.defer_depth.set(w.defer_depth.get() - 1);
            core::mem::replace(&mut *w.deferred.borrow_mut(), saved_layer_list)
        });
        if let Some(mine) = mine {
            if debug_on() {
                let (fx0, fy0, fx1, fy1) = cr.clip_extents();
                printerr(&format!(
                    "[paint-flush] owner=<{}#{}> n={} clip={:.0},{:.0}..{:.0},{:.0}\n",
                    dom_name(b),
                    dom_attr(b, c"id"),
                    mine.len(),
                    fx0,
                    fy0,
                    fx1,
                    fy1
                ));
            }
            if bump_cull {
                with(|w| w.no_cull.set(w.no_cull.get() + 1));
            }
            with(|w| {
                let mut l = w.layers.borrow_mut();
                l.flush_layered = b.as_ptr() == l.root
                    || (b.as_ptr() == l.owner
                        && !grouped
                        && !has_transform
                        && !clip_overflow
                        && !has_path_clip);
            });
            paint_flush_deferred(cr, mine, highlight);
            if bump_cull {
                with(|w| w.no_cull.set(w.no_cull.get() - 1));
            }
        }
    }
    if clip_overflow {
        paint_scrollbars(cr, b);
    }
    if b.kind() == BoxKind::Block
        && b.columns() >= 2
        && let Some(s) = style
    {
        paint_column_rules(cr, b, s);
    }
    if clip_overflow {
        cr.restore();
    }
    if has_path_clip {
        cr.restore();
    }
    if has_transform {
        cr.restore();
    }
    if grouped {
        cr.pop_group_to_source();
        let group_source = cr.source();
        let saved_op = cr.operator();
        if blend != cairo::OPERATOR_OVER {
            cr.set_operator(blend);
        }
        let mp = if mask_grad {
            Some(mask_layers_pattern(cr, b))
        } else {
            None
        };
        match &mp {
            Some(mp) => cr.mask(mp),
            None => cr.paint_with_alpha(op),
        }
        if blend != cairo::OPERATOR_OVER {
            cr.set_operator(saved_op);
        }
        paint_group_video_holes(cr, &group_source, mp.as_ref(), op, first_group_hole);
    }
    if has_sticky {
        paint_anchor_leave(cr, saved_anchor);
    }
    if debug_on() {
        let q = cr.clip_extents();
        if clip_moved(q, dbg_e) {
            let node = node_of(b);
            let is_el = node.is_some_and(|n| n.kind() == NodeKind::Element);
            printerr(&format!(
                "[clip-LEAK] <{}#{} class={} kind={} grp={} tf={} ov={} pc={} st={}> entry={:.0},{:.0}..{:.0},{:.0} exit={:.0},{:.0}..{:.0},{:.0}\n",
                dom_name(b),
                if is_el {
                    dom_attr(b, c"id")
                } else {
                    String::new()
                },
                if is_el {
                    truncated(&dom_attr(b, c"class"), 70)
                } else {
                    String::new()
                },
                b.kind_raw(),
                i32::from(grouped),
                i32::from(has_transform),
                i32::from(clip_overflow),
                i32::from(has_path_clip),
                i32::from(has_sticky),
                dbg_e.0,
                dbg_e.1,
                dbg_e.2,
                dbg_e.3,
                q.0,
                q.1,
                q.2,
                q.3
            ));
        }
    }
}

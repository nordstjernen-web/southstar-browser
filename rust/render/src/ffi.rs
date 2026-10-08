//! Southstar — the C ABI of src/render.h: the render context and profile, the relayout driver with its container-query passes, and the page rule, hover and active state a relayout records.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GStr};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::{Prop, StyleRef, StyleTable, display_of, styles_equal};

use crate::fonts::{self, Usage};
use crate::viewport;

type ResolveUrl = unsafe extern "C" fn(href: *const c_char, user_data: *mut c_void) -> *mut c_char;
type FontAllowed = unsafe extern "C" fn(abs_url: *const c_char, user_data: *mut c_void) -> GBoolean;

#[repr(C)]
pub struct RenderCtx {
    pub doc: *mut NsNode,
    pub sheets: *const *const c_void,
    pub sheet_docs: *const *const NsNode,
    pub n_sheets: c_uint,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub zoom: f64,
    pub images: *mut c_void,
    pub base_url: *const c_char,
    pub anim: *mut c_void,
    pub js: *mut c_void,
    pub focused_input: *const NsNode,
    pub hover_node: *const NsNode,
    pub caret_byte: usize,
    pub sel_anchor_byte: usize,
    pub resolve_url: Option<ResolveUrl>,
    pub font_allowed: Option<FontAllowed>,
    pub cb_ud: *mut c_void,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<RenderCtx>() == 144
        && core::mem::offset_of!(RenderCtx, viewport_width) == 32
        && core::mem::offset_of!(RenderCtx, caret_byte) == 104
);

#[repr(C)]
#[derive(Default)]
pub struct RenderProfile {
    pub css1_us: i64,
    pub style1_us: i64,
    pub layout1_us: i64,
    pub container_us: i64,
    pub css2_us: i64,
    pub style2_us: i64,
    pub layout2_us: i64,
    pub containers: c_uint,
    pub container_passes: c_uint,
    pub container_pass: GBoolean,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<RenderProfile>() == 72
        && core::mem::offset_of!(RenderProfile, container_pass) == 64
);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageRule {
    width: f64,
    height: f64,
    has_size: GBoolean,
    landscape: GBoolean,
    margin: [f64; 4],
    has_margin: [GBoolean; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FontDescriptors {
    weight: c_int,
    slant: c_int,
}

#[repr(C)]
struct FontFace {
    family: *const c_char,
    src_url: *const c_char,
    unicode_range: *const c_char,
    descriptors: FontDescriptors,
}

#[repr(C)]
struct SheetHead {
    _rules: *mut c_void,
    _imports: *mut c_void,
    _layer_names: *mut c_void,
    _layers: *mut c_void,
    font_faces: *mut GArray,
    _keyframes: *mut c_void,
    _property_rules: *mut c_void,
    page_rule: *const PageRule,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<PageRule>() == 72
        && core::mem::size_of::<FontFace>() == 32
        && core::mem::offset_of!(SheetHead, font_faces) == 32
        && core::mem::offset_of!(SheetHead, page_rule) == 56
);

unsafe extern "C" {
    fn ns_css_set_viewport(vw_px: f64, vh_px: f64);
    fn ns_css_set_focus_node(node: *const NsNode) -> *const NsNode;
    fn ns_css_set_hover_node(node: *const NsNode) -> *const NsNode;
    fn ns_css_set_render_zoom(zoom: f64);
    fn ns_css_selector_cache_begin();
    fn ns_css_selector_cache_end();
    fn ns_css_set_container_map(map: *mut GHashTable);
    fn ns_css_container_map_new() -> *mut GHashTable;
    fn ns_css_container_maps_equal(a: *mut GHashTable, b: *mut GHashTable) -> GBoolean;
    fn ns_css_container_map_add(
        map: *mut GHashTable,
        node: *const c_void,
        type_kw: *const c_char,
        name_kw: *const c_char,
        w: f64,
        h: f64,
        vertical: GBoolean,
    );
    fn ns_css_container_features_begin();
    fn ns_css_container_features_used() -> GBoolean;
    fn ns_css_compute(
        doc: *mut NsNode,
        sheets: *const *const c_void,
        docs: *const *const NsNode,
        n: c_uint,
    ) -> *mut GHashTable;
    fn ns_css_stylesheet_has_container_rules(sheet: *const c_void) -> GBoolean;
    fn ns_css_stylesheet_has_container_units(sheet: *const c_void) -> GBoolean;
    fn ns_css_stylesheet_has_hover_rules(sheet: *const c_void) -> GBoolean;
    fn ns_css_stylesheet_has_active_rules(sheet: *const c_void) -> GBoolean;
    fn ns_css_text_has_container_units(text: *const c_char, len: isize) -> GBoolean;
    fn ns_css_style_scale_font_size(style: *mut Style, factor: f64);
    fn ns_anim_load_from_stylesheet(anim: *mut c_void, sheet: *const c_void);
    fn ns_anim_observe_all(anim: *mut c_void, styles: *mut GHashTable, now_us: i64);
    fn ns_font_available() -> GBoolean;
    fn ns_font_request(
        family: *const c_char,
        src_url: *const c_char,
        base_url: *const c_char,
        descriptors: FontDescriptors,
    );
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_layout_pseudo_content_text(content: *const c_void, host: *const NsNode) -> *mut c_char;
    fn ns_layout_build(
        doc: *const NsNode,
        styles: *mut GHashTable,
        viewport_width: f64,
        focused_input: *const NsNode,
        focused_caret_byte: usize,
        focused_sel_anchor_byte: usize,
        images: *mut c_void,
        base_url: *const c_char,
    ) -> *mut NsBox;
    fn ns_box_free(b: *mut NsBox);
    fn ns_paint_list_ordinals_begin();
    fn ns_paint_list_ordinals_end();
    fn ns_js_set_style_table(js: *mut c_void, styles: *mut GHashTable);
    fn ns_js_set_layout_root(js: *mut c_void, root: *const NsBox);
    fn g_get_monotonic_time() -> i64;
}

static FONT_FAMILY: Prop = Prop::new(c"font-family");
static CONTENT: Prop = Prop::new(c"content");
static CONTAINER_TYPE: Prop = Prop::new(c"container-type");
static CONTAINER_NAME: Prop = Prop::new(c"container-name");
static WRITING_MODE: Prop = Prop::new(c"writing-mode");

static USES_HOVER: AtomicBool = AtomicBool::new(false);
static USES_ACTIVE: AtomicBool = AtomicBool::new(false);
static PAGE_RULE: Mutex<Option<PageRule>> = Mutex::new(None);

struct Containers {
    doc: usize,
    width: f64,
    map: usize,
    settled: bool,
}

static CONTAINERS: Mutex<Containers> = Mutex::new(Containers {
    doc: 0,
    width: 0.0,
    map: 0,
    settled: false,
});

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

impl RenderCtx {
    fn sheets(&self) -> &[*const c_void] {
        if self.sheets.is_null() {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.sheets, self.n_sheets as usize) }
    }

    fn doc(&self) -> Option<Node<'_>> {
        unsafe { Node::from_ptr(self.doc) }
    }

    fn zoom(&self) -> f64 {
        if self.zoom > 0.0 { self.zoom } else { 1.0 }
    }
}

fn sheet_head<'a>(sheet: *const c_void) -> Option<&'a SheetHead> {
    unsafe { sheet.cast::<SheetHead>().as_ref() }
}

fn font_faces<'a>(sheet: *const c_void) -> &'a [FontFace] {
    let Some(faces) = sheet_head(sheet).and_then(|h| unsafe { h.font_faces.as_ref() }) else {
        return &[];
    };
    if faces.data.is_null() {
        return &[];
    }
    unsafe { core::slice::from_raw_parts(faces.data.cast::<FontFace>(), faces.len as usize) }
}

fn add_styled(style: Option<StyleRef<'_>>, text: &[u8], families: &mut HashMap<Vec<u8>, Usage>) {
    let Some(list) = style
        .and_then(|s| s.value(&FONT_FAMILY))
        .and_then(|v| v.keyword_text())
    else {
        return;
    };
    for (family, usage) in families.iter_mut() {
        if fonts::list_names_family(list.to_bytes(), family) {
            usage.add_text(text);
        }
    }
}

fn add_pseudo(
    host: Node<'_>,
    pseudo: Option<StyleRef<'_>>,
    families: &mut HashMap<Vec<u8>, Usage>,
) {
    let Some(pseudo) = pseudo else {
        return;
    };
    let content = pseudo
        .value(&CONTENT)
        .map_or(ptr::null(), |v| v.as_ptr().cast());
    let text = unsafe { GStr::take(ns_layout_pseudo_content_text(content, host.as_ptr())) };
    if let Some(text) = text.as_deref().filter(|t| !t.is_empty()) {
        add_styled(Some(pseudo), text.to_bytes(), families);
    }
}

fn collect_font_usage(root: Node<'_>, styles: StyleTable, families: &mut HashMap<Vec<u8>, Usage>) {
    let mut cur = Some(root);
    while let Some(node) = cur {
        let mut descend = true;
        if node.is_element() {
            let style = styles.get(node);
            descend = !display_of(style).is_none();
            if let Some(style) = style.filter(|_| descend) {
                add_pseudo(node, style.before(), families);
                add_pseudo(node, style.after(), families);
            }
        } else if node.is_text() {
            if let (Some(text), Some(parent)) =
                (node.text().filter(|t| !t.is_empty()), node.parent())
            {
                add_styled(styles.get(parent), text.to_bytes(), families);
            }
        }
        cur = southstar_dom::index::next_in_subtree(node, Some(root), descend);
    }
}

fn request_fonts(c: &RenderCtx, styles: StyleTable) {
    if unsafe { ns_font_available() } == 0 {
        return;
    }
    let mut families: HashMap<Vec<u8>, Usage> = HashMap::new();
    for &sheet in c.sheets() {
        for face in font_faces(sheet) {
            if let Some(family) = text(face.family).filter(|f| !f.is_empty()) {
                families.entry(family.to_vec()).or_default();
            }
        }
    }
    if let Some(doc) = c.doc() {
        collect_font_usage(doc, styles, &mut families);
    }
    for &sheet in c.sheets() {
        for face in font_faces(sheet) {
            let (Some(family), false) = (text(face.family), face.src_url.is_null()) else {
                continue;
            };
            if !fonts::range_matches(text(face.unicode_range), families.get(family)) {
                continue;
            }
            let abs = match c.resolve_url {
                Some(resolve) => unsafe { resolve(face.src_url, c.cb_ud) },
                None => unsafe { ns_url_resolve(c.base_url, face.src_url) },
            };
            let Some(abs) = (unsafe { GStr::take(abs) }) else {
                continue;
            };
            if let Some(allowed) = c.font_allowed {
                if unsafe { allowed(abs.as_ptr(), c.cb_ud) } == 0 {
                    continue;
                }
            }
            unsafe { ns_font_request(face.family, abs.as_ptr(), c.base_url, face.descriptors) };
        }
    }
}

fn feed_animations(c: &RenderCtx, styles: *mut GHashTable) {
    if c.anim.is_null() {
        return;
    }
    for &sheet in c.sheets() {
        if !sheet.is_null() {
            unsafe { ns_anim_load_from_stylesheet(c.anim, sheet) };
        }
    }
    unsafe { ns_anim_observe_all(c.anim, styles, now_us()) };
}

fn apply_zoom(c: &RenderCtx, styles: *mut GHashTable) {
    let zoom = c.zoom();
    if (zoom - 1.0).abs() <= 0.001 {
        return;
    }
    for (_, style) in unsafe { glib::hash_table_entries(styles) } {
        unsafe { ns_css_style_scale_font_size(style.cast(), zoom) };
    }
}

fn style_pass(c: &RenderCtx, styles: *mut GHashTable) {
    feed_animations(c, styles);
    request_fonts(c, unsafe { StyleTable::from_ptr(styles) });
    apply_zoom(c, styles);
}

fn styles_tables_equal(a: *mut GHashTable, b: *mut GHashTable) -> bool {
    if unsafe { glib::g_hash_table_size(a) != glib::g_hash_table_size(b) } {
        return false;
    }
    unsafe { glib::hash_table_entries(a) }.all(|(node, style)| {
        let other = unsafe { glib::g_hash_table_lookup(b, node) };
        unsafe {
            styles_equal(
                StyleRef::from_ptr(style.cast()),
                StyleRef::from_ptr(other.cast()),
            )
        }
    })
}

fn container_entry_sig(b: BoxRef<'_>, kind: &CStr, names: Option<&CStr>) -> u64 {
    let queries_block = kind.to_bytes().eq_ignore_ascii_case(b"size");
    let str_hash = |s: &CStr| u64::from(unsafe { glib::g_str_hash(s.as_ptr().cast()) });
    let parts = [
        b.dom_ptr() as u64,
        str_hash(kind),
        names.map_or(0, str_hash),
        (b.content_width() * 64.0) as i64 as u64,
        if queries_block {
            (b.content_height() * 64.0) as i64 as u64
        } else {
            0
        },
    ];
    parts.iter().fold(1_469_598_103_934_665_603u64, |h, &part| {
        (h ^ part).wrapping_mul(1_099_511_628_211)
    })
}

fn collect_containers(b: Option<BoxRef<'_>>, map: *mut GHashTable, mut sig: Option<&mut u64>) {
    let Some(b) = b else {
        return;
    };
    let style = unsafe { StyleRef::from_ptr(b.style()) };
    if let Some(style) = style.filter(|_| !b.dom_ptr().is_null()) {
        let kind = style
            .value(&CONTAINER_TYPE)
            .and_then(|v| v.keyword_text())
            .filter(|k| !k.to_bytes().eq_ignore_ascii_case(b"normal"));
        if let Some(kind) = kind {
            let names = style.value(&CONTAINER_NAME).and_then(|v| v.keyword_text());
            let vertical = style
                .value(&WRITING_MODE)
                .and_then(|v| v.keyword_text())
                .is_some_and(|w| w.to_bytes().starts_with(b"vertical"));
            unsafe {
                ns_css_container_map_add(
                    map,
                    b.dom_ptr(),
                    kind.as_ptr(),
                    names.map_or(ptr::null(), CStr::as_ptr),
                    b.content_width(),
                    b.content_height(),
                    glib::boolean(vertical),
                )
            };
            if let Some(sig) = sig.as_deref_mut() {
                *sig = sig.wrapping_add(container_entry_sig(b, kind, names));
            }
        }
    }
    let mut child = b.first_child();
    while let Some(ch) = child {
        collect_containers(Some(ch), map, sig.as_deref_mut());
        child = ch.next_sibling();
    }
}

fn predicted_map(doc: *mut NsNode, viewport_width: f64, want: bool) -> *mut GHashTable {
    let cq = lock(&CONTAINERS);
    if !want || cq.map == 0 || cq.doc != doc as usize || cq.width != viewport_width {
        return ptr::null_mut();
    }
    cq.map as *mut GHashTable
}

fn settled(predicted: *mut GHashTable, measured: *mut GHashTable) -> bool {
    let settled =
        !predicted.is_null() && unsafe { ns_css_container_maps_equal(predicted, measured) } != 0;
    lock(&CONTAINERS).settled = settled;
    settled
}

fn remember(doc: *mut NsNode, viewport_width: f64, layout: *mut NsBox, want: bool) {
    let mut cq = lock(&CONTAINERS);
    if cq.map != 0 {
        unsafe { glib::g_hash_table_destroy(cq.map as *mut GHashTable) };
        cq.map = 0;
    }
    cq.doc = 0;
    if !want {
        return;
    }
    let map = unsafe { ns_css_container_map_new() };
    collect_containers(unsafe { BoxRef::from_ptr(layout) }, map, None);
    cq.map = map as usize;
    cq.doc = doc as usize;
    cq.width = viewport_width;
}

fn dom_uses_container_units(root: Option<Node<'_>>) -> bool {
    let mut cur = root;
    while let Some(node) = cur {
        if node.is_element() {
            let style = node.attr(c"style").map_or(ptr::null(), CStr::as_ptr);
            if unsafe { ns_css_text_has_container_units(style, -1) } != 0 {
                return true;
            }
        }
        cur = southstar_dom::index::next_in_subtree(node, root, true);
    }
    false
}

fn containers_wanted(c: &RenderCtx) -> (bool, bool) {
    let mut units = dom_uses_container_units(c.doc());
    let mut want = units;
    for &sheet in c.sheets() {
        if unsafe { ns_css_stylesheet_has_container_units(sheet) } != 0 {
            units = true;
            want = true;
        }
        if unsafe { ns_css_stylesheet_has_container_rules(sheet) } != 0 {
            want = true;
        }
    }
    (want, units)
}

fn selector_cache_wanted(c: &RenderCtx, predicted: *mut GHashTable) -> bool {
    if !predicted.is_null() && lock(&CONTAINERS).settled {
        return false;
    }
    c.sheets().iter().any(|&sheet| unsafe {
        ns_css_stylesheet_has_container_rules(sheet) != 0
            || ns_css_stylesheet_has_container_units(sheet) != 0
    })
}

fn find_viewport_meta(n: Option<Node<'_>>, depth: i32) -> Option<Node<'_>> {
    let n = n?;
    if depth >= southstar_dom::MAX_DEPTH {
        return None;
    }
    if n.element_name() == Some(b"meta")
        && n.attr(c"name")
            .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"viewport"))
    {
        return Some(n);
    }
    southstar_dom::children(n).find_map(|child| find_viewport_meta(Some(child), depth + 1))
}

fn effective_viewport_width(c: &RenderCtx) -> f64 {
    let content = find_viewport_meta(c.doc(), 0).and_then(|meta| meta.attr(c"content"));
    let hint = viewport::width_hint(content.map(CStr::to_bytes));
    if hint > c.viewport_width {
        hint
    } else {
        c.viewport_width
    }
}

fn build_layout(c: &RenderCtx, styles: *mut GHashTable, viewport_width: f64) -> *mut NsBox {
    unsafe {
        ns_paint_list_ordinals_begin();
        let layout = ns_layout_build(
            c.doc,
            styles,
            viewport_width,
            c.focused_input,
            c.caret_byte,
            c.sel_anchor_byte,
            c.images,
            c.base_url,
        );
        ns_paint_list_ordinals_end();
        layout
    }
}

fn record_page_state(c: &RenderCtx) {
    let page_rule = c
        .sheets()
        .iter()
        .filter_map(|&sheet| sheet_head(sheet).and_then(|h| unsafe { h.page_rule.as_ref() }))
        .next_back()
        .copied();
    *lock(&PAGE_RULE) = page_rule;
    let uses = |has: unsafe extern "C" fn(*const c_void) -> GBoolean| {
        c.sheets().iter().any(|&sheet| unsafe { has(sheet) } != 0)
    };
    USES_HOVER.store(uses(ns_css_stylesheet_has_hover_rules), Ordering::Relaxed);
    USES_ACTIVE.store(uses(ns_css_stylesheet_has_active_rules), Ordering::Relaxed);
}

fn relayout(
    c: &RenderCtx,
    out_layout: &mut *mut NsBox,
    mut profile: Option<&mut RenderProfile>,
) -> *mut GHashTable {
    if let Some(p) = profile.as_deref_mut() {
        *p = RenderProfile::default();
    }
    let timed = profile.is_some();
    let clock = || if timed { now_us() } else { 0 };

    let viewport_width = effective_viewport_width(c);
    unsafe {
        ns_css_set_viewport(viewport_width, c.viewport_height);
        ns_css_set_focus_node(c.focused_input);
        ns_css_set_hover_node(c.hover_node);
    }
    record_page_state(c);

    let t0 = clock();
    unsafe { ns_css_set_render_zoom(c.zoom()) };
    let (want_cq, uses_cq_units) = containers_wanted(c);
    let predicted = predicted_map(c.doc, viewport_width, want_cq);
    let cache_selectors = selector_cache_wanted(c, predicted);
    if cache_selectors {
        unsafe { ns_css_selector_cache_begin() };
    }
    let compute = || unsafe { ns_css_compute(c.doc, c.sheets, c.sheet_docs, c.n_sheets) };
    unsafe { ns_css_set_container_map(predicted) };
    let mut styles = compute();
    unsafe { ns_css_set_container_map(ptr::null_mut()) };
    let t1 = clock();

    style_pass(c, styles);
    let t2 = clock();

    let mut layout = build_layout(c, styles, viewport_width);
    let t3 = clock();
    if let Some(p) = profile.as_deref_mut() {
        p.css1_us = t1 - t0;
        p.style1_us = t2 - t1;
        p.layout1_us = t3 - t2;
    }

    let containers = unsafe { ns_css_container_map_new() };
    let mut container_sig = 0u64;
    let tc0 = clock();
    if want_cq {
        collect_containers(
            unsafe { BoxRef::from_ptr(layout) },
            containers,
            Some(&mut container_sig),
        );
    }
    let tc1 = clock();
    let mut n_containers = unsafe { glib::g_hash_table_size(containers) };
    if let Some(p) = profile.as_deref_mut() {
        p.container_us = tc1 - tc0;
        p.containers = n_containers;
    }
    let mut container_passes = if uses_cq_units { 3 } else { 1 };
    let mut stale = !predicted.is_null();
    if settled(predicted, containers) {
        container_passes = 0;
    }
    let mut pass = 0;
    while pass < container_passes && (n_containers > 0 || stale) {
        if let Some(p) = profile.as_deref_mut() {
            p.container_pass = glib::TRUE;
            p.container_passes += 1;
        }
        unsafe {
            ns_css_set_container_map(containers);
            ns_css_container_features_begin();
        }
        let t4 = clock();
        let styles2 = compute();
        let t5 = clock();
        let features_used = unsafe { ns_css_container_features_used() } != 0;
        unsafe { ns_css_set_container_map(ptr::null_mut()) };
        if (!features_used && !stale) || styles_tables_equal(styles, styles2) {
            if let Some(p) = profile.as_deref_mut() {
                p.css2_us += t5 - t4;
            }
            unsafe { glib::g_hash_table_destroy(styles2) };
            break;
        }
        style_pass(c, styles2);
        let t6 = clock();
        let layout2 = build_layout(c, styles2, viewport_width);
        let t7 = clock();
        if let Some(p) = profile.as_deref_mut() {
            p.css2_us += t5 - t4;
            p.style2_us += t6 - t5;
            p.layout2_us += t7 - t6;
        }
        unsafe {
            ns_box_free(layout);
            glib::g_hash_table_destroy(styles);
        }
        layout = layout2;
        styles = styles2;
        stale = false;
        if pass + 1 < container_passes {
            unsafe { glib::g_hash_table_remove_all(containers) };
            let tr0 = clock();
            let mut next_sig = 0u64;
            collect_containers(
                unsafe { BoxRef::from_ptr(layout) },
                containers,
                Some(&mut next_sig),
            );
            let tr1 = clock();
            n_containers = unsafe { glib::g_hash_table_size(containers) };
            if let Some(p) = profile.as_deref_mut() {
                p.container_us += tr1 - tr0;
                p.containers = n_containers;
            }
            if next_sig == container_sig {
                break;
            }
            container_sig = next_sig;
        }
        pass += 1;
    }
    unsafe { glib::g_hash_table_destroy(containers) };
    remember(c.doc, viewport_width, layout, want_cq);
    unsafe {
        if cache_selectors {
            ns_css_selector_cache_end();
        }
        ns_css_set_focus_node(ptr::null());
        ns_css_set_hover_node(ptr::null());
        if !c.js.is_null() {
            ns_js_set_style_table(c.js, styles);
            ns_js_set_layout_root(c.js, layout);
        }
    }
    *out_layout = layout;
    styles
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_render_page_uses_hover() -> GBoolean {
    glib::boolean(USES_HOVER.load(Ordering::Relaxed))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_render_page_uses_active() -> GBoolean {
    glib::boolean(USES_ACTIVE.load(Ordering::Relaxed))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_render_page_rule() -> *const PageRule {
    lock(&PAGE_RULE)
        .as_ref()
        .map_or(ptr::null(), |rule| rule as *const PageRule)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_render_relayout_profile(
    c: *const RenderCtx,
    out_layout: *mut *mut NsBox,
    profile: *mut RenderProfile,
) -> *mut GHashTable {
    let Some(out_layout) = (unsafe { out_layout.as_mut() }) else {
        return ptr::null_mut();
    };
    *out_layout = ptr::null_mut();
    let Some(c) = (unsafe { c.as_ref() }) else {
        return ptr::null_mut();
    };
    relayout(c, out_layout, unsafe { profile.as_mut() })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_render_relayout(
    c: *const RenderCtx,
    out_layout: *mut *mut NsBox,
) -> *mut GHashTable {
    unsafe { ns_render_relayout_profile(c, out_layout, ptr::null_mut()) }
}

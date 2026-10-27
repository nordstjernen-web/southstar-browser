//! Southstar — the C ABI of rendering: the render context and profile, the relayout driver with its container-query passes, and the page rule, hover and active state a relayout records.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use southstar_anim::AnimHandle;
use southstar_css::{ContainerMap, OwnedSheet, PageRule, Sheet, StyleData, StyleMap};
use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GStr};
use southstar_layout::{BoxPool, BoxRef, NsBox};
use southstar_layout_position::BuildInput;
use southstar_style::{Prop, StyleRef, StyleTable, display_of, styles_equal};

use crate::fonts::{self, Usage};

type ResolveUrl = unsafe extern "C" fn(href: *const c_char, user_data: *mut c_void) -> *mut c_char;
type FontAllowed = unsafe extern "C" fn(abs_url: *const c_char, user_data: *mut c_void) -> GBoolean;

pub struct RenderCtx<'a> {
    pub doc: *mut NsNode,
    pub sheets: &'a [OwnedSheet],
    pub sheet_docs: &'a [Node<'a>],
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub zoom: f64,
    pub images: *mut c_void,
    pub base_url: *const c_char,
    pub anim: Option<AnimHandle>,
    pub js: *mut c_void,
    pub focused_input: *const NsNode,
    pub hover_node: *const NsNode,
    pub caret_byte: usize,
    pub sel_anchor_byte: usize,
    pub resolve_url: Option<ResolveUrl>,
    pub font_allowed: Option<FontAllowed>,
    pub cb_ud: *mut c_void,
}

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

unsafe extern "C" {
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
    map: Option<ContainerMap>,
    settled: bool,
}

static CONTAINERS: Mutex<Containers> = Mutex::new(Containers {
    doc: 0,
    width: 0.0,
    map: None,
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

impl RenderCtx<'_> {
    fn style_sheets(&self) -> impl Iterator<Item = &Sheet> {
        self.sheets.iter().map(|sheet| &**sheet)
    }

    fn doc(&self) -> Option<Node<'_>> {
        unsafe { Node::from_ptr(self.doc) }
    }

    fn zoom(&self) -> f64 {
        if self.zoom > 0.0 { self.zoom } else { 1.0 }
    }
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
    let text = pseudo
        .value(&CONTENT)
        .and_then(|v| v.keyword_text())
        .and_then(|raw| {
            southstar_layout_content::pseudo::resolve_content(raw.to_bytes(), Some(host))
        });
    if let Some(text) = text.filter(|t| !t.is_empty()) {
        add_styled(Some(pseudo), &text, families);
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
        } else if node.is_text()
            && let (Some(text), Some(parent)) =
                (node.text().filter(|t| !t.is_empty()), node.parent())
        {
            add_styled(styles.get(parent), text.to_bytes(), families);
        }
        cur = southstar_dom::index::next_in_subtree(node, Some(root), descend);
    }
}

fn request_fonts(c: &RenderCtx, styles: StyleTable) {
    if !southstar_font::available() {
        return;
    }
    let mut families: HashMap<Vec<u8>, Usage> = HashMap::new();
    for sheet in c.style_sheets() {
        for face in sheet.font_faces() {
            if let Some(family) = face.family.map(CStr::to_bytes).filter(|f| !f.is_empty()) {
                families.entry(family.to_vec()).or_default();
            }
        }
    }
    if let Some(doc) = c.doc() {
        collect_font_usage(doc, styles, &mut families);
    }
    for sheet in c.style_sheets() {
        for face in sheet.font_faces() {
            let (Some(family), Some(src_url)) = (face.family, face.src_url) else {
                continue;
            };
            let range = face.unicode_range.map(CStr::to_bytes);
            if !fonts::range_matches(range, families.get(family.to_bytes())) {
                continue;
            }
            let abs = match c.resolve_url {
                Some(resolve) => unsafe { resolve(src_url.as_ptr(), c.cb_ud) },
                None => southstar_url::bytes::resolve(text(c.base_url), src_url.to_bytes())
                    .map_or(ptr::null_mut(), |abs| glib::strdup(&abs)),
            };
            let Some(abs) = (unsafe { GStr::take(abs) }) else {
                continue;
            };
            if let Some(allowed) = c.font_allowed
                && unsafe { allowed(abs.as_ptr(), c.cb_ud) } == 0
            {
                continue;
            }
            let descriptors = southstar_font::Descriptors {
                weight: face.weight,
                slant: face.slant,
            };
            southstar_font::request(
                family.to_bytes(),
                abs.to_bytes(),
                text(c.base_url),
                descriptors,
            );
        }
    }
}

fn feed_animations(c: &RenderCtx, styles: &StyleMap) {
    let Some(anim) = c.anim else {
        return;
    };
    for sheet in c.style_sheets() {
        anim.load_keyframes(sheet);
    }
    anim.observe_all(styles, now_us());
}

fn apply_zoom(c: &RenderCtx, styles: &StyleMap) {
    let zoom = c.zoom();
    if (zoom - 1.0).abs() <= 0.001 {
        return;
    }
    for (_, style) in styles.entries() {
        // SAFETY: the table holds the live computed styles of this pass, and none is borrowed while it is scaled.
        if let Some(style) = unsafe { style.cast::<StyleData>().as_mut() } {
            southstar_css::style_scale_font_size(style, zoom);
        }
    }
}

fn style_pass(c: &RenderCtx, styles: &StyleMap) {
    feed_animations(c, styles);
    // SAFETY: the table is borrowed for the whole call.
    request_fonts(c, unsafe { StyleTable::from_ptr(styles) });
    apply_zoom(c, styles);
}

fn styles_tables_equal(a: &StyleMap, b: &StyleMap) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.entries().all(|(node, style)| {
        let other = b.lookup(node as *const c_void);
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

fn collect_containers(b: Option<BoxRef<'_>>, map: &mut ContainerMap, mut sig: Option<&mut u64>) {
    let Some(b) = b else {
        return;
    };
    let style = unsafe { StyleRef::from_ptr(b.style()) };
    if let (Some(style), Some(node)) = (style, b.dom()) {
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
            map.add(
                node,
                kind,
                names,
                b.content_width(),
                b.content_height(),
                vertical,
            );
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

fn predicted_map(doc: *mut NsNode, viewport_width: f64, want: bool) -> Option<ContainerMap> {
    let mut cq = lock(&CONTAINERS);
    let matches = want && cq.doc == doc as usize && cq.width == viewport_width;
    cq.map.take().filter(|_| matches)
}

fn settled(predicted: Option<&ContainerMap>, measured: &ContainerMap) -> bool {
    let settled = predicted.is_some_and(|predicted| predicted == measured);
    lock(&CONTAINERS).settled = settled;
    settled
}

fn remember(doc: *mut NsNode, viewport_width: f64, layout: *mut NsBox, want: bool) {
    let mut cq = lock(&CONTAINERS);
    cq.map = None;
    cq.doc = 0;
    if !want {
        return;
    }
    let mut map = ContainerMap::new();
    // SAFETY: layout is null or the box tree this relayout just built.
    collect_containers(unsafe { BoxRef::from_ptr(layout) }, &mut map, None);
    cq.map = Some(map);
    cq.doc = doc as usize;
    cq.width = viewport_width;
}

fn dom_uses_container_units(root: Option<Node<'_>>) -> bool {
    let mut cur = root;
    while let Some(node) = cur {
        if node.is_element()
            && node
                .attr(c"style")
                .is_some_and(|style| southstar_css::text_has_container_units(style.to_bytes()))
        {
            return true;
        }
        cur = southstar_dom::index::next_in_subtree(node, root, true);
    }
    false
}

fn containers_wanted(c: &RenderCtx) -> (bool, bool) {
    let mut units = dom_uses_container_units(c.doc());
    let mut want = units;
    for sheet in c.style_sheets() {
        if sheet.has_container_units() {
            units = true;
            want = true;
        }
        if sheet.has_container_rules() {
            want = true;
        }
    }
    (want, units)
}

fn selector_cache_wanted(c: &RenderCtx, predicted: Option<&ContainerMap>) -> bool {
    if predicted.is_some() && lock(&CONTAINERS).settled {
        return false;
    }
    c.style_sheets()
        .any(|sheet| sheet.has_container_rules() || sheet.has_container_units())
}

fn build_layout(c: &RenderCtx, styles: &StyleMap, viewport_width: f64) -> *mut NsBox {
    southstar_paint::list_ordinals_begin();
    let layout = southstar_layout_position::layout_build(BuildInput {
        doc: c.doc(),
        // SAFETY: styles is the style table this relayout computed for the context's document.
        styles: unsafe { StyleTable::from_ptr(styles) },
        viewport_width,
        // SAFETY: the focused input is null or a live node of the context's document.
        focused_input: unsafe { Node::from_ptr(c.focused_input) },
        caret_byte: c.caret_byte,
        anchor_byte: c.sel_anchor_byte,
        image_cache: c.images,
        base_url: c.base_url,
    });
    southstar_paint::list_ordinals_end();
    layout
}

fn record_page_state(c: &RenderCtx) {
    let page_rule = c
        .style_sheets()
        .filter_map(Sheet::page_rule)
        .last()
        .copied();
    *lock(&PAGE_RULE) = page_rule;
    let uses_hover = c.style_sheets().any(Sheet::has_hover_rules);
    let uses_active = c.style_sheets().any(Sheet::has_active_rules);
    USES_HOVER.store(uses_hover, Ordering::Relaxed);
    USES_ACTIVE.store(uses_active, Ordering::Relaxed);
}

fn relayout(
    c: &RenderCtx,
    out_layout: &mut *mut NsBox,
    mut profile: Option<&mut RenderProfile>,
) -> Rc<StyleMap> {
    if let Some(p) = profile.as_deref_mut() {
        *p = RenderProfile::default();
    }
    let timed = profile.is_some();
    let clock = || if timed { now_us() } else { 0 };

    let viewport_width = c.viewport_width;
    southstar_css_media::set_base_viewport(viewport_width, c.viewport_height);
    unsafe {
        southstar_css::set_focus_node(Node::from_ptr(c.focused_input));
        southstar_css::set_hover_node(Node::from_ptr(c.hover_node));
    }
    record_page_state(c);

    let t0 = clock();
    southstar_css::set_render_zoom(c.zoom());
    let (want_cq, uses_cq_units) = containers_wanted(c);
    let predicted = predicted_map(c.doc, viewport_width, want_cq);
    let cache_selectors = selector_cache_wanted(c, predicted.as_ref());
    if cache_selectors {
        southstar_css::selector_cache_begin();
    }
    let compute = |containers: Option<&ContainerMap>| {
        Rc::new(southstar_css::compute_styles(
            c.doc(),
            c.sheets,
            c.sheet_docs,
            containers,
        ))
    };
    let mut styles = compute(predicted.as_ref());
    let t1 = clock();

    style_pass(c, &styles);
    let t2 = clock();

    let mut layout = build_layout(c, &styles, viewport_width);
    let t3 = clock();
    if let Some(p) = profile.as_deref_mut() {
        p.css1_us = t1 - t0;
        p.style1_us = t2 - t1;
        p.layout1_us = t3 - t2;
    }

    let mut containers = ContainerMap::new();
    let mut container_sig = 0u64;
    let tc0 = clock();
    if want_cq {
        collect_containers(
            unsafe { BoxRef::from_ptr(layout) },
            &mut containers,
            Some(&mut container_sig),
        );
    }
    let tc1 = clock();
    let mut n_containers = containers.len();
    if let Some(p) = profile.as_deref_mut() {
        p.container_us = tc1 - tc0;
        p.containers = n_containers as c_uint;
    }
    let mut container_passes = if uses_cq_units { 3 } else { 1 };
    let mut stale = predicted.is_some();
    if settled(predicted.as_ref(), &containers) {
        container_passes = 0;
    }
    let mut pass = 0;
    while pass < container_passes && (n_containers > 0 || stale) {
        if let Some(p) = profile.as_deref_mut() {
            p.container_pass = glib::TRUE;
            p.container_passes += 1;
        }
        southstar_css::container_features_begin();
        let t4 = clock();
        let styles2 = compute(Some(&containers));
        let t5 = clock();
        let features_used = southstar_css::container_features_used();
        if (!features_used && !stale) || styles_tables_equal(&styles, &styles2) {
            if let Some(p) = profile.as_deref_mut() {
                p.css2_us += t5 - t4;
            }
            drop(styles2);
            break;
        }
        style_pass(c, &styles2);
        let t6 = clock();
        let layout2 = build_layout(c, &styles2, viewport_width);
        let t7 = clock();
        if let Some(p) = profile.as_deref_mut() {
            p.css2_us += t5 - t4;
            p.style2_us += t6 - t5;
            p.layout2_us += t7 - t6;
        }
        unsafe {
            BoxPool::free_tree(BoxRef::from_ptr(layout));
        }
        layout = layout2;
        styles = styles2;
        stale = false;
        if pass + 1 < container_passes {
            containers.clear();
            let tr0 = clock();
            let mut next_sig = 0u64;
            collect_containers(
                unsafe { BoxRef::from_ptr(layout) },
                &mut containers,
                Some(&mut next_sig),
            );
            let tr1 = clock();
            n_containers = containers.len();
            if let Some(p) = profile.as_deref_mut() {
                p.container_us += tr1 - tr0;
                p.containers = n_containers as c_uint;
            }
            if next_sig == container_sig {
                break;
            }
            container_sig = next_sig;
        }
        pass += 1;
    }
    drop(containers);
    drop(predicted);
    remember(c.doc, viewport_width, layout, want_cq);
    if cache_selectors {
        southstar_css::selector_cache_end();
    }
    southstar_css::set_focus_node(None);
    southstar_css::set_hover_node(None);
    *out_layout = layout;
    styles
}

pub fn page_uses_hover() -> bool {
    USES_HOVER.load(Ordering::Relaxed)
}

pub fn page_uses_active() -> bool {
    USES_ACTIVE.load(Ordering::Relaxed)
}

pub fn page_rule() -> Option<PageRule> {
    *lock(&PAGE_RULE)
}

pub unsafe fn ns_render_relayout_profile(
    c: *const RenderCtx,
    out_layout: *mut *mut NsBox,
    profile: *mut RenderProfile,
) -> Option<Rc<StyleMap>> {
    let out_layout = unsafe { out_layout.as_mut() }?;
    *out_layout = ptr::null_mut();
    let c = unsafe { c.as_ref() }?;
    Some(relayout(c, out_layout, unsafe { profile.as_mut() }))
}

pub unsafe fn ns_render_relayout(
    c: *const RenderCtx,
    out_layout: *mut *mut NsBox,
) -> Option<Rc<StyleMap>> {
    unsafe { ns_render_relayout_profile(c, out_layout, ptr::null_mut()) }
}

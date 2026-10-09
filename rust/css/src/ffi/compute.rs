//! Southstar — the C ABI of computing a document's styles: the walk that gathers, shares or cascades each element's style and its pseudo-elements', framed documents with their own sheets and viewport, and the incremental restyle that reuses the previous pass's styles for clean elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{c_char, c_int, c_uint, c_void};
use core::mem::size_of;
use core::ptr;
use core::slice;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use southstar_dom::{Kind, Node, NsNode, children};
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};

use super::cascade::{RawMatch, ns_css_cascade_apply};
use super::computed_units::RawStyle;
use super::container::{
    ns_css_container_map_signature, ns_css_container_stack_pop, ns_css_container_stack_push,
    ns_css_container_stack_reset,
};
use super::custom_props::{RawVarMatch, ns_css_build_vars, ns_css_compute_registered_vars};
use super::element_state::ns_css_language_cache_reset;
use super::fixups::{
    ns_css_display_contents_to_none, ns_css_frame_viewport_from_style,
    ns_css_strip_native_widget_decorations,
};
use super::gather::{
    RawDest, ns_css_ancestor_filter_begin, ns_css_ancestor_filter_end,
    ns_css_ancestor_filter_enter, ns_css_ancestor_filter_leave, ns_css_ancestor_filter_restore,
    ns_css_ancestor_filter_save, ns_css_ancestor_filter_subject, ns_css_decl_sheet_cache_trim,
    ns_css_gather_element_declarations, ns_css_gather_matches, ns_css_rule_index_ensure,
};
use super::layers::ns_css_layer_ranks_build;
use super::matcher::{
    ns_css_active_node, ns_css_focus_node, ns_css_fullscreen_node, ns_css_has_memo_begin,
    ns_css_has_memo_end, ns_css_hover_node, ns_css_selector_batch_begin, ns_css_selector_batch_end,
};
use super::pending::{RawPendingMatch, ns_css_resolve_pending};
use super::registry::{
    ns_css_registered_property_serial, ns_css_registered_props_begin, ns_css_registered_props_end,
};
use super::restyle::{ns_css_restyle_dirty, ns_css_restyle_dirty_clear, ns_css_restyle_prepare};
use super::selector::ns_css_selector_attr_ancestor_hashes;
use super::sheet::RawSheet;
use super::style_alloc::{ns_style_alloc, ns_style_clone_shared, ns_style_free};
use super::style_share::{
    ns_css_style_share_begin, ns_css_style_share_end, ns_css_style_share_find,
    ns_css_style_share_insert,
};
use super::ua::ns_css_ua_sheet;
use super::value::{KIND_LENGTH, ns_css_value_free};
use super::vars::ns_var_map_unref;
use super::viewport;
use crate::display::{BOX_CONTENTS, BOX_NONE};
use crate::incremental::{self, PassKey};
use crate::prop::Prop;
use crate::units::PX;

const MAX_DEPTH: i32 = 512;
const ORIGIN_UA: c_int = 0;
const ORIGIN_AUTHOR: c_int = 2;
const NODE_QUIRKS: u32 = 1 << 5;
const PE_BEFORE: c_uint = 1;
const PE_AFTER: c_uint = 2;
const PSEUDO_ELEMENTS: [c_uint; 9] = [1, 2, 3, 4, 5, 6, 7, 8, 9];
const HIDDEN_BEFORE: usize = 9;
const HIDDEN_AFTER: usize = 10;

unsafe extern "C" {
    fn g_array_set_size(array: *mut GArray, length: c_uint) -> *mut GArray;
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
}

fn pseudo_slot(pe: c_uint) -> usize {
    match pe {
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        8 => 4,
        5 => 5,
        6 => 6,
        7 => 7,
        _ => 8,
    }
}

unsafe extern "C" fn value_free_notify(data: *mut c_void) {
    unsafe { ns_css_value_free(data.cast()) };
}

unsafe extern "C" fn style_free_notify(data: *mut c_void) {
    unsafe { ns_style_free(data.cast()) };
}

unsafe extern "C" fn var_map_unref_notify(data: *mut c_void) {
    unsafe { ns_var_map_unref(data.cast()) };
}

unsafe fn items<'a, T>(data: *const T, len: usize) -> &'a [T] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, len) }
    }
}

struct Arrays {
    matches: *mut GArray,
    vars: *mut GArray,
    pending: *mut GArray,
}

impl Arrays {
    fn new() -> Self {
        let array =
            |size: usize| unsafe { glib::g_array_new(glib::FALSE, glib::FALSE, size as c_uint) };
        Arrays {
            matches: array(size_of::<RawMatch>()),
            vars: array(size_of::<RawVarMatch>()),
            pending: array(size_of::<RawPendingMatch>()),
        }
    }

    fn clear(&self) {
        for array in [self.matches, self.vars, self.pending] {
            unsafe { g_array_set_size(array, 0) };
        }
    }

    fn dest(&self, pe: c_uint) -> RawDest {
        RawDest {
            pe,
            out: self.matches,
            var_out: self.vars,
            pending_out: self.pending,
        }
    }
}

impl Drop for Arrays {
    fn drop(&mut self) {
        for array in [self.matches, self.vars, self.pending] {
            unsafe { g_array_free(array, glib::TRUE) };
        }
    }
}

struct Scratch {
    element: Arrays,
    owned: *mut GPtrArray,
    pseudo: Vec<Arrays>,
    pseudo_owned: *mut GPtrArray,
}

impl Scratch {
    fn new() -> Self {
        let owned = || unsafe { glib::g_ptr_array_new_with_free_func(Some(value_free_notify)) };
        Scratch {
            element: Arrays::new(),
            owned: owned(),
            pseudo: Vec::new(),
            pseudo_owned: owned(),
        }
    }

    fn pseudo(&mut self, index: usize) -> &Arrays {
        while self.pseudo.len() <= index {
            self.pseudo.push(Arrays::new());
        }
        &self.pseudo[index]
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        unsafe {
            glib::g_ptr_array_free(self.owned, glib::TRUE);
            glib::g_ptr_array_free(self.pseudo_owned, glib::TRUE);
        }
    }
}

thread_local! {
    static SCRATCH: Cell<Option<Box<Scratch>>> = const { Cell::new(None) };
}

struct StyleRef(*mut RawStyle);

unsafe impl Send for StyleRef {}

impl Drop for StyleRef {
    fn drop(&mut self) {
        unsafe { ns_style_free(self.0) };
    }
}

type Styles = HashMap<usize, StyleRef>;

struct Previous {
    key: PassKey,
    styles: Styles,
}

struct Incremental {
    previous: Option<Previous>,
    before_change: Option<Styles>,
}

static INCREMENTAL: Mutex<Incremental> = Mutex::new(Incremental {
    previous: None,
    before_change: None,
});
static EXCLUDED: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
static EXCLUDED_COUNT: AtomicUsize = AtomicUsize::new(0);
static ZOOM: AtomicU64 = AtomicU64::new(1f64.to_bits());
static NEXT_SHARE_ID: AtomicU64 = AtomicU64::new(0);

fn incremental_state() -> MutexGuard<'static, Incremental> {
    INCREMENTAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn set_excluded(node: usize, exclude: bool) {
    let mut set = EXCLUDED.lock().unwrap_or_else(PoisonError::into_inner);
    let set = set.get_or_insert_with(HashSet::new);
    if exclude {
        set.insert(node);
    } else {
        set.remove(&node);
    }
    EXCLUDED_COUNT.store(set.len(), Ordering::Relaxed);
}

fn excluded(node: usize) -> bool {
    EXCLUDED_COUNT.load(Ordering::Relaxed) != 0
        && EXCLUDED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|set| set.contains(&node))
}

fn zoom() -> f64 {
    f64::from_bits(ZOOM.load(Ordering::Relaxed))
}

fn font_size_px(style: &RawStyle) -> Option<f64> {
    let value = unsafe { style.values[Prop::FontSize.id()].as_ref() }?;
    let length = unsafe { value.u.length };
    (value.kind == KIND_LENGTH && length.unit == PX).then_some(length.v)
}

fn frame_viewport(frame: Node<'_>, parent: *const RawStyle) -> Option<(f64, f64)> {
    let size_frame = viewport::frame_viewport()?;
    let (mut w, mut h) = (0.0, 0.0);
    if unsafe { ns_css_frame_viewport_from_style(parent, &mut w, &mut h) } == 0 {
        unsafe { size_frame(frame.as_ptr(), &mut w, &mut h) };
    }
    let (vw, vh) = viewport::get();
    let differs = (w - vw).abs() > 0.01 || (h - vh).abs() > 0.01;
    (w > 0.0 && h > 0.0 && differs).then(|| viewport::replace(w, h))
}

fn pseudo_mask(sheet: *const RawSheet) -> c_uint {
    unsafe { sheet.as_ref() }.map_or(0, |sheet| sheet.pseudo_mask)
}

struct Walk<'a> {
    ua: *const RawSheet,
    layer_ranks: *mut GHashTable,
    out: *mut GHashTable,
    registered: *mut GHashTable,
    adjust_cache: *mut GHashTable,
    doc_sheets: Option<HashMap<usize, Vec<*const RawSheet>>>,
    reuse: Option<&'a Styles>,
    scratch: &'a mut Scratch,
    root_px: f64,
    depth: i32,
    reused: u32,
    recomputed: u32,
}

impl Walk<'_> {
    fn walk(
        &mut self,
        node: Node<'_>,
        author: &[*const RawSheet],
        parent: *const RawStyle,
        layout_parent: *const RawStyle,
        under_dirty: bool,
    ) {
        if self.depth >= MAX_DEPTH {
            return;
        }
        self.depth += 1;
        let ptr = node.as_ptr();
        let is_document = node.kind() == Kind::Document;
        let saved_viewport = if is_document {
            node.parent()
                .and_then(|frame| frame_viewport(frame, parent))
        } else {
            None
        };
        let own_sheets: Vec<*const RawSheet>;
        let author = match &self.doc_sheets {
            Some(by_doc) if is_document => {
                own_sheets = by_doc.get(&(ptr as usize)).cloned().unwrap_or_default();
                &own_sheets[..]
            }
            _ => author,
        };
        let mut child_parent = parent;
        let mut child_layout_parent = layout_parent;
        let mut recurse_dirty = under_dirty;
        if node.is_element() {
            let mut dirty =
                under_dirty || unsafe { ns_css_restyle_dirty(ptr) } != 0 || excluded(ptr as usize);
            let previous = if dirty {
                None
            } else {
                self.reuse
                    .and_then(|styles| styles.get(&(ptr as usize)))
                    .map(|style| style.0)
            };
            let style = match previous {
                Some(style) => {
                    unsafe { (*style).ref_count += 1 };
                    self.reused += 1;
                    style
                }
                None => {
                    self.recomputed += 1;
                    dirty = true;
                    self.compute_element(node, author, parent, layout_parent)
                }
            };
            unsafe { glib::g_hash_table_insert(self.out, ptr.cast_mut().cast(), style.cast()) };
            let computed = unsafe { &*style };
            child_parent = style;
            if computed.display.box_ != BOX_CONTENTS {
                child_layout_parent = style;
            }
            if self.root_px <= 0.0 {
                if let Some(px) = font_size_px(computed) {
                    self.root_px = px;
                }
            }
            recurse_dirty = dirty;
        }
        let pushed = ns_css_container_stack_push(ptr.cast()) != 0;
        let outer_filter = if is_document && node.parent().is_some() {
            ns_css_ancestor_filter_save()
        } else {
            ptr::null_mut()
        };
        let filter_element = unsafe { ns_css_ancestor_filter_enter(ptr) } != 0;
        for child in children(node) {
            self.walk(
                child,
                author,
                child_parent,
                child_layout_parent,
                recurse_dirty,
            );
        }
        if filter_element {
            unsafe { ns_css_ancestor_filter_leave(ptr) };
        }
        unsafe { ns_css_ancestor_filter_restore(outer_filter) };
        if pushed {
            ns_css_container_stack_pop();
        }
        if let Some((w, h)) = saved_viewport {
            viewport::replace(w, h);
        }
        self.depth -= 1;
    }

    #[inline(never)]
    fn compute_element(
        &mut self,
        node: Node<'_>,
        author: &[*const RawSheet],
        parent: *const RawStyle,
        layout_parent: *const RawStyle,
    ) -> *mut RawStyle {
        let ptr = node.as_ptr();
        let element = self.scratch.element.dest(0);
        let owned = self.scratch.owned;
        self.scratch.element.clear();
        unsafe { glib::g_ptr_array_set_size(owned, 0) };
        let mask = author.iter().fold(pseudo_mask(self.ua), |mask, &sheet| {
            mask | pseudo_mask(sheet)
        });
        let mut dests = [element; 10];
        let mut n = 1;
        if mask != 0 {
            for pe in PSEUDO_ELEMENTS {
                if mask & (1 << pe) == 0 {
                    continue;
                }
                let arrays = self.scratch.pseudo(n - 1);
                arrays.clear();
                dests[n] = arrays.dest(pe);
                n += 1;
            }
        }
        let n_dests = n as c_uint;
        let dests_ptr = dests.as_ptr();
        unsafe {
            ns_css_ancestor_filter_subject(ptr);
            ns_css_gather_matches(
                self.ua,
                ORIGIN_UA,
                0,
                ptr,
                dests_ptr.cast(),
                n_dests,
                self.layer_ranks,
            );
            for (i, &sheet) in author.iter().enumerate() {
                ns_css_gather_matches(
                    sheet,
                    ORIGIN_AUTHOR,
                    (i + 1) as c_int,
                    ptr,
                    dests_ptr.cast(),
                    n_dests,
                    self.layer_ranks,
                );
            }
            ns_css_gather_element_declarations(
                ptr,
                element.out,
                element.var_out,
                element.pending_out,
            );
        }
        let mut shared = ptr::null();
        let mut keyed = unsafe {
            ns_css_style_share_find(parent, self.root_px, dests_ptr, n_dests, &mut shared)
        } != 0;
        if !keyed {
            set_excluded(ptr as usize, true);
        }
        if !shared.is_null() {
            let style = unsafe { ns_style_clone_shared(shared) };
            unsafe { ns_css_display_contents_to_none(ptr, style) };
            self.scratch.element.clear();
            unsafe { glib::g_ptr_array_set_size(owned, 0) };
            return style;
        }
        let style = ns_style_alloc();
        let parent_vars = unsafe { parent.as_ref() }.map_or(ptr::null_mut(), |p| p.vars);
        let is_root = node.parent().is_some_and(|p| p.kind() == Kind::Document);
        unsafe {
            (*style).share_id = NEXT_SHARE_ID.fetch_add(1, Ordering::Relaxed) + 1;
            (*style).vars = ns_css_build_vars(
                parent_vars.cast(),
                element.var_out,
                self.registered,
                self.adjust_cache,
            )
            .cast();
            ns_css_resolve_pending(
                element.pending_out,
                (*style).vars,
                self.registered,
                element.out,
                owned,
                ptr,
            );
            ns_css_cascade_apply(
                element.out,
                style,
                parent,
                layout_parent,
                glib::boolean(is_root),
                self.root_px,
            );
            ns_css_compute_registered_vars(style, parent, self.registered, self.root_px);
            ns_css_strip_native_widget_decorations(ptr, style);
            if ns_css_display_contents_to_none(ptr, style) != 0 {
                keyed = false;
            }
        }
        self.scratch.element.clear();
        unsafe { glib::g_ptr_array_set_size(owned, 0) };
        for dest in &dests[1..n] {
            self.pseudo_style(ptr, style, dest);
        }
        if keyed {
            ns_css_style_share_insert(style);
        }
        style
    }

    fn pseudo_style(&mut self, node: *const NsNode, style: *mut RawStyle, dest: &RawDest) {
        let len = |array: *mut GArray| unsafe { array.as_ref() }.map_or(0, |a| a.len);
        if len(dest.out) == 0 && len(dest.pending_out) == 0 {
            return;
        }
        let owned = self.scratch.pseudo_owned;
        let generated = dest.pe == PE_BEFORE || dest.pe == PE_AFTER;
        let pseudo = ns_style_alloc();
        unsafe {
            (*pseudo).vars = ns_css_build_vars(
                (*style).vars.cast(),
                dest.var_out,
                self.registered,
                self.adjust_cache,
            )
            .cast();
            ns_css_resolve_pending(
                dest.pending_out,
                (*pseudo).vars,
                self.registered,
                dest.out,
                owned,
                node,
            );
            ns_css_cascade_apply(
                dest.out,
                pseudo,
                style,
                if generated { style } else { ptr::null() },
                glib::FALSE,
                self.root_px,
            );
            ns_css_compute_registered_vars(pseudo, style, self.registered, self.root_px);
        }
        let computed = unsafe { &*pseudo };
        let keep = !generated || !computed.values[Prop::Content.id()].is_null();
        let slot = if !keep {
            None
        } else if generated && computed.display.box_ == BOX_NONE {
            Some(if dest.pe == PE_BEFORE {
                HIDDEN_BEFORE
            } else {
                HIDDEN_AFTER
            })
        } else {
            Some(pseudo_slot(dest.pe))
        };
        match slot {
            Some(slot) => unsafe { (*style).pseudo_styles[slot] = pseudo },
            None => unsafe { ns_style_free(pseudo) },
        }
        unsafe { glib::g_ptr_array_set_size(owned, 0) };
    }
}

fn retained(out: *mut GHashTable) -> Styles {
    unsafe { glib::hash_table_entries(out) }
        .map(|(node, style)| {
            let style = style.cast::<RawStyle>();
            unsafe { (*style).ref_count += 1 };
            (node as usize, StyleRef(style))
        })
        .collect()
}

fn interaction_key(doc: usize, sheets: u64, containers: u64) -> PassKey {
    PassKey {
        doc,
        sheets,
        containers,
        focus: ns_css_focus_node() as usize,
        hover: ns_css_hover_node() as usize,
        active: ns_css_active_node() as usize,
        fullscreen: ns_css_fullscreen_node() as usize,
    }
}

fn ms(from: Instant, to: Instant) -> f64 {
    to.duration_since(from).as_secs_f64() * 1000.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_compute(
    doc: *mut NsNode,
    author_sheets: *const *const RawSheet,
    sheet_docs: *const *const NsNode,
    n_sheets: usize,
) -> *mut GHashTable {
    let out = unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_direct_hash),
            Some(glib::g_direct_equal),
            None,
            Some(style_free_notify),
        )
    };
    ns_css_language_cache_reset();
    let doc_node = unsafe { Node::from_ptr(doc) };
    let quirks = doc_node.is_some_and(|d| d.flags() & NODE_QUIRKS != 0);
    let ua = ns_css_ua_sheet(glib::boolean(quirks)).cast::<RawSheet>();
    let profile = std::env::var_os("NS_PROFILE").is_some();
    let started = Instant::now();
    let authors = unsafe { items(author_sheets, n_sheets) };
    unsafe {
        ns_css_rule_index_ensure(ua);
        for &sheet in authors {
            ns_css_rule_index_ensure(sheet);
        }
    }
    let indexed = Instant::now();
    let layer_ranks =
        unsafe { ns_css_layer_ranks_build(ua.cast(), author_sheets.cast(), n_sheets) };
    let registered = unsafe { ns_css_registered_props_begin(ua, author_sheets, n_sheets) };
    ns_css_decl_sheet_cache_trim();
    ns_css_container_stack_reset();
    ns_css_style_share_begin();
    let adjust_cache = unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_direct_hash),
            Some(glib::g_direct_equal),
            Some(var_map_unref_notify),
            Some(var_map_unref_notify),
        )
    };
    ns_css_has_memo_begin();
    ns_css_selector_batch_begin();

    let serial = |sheet: *const RawSheet| unsafe { sheet.as_ref() }.map_or(0, RawSheet::serial);
    let author_serials: Vec<u64> = authors.iter().map(|&sheet| serial(sheet)).collect();
    let sig = incremental::sheet_signature(
        serial(ua),
        &author_serials,
        ns_css_registered_property_serial(),
    );
    let eligible =
        unsafe { ns_css_restyle_prepare(ua.cast(), author_sheets.cast(), n_sheets, sig) } != 0;
    let usable = std::env::var_os("NS_NO_INCR_RESTYLE").is_none()
        && eligible
        && incremental::zoom_allows_reuse(zoom());
    let containers = ns_css_container_map_signature();
    let key = interaction_key(doc as usize, sig, containers);
    let previous = incremental_state().previous.take();
    let active = usable && previous.as_ref().is_some_and(|p| p.key == key);

    ns_css_ancestor_filter_begin(ns_css_selector_attr_ancestor_hashes());
    let doc_sheets = (!sheet_docs.is_null()).then(|| {
        let mut by_doc: HashMap<usize, Vec<*const RawSheet>> = HashMap::new();
        for (&owner, &sheet) in unsafe { items(sheet_docs, n_sheets) }.iter().zip(authors) {
            by_doc.entry(owner as usize).or_default().push(sheet);
        }
        by_doc
    });
    let mut scratch = SCRATCH.take().unwrap_or_else(|| Box::new(Scratch::new()));
    let mut walk = Walk {
        ua,
        layer_ranks,
        out,
        registered,
        adjust_cache,
        doc_sheets,
        reuse: previous.as_ref().filter(|_| active).map(|p| &p.styles),
        scratch: &mut scratch,
        root_px: 0.0,
        depth: 0,
        reused: 0,
        recomputed: 0,
    };
    if let Some(doc) = doc_node {
        walk.walk(doc, authors, ptr::null(), ptr::null(), false);
    }
    let (reused, recomputed) = (walk.reused, walk.recomputed);
    SCRATCH.set(Some(scratch));
    ns_css_ancestor_filter_end();

    if usable {
        let current = Previous {
            key: interaction_key(doc as usize, sig, containers),
            styles: retained(out),
        };
        let mut state = incremental_state();
        let stale = state.before_change.take();
        state.before_change = previous.map(|p| p.styles);
        state.previous = Some(current);
        drop(state);
        drop(stale);
        if profile {
            glib::stderr_write(
                format!(
                    "[incr] active={} reused={reused} recomputed={recomputed}\n",
                    i32::from(active)
                )
                .as_bytes(),
            );
        }
    } else if let Some(previous) = previous {
        let stale = incremental_state().before_change.take();
        drop(previous);
        drop(stale);
    }
    ns_css_restyle_dirty_clear();
    ns_css_has_memo_end();
    ns_css_selector_batch_end();
    ns_css_style_share_end();
    unsafe {
        glib::g_hash_table_destroy(adjust_cache);
        glib::g_hash_table_destroy(layer_ranks);
    }
    ns_css_registered_props_end();
    if profile {
        let finished = Instant::now();
        glib::stderr_write(
            format!(
                "[profile]   css.idx={:.1}ms css.cascade={:.1}ms\n",
                ms(started, indexed),
                ms(indexed, finished)
            )
            .as_bytes(),
        );
    }
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_incremental_exclude(node: *const c_void, exclude: GBoolean) {
    if !node.is_null() {
        set_excluded(node as usize, exclude != 0);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_style_before_change(node: *const c_void) -> *const RawStyle {
    if node.is_null() {
        return ptr::null();
    }
    incremental_state()
        .before_change
        .as_ref()
        .and_then(|styles| styles.get(&(node as usize)))
        .map_or(ptr::null(), |style| style.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_render_zoom(zoom: f64) {
    let zoom = if zoom > 0.0 { zoom } else { 1.0 };
    ZOOM.store(zoom.to_bits(), Ordering::Relaxed);
}

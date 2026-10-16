//! Southstar — the C ABI of computing a document's styles: the walk that gathers, shares or cascades each element's style and its pseudo-elements', framed documents with their own sheets and viewport, and the incremental restyle that reuses the previous pass's styles for clean elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{c_int, c_uint, c_void};
use core::ptr;
use core::slice;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use southstar_dom::{Kind, Node, NsNode, children};
use southstar_glib::{self as glib, GBoolean, GHashTable};

use super::cascade::{Parents, cascade_apply};
use super::computed_units::{PROP_COUNT, RawStyle};
use super::container::{container_map_signature, container_stack_push};
use super::custom_props::{build_vars, compute_registered_vars};
use super::fixups::{
    display_contents_to_none, frame_viewport_from_style, strip_native_widget_decorations,
};
use super::gather::{
    Matches, Origin, ancestor_filter_begin, ancestor_filter_end, ancestor_filter_enter,
    ancestor_filter_leave, ancestor_filter_restore, ancestor_filter_save, ancestor_filter_subject,
    decl_sheet_cache_trim, gather_element_declarations, gather_matches, rule_index_ensure,
};
use super::layers::{LayerRanks, layer_ranks_build};
use super::matcher::{active_node, focus_node, fullscreen_node, hover_node};
use super::pending::{Resolved, resolve_pending};
use super::registry::{registered_property_serial, registered_props_begin, registered_props_end};
use super::selector::attr_ancestor_hashes;
use super::selector_view::SheetRef;
use super::sheet::RawSheet;
use super::style_alloc::{ns_style_free, style_alloc, style_clone_shared};
use super::style_share::{
    Shared, style_share_begin, style_share_end, style_share_find, style_share_insert,
};
use super::ua::ua_sheet;
use super::value::{KIND_LENGTH, OwnedValue};
use super::values::ns_css_value_equal;
use super::vars::var_map_unref;
use super::viewport;
use crate::NsCssValue;
use crate::display::{BOX_CONTENTS, BOX_NONE};
use crate::incremental::{self, PassKey};
use crate::prop::Prop;
use crate::units::PX;
use crate::{container, element_state, matcher, restyle};

const MAX_DEPTH: i32 = 512;
const ORIGIN_UA: c_int = 0;
const ORIGIN_AUTHOR: c_int = 2;
const NODE_QUIRKS: u32 = 1 << 5;
const PE_BEFORE: c_uint = 1;
const PE_AFTER: c_uint = 2;
const PSEUDO_ELEMENTS: [c_uint; 9] = [1, 2, 3, 4, 5, 6, 7, 8, 9];
const HIDDEN_BEFORE: usize = 9;
const HIDDEN_AFTER: usize = 10;

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

unsafe extern "C" fn style_free_notify(data: *mut c_void) {
    unsafe { ns_style_free(data.cast()) };
}

unsafe extern "C" fn var_map_unref_notify(data: *mut c_void) {
    unsafe { var_map_unref(data.cast()) };
}

unsafe fn items<'a, T>(data: *const T, len: usize) -> &'a [T] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, len) }
    }
}

struct Scratch {
    matches: Vec<Matches>,
    owned: Vec<OwnedValue>,
    pseudo_owned: Vec<OwnedValue>,
}

impl Scratch {
    fn new() -> Self {
        Scratch {
            matches: Vec::new(),
            owned: Vec::new(),
            pseudo_owned: Vec::new(),
        }
    }
}

fn targets(matches: &mut Vec<Matches>, n: usize) -> &mut [Matches] {
    if matches.len() < n {
        matches.resize_with(n, Matches::default);
    }
    let targets = &mut matches[..n];
    for target in targets.iter_mut() {
        target.clear();
    }
    targets
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

impl StyleRef {
    fn get(&self) -> &RawStyle {
        // SAFETY: a StyleRef holds a reference on a live style, so the style outlives it.
        unsafe { &*self.0 }
    }
}

fn same_value(a: *const NsCssValue, b: *const NsCssValue) -> bool {
    // SAFETY: both values belong to styles that stay alive for the whole style pass.
    ptr::eq(a, b) || unsafe { ns_css_value_equal(a, b) } != 0
}

fn keeps_children(before: &RawStyle, after: &RawStyle, variables_may_differ: bool) -> bool {
    before.display == after.display
        && (!variables_may_differ || ptr::eq(before.vars, after.vars))
        && (0..PROP_COUNT).all(|prop| {
            !super::cascade::passed_to_children(prop)
                || same_value(before.values[prop], after.values[prop])
        })
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

fn frame_viewport(frame: Node<'_>, parent: Option<&RawStyle>) -> Option<(f64, f64)> {
    let size_frame = viewport::frame_viewport()?;
    let (w, h) = frame_viewport_from_style(parent).unwrap_or_else(|| {
        let (mut w, mut h) = (0.0, 0.0);
        unsafe { size_frame(frame.as_ptr(), &mut w, &mut h) };
        (w, h)
    });
    let (vw, vh) = viewport::get();
    let differs = (w - vw).abs() > 0.01 || (h - vh).abs() > 0.01;
    (w > 0.0 && h > 0.0 && differs).then(|| viewport::replace(w, h))
}

fn pseudo_mask(sheet: Option<&RawSheet>) -> c_uint {
    sheet.map_or(0, |sheet| sheet.pseudo_mask)
}

type Sheets<'a> = [Option<&'a RawSheet>];

#[derive(Clone, Copy)]
struct Pass {
    registered: *mut GHashTable,
    adjust_cache: *mut GHashTable,
    root_px: f64,
}

struct Walk<'a> {
    ua: Option<&'a RawSheet>,
    layer_ranks: &'a LayerRanks,
    out: *mut GHashTable,
    registered: *mut GHashTable,
    adjust_cache: *mut GHashTable,
    doc_sheets: Option<HashMap<usize, Vec<Option<&'a RawSheet>>>>,
    reuse: Option<&'a Styles>,
    scratch: &'a mut Scratch,
    root_px: f64,
    depth: i32,
    reused: u32,
    recomputed: u32,
}

impl<'a> Walk<'a> {
    fn walk(
        &mut self,
        node: Node<'_>,
        author: &Sheets<'a>,
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
                .and_then(|frame| frame_viewport(frame, unsafe { parent.as_ref() }))
        } else {
            None
        };
        let own_sheets: Vec<Option<&'a RawSheet>>;
        let author = match &self.doc_sheets {
            Some(by_doc) if is_document => {
                own_sheets = by_doc.get(&(ptr as usize)).cloned().unwrap_or_default();
                &own_sheets[..]
            }
            _ => author,
        };
        let nested_document = is_document && node.parent().is_some();
        let (mut child_parent, mut child_layout_parent) = if nested_document {
            (ptr::null(), ptr::null())
        } else {
            (parent, layout_parent)
        };
        let outer_root_px = if nested_document {
            core::mem::replace(&mut self.root_px, 0.0)
        } else {
            self.root_px
        };
        let mut recurse_dirty = under_dirty;
        if node.is_element() {
            let restyled = under_dirty || restyle::is_dirty(node);
            let inline_styled = !restyled && restyle::inline_style_changed(node);
            let animated = !restyled && excluded(ptr as usize);
            let own_only = inline_styled || animated;
            let mut dirty = restyled || own_only;
            let reuse = self.reuse;
            let before = reuse.and_then(|styles| styles.get(&(ptr as usize)));
            let previous = if dirty {
                None
            } else {
                before.map(|style| style.0)
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
            if self.root_px <= 0.0
                && let Some(px) = font_size_px(computed)
            {
                self.root_px = px;
            }
            recurse_dirty = dirty
                && !(own_only
                    && before.is_some_and(|before| {
                        keeps_children(before.get(), computed, inline_styled)
                    }));
        }
        let pushed = container_stack_push(node);
        let outer_filter = if nested_document {
            ancestor_filter_save()
        } else {
            None
        };
        let filter_element = ancestor_filter_enter(node);
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
            ancestor_filter_leave(node);
        }
        ancestor_filter_restore(outer_filter);
        if pushed {
            container::stack_pop();
        }
        if let Some((w, h)) = saved_viewport {
            viewport::replace(w, h);
        }
        if nested_document {
            self.root_px = outer_root_px;
        }
        self.depth -= 1;
    }

    #[inline(never)]
    fn compute_element(
        &mut self,
        node: Node<'_>,
        author: &Sheets<'a>,
        parent: *const RawStyle,
        layout_parent: *const RawStyle,
    ) -> *mut RawStyle {
        let mask = author.iter().fold(pseudo_mask(self.ua), |mask, &sheet| {
            mask | pseudo_mask(sheet)
        });
        let mut pes = [0; 10];
        let mut n = 1;
        if mask != 0 {
            for pe in PSEUDO_ELEMENTS {
                if mask & (1 << pe) == 0 {
                    continue;
                }
                pes[n] = pe;
                n += 1;
            }
        }
        let pes = &pes[..n];
        let Scratch {
            matches,
            owned,
            pseudo_owned,
        } = &mut *self.scratch;
        owned.clear();
        let outs = targets(matches, n);
        ancestor_filter_subject(node);
        if let Some(ua) = self.ua {
            let origin = Origin {
                origin: ORIGIN_UA,
                sheet_index: 0,
            };
            gather_matches(ua, origin, node, pes, outs, self.layer_ranks);
        }
        for (i, sheet) in author.iter().enumerate() {
            if let Some(sheet) = sheet {
                let origin = Origin {
                    origin: ORIGIN_AUTHOR,
                    sheet_index: (i + 1) as c_int,
                };
                gather_matches(sheet, origin, node, pes, outs, self.layer_ranks);
            }
        }
        gather_element_declarations(node, &mut outs[0]);
        let parent_style = unsafe { parent.as_ref() };
        let shared = style_share_find(parent_style, self.root_px, pes, outs);
        let mut keyed = !matches!(shared, Shared::Unkeyed);
        if !keyed {
            set_excluded(node.as_ptr() as usize, true);
        }
        if let Shared::Hit(shared) = shared {
            let style = style_clone_shared(unsafe { &*shared });
            display_contents_to_none(node, unsafe { &mut *style });
            outs[0].clear();
            owned.clear();
            return style;
        }
        let style_ptr = style_alloc();
        let style = unsafe { &mut *style_ptr };
        let parent_vars = parent_style.map_or(ptr::null_mut(), |p| p.vars);
        let is_root = node.parent().is_some_and(|p| p.kind() == Kind::Document);
        let pass = Pass {
            registered: self.registered,
            adjust_cache: self.adjust_cache,
            root_px: self.root_px,
        };
        style.share_id = NEXT_SHARE_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let (element, pseudo) = outs.split_at_mut(1);
        let Matches {
            decls,
            vars,
            pending,
        } = &mut element[0];
        style.vars = build_vars(parent_vars, vars, pass.registered, pass.adjust_cache);
        let resolved = Resolved {
            matches: decls,
            owned,
        };
        resolve_pending(pending, style.vars, pass.registered, resolved, node);
        let parents = Parents {
            style: parent_style,
            layout: unsafe { layout_parent.as_ref() },
        };
        cascade_apply(decls, style, &parents, is_root, pass.root_px);
        compute_registered_vars(style, parent_style, pass.registered, pass.root_px);
        strip_native_widget_decorations(node, style);
        if display_contents_to_none(node, style) {
            keyed = false;
        }
        element[0].clear();
        owned.clear();
        for (&pe, dest) in pes[1..].iter().zip(pseudo.iter_mut()) {
            pseudo_style(&pass, node, style, pe, dest, pseudo_owned);
        }
        if keyed {
            style_share_insert(style_ptr);
        }
        style_ptr
    }
}

fn pseudo_style(
    pass: &Pass,
    node: Node<'_>,
    style: &mut RawStyle,
    pe: c_uint,
    dest: &mut Matches,
    owned: &mut Vec<OwnedValue>,
) {
    if dest.decls.is_empty() && dest.pending.is_empty() {
        return;
    }
    let generated = pe == PE_BEFORE || pe == PE_AFTER;
    let pseudo_ptr = style_alloc();
    let pseudo = unsafe { &mut *pseudo_ptr };
    let Matches {
        decls,
        vars,
        pending,
    } = dest;
    pseudo.vars = build_vars(style.vars, vars, pass.registered, pass.adjust_cache);
    let resolved = Resolved {
        matches: decls,
        owned,
    };
    resolve_pending(pending, pseudo.vars, pass.registered, resolved, node);
    let parent: &RawStyle = style;
    let parents = Parents {
        style: Some(parent),
        layout: generated.then_some(parent),
    };
    cascade_apply(decls, pseudo, &parents, false, pass.root_px);
    compute_registered_vars(pseudo, Some(parent), pass.registered, pass.root_px);
    let keep = !generated || !pseudo.values[Prop::Content.id()].is_null();
    let slot = if !keep {
        None
    } else if generated && pseudo.display.box_ == BOX_NONE {
        Some(if pe == PE_BEFORE {
            HIDDEN_BEFORE
        } else {
            HIDDEN_AFTER
        })
    } else {
        Some(pseudo_slot(pe))
    };
    match slot {
        Some(slot) => style.pseudo_styles[slot] = pseudo_ptr,
        None => unsafe { ns_style_free(pseudo_ptr) },
    }
    owned.clear();
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
        focus: Node::ptr_or_null(focus_node()) as usize,
        hover: Node::ptr_or_null(hover_node()) as usize,
        active: Node::ptr_or_null(active_node()) as usize,
        fullscreen: Node::ptr_or_null(fullscreen_node()) as usize,
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
    element_state::reset_language_cache();
    let doc_node = unsafe { Node::from_ptr(doc) };
    let quirks = doc_node.is_some_and(|d| d.flags() & NODE_QUIRKS != 0);
    let ua_ptr = ua_sheet(quirks);
    let profile = std::env::var_os("NS_PROFILE").is_some();
    let started = Instant::now();
    let author_ptrs = unsafe { items(author_sheets, n_sheets) };
    for &sheet in core::iter::once(&ua_ptr).chain(author_ptrs) {
        if let Some(sheet) = unsafe { sheet.cast_mut().as_mut() } {
            rule_index_ensure(sheet);
        }
    }
    let indexed = Instant::now();
    let ua = unsafe { ua_ptr.as_ref() };
    let authors: Vec<Option<&RawSheet>> = author_ptrs
        .iter()
        .map(|&sheet| unsafe { sheet.as_ref() })
        .collect();
    let sheet_refs: Vec<SheetRef<'_>> = ua
        .into_iter()
        .chain(authors.iter().flatten().copied())
        .map(SheetRef::new)
        .collect();
    let layer_ranks = layer_ranks_build(&sheet_refs);
    let registered = registered_props_begin(&sheet_refs);
    decl_sheet_cache_trim();
    container::stack_reset();
    style_share_begin();
    let adjust_cache = unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_direct_hash),
            Some(glib::g_direct_equal),
            Some(var_map_unref_notify),
            Some(var_map_unref_notify),
        )
    };
    matcher::has_memo_begin();
    matcher::batch_begin();

    let serial = |sheet: Option<&RawSheet>| sheet.map_or(0, RawSheet::serial);
    let author_serials: Vec<u64> = authors.iter().map(|&sheet| serial(sheet)).collect();
    let sig =
        incremental::sheet_signature(serial(ua), &author_serials, registered_property_serial());
    let eligible = restyle::prepare(&sheet_refs, sig);
    let usable = std::env::var_os("NS_NO_INCR_RESTYLE").is_none()
        && eligible
        && incremental::zoom_allows_reuse(zoom());
    let containers = container_map_signature();
    let key = interaction_key(doc as usize, sig, containers);
    let previous = incremental_state().previous.take();
    let active = usable && previous.as_ref().is_some_and(|p| p.key == key);

    ancestor_filter_begin(attr_ancestor_hashes());
    let doc_sheets = (!sheet_docs.is_null()).then(|| {
        let mut by_doc: HashMap<usize, Vec<Option<&RawSheet>>> = HashMap::new();
        for (&owner, &sheet) in unsafe { items(sheet_docs, n_sheets) }.iter().zip(&authors) {
            by_doc.entry(owner as usize).or_default().push(sheet);
        }
        by_doc
    });
    let mut scratch = SCRATCH.take().unwrap_or_else(|| Box::new(Scratch::new()));
    let mut walk = Walk {
        ua,
        layer_ranks: &layer_ranks,
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
        walk.walk(doc, &authors, ptr::null(), ptr::null(), false);
    }
    let (reused, recomputed) = (walk.reused, walk.recomputed);
    SCRATCH.set(Some(scratch));
    ancestor_filter_end();

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
    restyle::clear_dirty();
    matcher::has_memo_end();
    matcher::batch_end();
    style_share_end();
    unsafe { glib::g_hash_table_destroy(adjust_cache) };
    registered_props_end();
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

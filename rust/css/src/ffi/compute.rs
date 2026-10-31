//! Southstar — the C ABI of computing a document's styles: the walk that gathers, shares or cascades each element's style and its pseudo-elements', framed documents with their own sheets and viewport, and the incremental restyle that reuses the previous pass's styles for clean elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{c_int, c_uint};
use core::ops::Deref;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use southstar_dom::{Kind, Node, NsNode, children};
use southstar_glib as glib;

use super::cascade::{Parents, cascade_apply};
use super::computed_units::{PROP_COUNT, RawStyle};
use super::container::{
    ContainerMap, container_map_signature, container_stack_push, with_container_map,
};
use super::custom_props::{AdjustCache, build_vars, compute_registered_vars};
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
use super::pending::resolve_pending;
use super::registry::{
    RegisteredProps, registered_property_serial, registered_props_begin, registered_props_end,
};
use super::selector::attr_ancestor_hashes;
use super::selector_view::SheetRef;
use super::sheet::Sheet;
use super::style_alloc::{RetainedStyle, style_alloc};
use super::style_map::StyleMap;
use super::style_share::{
    Shared, style_share_begin, style_share_end, style_share_find, style_share_insert,
};
use super::ua::ua_sheet;
use super::values::values_equal;
use super::viewport;
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

struct Scratch {
    matches: Vec<Matches>,
}

impl Scratch {
    fn new() -> Self {
        Scratch {
            matches: Vec::new(),
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

fn same_value(before: &RawStyle, after: &RawStyle, prop: usize) -> bool {
    values_equal(before.value_at(prop), after.value_at(prop))
}

fn keeps_children(before: &RawStyle, after: &RawStyle, variables_may_differ: bool) -> bool {
    before.display == after.display
        && (!variables_may_differ || before.same_vars(after))
        && (0..PROP_COUNT).all(|prop| {
            !super::cascade::passed_to_children(prop) || same_value(before, after, prop)
        })
}

type Styles = HashMap<usize, RetainedStyle>;

struct Previous {
    key: PassKey,
    styles: Styles,
}

struct Incremental {
    previous: Option<Previous>,
    before_change: Option<Styles>,
}

thread_local! {
    static INCREMENTAL: RefCell<Incremental> = const {
        RefCell::new(Incremental {
            previous: None,
            before_change: None,
        })
    };
}

static EXCLUDED: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
static EXCLUDED_COUNT: AtomicUsize = AtomicUsize::new(0);
static ZOOM: AtomicU64 = AtomicU64::new(1f64.to_bits());
static NEXT_SHARE_ID: AtomicU64 = AtomicU64::new(0);

fn incremental_state<R>(f: impl FnOnce(&mut Incremental) -> R) -> R {
    INCREMENTAL.with_borrow_mut(f)
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
    let (v, unit) = style.value_at(Prop::FontSize.id())?.length()?;
    (unit == PX).then_some(v)
}

fn frame_viewport(frame: Node<'_>, parent: Option<&RawStyle>) -> Option<(f64, f64)> {
    let size_frame = viewport::frame_viewport()?;
    let (w, h) = frame_viewport_from_style(parent).unwrap_or_else(|| size_frame(frame));
    let (vw, vh) = viewport::get();
    let differs = (w - vw).abs() > 0.01 || (h - vh).abs() > 0.01;
    (w > 0.0 && h > 0.0 && differs).then(|| viewport::replace(w, h))
}

fn pseudo_mask(sheet: Option<&Sheet>) -> c_uint {
    sheet.map_or(0, |sheet| sheet.pseudo_mask.get())
}

type Sheets<'a> = [Option<&'a Sheet>];

#[derive(Clone, Copy)]
struct Pass<'a> {
    registered: &'a RegisteredProps,
    adjust_cache: &'a AdjustCache,
    root_px: f64,
}

struct Walk<'a> {
    ua: Option<&'a Sheet>,
    layer_ranks: &'a LayerRanks,
    out: StyleMap,
    registered: &'a RegisteredProps,
    adjust_cache: &'a AdjustCache,
    doc_sheets: Option<HashMap<usize, Vec<Option<&'a Sheet>>>>,
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
        parent: Option<&RawStyle>,
        layout_parent: Option<&RawStyle>,
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
        let own_sheets: Vec<Option<&'a Sheet>>;
        let author = match &self.doc_sheets {
            Some(by_doc) if is_document => {
                own_sheets = by_doc.get(&(ptr as usize)).cloned().unwrap_or_default();
                &own_sheets[..]
            }
            _ => author,
        };
        let nested_document = is_document && node.parent().is_some();
        let (mut child_parent, mut child_layout_parent) = if nested_document {
            (None, None)
        } else {
            (parent, layout_parent)
        };
        let own_style: RetainedStyle;
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
            let previous = if dirty { None } else { before };
            own_style = match previous {
                Some(style) => {
                    self.reused += 1;
                    style.clone()
                }
                None => {
                    self.recomputed += 1;
                    dirty = true;
                    self.compute_element(node, author, parent, layout_parent)
                }
            };
            self.out.insert(ptr as usize, own_style.clone());
            let computed = own_style.get();
            child_parent = Some(computed);
            if computed.display.box_ != BOX_CONTENTS {
                child_layout_parent = Some(computed);
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
        parent: Option<&RawStyle>,
        layout_parent: Option<&RawStyle>,
    ) -> RetainedStyle {
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
        let Scratch { matches } = &mut *self.scratch;
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
        let parent_style = parent;
        let shared = style_share_find(parent_style, self.root_px, pes, outs);
        let mut keyed = !matches!(shared, Shared::Unkeyed);
        if !keyed {
            set_excluded(node.as_ptr() as usize, true);
        }
        if let Shared::Hit(mut style) = shared {
            display_contents_to_none(node, &mut style);
            outs[0].clear();
            return RetainedStyle::new(style);
        }
        let mut style = style_alloc();
        let parent_vars = parent_style.and_then(|p| p.vars.as_ref());
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
        style.vars = build_vars(parent_vars, vars, pass.registered, Some(pass.adjust_cache));
        resolve_pending(pending, style.vars.as_deref(), pass.registered, decls, node);
        let parents = Parents {
            style: parent_style,
            layout: layout_parent,
        };
        cascade_apply(decls, &mut style, &parents, is_root, pass.root_px);
        compute_registered_vars(&style, parent_style, pass.registered, pass.root_px);
        strip_native_widget_decorations(node, &mut style);
        if display_contents_to_none(node, &mut style) {
            keyed = false;
        }
        element[0].clear();
        for (&pe, dest) in pes[1..].iter().zip(pseudo.iter_mut()) {
            pseudo_style(&pass, node, &mut style, pe, dest);
        }
        let style = RetainedStyle::new(style);
        if keyed {
            style_share_insert(&style);
        }
        style
    }
}

fn pseudo_style(pass: &Pass, node: Node<'_>, style: &mut RawStyle, pe: c_uint, dest: &mut Matches) {
    if dest.decls.is_empty() && dest.pending.is_empty() {
        return;
    }
    let generated = pe == PE_BEFORE || pe == PE_AFTER;
    let mut pseudo = style_alloc();
    let Matches {
        decls,
        vars,
        pending,
    } = dest;
    pseudo.vars = build_vars(
        style.vars.as_ref(),
        vars,
        pass.registered,
        Some(pass.adjust_cache),
    );
    resolve_pending(
        pending,
        pseudo.vars.as_deref(),
        pass.registered,
        decls,
        node,
    );
    let parent: &RawStyle = style;
    let parents = Parents {
        style: Some(parent),
        layout: generated.then_some(parent),
    };
    cascade_apply(decls, &mut pseudo, &parents, false, pass.root_px);
    compute_registered_vars(&pseudo, Some(parent), pass.registered, pass.root_px);
    let keep = !generated || pseudo.values[Prop::Content.id()].is_some();
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
    if let Some(slot) = slot {
        style.pseudo_styles[slot] = Some(RetainedStyle::new(pseudo));
    }
}

fn retained(out: &StyleMap) -> Styles {
    out.styles()
        .map(|(node, style)| (node, style.clone()))
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

pub fn compute_styles(
    doc: Option<Node<'_>>,
    sheets: &[impl Deref<Target = Sheet>],
    sheet_docs: &[Node<'_>],
    containers: Option<&mut ContainerMap>,
) -> StyleMap {
    let authors: Vec<Option<&Sheet>> = sheets.iter().map(|sheet| Some(&**sheet)).collect();
    with_container_map(containers, || compute(doc, &authors, sheet_docs))
}

fn compute(
    doc_node: Option<Node<'_>>,
    authors: &[Option<&Sheet>],
    sheet_docs: &[Node<'_>],
) -> StyleMap {
    element_state::reset_language_cache();
    let doc = Node::ptr_or_null(doc_node);
    let quirks = doc_node.is_some_and(|d| d.flags() & NODE_QUIRKS != 0);
    let ua = Some(ua_sheet(quirks));
    let profile = std::env::var_os("NS_PROFILE").is_some();
    let started = Instant::now();
    for &sheet in core::iter::once(&ua).chain(authors).flatten() {
        rule_index_ensure(sheet);
    }
    let indexed = Instant::now();
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
    let adjust_cache = AdjustCache::default();
    matcher::has_memo_begin();
    matcher::batch_begin();

    let serial = |sheet: Option<&Sheet>| sheet.map_or(0, Sheet::serial);
    let author_serials: Vec<u64> = authors.iter().map(|&sheet| serial(sheet)).collect();
    let sig =
        incremental::sheet_signature(serial(ua), &author_serials, registered_property_serial());
    let eligible = restyle::prepare(&sheet_refs, sig);
    let usable = std::env::var_os("NS_NO_INCR_RESTYLE").is_none()
        && eligible
        && incremental::zoom_allows_reuse(zoom());
    let containers = container_map_signature();
    let key = interaction_key(doc as usize, sig, containers);
    let previous = incremental_state(|state| state.previous.take());
    let active = usable && previous.as_ref().is_some_and(|p| p.key == key);

    ancestor_filter_begin(attr_ancestor_hashes());
    let doc_sheets = (!sheet_docs.is_empty()).then(|| {
        let mut by_doc: HashMap<usize, Vec<Option<&Sheet>>> = HashMap::new();
        for (owner, &sheet) in sheet_docs.iter().zip(authors) {
            by_doc
                .entry(owner.as_ptr() as usize)
                .or_default()
                .push(sheet);
        }
        by_doc
    });
    let mut scratch = SCRATCH.take().unwrap_or_else(|| Box::new(Scratch::new()));
    let mut walk = Walk {
        ua,
        layer_ranks: &layer_ranks,
        out: StyleMap::default(),
        registered: &registered,
        adjust_cache: &adjust_cache,
        doc_sheets,
        reuse: previous.as_ref().filter(|_| active).map(|p| &p.styles),
        scratch: &mut scratch,
        root_px: 0.0,
        depth: 0,
        reused: 0,
        recomputed: 0,
    };
    if let Some(doc) = doc_node {
        walk.walk(doc, authors, None, None, false);
    }
    let (reused, recomputed) = (walk.reused, walk.recomputed);
    let out = core::mem::take(&mut walk.out);
    SCRATCH.set(Some(scratch));
    ancestor_filter_end();

    if usable {
        let current = Previous {
            key: interaction_key(doc as usize, sig, containers),
            styles: retained(&out),
        };
        let stale = incremental_state(|state| {
            let stale = state.before_change.take();
            state.before_change = previous.map(|p| p.styles);
            state.previous = Some(current);
            stale
        });
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
        let stale = incremental_state(|state| state.before_change.take());
        drop(previous);
        drop(stale);
    }
    restyle::clear_dirty();
    matcher::has_memo_end();
    matcher::batch_end();
    style_share_end();
    drop(adjust_cache);
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

pub fn incremental_exclude(node: *const NsNode, exclude: bool) {
    if !node.is_null() {
        set_excluded(node as usize, exclude);
    }
}

pub fn style_before_change(node: Node<'_>) -> Option<RetainedStyle> {
    incremental_state(|state| {
        state
            .before_change
            .as_ref()?
            .get(&(node.as_ptr() as usize))
            .cloned()
    })
}

pub fn set_render_zoom(zoom: f64) {
    let zoom = if zoom > 0.0 { zoom } else { 1.0 };
    ZOOM.store(zoom.to_bits(), Ordering::Relaxed);
}

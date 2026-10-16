//! Southstar — building the box tree for a DOM subtree: documents and frames, replaced and form-control boxes, slots and shadow roots, display: contents, flex and grid items, rendered fieldset legends and the runs, blocks and anonymous tables of a block container.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Kind as NodeKind, MAX_DEPTH, Node, NsNode, children};
use southstar_layout::{BoxPool, BoxRef};
use southstar_layout_content::tags;
use southstar_layout_geom::{
    style_is_absolute_or_fixed, style_is_block, style_is_contents, style_is_flex_container,
    style_is_grid_container, style_is_none,
};
use southstar_layout_tree::append_child;
use southstar_style::{StyleRef, StyleTable};

use crate::atomic::{node_is_frame_fallback, style_has_atomic_inline_box};
use crate::classify::{
    SHADOW_ATTR, contains_block_media, continues_inline_run, is_inline_dom,
    node_has_media_metadata, text_is_ws_only,
};
use crate::ffi;
use crate::generated::{
    append_generated_after, build_pseudo_block_for, build_pseudo_inline_for, pseudo_item_block,
    register_abs_pseudo,
};
use crate::media::{
    KIND_BLOCK, KIND_MATH, build_audio_box, build_canvas_box, build_image_box, build_svg_box,
    build_video_box, collect_box_bg_image,
};
use crate::run;
use crate::state;

type Pending = Option<BoxRef<'static>>;

fn same(a: *const NsNode, n: Node<'_>) -> bool {
    ptr::eq(a, n.as_ptr())
}

fn style_ptr(s: Option<StyleRef<'_>>) -> *const southstar_layout::Style {
    s.map_or(ptr::null(), StyleRef::as_ptr)
}

fn has_text(b: BoxRef<'_>) -> bool {
    b.text().is_some_and(|t| !t.is_empty())
}

fn inline_run(
    first: Node<'_>,
    last: Option<Node<'_>>,
    styles: StyleTable,
) -> Option<BoxRef<'static>> {
    let raw = run::build_inline_run(Some(first), Node::ptr_or_null(last), styles, true);
    ffi::box_ref(raw)
}

fn append_or_free(parent: BoxRef<'_>, run: Option<BoxRef<'static>>) {
    match run {
        Some(r) if has_text(r) => append_child(parent, r),
        Some(r) => BoxPool::free_tree(Some(r)),
        None => {}
    }
}

fn append_some(parent: BoxRef<'_>, child: Option<BoxRef<'static>>) {
    if let Some(c) = child {
        append_child(parent, c);
    }
}

pub fn shadow_root(host: Option<Node<'_>>) -> Option<Node<'_>> {
    let host = host.filter(|h| h.is_element())?;
    children(host).find(|c| c.is_element() && c.attr(SHADOW_ATTR).is_some())
}

fn slot_host(slot: Node<'_>) -> Option<Node<'_>> {
    let mut p = slot.parent();
    while let Some(n) = p {
        if n.is_element() && n.attr(SHADOW_ATTR).is_some() {
            return n.parent();
        }
        p = n.parent();
    }
    None
}

fn find_slot<'a>(scope: Node<'a>, name: &[u8]) -> Option<Node<'a>> {
    for c in children(scope) {
        if !c.is_element() || c.attr(SHADOW_ATTR).is_some() {
            continue;
        }
        if c.name().map(CStr::to_bytes) == Some(b"slot") {
            let slot_name = c.attr(c"name").map_or(&b""[..], CStr::to_bytes);
            if slot_name == name {
                return Some(c);
            }
        }
        if let Some(found) = find_slot(c, name) {
            return Some(found);
        }
    }
    None
}

pub fn flat_parent(n: Option<Node<'_>>) -> Option<Node<'_>> {
    let n = n?;
    let p = n.parent();
    let Some(parent) = p.filter(|_| n.is_element()) else {
        return p;
    };
    let Some(sr) = shadow_root(Some(parent)).filter(|sr| !ptr::eq(sr.as_ptr(), n.as_ptr())) else {
        return p;
    };
    let name = n.attr(c"slot").map_or(&b""[..], CStr::to_bytes);
    find_slot(sr, name).or(p)
}

pub fn build_block(n: Option<Node<'_>>, styles: StyleTable) -> Option<BoxRef<'static>> {
    let n = n?;
    if state::with(|st| st.build_block_depth) >= MAX_DEPTH {
        return None;
    }
    state::with(|st| st.build_block_depth += 1);
    let out = build_block_impl(n, styles);
    state::with(|st| st.build_block_depth -= 1);
    out
}

fn is_media_or_table_tag(c: Node<'_>) -> bool {
    matches!(
        c.name().map(CStr::to_bytes),
        Some(b"img" | b"svg" | b"audio" | b"video" | b"table")
    )
}

fn blockified_text_item(
    block: BoxRef<'_>,
    c: Node<'_>,
    styles: StyleTable,
    pending: Option<&mut Pending>,
) {
    let item = BoxPool::new_box(KIND_BLOCK);
    let mut run = inline_run(c, c.next_sibling(), styles);
    if let Some(p) = pending
        && let Some(prefix) = p.take()
    {
        run = ffi::inline_merge_prefix(prefix, run);
    }
    match run {
        Some(r) if has_text(r) => {
            append_child(item, r);
            append_child(block, item);
        }
        other => {
            if let Some(r) = other {
                BoxPool::free_tree(Some(r));
            }
            BoxPool::free_tree(Some(item));
        }
    }
}

enum Flow {
    Next,
    Handled,
}

fn blockified_child(
    block: BoxRef<'_>,
    c: Node<'_>,
    styles: StyleTable,
    pending: &mut Pending,
    contents_pending: bool,
) -> Flow {
    if c.is_text() {
        if !text_is_ws_only(c.text()) {
            blockified_text_item(block, c, styles, contents_pending.then_some(pending));
        }
        return Flow::Next;
    }
    if !c.is_element() {
        return Flow::Next;
    }
    let cs = styles.get(c);
    if cs.is_some() && style_is_none(cs) {
        return Flow::Next;
    }
    if style_is_contents(cs) {
        let p = if contents_pending {
            Some(pending)
        } else {
            None
        };
        append_display_contents_children(block, c, styles, true, p);
        return Flow::Next;
    }
    if cs.is_some() && style_is_absolute_or_fixed(cs) {
        append_some(block, build_block(Some(c), styles));
        return Flow::Next;
    }
    if style_is_block(cs)
        || contains_block_media(Some(c), styles)
        || node_has_media_metadata(Some(c))
        || is_media_or_table_tag(c)
    {
        append_some(block, build_block(Some(c), styles));
        return Flow::Next;
    }
    append_some(
        block,
        build_blockified_inline_item(c, styles, Some(pending)),
    );
    Flow::Handled
}

pub fn append_display_contents_children(
    block: BoxRef<'_>,
    n: Node<'_>,
    styles: StyleTable,
    blockify_children: bool,
    pending_before: Option<&mut Pending>,
) {
    if state::with(|st| st.contents_depth) >= MAX_DEPTH {
        return;
    }
    state::with(|st| st.contents_depth += 1);
    let s = styles.get(n);
    let before = s.and_then(StyleRef::before);
    let mut contents_before = if before.is_some_and(|b| !style_is_absolute_or_fixed(Some(b))) {
        build_pseudo_inline_for(before, Some(n))
    } else {
        None
    };
    let mut no_pending: Pending = None;
    let has_pending_slot = pending_before.is_some();
    let pending = pending_before.unwrap_or(&mut no_pending);
    if let Some(prefix) = pending.take() {
        contents_before = ffi::inline_merge_prefix(prefix, contents_before);
    }
    append_some(block, contents_before);

    let sr = shadow_root(Some(n));
    let mut c = match sr {
        Some(sr) => sr.first_child(),
        None => n.first_child(),
    };
    while let Some(cur) = c {
        if cur.is_element() && tags::is_non_rendering(cur.name()) {
            c = cur.next_sibling();
            continue;
        }
        if blockify_children {
            if cur.is_text() {
                if !text_is_ws_only(cur.text()) {
                    blockified_text_item(block, cur, styles, None);
                }
                c = cur.next_sibling();
                continue;
            }
            if !cur.is_element() {
                c = cur.next_sibling();
                continue;
            }
            let cs = styles.get(cur);
            if cs.is_some() && style_is_none(cs) {
                c = cur.next_sibling();
                continue;
            }
            if style_is_contents(cs) {
                append_display_contents_children(block, cur, styles, true, None);
                c = cur.next_sibling();
                continue;
            }
            if cs.is_some() && style_is_absolute_or_fixed(cs) {
                append_some(block, build_block(Some(cur), styles));
                c = cur.next_sibling();
                continue;
            }
            if style_is_block(cs)
                || contains_block_media(Some(cur), styles)
                || node_has_media_metadata(Some(cur))
                || is_media_or_table_tag(cur)
            {
                append_some(block, build_block(Some(cur), styles));
                c = cur.next_sibling();
                continue;
            }
            let item = build_blockified_inline_item(
                cur,
                styles,
                if has_pending_slot {
                    Some(&mut *pending)
                } else {
                    None
                },
            );
            append_some(block, item);
            c = cur.next_sibling();
            continue;
        }
        if cur.is_element() {
            let cs = styles.get(cur);
            if cs.is_some() && style_is_none(cs) {
                c = cur.next_sibling();
                continue;
            }
            if style_is_contents(cs) {
                append_display_contents_children(block, cur, styles, false, None);
                c = cur.next_sibling();
                continue;
            }
        }
        if is_inline_dom(Some(cur), styles) {
            let start = cur;
            c = cur.next_sibling();
            while let Some(nx) = c {
                if nx.is_element() && tags::is_non_rendering(nx.name()) {
                    c = nx.next_sibling();
                    continue;
                }
                if nx.is_element() && style_is_contents(styles.get(nx)) {
                    break;
                }
                if !continues_inline_run(Some(nx), styles) {
                    break;
                }
                c = nx.next_sibling();
            }
            append_or_free(block, inline_run(start, c, styles));
        } else {
            append_some(block, build_block(Some(cur), styles));
            c = cur.next_sibling();
        }
    }

    let after = s.and_then(StyleRef::after);
    if after.is_some_and(|a| !style_is_absolute_or_fixed(Some(a))) {
        append_some(block, build_pseudo_inline_for(after, Some(n)));
    }
    state::with(|st| st.contents_depth -= 1);
}

fn build_blockified_inline_item(
    n: Node<'_>,
    styles: StyleTable,
    pending_before: Option<&mut Pending>,
) -> Option<BoxRef<'static>> {
    let s = styles.get(n);
    let item = BoxPool::new_box(KIND_BLOCK);
    item.set_dom(n.as_ptr().cast());
    item.set_style(style_ptr(s));
    collect_box_bg_image(item, s);
    if let Some(s) = s {
        register_abs_pseudo(Some(n), s.before());
        register_abs_pseudo(Some(n), s.after());
    }
    let before_block = s.and_then(|s| build_pseudo_block_for(s.before(), Some(n)));
    append_some(item, before_block);
    let after_block = s.and_then(|s| build_pseudo_block_for(s.after(), Some(n)));

    let saved = state::with(|st| {
        let saved = (
            st.pseudo_blocks_host,
            st.pseudo_block_before,
            st.pseudo_block_after,
        );
        st.pseudo_blocks_host = n.as_ptr();
        st.pseudo_block_before = before_block.is_some();
        st.pseudo_block_after = after_block.is_some();
        saved
    });
    let raw = run::build_inline_run(Some(n), Node::ptr_or_null(n.next_sibling()), styles, false);
    let mut run = ffi::box_ref(raw);
    state::with(|st| {
        (
            st.pseudo_blocks_host,
            st.pseudo_block_before,
            st.pseudo_block_after,
        ) = saved;
    });
    if let Some(p) = pending_before
        && let Some(prefix) = p.take()
    {
        run = ffi::inline_merge_prefix(prefix, run);
    }
    append_or_free(item, run);
    append_some(item, after_block);
    if item.first_child().is_none() && !style_has_atomic_inline_box(s) {
        BoxPool::free_tree(Some(item));
        return None;
    }
    Some(item)
}

fn style_can_be_rendered_legend(s: Option<StyleRef<'_>>) -> bool {
    s.is_some_and(|_| {
        !style_is_none(s) && ffi::float_side_of(s) < 0 && !style_is_absolute_or_fixed(s)
    })
}

fn fieldset_rendered_legend<'a>(n: Node<'a>, styles: StyleTable) -> Option<Node<'a>> {
    if n.element_name() != Some(b"fieldset") {
        return None;
    }
    children(n).find(|c| {
        c.element_name() == Some(b"legend") && style_can_be_rendered_legend(styles.get(*c))
    })
}

fn build_rendered_legend(n: Node<'_>, styles: StyleTable) -> BoxRef<'static> {
    let saved = state::with(|st| core::mem::replace(&mut st.blockified_legend, n.as_ptr()));
    let legend = build_block(Some(n), styles);
    state::with(|st| st.blockified_legend = saved);
    let legend = legend.unwrap_or_else(|| {
        let b = BoxPool::new_box(KIND_BLOCK);
        b.set_dom(n.as_ptr().cast());
        b.set_style(style_ptr(styles.get(n)));
        b
    });
    legend.set_rendered_legend(true);
    legend
}

pub fn build_form_control_block(
    n: Node<'_>,
    s: Option<StyleRef<'_>>,
    styles: StyleTable,
) -> BoxRef<'static> {
    let block = BoxPool::new_box(KIND_BLOCK);
    block.set_dom(n.as_ptr().cast());
    block.set_style(style_ptr(s));
    collect_box_bg_image(block, s);
    let prev = state::with(|st| core::mem::replace(&mut st.form_control_inline, n.as_ptr()));
    let run = inline_run(n, n.next_sibling(), styles);
    state::with(|st| st.form_control_inline = prev);
    append_or_free(block, run);
    block
}

fn build_document(n: Node<'_>, styles: StyleTable) -> BoxRef<'static> {
    let frame_url = n
        .parent()
        .filter(|p| matches!(p.element_name(), Some(b"iframe" | b"frame" | b"object")))
        .and_then(|p| p.attr(c"data-nd-frame-url"))
        .filter(|u| !u.is_empty());
    let saved_base = state::with(|st| {
        let saved = st.base_url.take();
        let fallback = frame_url.map(CStr::to_owned).or_else(|| saved.clone());
        let doc_base =
            southstar_dom::base_url::document_base(n, fallback.as_deref().map(CStr::to_bytes))
                .and_then(|base| CString::new(base).ok());
        st.base_url = doc_base.or(fallback);
        saved
    });
    let root = BoxPool::new_box(KIND_BLOCK);
    for c in children(n) {
        let cs = if c.is_element() { styles.get(c) } else { None };
        if style_is_contents(cs) {
            append_display_contents_children(root, c, styles, false, None);
            continue;
        }
        append_some(root, build_block(Some(c), styles));
    }
    state::with(|st| st.base_url = saved_base);
    root
}

fn build_slot(n: Node<'_>, s: Option<StyleRef<'_>>, styles: StyleTable) -> Option<BoxRef<'static>> {
    let host = slot_host(n);
    let sr = shadow_root(host);
    let slot_name = n.attr(c"name").map_or(&b""[..], CStr::to_bytes);
    let sbox = BoxPool::new_box(KIND_BLOCK);
    sbox.set_dom(n.as_ptr().cast());
    sbox.set_style(style_ptr(s));
    let mut any = false;
    if let (Some(host), Some(sr)) = (host, sr) {
        for lc in children(host) {
            if ptr::eq(lc.as_ptr(), sr.as_ptr()) {
                continue;
            }
            let cs = if lc.is_element() {
                lc.attr(c"slot")
            } else {
                None
            };
            if cs.map_or(&b""[..], CStr::to_bytes) != slot_name {
                continue;
            }
            let cb = if is_inline_dom(Some(lc), styles) {
                match inline_run(lc, lc.next_sibling(), styles) {
                    Some(r) if has_text(r) => Some(r),
                    Some(r) => {
                        BoxPool::free_tree(Some(r));
                        None
                    }
                    None => None,
                }
            } else {
                build_block(Some(lc), styles)
            };
            if let Some(cb) = cb {
                append_child(sbox, cb);
                any = true;
            }
        }
    }
    if any {
        return Some(sbox);
    }
    BoxPool::free_tree(Some(sbox));
    None
}

fn control_box(
    n: Node<'_>,
    s: Option<StyleRef<'_>>,
    styles: StyleTable,
) -> Option<BoxRef<'static>> {
    if s.is_some() && (style_is_absolute_or_fixed(s) || style_is_block(s)) {
        return Some(build_form_control_block(n, s, styles));
    }
    inline_run(n, n.next_sibling(), styles)
}

fn build_block_impl(n: Node<'_>, styles: StyleTable) -> Option<BoxRef<'static>> {
    if n.kind() == NodeKind::Document {
        return Some(build_document(n, styles));
    }
    if !n.is_element() || node_is_frame_fallback(n) || n.attr(SHADOW_ATTR).is_some() {
        return None;
    }
    let s = styles.get(n);
    if s.is_some() && style_is_none(s) {
        return None;
    }
    if s.is_some() && style_is_absolute_or_fixed(s) {
        if !state::with(|st| st.abs_force_build) {
            state::queue_absolute(n.as_ptr(), keyword_fixed(s));
            return None;
        }
        state::with(|st| st.abs_force_build = false);
    }
    let name = n.name().map(CStr::to_bytes);
    match name {
        Some(b"br") => {
            if ffi::clear_kind_of(s) == 0 {
                return None;
            }
            let clearance = BoxPool::new_box(KIND_BLOCK);
            clearance.set_dom(n.as_ptr().cast());
            clearance.set_style(style_ptr(s));
            return Some(clearance);
        }
        Some(b"img") => {
            let ib = build_image_box(n);
            ib.set_style(style_ptr(s));
            return Some(ib);
        }
        Some(b"math") => {
            let mb = BoxPool::new_box(KIND_MATH);
            mb.set_dom(n.as_ptr().cast());
            mb.set_style(style_ptr(s));
            return Some(mb);
        }
        Some(b"input" | b"textarea" | b"select") => {
            if let Some(b) = control_box(n, s, styles) {
                return Some(b);
            }
        }
        Some(b"svg") => return Some(build_svg_box(n, s, styles)),
        Some(b"canvas") => return Some(build_canvas_box(n, s)),
        Some(b"audio") => {
            let ab = build_audio_box(n);
            ab.set_style(style_ptr(s));
            return Some(ab);
        }
        Some(b"video") => {
            let vb = build_video_box(n);
            vb.set_style(style_ptr(s));
            collect_box_bg_image(vb, s);
            return Some(vb);
        }
        _ => {}
    }
    if node_has_media_metadata(Some(n)) {
        let has_src = n.attr(crate::classify::MEDIA_SRC_ATTR).is_some();
        let empty_skeleton = !children(n).any(|c| c.is_element());
        if has_src || empty_skeleton {
            let vb = build_video_box(n);
            vb.set_style(style_ptr(s));
            collect_box_bg_image(vb, s);
            return Some(vb);
        }
    }
    if ffi::is_table_box(n, styles) {
        return ffi::build_table(n, styles);
    }
    if name == Some(b"slot")
        && let Some(sbox) = build_slot(n, s, styles)
    {
        return Some(sbox);
    }
    let blockified_legend = state::with(|st| st.blockified_legend);
    if !style_is_block(s)
        && !contains_block_media(Some(n), styles)
        && !style_is_absolute_or_fixed(s)
        && !same(blockified_legend, n)
    {
        return None;
    }
    Some(build_container(n, s, styles))
}

fn keyword_fixed(s: Option<StyleRef<'_>>) -> bool {
    southstar_layout_content::controls::keyword_is(
        s.and_then(|s| s.get(southstar_style::PropId::Position)),
        c"fixed",
    )
}

fn is_summary(c: Node<'_>) -> bool {
    c.element_name() == Some(b"summary")
}

fn build_container(n: Node<'_>, s: Option<StyleRef<'_>>, styles: StyleTable) -> BoxRef<'static> {
    let block = BoxPool::new_box(KIND_BLOCK);
    block.set_dom(n.as_ptr().cast());
    block.set_style(style_ptr(s));
    let rendered_legend = fieldset_rendered_legend(n, styles);
    collect_box_bg_image(block, s);
    let details_collapsed =
        n.name().map(CStr::to_bytes) == Some(b"details") && n.attr(c"open").is_none();
    if let Some(s) = s {
        register_abs_pseudo(Some(n), s.before());
        register_abs_pseudo(Some(n), s.after());
    }
    let before = s.and_then(StyleRef::before);
    let before_block = build_pseudo_block_for(before, Some(n));
    let had_before_block = before_block.is_some();
    append_some(block, before_block);
    let mut pending: Pending =
        if before.is_some_and(|b| !had_before_block && !style_is_absolute_or_fixed(Some(b))) {
            build_pseudo_inline_for(before, Some(n))
        } else {
            None
        };
    let blockify_children = style_is_flex_container(s) || style_is_grid_container(s);
    if blockify_children && let Some(p) = pending.take() {
        append_child(block, pseudo_item_block(p, before));
    }
    let saved_skip = state::with(|st| st.inline_skip);
    if let Some(legend_node) = rendered_legend {
        let legend = build_rendered_legend(legend_node, styles);
        append_child(block, legend);
        state::with(|st| st.inline_skip = legend_node.as_ptr());
    }
    let is_legend = |c: Node<'_>| rendered_legend.is_some_and(|l| ptr::eq(l.as_ptr(), c.as_ptr()));

    let sr = shadow_root(Some(n));
    let mut c = match sr {
        Some(sr) => sr.first_child(),
        None => n.first_child(),
    };
    while let Some(cur) = c {
        if cur.is_element() && tags::is_non_rendering(cur.name()) {
            c = cur.next_sibling();
            continue;
        }
        if details_collapsed && !is_summary(cur) {
            c = cur.next_sibling();
            continue;
        }
        if is_legend(cur) {
            c = cur.next_sibling();
            continue;
        }
        if blockify_children {
            blockified_child(block, cur, styles, &mut pending, true);
            c = cur.next_sibling();
            continue;
        }
        if cur.is_element() && style_is_contents(styles.get(cur)) {
            append_display_contents_children(block, cur, styles, false, Some(&mut pending));
            c = cur.next_sibling();
            continue;
        }
        if ffi::node_is_table_internal(Some(cur), styles) {
            let start = cur;
            let mut end = Some(cur);
            while let Some(e) = end {
                if !(ffi::node_is_table_internal(Some(e), styles)
                    || (e.is_text() && text_is_ws_only(e.text())))
                {
                    break;
                }
                end = e.next_sibling();
            }
            if let Some(p) = pending.take() {
                append_child(block, p);
            }
            append_some(block, ffi::build_anonymous_table(start, end, styles));
            c = end;
            continue;
        }
        if is_inline_dom(Some(cur), styles) {
            let start = cur;
            c = cur.next_sibling();
            while let Some(nx) = c {
                if nx.is_element() && tags::is_non_rendering(nx.name()) {
                    c = nx.next_sibling();
                    continue;
                }
                if is_legend(nx) {
                    c = nx.next_sibling();
                    continue;
                }
                if details_collapsed && !is_summary(nx) {
                    break;
                }
                if nx.is_element() && style_is_contents(styles.get(nx)) {
                    break;
                }
                if !continues_inline_run(Some(nx), styles) {
                    break;
                }
                c = nx.next_sibling();
            }
            let mut run = inline_run(start, c, styles);
            if let Some(p) = pending.take() {
                run = ffi::inline_merge_prefix(p, run);
            }
            append_or_free(block, run);
        } else {
            if let Some(p) = pending.take() {
                append_child(block, p);
            }
            append_some(block, build_block(Some(cur), styles));
            c = cur.next_sibling();
        }
    }
    state::with(|st| st.inline_skip = saved_skip);
    if let Some(p) = pending.take() {
        append_child(block, p);
    }
    let after = s.and_then(StyleRef::after);
    let after_block = build_pseudo_block_for(after, Some(n));
    let had_after_block = after_block.is_some();
    append_some(block, after_block);
    if after.is_some_and(|a| !had_after_block && !style_is_absolute_or_fixed(Some(a)))
        && let Some(generated) = build_pseudo_inline_for(after, Some(n))
    {
        if blockify_children {
            append_child(block, pseudo_item_block(generated, after));
        } else {
            append_generated_after(block, generated);
        }
    }
    block
}

pub fn collect_atomic_block(
    n: Node<'_>,
    styles: StyleTable,
    depth: i32,
) -> Option<BoxRef<'static>> {
    let saved = state::with(|st| core::mem::replace(&mut st.inline_collect_depth, depth));
    let sub = build_block(Some(n), styles);
    state::with(|st| st.inline_collect_depth = saved);
    sub
}

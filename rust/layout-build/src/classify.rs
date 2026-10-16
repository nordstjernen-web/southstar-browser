//! Southstar — sorting DOM children into inline runs and blocks: inline-level nodes, absolutely positioned boxes that join a run, and inline elements holding block-level media.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Kind as NodeKind, MAX_DEPTH, Node, children};
use southstar_layout_content::tags;
use southstar_layout_geom::{
    INNER_FLEX, INNER_GRID, OUTER_INLINE, inner_is, is_atomic_inline, is_atomic_inline_container,
    is_table_internal, style_is_absolute_or_fixed, style_is_block, style_is_block_level,
    style_is_flex_container, style_is_grid_container, style_is_none,
};
use southstar_style::{StyleTable, display_of};

use crate::atomic::is_inline_level_replaced;
use crate::ffi;
use crate::state;

pub const MEDIA_SRC_ATTR: &CStr = c"data-nd-media-src";
pub const MEDIA_STREAM_ATTR: &CStr = c"data-nd-media-stream";
pub const MEDIA_POSTER_ATTR: &CStr = c"data-nd-media-poster";
pub const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";

pub fn is_replaced_block_tag(name: Option<&[u8]>) -> bool {
    matches!(
        name,
        Some(b"img" | b"svg" | b"canvas" | b"audio" | b"video" | b"math" | b"table")
    )
}

pub fn node_has_media_metadata(n: Option<Node<'_>>) -> bool {
    n.is_some_and(|n| {
        n.is_element() && (n.attr(MEDIA_SRC_ATTR).is_some() || n.attr(MEDIA_STREAM_ATTR).is_some())
    })
}

pub fn text_is_ws_only(text: Option<&CStr>) -> bool {
    text.is_none_or(|t| {
        t.to_bytes()
            .iter()
            .all(|&c| matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
    })
}

fn contains_block_media_depth(n: Node<'_>, styles: StyleTable, depth: i32) -> bool {
    if depth >= MAX_DEPTH || !n.is_element() {
        return false;
    }
    for c in children(n) {
        let Some(name) = c.element_name() else {
            continue;
        };
        if tags::node_is_non_rendering(Some(c)) {
            continue;
        }
        if node_has_media_metadata(Some(c))
            || (is_replaced_block_tag(Some(name)) && !is_inline_level_replaced(Some(c), styles))
            || name == b"iframe"
        {
            return true;
        }
        if !styles.as_ptr().is_null()
            && let Some(cs) = styles.get(c)
        {
            if style_is_none(Some(cs)) || style_is_absolute_or_fixed(Some(cs)) {
                continue;
            }
            if style_is_block_level(Some(cs)) {
                return true;
            }
            if is_atomic_inline(display_of(Some(cs))) {
                continue;
            }
        }
        if contains_block_media_depth(c, styles, depth + 1) {
            return true;
        }
    }
    false
}

pub fn contains_block_media(n: Option<Node<'_>>, styles: StyleTable) -> bool {
    let Some(n) = n else {
        return false;
    };
    let key = n.as_ptr() as usize;
    if let Some(hit) = state::with(|st| {
        st.block_media_cache
            .as_ref()
            .and_then(|c| c.get(&key).copied())
    }) {
        return hit;
    }
    let has_media = contains_block_media_depth(n, styles, 0);
    state::with(|st| {
        if let Some(cache) = st.block_media_cache.as_mut() {
            cache.insert(key, has_media);
        }
    });
    has_media
}

fn abs_joins_inline_run(n: Node<'_>, styles: StyleTable) -> bool {
    let Some(s) = styles.get(n) else {
        return false;
    };
    if !s.specified_inline() {
        return false;
    }
    let Some(parent) = n.parent() else {
        return false;
    };
    let ps = styles.get(parent);
    if ps.is_some() && (style_is_flex_container(ps) || style_is_grid_container(ps)) {
        return false;
    }
    let mut p = n.prev_sibling();
    while let Some(sib) = p {
        p = sib.prev_sibling();
        if sib.is_text() {
            if sib.text().is_some_and(|t| {
                t.to_bytes()
                    .iter()
                    .any(|&c| !matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
            }) {
                return true;
            }
            continue;
        }
        if !sib.is_element() {
            continue;
        }
        let ss = styles.get(sib);
        if ss.is_some() && (style_is_none(ss) || style_is_absolute_or_fixed(ss)) {
            continue;
        }
        return is_inline_dom(Some(sib), styles);
    }
    false
}

pub fn is_inline_dom(n: Option<Node<'_>>, styles: StyleTable) -> bool {
    let Some(n) = n else {
        return false;
    };
    if n.is_text() {
        return true;
    }
    if !n.is_element() {
        return false;
    }
    let name = n.name().map(CStr::to_bytes);
    if name == Some(b"slot") {
        return false;
    }
    if node_has_media_metadata(Some(n)) {
        return false;
    }
    if children(n).any(|sc| sc.is_element() && sc.attr(SHADOW_ATTR).is_some()) {
        return false;
    }
    if is_replaced_block_tag(name) {
        if name == Some(b"table") {
            return false;
        }
        let rs = styles.get(n);
        if rs.is_some() && style_is_none(rs) {
            return false;
        }
        if rs.is_some() && style_is_absolute_or_fixed(rs) {
            return abs_joins_inline_run(n, styles);
        }
        return is_inline_level_replaced(Some(n), styles);
    }
    let Some(s) = styles.get(n) else {
        return name.is_some_and(|nm| nm.contains(&b'-'));
    };
    let s = Some(s);
    if style_is_none(s) {
        return false;
    }
    if name == Some(b"br") {
        return ffi::clear_kind_of(s) == 0;
    }
    if style_is_absolute_or_fixed(s) {
        return abs_joins_inline_run(n, styles);
    }
    let d = display_of(s);
    if is_table_internal(d) {
        return false;
    }
    if (inner_is(d, INNER_FLEX) || inner_is(d, INNER_GRID)) && d.outer == OUTER_INLINE {
        return true;
    }
    if !style_is_block(s) && contains_block_media(Some(n), styles) {
        return false;
    }
    if is_atomic_inline_container(d) {
        return true;
    }
    !style_is_block(s)
}

fn node_leaves_inline_run_open(n: Node<'_>, styles: StyleTable) -> bool {
    if n.kind() == NodeKind::Comment {
        return true;
    }
    if !n.is_element() {
        return false;
    }
    let s = styles.get(n);
    s.is_some() && style_is_none(s)
}

pub fn continues_inline_run(n: Option<Node<'_>>, styles: StyleTable) -> bool {
    is_inline_dom(n, styles) || n.is_some_and(|n| node_leaves_inline_run_open(n, styles))
}

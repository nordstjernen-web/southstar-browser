//! Southstar — laying out one box by its kind, the container-query dimensions it sees, and shifting a laid-out subtree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;

use southstar_layout::{BoxKind, BoxRef, Style};
use southstar_layout_geom::length_or;
use southstar_style::{PropId, ValueRef};

use crate::block::layout_block;
use crate::ffi::{
    layout_image, layout_inline, layout_table, math_measure, set_container_dims, style_of,
};

thread_local! {
    static CQ_SEEN_CONTAINER: Cell<bool> = const { Cell::new(false) };
}

pub fn reset_container_queries() {
    CQ_SEEN_CONTAINER.set(false);
}

fn container_type_is(b: BoxRef<'_>, matches: impl Fn(&[u8]) -> bool) -> bool {
    style_of(b)
        .and_then(|s| s.get(PropId::ContainerType))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| matches(kw.to_bytes()))
}

fn is_query_container(b: BoxRef<'_>) -> bool {
    container_type_is(b, |kw| !kw.eq_ignore_ascii_case(b"normal"))
}

fn is_size_query_container(b: BoxRef<'_>) -> bool {
    container_type_is(b, |kw| kw.eq_ignore_ascii_case(b"size"))
}

fn set_dims_from_ancestors(b: BoxRef<'_>) {
    let mut inline_size = 0.0;
    let mut block_size = 0.0;
    let mut a = b.parent();
    while let Some(anc) = a {
        if inline_size <= 0.0 && is_query_container(anc) {
            inline_size = anc.content_width();
        }
        if block_size <= 0.0 && is_size_query_container(anc) {
            block_size = anc.content_height();
        }
        if inline_size > 0.0 && block_size > 0.0 {
            break;
        }
        a = anc.parent();
    }
    set_container_dims(inline_size, block_size);
}

pub fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    b.begin_layout(parent_content_width);
    if is_query_container(b) {
        CQ_SEEN_CONTAINER.set(true);
    }
    if CQ_SEEN_CONTAINER.get() {
        set_dims_from_ancestors(b);
    }
    match b.kind() {
        BoxKind::Block | BoxKind::TableCaption => layout_block(b, parent_content_width, inherited),
        BoxKind::Inline => layout_inline(b, parent_content_width, inherited),
        BoxKind::Image | BoxKind::Video | BoxKind::Svg => layout_image(b, parent_content_width),
        BoxKind::Table => layout_table(b, parent_content_width, inherited),
        BoxKind::Math => {
            let font_px = length_or(style_of(b).and_then(|s| s.get(PropId::FontSize)), 16.0);
            let (w, asc, desc) = math_measure(b.dom(), font_px);
            b.set_content_width(w);
            b.set_content_height(asc + desc);
        }
        _ => {
            b.set_content_width(parent_content_width);
            b.set_content_height(0.0);
        }
    }
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    b.set_x(b.x() + dx);
    b.set_y(b.y() + dy);
    b.translate_grid_tracks(dx, dy);
    let mut c = b.first_child();
    while let Some(child) = c {
        shift_box_tree(child, dx, dy);
        c = child.next_sibling();
    }
    for ab in b.inline_atomic_boxes() {
        shift_box_tree(ab, dx, dy);
    }
}

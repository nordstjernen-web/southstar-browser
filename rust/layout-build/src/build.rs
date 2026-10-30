//! Southstar — building and laying out a document: the per-build state, the counter and text-input-column passes, block layout, positioned boxes and paint bounds.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;
use std::collections::{HashMap, HashSet};

use southstar_dom::{Node, NsNode, controls};
use southstar_layout::{BoxPool, BoxRef};
use southstar_layout_content::text_input;
use southstar_layout_content::{counters_build, counters_clear};
use southstar_style::StyleTable;

use crate::block::build_block;
use crate::{ffi, passes, state};

fn build_once(
    doc: Option<Node<'_>>,
    styles: StyleTable,
    viewport_width: f64,
) -> Option<BoxRef<'static>> {
    state::with(|st| {
        st.abs_pending = Some(Vec::new());
        st.abs_seen = Some(HashSet::new());
        st.abs_placeholders = Some(HashMap::new());
        st.abs_static = Some(HashMap::new());
        st.block_media_cache = Some(HashMap::new());
    });
    passes::reset_container_queries();
    ffi::set_container_dims(0.0, 0.0);
    let root = build_block(doc, styles);
    if let Some(root) = root {
        root.set_x(0.0);
        root.set_y(0.0);
        southstar_layout_geom::dispatch::layout_block(root, viewport_width, None);
        passes::apply_position_offsets(root, viewport_width, root.content_height());
        passes::process_absolute_boxes(root, styles, viewport_width);
        ffi::sync_inline_atomic_offsets(root);
        ffi::compute_paint_bounds(root);
    }
    state::with(|st| {
        st.abs_pending = None;
        st.abs_seen = None;
        st.abs_placeholders = None;
        st.abs_static = None;
        st.block_media_cache = None;
    });
    root
}

pub struct BuildInput<'a> {
    pub doc: Option<Node<'a>>,
    pub styles: StyleTable<'a>,
    pub viewport_width: f64,
    pub focused_input: Option<Node<'a>>,
    pub caret_byte: usize,
    pub anchor_byte: usize,
    pub image_cache: Option<southstar_image::ImageCache>,
    pub base_url: Option<&'a [u8]>,
}

fn contenteditable_focus(n: Option<Node<'_>>) -> bool {
    n.is_some_and(|n| {
        n.element_name()
            .is_some_and(|name| name != b"input" && name != b"textarea")
            && controls::ce_attr_enables(n.attr(c"contenteditable"))
    })
}

pub fn layout_build(input: BuildInput<'_>) -> Option<BoxRef<'static>> {
    passes::install_builders();
    state::with(|st| {
        st.focused_input = Node::ptr_or_null(input.focused_input);
        st.focused_is_contenteditable = contenteditable_focus(input.focused_input);
        st.caret_byte = input.caret_byte;
        st.anchor_byte = input.anchor_byte;
        st.image_cache = input.image_cache;
        st.base_url = input
            .base_url
            .and_then(|url| std::ffi::CString::new(url).ok());
    });
    ffi::image_cache_begin_generation(input.image_cache);
    counters_build(input.doc, input.styles);
    let mut root = build_once(input.doc, input.styles, input.viewport_width);
    if !text_input::columns_active() && root.is_some_and(text_input::fit_columns) {
        BoxPool::free_tree(root);
        ffi::image_cache_begin_generation(input.image_cache);
        root = build_once(input.doc, input.styles, input.viewport_width);
        text_input::clear_columns();
    }
    ffi::image_cache_collect(input.image_cache);
    state::with(|st| {
        st.focused_input = ptr::null::<NsNode>();
        st.focused_is_contenteditable = false;
        st.caret_byte = 0;
        st.anchor_byte = 0;
        st.image_cache = None;
        st.base_url = None;
    });
    counters_clear();
    ffi::record_frame_viewports(root);
    root
}

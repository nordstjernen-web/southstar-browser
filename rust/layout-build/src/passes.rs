//! Southstar — the layout passes a build runs over the finished box tree, which the positioning crate installs, and the builders tree and form-control code call back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use std::sync::{Once, OnceLock};

use southstar_dom::Node;
use southstar_layout::BoxRef;
use southstar_style::{PropId, StyleRef, StyleTable};

use crate::{block, classify, ffi, generated, media, run, state};

pub struct Passes {
    pub reset_container_queries: fn(),
    pub apply_position_offsets: fn(BoxRef<'_>, f64, f64),
    pub process_absolute_boxes: fn(BoxRef<'_>, StyleTable, f64),
}

static PASSES: OnceLock<Passes> = OnceLock::new();

pub fn install(passes: Passes) {
    let _ = PASSES.set(passes);
}

fn passes() -> &'static Passes {
    PASSES
        .get()
        .expect("the layout passes are installed before the first build")
}

pub fn reset_container_queries() {
    (passes().reset_container_queries)();
}

pub fn apply_position_offsets(root: BoxRef<'_>, w: f64, h: f64) {
    (passes().apply_position_offsets)(root, w, h);
}

pub fn process_absolute_boxes(root: BoxRef<'_>, st: StyleTable, vw: f64) {
    (passes().process_absolute_boxes)(root, st, vw);
}

fn build_block(n: Node<'_>, st: StyleTable) -> Option<BoxRef<'static>> {
    block::build_block(Some(n), st)
}

fn build_inline_run(
    first: Node<'_>,
    last_excl: Option<Node<'_>>,
    st: StyleTable,
) -> Option<BoxRef<'static>> {
    ffi::box_ref(run::build_inline_run(
        Some(first),
        Node::ptr_or_null(last_excl),
        st,
        true,
    ))
}

fn build_pseudo_block_for(ps: StyleRef<'_>, host: Node<'_>) -> Option<BoxRef<'static>> {
    generated::build_pseudo_block_for(Some(ps), Some(host))
}

fn build_pseudo_inline_for(ps: StyleRef<'_>, host: Node<'_>) -> Option<BoxRef<'static>> {
    generated::build_pseudo_inline_for(Some(ps), Some(host))
}

fn register_abs_pseudo(host: Node<'_>, ps: Option<StyleRef<'_>>) {
    generated::register_abs_pseudo(Some(host), ps);
}

fn queue_absolute_node(n: Node<'_>, s: StyleRef<'_>) {
    let fixed = southstar_layout_content::controls::keyword_is(s.get(PropId::Position), c"fixed");
    state::queue_absolute(n.as_ptr(), fixed);
}

fn collect_box_bg_image(b: BoxRef<'_>, s: Option<StyleRef<'_>>) {
    media::collect_box_bg_image(b, s);
}

fn append_display_contents_children(b: BoxRef<'_>, n: Node<'_>, st: StyleTable) {
    block::append_display_contents_children(b, n, st, false, None);
}

fn append_node_children(b: BoxRef<'_>, n: Node<'_>, st: StyleTable) {
    block::append_node_children(b, n, st, false, None);
}

fn append_generated_after(b: BoxRef<'_>, generated_box: BoxRef<'static>) {
    generated::append_generated_after(b, generated_box);
}

fn is_inline_dom(n: Node<'_>, st: StyleTable) -> bool {
    classify::is_inline_dom(Some(n), st)
}

fn continues_inline_run(n: Node<'_>, st: StyleTable) -> bool {
    classify::continues_inline_run(Some(n), st)
}

fn peek_image(src: &CStr) -> *mut c_void {
    media::peek_image(Some(src))
}

pub fn install_builders() {
    static BUILDERS: Once = Once::new();
    BUILDERS.call_once(|| {
        southstar_layout_tree::install(southstar_layout_tree::Builders {
            build_block,
            build_inline_run,
            build_pseudo_block_for,
            build_pseudo_inline_for,
            register_abs_pseudo,
            queue_absolute_node,
            collect_box_bg_image,
            append_display_contents_children,
            append_node_children,
            append_generated_after,
            is_inline_dom,
            continues_inline_run,
        });
        southstar_layout_content::install_image_peek(peek_image);
    });
}

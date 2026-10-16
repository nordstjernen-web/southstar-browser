//! Southstar — the box builders table construction calls back into, installed by the box-tree builder, and the box freeing the engine calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use std::sync::OnceLock;

use southstar_dom::Node;
use southstar_layout::{BoxPool, BoxRef, NsBox};
use southstar_style::StyleRef;

pub use southstar_style::StyleTable as Styles;

pub struct Builders {
    pub build_block: fn(Node<'_>, Styles) -> Option<BoxRef<'static>>,
    pub build_inline_run: fn(Node<'_>, Option<Node<'_>>, Styles) -> Option<BoxRef<'static>>,
    pub build_pseudo_block_for: fn(StyleRef<'_>, Node<'_>) -> Option<BoxRef<'static>>,
    pub build_pseudo_inline_for: fn(StyleRef<'_>, Node<'_>) -> Option<BoxRef<'static>>,
    pub register_abs_pseudo: fn(Node<'_>, Option<StyleRef<'_>>),
    pub queue_absolute_node: fn(Node<'_>, StyleRef<'_>),
    pub collect_box_bg_image: fn(BoxRef<'_>, Option<StyleRef<'_>>),
    pub append_display_contents_children: fn(BoxRef<'_>, Node<'_>, Styles),
    pub append_generated_after: fn(BoxRef<'_>, BoxRef<'static>),
    pub is_inline_dom: fn(Node<'_>, Styles) -> bool,
    pub continues_inline_run: fn(Node<'_>, Styles) -> bool,
}

static BUILDERS: OnceLock<Builders> = OnceLock::new();

pub fn install(builders: Builders) {
    let _ = BUILDERS.set(builders);
}

fn builders() -> &'static Builders {
    BUILDERS
        .get()
        .expect("the box builders are installed before the first build")
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn build_block(n: Node<'_>, styles: Styles) -> Option<BoxRef<'static>> {
    (builders().build_block)(n, styles)
}

pub fn build_inline_run(
    first: Node<'_>,
    last_excl: Option<Node<'_>>,
    styles: Styles,
) -> Option<BoxRef<'static>> {
    (builders().build_inline_run)(first, last_excl, styles)
}

pub fn build_pseudo_block_for(ps: StyleRef<'_>, host: Node<'_>) -> Option<BoxRef<'static>> {
    (builders().build_pseudo_block_for)(ps, host)
}

pub fn build_pseudo_inline_for(ps: StyleRef<'_>, host: Node<'_>) -> Option<BoxRef<'static>> {
    (builders().build_pseudo_inline_for)(ps, host)
}

pub fn register_abs_pseudo(host: Node<'_>, ps: Option<StyleRef<'_>>) {
    (builders().register_abs_pseudo)(host, ps);
}

pub fn queue_absolute_node(n: Node<'_>, s: StyleRef<'_>) {
    (builders().queue_absolute_node)(n, s);
}

pub fn collect_box_bg_image(b: BoxRef<'_>, s: Option<StyleRef<'_>>) {
    (builders().collect_box_bg_image)(b, s);
}

pub fn append_display_contents_children(block: BoxRef<'_>, n: Node<'_>, styles: Styles) {
    (builders().append_display_contents_children)(block, n, styles);
}

pub fn append_generated_after(block: BoxRef<'_>, generated: BoxRef<'static>) {
    (builders().append_generated_after)(block, generated);
}

pub fn tag_is_non_rendering(name: &CStr) -> bool {
    southstar_layout_content::tags::is_non_rendering(Some(name))
}

pub fn is_inline_dom(n: Node<'_>, styles: Styles) -> bool {
    (builders().is_inline_dom)(n, styles)
}

pub fn continues_inline_run(n: Node<'_>, styles: Styles) -> bool {
    (builders().continues_inline_run)(n, styles)
}

pub fn parse_int(s: Option<&CStr>, dflt: i32, min_v: i32, max_v: i32) -> i32 {
    southstar_dom::controls::parse_int(s, dflt, min_v, max_v)
}

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_free(b: *mut NsBox) {
    BoxPool::free_tree(box_ref(b));
}

pub fn dom_ptr_of(n: Node<'_>) -> *const c_void {
    n.as_ptr().cast()
}

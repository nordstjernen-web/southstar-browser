//! Southstar — the box-tree entry points layout.c calls and the box-building calls table construction makes back into layout.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable};
use southstar_layout::{BoxPool, BoxRef, NsBox, Style};
use southstar_style::StyleRef;

unsafe extern "C" {
    fn ns_layout_build_block(n: *const NsNode, styles: *mut GHashTable) -> *mut NsBox;
    fn ns_layout_build_inline_run(
        first: *const NsNode,
        last_excl: *const NsNode,
        styles: *mut GHashTable,
    ) -> *mut NsBox;
    fn ns_layout_build_pseudo_block_for(ps: *const Style, host: *const NsNode) -> *mut NsBox;
    fn ns_layout_build_pseudo_inline_for(ps: *const Style, host: *const NsNode) -> *mut NsBox;
    fn ns_layout_register_abs_pseudo(host: *const NsNode, ps: *const Style);
    fn ns_layout_queue_absolute_node(n: *const NsNode, s: *const Style);
    fn ns_layout_collect_box_bg_image(b: *mut NsBox, s: *const Style);
    fn ns_layout_append_display_contents_children(
        block: *mut NsBox,
        n: *const NsNode,
        styles: *mut GHashTable,
        blockify_children: GBoolean,
        pending_before: *mut *mut NsBox,
    );
    fn ns_layout_append_generated_after(block: *mut NsBox, generated: *mut NsBox);
    fn ns_layout_is_inline_dom(n: *const NsNode, styles: *mut GHashTable) -> GBoolean;
    fn ns_layout_continues_inline_run(n: *const NsNode, styles: *mut GHashTable) -> GBoolean;
}

pub use southstar_style::StyleTable as Styles;

fn node_ptr(n: Option<Node<'_>>) -> *const NsNode {
    n.map_or(ptr::null(), Node::as_ptr)
}

fn style_ptr(s: Option<StyleRef<'_>>) -> *const Style {
    s.map_or(ptr::null(), StyleRef::as_ptr)
}

fn owned<'a>(b: *mut NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn build_block<'a>(n: Node<'_>, styles: Styles) -> Option<BoxRef<'a>> {
    owned(unsafe { ns_layout_build_block(n.as_ptr(), styles.as_ptr()) })
}

pub fn build_inline_run<'a>(
    first: Node<'_>,
    last_excl: Option<Node<'_>>,
    styles: Styles,
) -> Option<BoxRef<'a>> {
    owned(unsafe {
        ns_layout_build_inline_run(first.as_ptr(), node_ptr(last_excl), styles.as_ptr())
    })
}

pub fn build_pseudo_block_for<'a>(ps: StyleRef<'_>, host: Node<'_>) -> Option<BoxRef<'a>> {
    owned(unsafe { ns_layout_build_pseudo_block_for(ps.as_ptr(), host.as_ptr()) })
}

pub fn build_pseudo_inline_for<'a>(ps: StyleRef<'_>, host: Node<'_>) -> Option<BoxRef<'a>> {
    owned(unsafe { ns_layout_build_pseudo_inline_for(ps.as_ptr(), host.as_ptr()) })
}

pub fn register_abs_pseudo(host: Node<'_>, ps: Option<StyleRef<'_>>) {
    unsafe { ns_layout_register_abs_pseudo(host.as_ptr(), style_ptr(ps)) }
}

pub fn queue_absolute_node(n: Node<'_>, s: StyleRef<'_>) {
    unsafe { ns_layout_queue_absolute_node(n.as_ptr(), s.as_ptr()) }
}

pub fn collect_box_bg_image(b: BoxRef<'_>, s: Option<StyleRef<'_>>) {
    unsafe { ns_layout_collect_box_bg_image(b.as_mut_ptr(), style_ptr(s)) }
}

pub fn append_display_contents_children(block: BoxRef<'_>, n: Node<'_>, styles: Styles) {
    unsafe {
        ns_layout_append_display_contents_children(
            block.as_mut_ptr(),
            n.as_ptr(),
            styles.as_ptr(),
            0,
            ptr::null_mut(),
        );
    }
}

pub fn append_generated_after(block: BoxRef<'_>, generated: BoxRef<'_>) {
    unsafe { ns_layout_append_generated_after(block.as_mut_ptr(), generated.as_mut_ptr()) }
}

pub fn tag_is_non_rendering(name: &CStr) -> bool {
    southstar_layout_content::tags::is_non_rendering(Some(name))
}

pub fn is_inline_dom(n: Node<'_>, styles: Styles) -> bool {
    unsafe { ns_layout_is_inline_dom(n.as_ptr(), styles.as_ptr()) != 0 }
}

pub fn continues_inline_run(n: Node<'_>, styles: Styles) -> bool {
    unsafe { ns_layout_continues_inline_run(n.as_ptr(), styles.as_ptr()) != 0 }
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

//! Southstar — the box-tree entry points layout.c calls and the box-building calls table construction makes back into layout.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable, boolean};
use southstar_layout::{BoxPool, BoxRef, NsBox, Style};
use southstar_style::{StyleRef, StyleTable};

use crate::{order, runs, table};

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
    fn ns_layout_tag_is_non_rendering(name: *const c_char) -> GBoolean;
    fn ns_layout_is_inline_dom(n: *const NsNode, styles: *mut GHashTable) -> GBoolean;
    fn ns_layout_continues_inline_run(n: *const NsNode, styles: *mut GHashTable) -> GBoolean;
}

#[derive(Clone, Copy)]
pub struct Styles(*mut GHashTable);

impl Styles {
    pub fn get<'a>(self, n: Node<'a>) -> Option<StyleRef<'a>> {
        unsafe { StyleTable::from_ptr(self.0) }.get(n)
    }

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

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
    owned(unsafe { ns_layout_build_block(n.as_ptr(), styles.0) })
}

pub fn build_inline_run<'a>(
    first: Node<'_>,
    last_excl: Option<Node<'_>>,
    styles: Styles,
) -> Option<BoxRef<'a>> {
    owned(unsafe { ns_layout_build_inline_run(first.as_ptr(), node_ptr(last_excl), styles.0) })
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
            styles.0,
            0,
            ptr::null_mut(),
        );
    }
}

pub fn append_generated_after(block: BoxRef<'_>, generated: BoxRef<'_>) {
    unsafe { ns_layout_append_generated_after(block.as_mut_ptr(), generated.as_mut_ptr()) }
}

pub fn tag_is_non_rendering(name: &CStr) -> bool {
    unsafe { ns_layout_tag_is_non_rendering(name.as_ptr()) != 0 }
}

pub fn is_inline_dom(n: Node<'_>, styles: Styles) -> bool {
    unsafe { ns_layout_is_inline_dom(n.as_ptr(), styles.0) != 0 }
}

pub fn continues_inline_run(n: Node<'_>, styles: Styles) -> bool {
    unsafe { ns_layout_continues_inline_run(n.as_ptr(), styles.0) != 0 }
}

pub fn parse_int(s: Option<&CStr>, dflt: i32, min_v: i32, max_v: i32) -> i32 {
    southstar_dom::controls::parse_int(s, dflt, min_v, max_v)
}

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn raw(b: Option<BoxRef<'_>>) -> *mut NsBox {
    b.map_or(ptr::null_mut(), BoxRef::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_free(b: *mut NsBox) {
    BoxPool::free_tree(box_ref(b));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_box_append_child(parent: *mut NsBox, child: *mut NsBox) {
    if let (Some(parent), Some(child)) = (box_ref(parent), box_ref(child)) {
        runs::append_child(parent, child);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_merge_prefix(
    prefix: *mut NsBox,
    suffix: *mut NsBox,
) -> *mut NsBox {
    raw(runs::merge_prefix(box_ref(prefix), box_ref(suffix)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_run_split(run: *mut NsBox, split: usize) -> *mut NsBox {
    box_ref(run).map_or(ptr::null_mut(), |run| {
        runs::run_split(run, split).as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_runs_join_splits(b: *mut NsBox) {
    if let Some(b) = box_ref(b) {
        runs::join_splits(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_reorder_children_by_order(b: *mut NsBox) {
    if let Some(b) = box_ref(b) {
        order::reorder_children(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_is_table_box(
    n: *const NsNode,
    styles: *mut GHashTable,
) -> GBoolean {
    boolean(table::is_table_box(node(n), Styles(styles)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_node_is_table_internal(
    n: *const NsNode,
    styles: *mut GHashTable,
) -> GBoolean {
    boolean(table::node_is_table_internal(node(n), Styles(styles)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_build_table(
    n: *const NsNode,
    styles: *mut GHashTable,
) -> *mut NsBox {
    node(n).map_or(ptr::null_mut(), |n| {
        table::build_table(n, Styles(styles)).as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_build_anonymous_table(
    start: *const NsNode,
    end: *const NsNode,
    styles: *mut GHashTable,
) -> *mut NsBox {
    raw(table::build_anonymous_table(
        node(start),
        node(end),
        Styles(styles),
    ))
}

pub fn dom_ptr_of(n: Node<'_>) -> *const c_void {
    n.as_ptr().cast()
}

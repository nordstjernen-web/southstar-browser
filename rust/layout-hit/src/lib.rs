//! Southstar — hit testing ported from layout.c: the box, element, form control, link, scroll container and scrollbar under a point of the laid-out page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod geometry;
mod stack;
mod tree;
mod walks;

use core::ffi::CStr;
use core::ptr;

use southstar_dom::Node;
use southstar_layout::BoxRef;

pub struct LinkHit<'a> {
    pub href: Option<&'a CStr>,
    pub dom: Option<Node<'a>>,
}

pub fn set_hit_viewport(scroll_x: f64, scroll_y: f64) {
    geometry::set_viewport(scroll_x, scroll_y);
}

pub fn is_fixed(b: BoxRef<'_>) -> bool {
    geometry::is_fixed(b)
}

pub fn hit_offset(b: BoxRef<'_>) -> (f64, f64) {
    geometry::hit_offset(b)
}

pub fn hit_test(root: BoxRef<'_>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    tree::hit_test(root, x, y)
}

pub fn hit_node(root: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    walks::hit_node(root, x, y)
}

pub fn hit_form_dom(root: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    walks::form_hit(root, x, y, ptr::null())
}

pub fn hit_inline_dom(root: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    walks::inline_dom(root, x, y)
}

pub fn hit_link(root: BoxRef<'_>, x: f64, y: f64) -> Option<LinkHit<'_>> {
    let range = walks::link_range(root, x, y)?;
    Some(LinkHit {
        href: ffi::text(range.href_ptr()),
        dom: ffi::node(range.dom_ptr()),
    })
}

pub fn hit_scrollable(root: BoxRef<'_>, x: f64, y: f64) -> Option<(BoxRef<'_>, f64, f64)> {
    walks::scrollable(root, x, y)
}

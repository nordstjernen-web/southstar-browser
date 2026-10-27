//! Southstar — the node helpers the page record offers binding crates: wrapper invalidation, template and form-control checks, single-node insertion, attribute-change and src bookkeeping, and the page's form reset, layout root and CSP header.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_layout::BoxRef;

use crate::ffi::{NsJs, record, wrapper};

pub fn arm_js_invalidate(node: Node<'_>) {
    wrapper::arm_js_invalidate(node);
}

pub fn img_src_layout_neutral(node: Node<'_>) -> bool {
    record::img_src_layout_neutral(node)
}

pub fn in_template_content(node: Node<'_>) -> bool {
    record::in_template_content(node)
}

pub fn is_disabled_form_control(node: Node<'_>) -> bool {
    record::is_disabled_form_control(node)
}

pub fn insert_before_single(js: *const NsJs, child: Node<'_>, reference: Node<'_>) {
    record::insert_before_single(js, child, reference);
}

pub fn ce_attr_changed(
    js: *const NsJs,
    node: Node<'_>,
    name: &CStr,
    old: Option<&CStr>,
    new: Option<&CStr>,
) {
    record::ce_attr_changed(js, node, name, old, new);
}

pub fn set_src_recorded(js: *const NsJs, node: Node<'_>, src: &[u8]) {
    record::set_src_recorded(js, node, src);
}

pub fn media_native_formats() -> u32 {
    record::media_native_formats()
}

pub fn form_reset(js: *const NsJs, form: Node<'_>) {
    record::form_reset(js, form);
}

pub fn set_layout_root(js: *const NsJs, root: Option<BoxRef<'_>>) {
    record::set_layout_root(js, root);
}

pub fn add_csp_header(js: *const NsJs, header: Option<&CStr>) {
    record::add_csp_header(js, header);
}

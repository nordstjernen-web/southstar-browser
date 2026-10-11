//! Southstar — the box-tree entry points layout.c calls, and the layout.c, paint and glib calls box-tree construction makes back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable, boolean, g_array_append_vals, g_free, strdup};
use southstar_layout::{BoxRef, InlineAtomic, InlineAttr, LinkRange, NsBox, Style};
use southstar_style::{NsCssValue, StyleRef, StyleTable, ValueRef};

use crate::{atomic, run};

#[repr(C)]
struct RawCollectState {
    focused_input: *const NsNode,
    open_select: *const NsNode,
    inline_skip: *const NsNode,
    pseudo_blocks_host: *const NsNode,
    form_control_inline: *const NsNode,
    focused_caret_byte: usize,
    focused_sel_anchor_byte: usize,
    inline_collect_depth: c_int,
    focused_is_contenteditable: GBoolean,
    datalist_open: GBoolean,
    pseudo_block_before: GBoolean,
    pseudo_block_after: GBoolean,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawCollectState>() == 80);

unsafe extern "C" {
    fn ns_layout_collect_state_get(out: *mut RawCollectState);
    fn ns_layout_collect_absolute(
        n: *const NsNode,
        fixed: GBoolean,
        placeholder: GBoolean,
    ) -> *mut NsBox;
    fn ns_layout_collect_atomic_block(
        n: *const NsNode,
        styles: *mut GHashTable,
        depth: c_int,
    ) -> *mut NsBox;
    fn ns_layout_pseudo_block_with_text(ps: *const Style, txt: *const c_char) -> *mut NsBox;
    fn ns_layout_box_new_inline() -> *mut NsBox;
    fn ns_paint_li_is_inside(s: *const Style) -> GBoolean;
    fn ns_paint_li_marker_text(
        li: *const NsNode,
        s: *const Style,
        out: *mut c_char,
        out_sz: usize,
    ) -> GBoolean;
    fn ns_css_font_stretch_rank(v: *const NsCssValue) -> c_int;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_isalnum(c: u32) -> GBoolean;
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
}

#[derive(Clone, Copy)]
pub struct CollectState {
    pub focused_input: *const NsNode,
    pub open_select: *const NsNode,
    pub inline_skip: *const NsNode,
    pub pseudo_blocks_host: *const NsNode,
    pub form_control_inline: *const NsNode,
    pub caret_byte: usize,
    pub anchor_byte: usize,
    pub inline_collect_depth: c_int,
    pub focused_is_contenteditable: bool,
    pub datalist_open: bool,
    pub pseudo_block_before: bool,
    pub pseudo_block_after: bool,
}

pub fn collect_state() -> CollectState {
    let mut raw = RawCollectState {
        focused_input: ptr::null(),
        open_select: ptr::null(),
        inline_skip: ptr::null(),
        pseudo_blocks_host: ptr::null(),
        form_control_inline: ptr::null(),
        focused_caret_byte: 0,
        focused_sel_anchor_byte: 0,
        inline_collect_depth: 0,
        focused_is_contenteditable: 0,
        datalist_open: 0,
        pseudo_block_before: 0,
        pseudo_block_after: 0,
    };
    unsafe { ns_layout_collect_state_get(&mut raw) };
    CollectState {
        focused_input: raw.focused_input,
        open_select: raw.open_select,
        inline_skip: raw.inline_skip,
        pseudo_blocks_host: raw.pseudo_blocks_host,
        form_control_inline: raw.form_control_inline,
        caret_byte: raw.focused_caret_byte,
        anchor_byte: raw.focused_sel_anchor_byte,
        inline_collect_depth: raw.inline_collect_depth,
        focused_is_contenteditable: raw.focused_is_contenteditable != 0,
        datalist_open: raw.datalist_open != 0,
        pseudo_block_before: raw.pseudo_block_before != 0,
        pseudo_block_after: raw.pseudo_block_after != 0,
    }
}

pub fn collect_absolute(n: Node<'_>, fixed: bool, placeholder: bool) -> *mut NsBox {
    unsafe { ns_layout_collect_absolute(n.as_ptr(), boolean(fixed), boolean(placeholder)) }
}

pub fn collect_atomic_block(n: Node<'_>, styles: StyleTable, depth: c_int) -> *mut NsBox {
    unsafe { ns_layout_collect_atomic_block(n.as_ptr(), styles.as_ptr(), depth) }
}

pub fn pseudo_block_with_text(ps: StyleRef<'_>, txt: Option<&[u8]>) -> *mut NsBox {
    let txt = txt.map(strdup);
    let b = unsafe {
        ns_layout_pseudo_block_with_text(ps.as_ptr(), txt.map_or(ptr::null(), |t| t.cast_const()))
    };
    if let Some(t) = txt {
        unsafe { g_free(t.cast()) };
    }
    b
}

pub fn li_marker(li: Node<'_>, s: Option<StyleRef<'_>>) -> Option<Vec<u8>> {
    let sp = s.map_or(ptr::null(), StyleRef::as_ptr);
    if unsafe { ns_paint_li_is_inside(sp) } == 0 {
        return None;
    }
    let mut marker = [0 as c_char; 64];
    let ok = unsafe { ns_paint_li_marker_text(li.as_ptr(), sp, marker.as_mut_ptr(), 64) };
    (ok != 0).then(|| {
        unsafe { CStr::from_ptr(marker.as_ptr()) }
            .to_bytes()
            .to_vec()
    })
}

pub fn font_stretch_rank(v: Option<ValueRef<'_>>) -> c_int {
    unsafe { ns_css_font_stretch_rank(v.map_or(ptr::null(), ValueRef::as_ptr)) }
}

pub fn utf8_get_char(s: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 8];
    let tail = &s[at.min(s.len())..];
    let n = tail.len().min(7);
    buf[..n].copy_from_slice(&tail[..n]);
    unsafe { g_utf8_get_char(buf.as_ptr().cast()) }
}

pub fn unichar_isalnum(c: u32) -> bool {
    unsafe { g_unichar_isalnum(c) != 0 }
}

pub fn format_g(v: f64) -> Vec<u8> {
    let p = unsafe { g_strdup_printf(c"%g".as_ptr(), v) };
    if p.is_null() {
        return Vec::new();
    }
    let out = unsafe { CStr::from_ptr(p) }.to_bytes().to_vec();
    unsafe { g_free(p.cast()) };
    out
}

pub struct RawLink {
    pub start: usize,
    pub len: usize,
    pub href: *const c_char,
    pub target: *const c_char,
    pub dom: *const NsNode,
}

pub struct RunParts {
    pub text: Vec<u8>,
    pub links: Vec<RawLink>,
    pub attrs: Vec<InlineAttr>,
    pub atomics: Vec<(usize, *mut NsBox)>,
}

fn dup_or_null(p: *const c_char) -> *mut c_char {
    if p.is_null() {
        return ptr::null_mut();
    }
    strdup(unsafe { CStr::from_ptr(p) }.to_bytes())
}

pub fn new_inline_run(parts: RunParts) -> *mut NsBox {
    let raw = unsafe { ns_layout_box_new_inline() };
    let Some(b) = (unsafe { BoxRef::from_ptr(raw) }) else {
        return raw;
    };
    if !parts.links.is_empty() {
        let links = b.ensure_links_array();
        for l in &parts.links {
            let out = LinkRange::new(
                l.start,
                l.len,
                dup_or_null(l.href),
                dup_or_null(l.target),
                l.dom.cast(),
            );
            unsafe { g_array_append_vals(links, ptr::from_ref(&out).cast(), 1) };
        }
    }
    for a in &parts.attrs {
        if let Some(src) = a.bg_image_src() {
            b.set_bg_image_if_unset(src, a.bg_image());
        }
    }
    b.put_attrs(Some(&parts.attrs));
    if !parts.atomics.is_empty() {
        let atomics: Vec<InlineAtomic> = parts
            .atomics
            .iter()
            .map(|&(off, child)| {
                if let Some(c) = unsafe { BoxRef::from_ptr(child) }
                    && c.parent().is_none()
                {
                    c.set_parent(Some(b));
                }
                InlineAtomic::new(off, child)
            })
            .collect();
        b.put_atomics(Some(&atomics));
    }
    b.set_text_bytes(&parts.text);
    raw
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn style<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_collect_inline_run(
    first: *const NsNode,
    last_excl: *const NsNode,
    styles: *mut GHashTable,
    abs_placeholders: GBoolean,
) -> *mut NsBox {
    run::build_inline_run(
        node(first),
        last_excl,
        unsafe { StyleTable::from_ptr(styles) },
        abs_placeholders != 0,
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_is_atomic_inline(
    n: *const NsNode,
    styles: *mut GHashTable,
) -> GBoolean {
    boolean(atomic::is_atomic_inline(node(n), unsafe {
        StyleTable::from_ptr(styles)
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_is_inline_level_replaced(
    n: *const NsNode,
    styles: *mut GHashTable,
) -> GBoolean {
    boolean(atomic::is_inline_level_replaced(node(n), unsafe {
        StyleTable::from_ptr(styles)
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_style_has_atomic_inline_box(s: *const Style) -> GBoolean {
    boolean(atomic::style_has_atomic_inline_box(style(s)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_node_is_frame_fallback(n: *const NsNode) -> GBoolean {
    boolean(node(n).is_some_and(atomic::node_is_frame_fallback))
}

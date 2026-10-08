//! Southstar — the C ABI of text selection, as declared in src/selection.h, over the style and inline-paint calls it needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::sync::OnceLock;

use southstar_glib::{
    FALSE, GBoolean, GHashTable, boolean, g_direct_equal, g_direct_hash, g_free,
    g_hash_table_insert, g_hash_table_new_full, g_malloc, strdup,
};
use southstar_layout::{BoxRef, NsBox, Style};

use crate::Selection;

#[repr(C)]
pub struct NsSelection {
    anchor_box: *const NsBox,
    anchor_byte: usize,
    focus_box: *const NsBox,
    focus_byte: usize,
    active: GBoolean,
}

#[repr(C)]
struct NsSelectionRun {
    start: usize,
    end: usize,
}

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_css_keyword_is(v: *const c_void, kw: *const c_char) -> GBoolean;
    fn ns_paint_inline_xy_to_byte(
        b: *const NsBox,
        rel_x: f64,
        rel_y: f64,
        out_byte: *mut usize,
    ) -> GBoolean;
    fn ns_paint_inline_word_range(
        b: *const NsBox,
        byte: usize,
        out_start: *mut usize,
        out_end: *mut usize,
    ) -> GBoolean;
    fn ns_paint_inline_range_extents(
        b: *const NsBox,
        start: usize,
        len: usize,
        element: *const c_void,
        out_x: *mut f64,
        out_y: *mut f64,
        out_w: *mut f64,
        out_h: *mut f64,
    ) -> GBoolean;
}

fn user_select_prop() -> usize {
    static PROP: OnceLock<usize> = OnceLock::new();
    *PROP.get_or_init(|| unsafe { ns_css_prop_id(c"user-select".as_ptr()) } as usize)
}

pub fn user_select_is_none(b: BoxRef<'_>) -> Option<bool> {
    let style: *const Style = b.style();
    if style.is_null() {
        return None;
    }
    let value = unsafe { *style.cast::<*const c_void>().add(user_select_prop()) };
    if value.is_null() {
        return None;
    }
    Some(unsafe { ns_css_keyword_is(value, c"none".as_ptr()) } != 0)
}

pub fn xy_to_byte(b: BoxRef<'_>, x: f64, y: f64) -> usize {
    let mut byte = 0;
    unsafe { ns_paint_inline_xy_to_byte(b.as_ptr(), x, y, &mut byte) };
    byte
}

pub fn word_range(b: BoxRef<'_>, byte: usize) -> Option<(usize, usize)> {
    let (mut start, mut end) = (0, 0);
    let found = unsafe { ns_paint_inline_word_range(b.as_ptr(), byte, &mut start, &mut end) };
    (found != 0).then_some((start, end))
}

pub fn range_extents(b: BoxRef<'_>, start: usize, len: usize) -> Option<(f64, f64, f64, f64)> {
    let (mut x, mut y, mut w, mut h) = (b.x(), b.y(), b.content_width(), b.content_height());
    let found = unsafe {
        ns_paint_inline_range_extents(
            b.as_ptr(),
            start,
            len,
            ptr::null(),
            &mut x,
            &mut y,
            &mut w,
            &mut h,
        )
    };
    (found != 0).then_some((x, y, w, h))
}

unsafe fn load<'a>(sel: &NsSelection) -> Selection<'a> {
    unsafe {
        Selection {
            anchor: BoxRef::from_ptr(sel.anchor_box),
            anchor_byte: sel.anchor_byte,
            focus: BoxRef::from_ptr(sel.focus_box),
            focus_byte: sel.focus_byte,
            active: sel.active != 0,
        }
    }
}

fn store(out: &mut NsSelection, sel: &Selection<'_>) {
    *out = NsSelection {
        anchor_box: sel.anchor.map_or(ptr::null(), |b| b.as_ptr()),
        anchor_byte: sel.anchor_byte,
        focus_box: sel.focus.map_or(ptr::null(), |b| b.as_ptr()),
        focus_byte: sel.focus_byte,
        active: boolean(sel.active),
    };
}

unsafe fn update<'a>(
    sel: *mut NsSelection,
    f: impl FnOnce(&mut Selection<'a>) -> bool,
) -> GBoolean {
    let Some(out) = (unsafe { sel.as_mut() }) else {
        return FALSE;
    };
    let mut s = unsafe { load(out) };
    let result = f(&mut s);
    store(out, &s);
    boolean(result)
}

unsafe fn read<'a>(sel: *const NsSelection) -> Selection<'a> {
    unsafe { sel.as_ref().map(|s| load(s)).unwrap_or_default() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_clear(sel: *mut NsSelection) {
    unsafe {
        update(sel, |s| {
            s.clear();
            false
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_has_range(sel: *const NsSelection) -> GBoolean {
    boolean(unsafe { read(sel) }.has_range())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_text_at(root: *const NsBox, x: f64, y: f64) -> GBoolean {
    boolean(crate::text_at(unsafe { BoxRef::from_ptr(root) }, x, y))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_anchor_at(
    sel: *mut NsSelection,
    root: *const NsBox,
    x: f64,
    y: f64,
) -> GBoolean {
    let root = unsafe { BoxRef::from_ptr(root) };
    unsafe { update(sel, |s| s.anchor_at(root, x, y)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_extend_to(
    sel: *mut NsSelection,
    root: *const NsBox,
    x: f64,
    y: f64,
) -> GBoolean {
    let root = unsafe { BoxRef::from_ptr(root) };
    unsafe { update(sel, |s| s.extend_to(root, x, y)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_select_all(
    sel: *mut NsSelection,
    root: *const NsBox,
) -> GBoolean {
    let root = unsafe { BoxRef::from_ptr(root) };
    unsafe { update(sel, |s| s.select_all(root)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_select_word_at(
    sel: *mut NsSelection,
    root: *const NsBox,
    x: f64,
    y: f64,
) -> GBoolean {
    let root = unsafe { BoxRef::from_ptr(root) };
    unsafe { update(sel, |s| s.select_word_at(root, x, y)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_select_block_at(
    sel: *mut NsSelection,
    root: *const NsBox,
    x: f64,
    y: f64,
) -> GBoolean {
    let root = unsafe { BoxRef::from_ptr(root) };
    unsafe { update(sel, |s| s.select_block_at(root, x, y)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_ranges(
    root: *const NsBox,
    sel: *const NsSelection,
) -> *mut GHashTable {
    let runs = unsafe { read(sel) }.ranges(unsafe { BoxRef::from_ptr(root) });
    if runs.is_empty() {
        return ptr::null_mut();
    }
    unsafe {
        let table = g_hash_table_new_full(
            Some(g_direct_hash),
            Some(g_direct_equal),
            None,
            Some(g_free),
        );
        for run in runs {
            let value = g_malloc(size_of::<NsSelectionRun>()).cast::<NsSelectionRun>();
            value.write(NsSelectionRun {
                start: run.start,
                end: run.end,
            });
            g_hash_table_insert(table, run.b.as_ptr().cast_mut().cast(), value.cast());
        }
        table
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_bounds(
    root: *const NsBox,
    sel: *const NsSelection,
    out_x: *mut f64,
    out_y: *mut f64,
    out_w: *mut f64,
    out_h: *mut f64,
) -> GBoolean {
    let Some((x, y, w, h)) = unsafe { read(sel) }.bounds(unsafe { BoxRef::from_ptr(root) }) else {
        return FALSE;
    };
    for (out, v) in [(out_x, x), (out_y, y), (out_w, w), (out_h, h)] {
        if let Some(out) = unsafe { out.as_mut() } {
            *out = v;
        }
    }
    boolean(true)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_selection_collect_text(
    root: *const NsBox,
    sel: *const NsSelection,
) -> *mut c_char {
    unsafe { read(sel) }
        .collect_text(unsafe { BoxRef::from_ptr(root) })
        .map_or(ptr::null_mut(), |text| strdup(&text))
}

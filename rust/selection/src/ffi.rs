//! Southstar — the stored form of a text selection and the C ABI paint reads its ranges through, over the style and inline-paint calls selection needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use core::ptr;

use southstar_glib::{
    FALSE, GBoolean, GHashTable, boolean, g_direct_equal, g_direct_hash, g_free,
    g_hash_table_insert, g_hash_table_new_full, g_malloc,
};
use southstar_layout::{BoxRef, NsBox};
use southstar_style::PropId;

use crate::Selection;

#[repr(C)]
#[derive(Clone, Copy)]
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

pub fn user_select_is_none(b: BoxRef<'_>) -> Option<bool> {
    let value = southstar_style::box_style(b)?.get(PropId::UserSelect)?;
    Some(value.is_keyword(c"none"))
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

impl NsSelection {
    pub const EMPTY: NsSelection = NsSelection {
        anchor_box: ptr::null(),
        anchor_byte: 0,
        focus_box: ptr::null(),
        focus_byte: 0,
        active: FALSE,
    };

    /// # Safety
    ///
    /// The boxes the selection names, if any, stay alive for `'a`.
    pub unsafe fn load<'a>(&self) -> Selection<'a> {
        Selection {
            // SAFETY: the caller guarantees the anchor box is null or alive for 'a.
            anchor: unsafe { BoxRef::from_ptr(self.anchor_box) },
            anchor_byte: self.anchor_byte,
            // SAFETY: the caller guarantees the focus box is null or alive for 'a.
            focus: unsafe { BoxRef::from_ptr(self.focus_box) },
            focus_byte: self.focus_byte,
            active: self.active != 0,
        }
    }

    pub fn store(sel: &Selection<'_>) -> NsSelection {
        NsSelection {
            anchor_box: sel.anchor.map_or(ptr::null(), |b| b.as_ptr()),
            anchor_byte: sel.anchor_byte,
            focus_box: sel.focus.map_or(ptr::null(), |b| b.as_ptr()),
            focus_byte: sel.focus_byte,
            active: boolean(sel.active),
        }
    }
}

unsafe fn read<'a>(sel: *const NsSelection) -> Selection<'a> {
    // SAFETY: paint passes null or the page's stored selection, alive for the call.
    let Some(sel) = (unsafe { sel.as_ref() }) else {
        return Selection::default();
    };
    // SAFETY: the page clears its selection before it frees the layout, so the boxes it names are alive.
    unsafe { sel.load() }
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

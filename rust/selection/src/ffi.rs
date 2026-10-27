//! Southstar — the stored form of a text selection that paint reads its ranges through, and the style and inline-paint calls selection needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;

use southstar_glib::{FALSE, GBoolean, boolean};
use southstar_layout::{BoxRef, NsBox, paint_links};
use southstar_style::PropId;

use crate::{Run, Selection};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NsSelection {
    anchor_box: *const NsBox,
    anchor_byte: usize,
    focus_box: *const NsBox,
    focus_byte: usize,
    active: GBoolean,
}

pub fn user_select_is_none(b: BoxRef<'_>) -> Option<bool> {
    let value = southstar_style::box_style(b)?.get(PropId::UserSelect)?;
    Some(value.is_keyword(c"none"))
}

pub fn xy_to_byte(b: BoxRef<'_>, x: f64, y: f64) -> usize {
    paint_links::inline_xy_to_byte(b, x, y).unwrap_or(0)
}

pub fn word_range(b: BoxRef<'_>, byte: usize) -> Option<(usize, usize)> {
    paint_links::inline_word_range(b, byte)
}

pub fn range_extents(b: BoxRef<'_>, start: usize, len: usize) -> Option<(f64, f64, f64, f64)> {
    paint_links::inline_range_extents(b, start, len, None)
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

pub fn stored_ranges<'a>(root: BoxRef<'a>, sel: &NsSelection) -> Vec<Run<'a>> {
    // SAFETY: the page clears its selection before it frees the layout, so the boxes it names are alive while its root is.
    let sel: Selection<'a> = unsafe { sel.load() };
    sel.ranges(Some(root))
}

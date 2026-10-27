//! Southstar — the stored form of a text selection that paint reads its ranges through, and the style and inline-paint calls selection needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxRef, paint_links};
use southstar_style::PropId;

use crate::{Run, Selection};

#[derive(Clone, Copy)]
pub struct NsSelection {
    anchor_box: Option<BoxRef<'static>>,
    anchor_byte: usize,
    focus_box: Option<BoxRef<'static>>,
    focus_byte: usize,
    active: bool,
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
        anchor_box: None,
        anchor_byte: 0,
        focus_box: None,
        focus_byte: 0,
        active: false,
    };

    pub fn load<'a>(&self) -> Selection<'a> {
        Selection {
            anchor: self.anchor_box,
            anchor_byte: self.anchor_byte,
            focus: self.focus_box,
            focus_byte: self.focus_byte,
            active: self.active,
        }
    }

    pub fn store(sel: &Selection<'_>) -> NsSelection {
        NsSelection {
            anchor_box: sel.anchor.map(BoxRef::into_static),
            anchor_byte: sel.anchor_byte,
            focus_box: sel.focus.map(BoxRef::into_static),
            focus_byte: sel.focus_byte,
            active: sel.active,
        }
    }
}

pub fn stored_ranges<'a>(root: BoxRef<'a>, sel: &NsSelection) -> Vec<Run<'a>> {
    sel.load().ranges(Some(root))
}

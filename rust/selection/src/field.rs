//! Southstar — the byte offset in a form field's value under a point on its rendered text.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr, children, inline_kind};

use crate::{LINE_SEPARATOR, ffi};

const BULLET_LEN: usize = "\u{2022}".len();

pub struct FieldText<'v> {
    pub value: &'v [u8],
    pub password: bool,
}

fn char_len(lead: u8) -> usize {
    match lead {
        0..0xc0 => 1,
        0xc0..0xe0 => 2,
        0xe0..0xf0 => 3,
        _ => 4,
    }
}

impl FieldText<'_> {
    fn shown_len(&self, at: usize, len: usize) -> usize {
        if self.password {
            return BULLET_LEN;
        }
        match self.value[at] {
            b'\n' => LINE_SEPARATOR.len(),
            b'\r' => 0,
            _ => len,
        }
    }

    fn value_offset(&self, value_start: usize, shown: usize) -> usize {
        let value = self.value;
        let mut at = value_start.min(value.len());
        let mut consumed = 0;
        while at < value.len() && consumed < shown {
            let len = char_len(value[at]).min(value.len() - at);
            let shown_len = self.shown_len(at, len);
            if consumed + shown_len > shown {
                break;
            }
            consumed += shown_len;
            at += len;
        }
        at
    }

    fn offset_in(&self, attr: &InlineAttr, run_byte: usize) -> usize {
        let shown = run_byte.clamp(attr.start, attr.start + attr.len) - attr.start;
        self.value_offset(attr.value_start, attr.split_offset + shown)
    }
}

struct Located<'a> {
    run: BoxRef<'a>,
    attr: InlineAttr,
    x: f64,
    y: f64,
}

impl Located<'_> {
    fn inside(&self) -> bool {
        (0.0..=self.run.content_width()).contains(&self.x)
            && (0.0..=self.run.content_height()).contains(&self.y)
    }

    fn byte(&self) -> usize {
        let x = self.x.clamp(0.0, self.run.content_width());
        let y = self.y.clamp(0.0, self.run.content_height());
        ffi::xy_to_byte(self.run, x, y)
    }
}

fn is_field(attr: &InlineAttr, field: Node<'_>) -> bool {
    attr.kind == inline_kind::FIELD_VALUE && attr.dom_ptr() == field.as_ptr().cast()
}

fn locate<'a>(b: BoxRef<'a>, field: Node<'_>, x: f64, y: f64) -> Option<Located<'a>> {
    if let Some(attr) = b.attrs().iter().find(|a| is_field(a, field)).cloned() {
        return Some(Located {
            run: b,
            attr,
            x: x - b.x(),
            y: y - b.y(),
        });
    }
    let (cx, cy) = (x + b.scroll_x(), y + b.scroll_y());
    if let Some(found) = children(b).find_map(|c| locate(c, field, cx, cy)) {
        return Some(found);
    }
    b.inline_atomics()
        .unwrap_or_default()
        .iter()
        .find_map(|atomic| {
            let ab = atomic.box_ref()?;
            let (ox, oy) = atomic.owner_offset();
            let dx = b.x() + ox + ab.rel_dx() - ab.x();
            let dy = b.y() + oy + ab.rel_dy() - ab.y();
            locate(ab, field, cx - dx, cy - dy)
        })
}

pub fn field_offset_at(
    root: BoxRef<'_>,
    field: Node<'_>,
    text: &FieldText<'_>,
    x: f64,
    y: f64,
    clamp: bool,
) -> Option<usize> {
    let at = locate(root, field, x, y)?;
    if !clamp && !at.inside() {
        return None;
    }
    Some(text.offset_in(&at.attr, at.byte()))
}

pub fn field_word_at(
    root: BoxRef<'_>,
    field: Node<'_>,
    text: &FieldText<'_>,
    x: f64,
    y: f64,
) -> Option<(usize, usize)> {
    let at = locate(root, field, x, y).filter(Located::inside)?;
    let (first, end) = (at.attr.start, at.attr.start + at.attr.len);
    let byte = at.byte().clamp(first, end.saturating_sub(1).max(first));
    let (start, stop) = ffi::word_range(at.run, byte)?;
    Some((
        text.offset_in(&at.attr, start),
        text.offset_in(&at.attr, stop),
    ))
}

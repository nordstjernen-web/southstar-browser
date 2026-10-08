//! Southstar — paginates a laid-out page onto sheets of paper.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::CStr;

use ffi::BreakProp;
use southstar_layout::{BoxKind, BoxRef, children};

const MAX_SPANS: usize = 1 << 20;
const MAX_SHEETS: usize = 4096;

#[derive(Clone, Copy)]
struct Span {
    top: f64,
    bottom: f64,
}

#[derive(Default)]
struct Breaks {
    forced: Vec<f64>,
    spans: Vec<Span>,
}

pub(crate) fn exceeds(value: f64, floor: f64) -> bool {
    value > floor
}

fn forces_page_break(kw: Option<&CStr>) -> bool {
    kw.is_some_and(|kw| {
        matches!(
            kw.to_bytes(),
            b"page" | b"always" | b"all" | b"left" | b"right" | b"recto" | b"verso"
        )
    })
}

fn avoids_page_break(kw: Option<&CStr>) -> bool {
    kw.is_some_and(|kw| matches!(kw.to_bytes(), b"avoid" | b"avoid-page"))
}

fn page_extent(b: BoxRef<'_>) -> (f64, f64) {
    let (margin, padding, border) = (b.margin(), b.padding(), b.border());
    let top = b.y() + margin.top;
    let bottom =
        top + b.content_height() + padding.top + padding.bottom + border.top + border.bottom;
    (top, bottom)
}

impl Breaks {
    fn add_span(&mut self, top: f64, bottom: f64, page_h: f64) {
        if !exceeds(bottom, top) || bottom - top > page_h {
            return;
        }
        self.spans.push(Span { top, bottom });
    }

    fn add_text_line_spans(&mut self, b: BoxRef<'_>, top: f64, bottom: f64, page_h: f64) {
        let line_h = ffi::line_height_px(b);
        if !exceeds(line_h, 1.0) || bottom - top <= line_h * 1.5 {
            self.add_span(top, bottom, page_h);
            return;
        }
        let mut room = MAX_SPANS.saturating_sub(self.spans.len());
        let mut y = top;
        while y < bottom - 0.5 && y + line_h > y && room > 0 {
            let line_bottom = if y + line_h < bottom {
                y + line_h
            } else {
                bottom
            };
            self.add_span(y, line_bottom, page_h);
            y += line_h;
            room -= 1;
        }
    }

    fn collect(&mut self, b: BoxRef<'_>, page_h: f64) {
        let (top, bottom) = page_extent(b);
        if b.parent().is_some() {
            if forces_page_break(ffi::break_keyword(b, BreakProp::Before)) {
                self.forced.push(top);
            }
            if forces_page_break(ffi::break_keyword(b, BreakProp::After)) {
                self.forced.push(bottom);
            }
            if avoids_page_break(ffi::break_keyword(b, BreakProp::Inside)) {
                self.add_span(top, bottom, page_h);
                return;
            }
        }
        if b.first_child().is_none() {
            let has_text = b.text().is_some_and(|t| !t.is_empty());
            if b.kind() == BoxKind::Inline && has_text {
                self.add_text_line_spans(b, top, bottom, page_h);
            } else {
                self.add_span(top, bottom, page_h);
            }
            return;
        }
        for child in children(b) {
            self.collect(child, page_h);
        }
    }

    fn next_forced(&self, after: f64, limit: f64) -> Option<f64> {
        self.forced
            .iter()
            .copied()
            .find(|&f| f > after + 0.5 && f <= limit + 0.5)
    }

    fn pull_above_spans(&self, start: f64, limit: f64) -> f64 {
        let mut cut = limit;
        for _ in 0..8 {
            let mut moved = cut;
            for s in &self.spans {
                if s.top > start + 0.5 && s.top < moved - 0.5 && s.bottom > moved + 0.5 {
                    moved = s.top;
                }
            }
            if moved >= cut - 0.01 {
                break;
            }
            cut = moved;
        }
        if cut > start + 1.0 { cut } else { limit }
    }
}

pub fn page_offsets(root: Option<BoxRef<'_>>, page_content_height: f64) -> Vec<f64> {
    let mut offsets = vec![0.0];
    let Some(root) = root else {
        return offsets;
    };
    if !exceeds(page_content_height, 1.0) {
        return offsets;
    }
    let mut breaks = Breaks::default();
    breaks.collect(root, page_content_height);
    breaks.forced.retain(|f| !f.is_nan());
    breaks.forced.sort_by(f64::total_cmp);

    let doc_bottom = root.max_bottom(0.0);
    let mut cur = 0.0;
    while cur + page_content_height < doc_bottom - 0.5 && offsets.len() < MAX_SHEETS {
        let limit = cur + page_content_height;
        let mut cut = breaks
            .next_forced(cur, limit)
            .unwrap_or_else(|| breaks.pull_above_spans(cur, limit));
        if cut <= cur + 1.0 {
            cut = limit;
        }
        offsets.push(cut);
        cur = cut;
    }
    offsets
}

pub fn page_bottom(offsets: &[f64], i: usize, page_content_height: f64) -> f64 {
    match (offsets.get(i), offsets.get(i + 1)) {
        (Some(_), Some(&next)) => next,
        (Some(&top), None) => top + page_content_height,
        _ => 0.0,
    }
}

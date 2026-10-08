//! Southstar — text selection on the rendered page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use southstar_layout::{BoxKind, BoxRef, children};

const LINE_SEPARATOR: &[u8] = "\u{2028}".as_bytes();
const PARAGRAPH_SEPARATOR: &[u8] = "\u{2029}".as_bytes();
const ZERO_WIDTH_SPACE: &[u8] = "\u{200b}".as_bytes();
const BYTE_ORDER_MARK: &[u8] = "\u{feff}".as_bytes();

#[derive(Clone, Copy, Default)]
pub struct Selection<'a> {
    pub anchor: Option<BoxRef<'a>>,
    pub anchor_byte: usize,
    pub focus: Option<BoxRef<'a>>,
    pub focus_byte: usize,
    pub active: bool,
}

pub struct Run<'a> {
    pub b: BoxRef<'a>,
    pub start: usize,
    pub end: usize,
}

struct Endpoints<'a> {
    first: BoxRef<'a>,
    first_byte: usize,
    last: BoxRef<'a>,
    last_byte: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Walk {
    Before,
    Inside,
    Done,
}

fn text(b: BoxRef<'_>) -> &[u8] {
    b.text().map_or(&[], |t| t.to_bytes())
}

fn has_text(b: BoxRef<'_>) -> bool {
    !text(b).is_empty()
}

fn same(a: Option<BoxRef<'_>>, b: Option<BoxRef<'_>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.same(b),
        (None, None) => true,
        _ => false,
    }
}

fn user_selectable(b: BoxRef<'_>) -> bool {
    let mut p = Some(b);
    while let Some(cur) = p {
        if let Some(none) = ffi::user_select_is_none(cur) {
            return !none;
        }
        p = cur.parent();
    }
    true
}

fn containing_block(b: BoxRef<'_>) -> Option<BoxRef<'_>> {
    let mut p = b.parent();
    while let Some(cur) = p.filter(|c| c.kind() == BoxKind::Inline) {
        p = cur.parent();
    }
    p
}

fn xy_inside(b: BoxRef<'_>, x: f64, y: f64) -> bool {
    let (w, h) = (b.content_width(), b.content_height());
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    x >= b.x() && x <= b.x() + w && y >= b.y() && y <= b.y() + h
}

fn selectable_text_run(b: BoxRef<'_>) -> bool {
    b.kind() == BoxKind::Inline && has_text(b) && user_selectable(b)
}

fn inline_at(root: BoxRef<'_>, x: f64, y: f64) -> Option<(BoxRef<'_>, f64, f64)> {
    if root.clips_out_point(x, y) {
        return None;
    }
    let (cx, cy) = (x + root.scroll_x(), y + root.scroll_y());
    if let Some(hit) = children(root).find_map(|c| inline_at(c, cx, cy)) {
        return Some(hit);
    }
    (selectable_text_run(root) && xy_inside(root, x, y)).then_some((root, x, y))
}

#[derive(Default)]
struct Nearest<'a> {
    best: Option<(BoxRef<'a>, f64, f64)>,
    gap_y: f64,
    gap_x: f64,
}

fn gap(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo - v
    } else if v > hi {
        v - hi
    } else {
        0.0
    }
}

impl<'a> Nearest<'a> {
    fn consider(&mut self, b: BoxRef<'a>, x: f64, y: f64) {
        let gap_y = gap(y, b.y(), b.y() + b.content_height());
        let gap_x = gap(x, b.x(), b.x() + b.content_width());
        if self.best.is_none()
            || gap_y < self.gap_y - 0.01
            || (gap_y <= self.gap_y + 0.01 && gap_x < self.gap_x)
        {
            self.best = Some((b, x, y));
            self.gap_y = gap_y;
            self.gap_x = gap_x;
        }
    }

    fn walk(&mut self, root: BoxRef<'a>, x: f64, y: f64) {
        if selectable_text_run(root) && root.content_width() > 0.0 && root.content_height() > 0.0 {
            self.consider(root, x, y);
        }
        if root.clips_out_point(x, y) {
            return;
        }
        let (cx, cy) = (x + root.scroll_x(), y + root.scroll_y());
        for c in children(root) {
            self.walk(c, cx, cy);
        }
    }
}

fn resolve_point(root: Option<BoxRef<'_>>, x: f64, y: f64) -> Option<(BoxRef<'_>, usize)> {
    let root = root?;
    let (b, hx, hy) = inline_at(root, x, y).or_else(|| {
        let mut nearest = Nearest::default();
        nearest.walk(root, x, y);
        nearest.best
    })?;
    let mut local_x = hx - b.x();
    let mut local_y = hy - b.y();
    if local_x < 0.0 {
        local_x = 0.0;
    }
    if local_y < 0.0 {
        local_y = 0.0;
    }
    if local_x > b.content_width() {
        local_x = b.content_width();
    }
    if local_y > b.content_height() {
        local_y = b.content_height();
    }
    let mut byte = ffi::xy_to_byte(b, local_x, local_y);
    if b.text().is_some() {
        byte = byte.min(text(b).len());
    }
    Some((b, byte))
}

fn inline_boxes(root: Option<BoxRef<'_>>) -> Vec<BoxRef<'_>> {
    let mut out = Vec::new();
    let mut stack: Vec<BoxRef<'_>> = root.into_iter().collect();
    while let Some(b) = stack.pop() {
        if b.kind() == BoxKind::Inline {
            out.push(b);
        }
        let first = stack.len();
        stack.extend(children(b));
        stack[first..].reverse();
    }
    out
}

fn edges(root: BoxRef<'_>) -> Option<(BoxRef<'_>, BoxRef<'_>)> {
    let runs: Vec<_> = inline_boxes(Some(root))
        .into_iter()
        .filter(|&b| has_text(b) && user_selectable(b))
        .collect();
    Some((*runs.first()?, *runs.last()?))
}

impl<'a> Selection<'a> {
    pub fn clear(&mut self) {
        *self = Selection::default();
    }

    pub fn has_range(&self) -> bool {
        match (self.active, self.anchor, self.focus) {
            (true, Some(a), Some(f)) => !(a.same(f) && self.anchor_byte == self.focus_byte),
            _ => false,
        }
    }

    fn set(
        &mut self,
        anchor: BoxRef<'a>,
        anchor_byte: usize,
        focus: BoxRef<'a>,
        focus_byte: usize,
    ) {
        *self = Selection {
            anchor: Some(anchor),
            anchor_byte,
            focus: Some(focus),
            focus_byte,
            active: true,
        };
    }

    pub fn anchor_at(&mut self, root: Option<BoxRef<'a>>, x: f64, y: f64) -> bool {
        match resolve_point(root, x, y) {
            Some((b, byte)) => {
                self.set(b, byte, b, byte);
                true
            }
            None => {
                self.clear();
                false
            }
        }
    }

    pub fn extend_to(&mut self, root: Option<BoxRef<'a>>, x: f64, y: f64) -> bool {
        if !self.active || self.anchor.is_none() {
            return false;
        }
        let Some((b, byte)) = resolve_point(root, x, y) else {
            return false;
        };
        self.focus = Some(b);
        self.focus_byte = byte;
        true
    }

    pub fn select_word_at(&mut self, root: Option<BoxRef<'a>>, x: f64, y: f64) -> bool {
        let Some((b, byte)) = resolve_point(root, x, y) else {
            return false;
        };
        let Some((start, end)) = ffi::word_range(b, byte) else {
            return false;
        };
        self.set(b, start, b, end);
        true
    }

    pub fn select_block_at(&mut self, root: Option<BoxRef<'a>>, x: f64, y: f64) -> bool {
        let Some((b, _)) = resolve_point(root, x, y) else {
            return false;
        };
        let block = containing_block(b).unwrap_or(b);
        let Some((first, last)) = edges(block) else {
            return false;
        };
        self.set(first, 0, last, text(last).len());
        true
    }

    pub fn select_all(&mut self, root: Option<BoxRef<'a>>) -> bool {
        let Some((first, last)) = root.and_then(edges) else {
            return false;
        };
        self.set(first, 0, last, text(last).len());
        true
    }

    fn ordered(&self, root: BoxRef<'a>) -> Option<Endpoints<'a>> {
        let (anchor, focus) = (self.anchor?, self.focus?);
        if anchor.same(focus) {
            return Some(Endpoints {
                first: anchor,
                first_byte: self.anchor_byte.min(self.focus_byte),
                last: anchor,
                last_byte: self.anchor_byte.max(self.focus_byte),
            });
        }
        let first_seen = inline_boxes(Some(root))
            .into_iter()
            .find(|b| b.same(anchor) || b.same(focus));
        Some(if same(first_seen, Some(anchor)) {
            Endpoints {
                first: anchor,
                first_byte: self.anchor_byte,
                last: focus,
                last_byte: self.focus_byte,
            }
        } else {
            Endpoints {
                first: focus,
                first_byte: self.focus_byte,
                last: anchor,
                last_byte: self.anchor_byte,
            }
        })
    }

    pub fn ranges(&self, root: Option<BoxRef<'a>>) -> Vec<Run<'a>> {
        let mut runs = Vec::new();
        let Some(root) = root.filter(|_| self.has_range()) else {
            return runs;
        };
        let Some(ends) = self.ordered(root) else {
            return runs;
        };
        let mut record = |b: BoxRef<'a>, start: usize, end: usize| {
            if b.text().is_none() || !user_selectable(b) {
                return;
            }
            let len = text(b).len();
            let (start, end) = (start.min(len), end.min(len));
            if start < end {
                runs.push(Run { b, start, end });
            }
        };
        let mut state = Walk::Before;
        for b in inline_boxes(Some(root)) {
            if state == Walk::Done {
                break;
            }
            if ends.first.same(ends.last) {
                if b.same(ends.first) {
                    record(b, ends.first_byte, ends.last_byte);
                    state = Walk::Done;
                }
            } else if state == Walk::Before {
                if b.same(ends.first) {
                    record(b, ends.first_byte, text(b).len());
                    state = Walk::Inside;
                }
            } else if b.same(ends.last) {
                record(b, 0, ends.last_byte);
                state = Walk::Done;
            } else if has_text(b) {
                record(b, 0, text(b).len());
            }
        }
        runs
    }

    pub fn bounds(&self, root: Option<BoxRef<'a>>) -> Option<(f64, f64, f64, f64)> {
        let mut extent: Option<(f64, f64, f64, f64)> = None;
        for run in self.ranges(root) {
            let b = run.b;
            let whole = run.start == 0 && b.text().is_some() && run.end == text(b).len();
            let (rx, ry, rw, rh) = if whole {
                (b.x(), b.y(), b.content_width(), b.content_height())
            } else {
                let Some((x, y, w, h)) = ffi::range_extents(b, run.start, run.end - run.start)
                else {
                    continue;
                };
                (x + b.x(), y + b.y(), w, h)
            };
            extent = Some(match extent {
                None => (rx, ry, rx + rw, ry + rh),
                Some((mut x0, mut y0, mut x1, mut y1)) => {
                    if rx < x0 {
                        x0 = rx;
                    }
                    if ry < y0 {
                        y0 = ry;
                    }
                    if rx + rw > x1 {
                        x1 = rx + rw;
                    }
                    if ry + rh > y1 {
                        y1 = ry + rh;
                    }
                    (x0, y0, x1, y1)
                }
            });
        }
        extent.map(|(x0, y0, x1, y1)| (x0, y0, x1 - x0, y1 - y0))
    }

    pub fn collect_text(&self, root: Option<BoxRef<'a>>) -> Option<Vec<u8>> {
        let root = root.filter(|_| self.has_range())?;
        let ends = self.ordered(root)?;
        let mut out = Vec::new();
        let mut prev_block: Option<BoxRef<'a>> = None;
        let mut append = |b: BoxRef<'a>, start: usize, end: usize| {
            if end <= start || b.text().is_none() {
                return;
            }
            let block = containing_block(b);
            if !out.is_empty() && !same(block, prev_block) {
                out.push(b'\n');
            }
            prev_block = block;
            append_copied_run(&mut out, &text(b)[start..end]);
        };
        let mut state = Walk::Before;
        for b in inline_boxes(Some(root)) {
            if state == Walk::Done {
                break;
            }
            if !has_text(b) {
                if state == Walk::Inside && b.same(ends.last) {
                    state = Walk::Done;
                }
                continue;
            }
            let len = text(b).len();
            if ends.first.same(ends.last) {
                if b.same(ends.first) {
                    if user_selectable(b) {
                        append(b, ends.first_byte.min(len), ends.last_byte.min(len));
                    }
                    state = Walk::Done;
                }
            } else if state == Walk::Before {
                if b.same(ends.first) {
                    if user_selectable(b) {
                        append(b, ends.first_byte.min(len), len);
                    }
                    state = Walk::Inside;
                }
            } else if b.same(ends.last) {
                if user_selectable(b) {
                    append(b, 0, ends.last_byte.min(len));
                }
                state = Walk::Done;
            } else if user_selectable(b) {
                append(b, 0, len);
            }
        }
        Some(out)
    }
}

pub fn text_at(root: Option<BoxRef<'_>>, x: f64, y: f64) -> bool {
    root.is_some_and(|root| inline_at(root, x, y).is_some())
}

fn first_char_len(bytes: &[u8]) -> Option<usize> {
    let head = &bytes[..bytes.len().min(4)];
    let valid = match core::str::from_utf8(head) {
        Ok(s) => s,
        Err(e) => core::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or_default(),
    };
    valid.chars().next().map(char::len_utf8)
}

fn append_copied_run(out: &mut Vec<u8>, run: &[u8]) {
    let mut i = 0;
    while i < run.len() {
        let Some(step) = first_char_len(&run[i..]) else {
            out.push(run[i]);
            i += 1;
            continue;
        };
        let ch = &run[i..i + step];
        if ch == LINE_SEPARATOR || ch == PARAGRAPH_SEPARATOR {
            out.push(b'\n');
        } else if ch != ZERO_WIDTH_SPACE && ch != BYTE_ORDER_MARK {
            out.extend_from_slice(ch);
        }
        i += step;
    }
}

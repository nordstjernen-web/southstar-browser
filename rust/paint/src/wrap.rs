//! Southstar — text-wrap-style: the narrower wrap width that balances a short paragraph's lines or keeps its last line from holding a single word.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_style::{PropId as P, StyleRef};

use crate::text_layout::{self, Layout};
use crate::util::{get, keyword};

const MAX_BALANCED_LINES: i32 = 10;
const PRETTY_STEPS: i32 = 12;
const PRETTY_MIN_FRACTION: f64 = 0.75;

fn lines_at(layout: &Layout, width: i32) -> i32 {
    layout.set_width(width);
    layout.line_count()
}

fn balanced_width(layout: &Layout, full: i32, lines: i32) -> i32 {
    let mut fits = full;
    let mut too_narrow = full / lines.max(1) / 2;
    while fits - too_narrow > text_layout::SCALE {
        let mid = too_narrow + (fits - too_narrow) / 2;
        if lines_at(layout, mid) <= lines {
            fits = mid;
        } else {
            too_narrow = mid;
        }
    }
    fits
}

fn last_line_has_one_word(layout: &Layout) -> bool {
    let count = layout.line_count();
    let Some(line) = layout.line(count - 1) else {
        return false;
    };
    let text = layout.text();
    let start = usize::try_from(line.start_index()).unwrap_or(0);
    let end = start + usize::try_from(line.length()).unwrap_or(0);
    text.as_bytes()
        .get(start..end.min(text.len()))
        .is_some_and(|word| !word.trim_ascii().contains(&b' '))
}

fn pretty_width(layout: &Layout, full: i32, lines: i32) -> i32 {
    if !last_line_has_one_word(layout) {
        return full;
    }
    let step = (f64::from(full) * (1.0 - PRETTY_MIN_FRACTION) / f64::from(PRETTY_STEPS)) as i32;
    (1..=PRETTY_STEPS)
        .map(|i| full - step * i)
        .find(|&width| lines_at(layout, width) == lines && !last_line_has_one_word(layout))
        .unwrap_or(full)
}

pub fn apply_text_wrap_style(layout: &Layout, s: Option<StyleRef<'_>>) {
    let full = layout.width();
    if full <= 0 || layout.ellipsize() != text_layout::ELLIPSIZE_NONE {
        return;
    }
    let balance = match keyword(get(s, P::TextWrapStyle)).map(|k| k.to_bytes()) {
        Some(b"balance") => true,
        Some(b"pretty" | b"avoid-orphans") => false,
        _ => return,
    };
    let lines = layout.line_count();
    if lines < 2 || (balance && lines > MAX_BALANCED_LINES) {
        return;
    }
    let width = if balance {
        balanced_width(layout, full, lines)
    } else {
        pretty_width(layout, full, lines)
    };
    layout.set_width(width);
}

pub fn wrap_shift(layout: &Layout, content_width: f64) -> f64 {
    let width = f64::from(layout.width()) / text_layout::SCALE_F;
    if layout.width() <= 0 || width >= content_width {
        return 0.0;
    }
    match layout.alignment() {
        text_layout::ALIGN_CENTER => (content_width - width) / 2.0,
        text_layout::ALIGN_RIGHT => content_width - width,
        _ => 0.0,
    }
}

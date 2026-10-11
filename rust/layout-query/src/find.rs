//! Southstar — find-in-page over the laid-out text runs: counting the matches of a needle, the first run with a match below or above a scroll offset and the ordinal of a run's first match.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxKind, BoxRef};

fn matches_at(text: &[u8], needle: &[u8], case_sensitive: bool) -> bool {
    if case_sensitive {
        text == needle
    } else {
        text.eq_ignore_ascii_case(needle)
    }
}

pub fn count_in_text(text: &[u8], needle: &[u8], case_sensitive: bool) -> u32 {
    if needle.is_empty() {
        return 0;
    }
    let mut hits = 0u32;
    let mut i = 0;
    while i + needle.len() <= text.len() {
        if matches_at(&text[i..i + needle.len()], needle, case_sensitive) {
            hits += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    hits
}

fn run_text(b: BoxRef<'_>) -> Option<&[u8]> {
    if b.kind() != BoxKind::Inline {
        return None;
    }
    b.text().map(|t| t.to_bytes())
}

fn run_has_match(b: BoxRef<'_>, needle: &[u8], case_sensitive: bool) -> bool {
    run_text(b).is_some_and(|t| count_in_text(t, needle, case_sensitive) > 0)
}

pub fn count_matches(root: BoxRef<'_>, needle: &[u8], case_sensitive: bool) -> u32 {
    let own = run_text(root).map_or(0, |t| count_in_text(t, needle, case_sensitive));
    southstar_layout::children(root)
        .fold(own, |sum, c| sum + count_matches(c, needle, case_sensitive))
}

pub fn first_match_below<'a>(
    root: BoxRef<'a>,
    needle: &[u8],
    y_threshold: f64,
    case_sensitive: bool,
) -> Option<BoxRef<'a>> {
    if root.y() > y_threshold && run_has_match(root, needle, case_sensitive) {
        return Some(root);
    }
    southstar_layout::children(root)
        .find_map(|c| first_match_below(c, needle, y_threshold, case_sensitive))
}

pub fn first_match_above<'a>(
    root: BoxRef<'a>,
    needle: &[u8],
    y_threshold: f64,
    case_sensitive: bool,
) -> Option<BoxRef<'a>> {
    let mut best =
        (root.y() < y_threshold && run_has_match(root, needle, case_sensitive)).then_some(root);
    for c in southstar_layout::children(root) {
        if let Some(m) = first_match_above(c, needle, y_threshold, case_sensitive)
            && best.is_none_or(|b| m.y() > b.y())
        {
            best = Some(m);
        }
    }
    best
}

fn ordinal_walk(
    root: BoxRef<'_>,
    needle: &[u8],
    target: BoxRef<'_>,
    case_sensitive: bool,
    acc: &mut u32,
) -> bool {
    if let Some(text) = run_text(root) {
        let here = count_in_text(text, needle, case_sensitive);
        if root.same(target) {
            if here > 0 {
                *acc += 1;
                return true;
            }
            return false;
        }
        *acc += here;
    }
    southstar_layout::children(root).any(|c| ordinal_walk(c, needle, target, case_sensitive, acc))
}

pub fn match_ordinal(
    root: BoxRef<'_>,
    needle: &[u8],
    target: BoxRef<'_>,
    case_sensitive: bool,
) -> u32 {
    let mut acc = 0;
    if ordinal_walk(root, needle, target, case_sensitive, &mut acc) {
        acc
    } else {
        0
    }
}

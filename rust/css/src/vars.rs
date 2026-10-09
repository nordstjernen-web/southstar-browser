//! Southstar — var() substituted into a value: custom properties looked up through an element's variable map, CSS-wide keywords and registered initial values, fallbacks, and the expansion budget that keeps self-referencing variables from growing without bound.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::declarations::find_function;
use crate::scan::{is_ws, scan_until, trim_range};

const EXPAND_MAX: usize = 1024 * 1024;
const EXPAND_CALLS: u32 = 100_000;
const DEPTH_MAX: i32 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wide {
    None,
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
    RevertRule,
}

pub(crate) trait Lookup {
    fn value(&self, name: &[u8]) -> Option<&[u8]>;
    fn registered_initial(&self, name: &[u8]) -> Option<Option<&[u8]>>;
}

pub(crate) fn wide_kind(text: &[u8]) -> Wide {
    let start = text.iter().position(|&c| !is_ws(c)).unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|&c| !is_ws(c))
        .map_or(start, |i| i + 1);
    let word = &text[start..end.max(start)];
    [
        (&b"inherit"[..], Wide::Inherit),
        (b"initial", Wide::Initial),
        (b"unset", Wide::Unset),
        (b"revert", Wide::Revert),
        (b"revert-layer", Wide::RevertLayer),
        (b"revert-rule", Wide::RevertRule),
    ]
    .iter()
    .find(|(kw, _)| word.eq_ignore_ascii_case(kw))
    .map_or(Wide::None, |&(_, kind)| kind)
}

fn invalid(text: Option<&[u8]>) -> bool {
    text.is_none_or(|t| wide_kind(t) != Wide::None)
}

#[derive(Default)]
struct Budget {
    out_bytes: usize,
    calls: u32,
    overflow: bool,
}

impl Budget {
    fn take(&mut self, n: usize, valid: &mut bool) -> bool {
        if self.overflow {
            return false;
        }
        if n > EXPAND_MAX - self.out_bytes {
            self.overflow = true;
            *valid = false;
            return false;
        }
        self.out_bytes += n;
        true
    }
}

fn expand(
    text: &[u8],
    vars: Option<&dyn Lookup>,
    depth: i32,
    valid: &mut bool,
    budget: &mut Budget,
) -> Vec<u8> {
    if depth > DEPTH_MAX {
        return text.to_vec();
    }
    budget.calls += 1;
    if budget.overflow || budget.calls > EXPAND_CALLS {
        budget.overflow = true;
        *valid = false;
        return Vec::new();
    }
    let end = text.len();
    let mut out = Vec::new();
    let mut p = 0;
    while p < end {
        let Some(fun) = find_function(text, p, end, b"var") else {
            if budget.take(end - p, valid) {
                out.extend_from_slice(&text[p..end]);
            }
            break;
        };
        if !budget.take(fun - p, valid) {
            break;
        }
        out.extend_from_slice(&text[p..fun]);
        let args_start = fun + 4;
        let (args_end, term) = scan_until(text, args_start, end, b")");
        if term != b')' {
            break;
        }
        let (comma, comma_term) = scan_until(text, args_start, args_end, b",");
        let fallback = (comma_term == b',').then(|| trim_range(text, comma + 1, args_end));
        let name = trim_range(
            text,
            args_start,
            if comma_term == b',' { comma } else { args_end },
        );
        let replacement = vars
            .filter(|_| name.starts_with(b"--"))
            .and_then(|vars| vars.value(name));
        let substituted = match replacement {
            Some(replacement) if !replacement.is_empty() && !invalid(Some(replacement)) => {
                let mut sub_valid = true;
                let mut sub = Some(expand(replacement, vars, depth + 1, &mut sub_valid, budget));
                if sub_valid && invalid(sub.as_deref()) {
                    match vars.and_then(|v| v.registered_initial(name)) {
                        Some(initial) => {
                            sub =
                                initial.map(|i| expand(i, vars, depth + 1, &mut sub_valid, budget));
                        }
                        None => sub_valid = false,
                    }
                }
                sub_valid.then(|| sub.unwrap_or_default())
            }
            _ => None,
        };
        match (substituted, fallback) {
            (Some(sub), _) => out.extend_from_slice(&sub),
            (None, Some(fallback)) => {
                let mut nested_valid = true;
                let sub = expand(fallback, vars, depth + 1, &mut nested_valid, budget);
                if nested_valid {
                    out.extend_from_slice(&sub);
                } else {
                    *valid = false;
                }
            }
            (None, None) => *valid = false,
        }
        p = args_end + 1;
    }
    out
}

pub(crate) fn substitute(text: &[u8], vars: Option<&dyn Lookup>, depth: i32) -> Option<Vec<u8>> {
    let mut valid = true;
    let mut budget = Budget::default();
    let out = expand(text, vars, depth, &mut valid, &mut budget);
    valid.then_some(out)
}

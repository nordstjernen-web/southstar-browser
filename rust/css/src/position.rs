//! Southstar — CSS <position> values: the edge keywords, splitting a position into its horizontal and vertical parts, and the canonical order of one to four position tokens.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::ffi;
use crate::gradient::token_is_length_pct;
use crate::scan::{split_ws_limit, split_ws_paren};

pub(crate) fn is_h_edge(t: &[u8]) -> bool {
    t.eq_ignore_ascii_case(b"left") || t.eq_ignore_ascii_case(b"right")
}

pub(crate) fn is_v_edge(t: &[u8]) -> bool {
    t.eq_ignore_ascii_case(b"top") || t.eq_ignore_ascii_case(b"bottom")
}

pub(crate) fn is_keyword(t: &[u8]) -> bool {
    is_h_edge(t) || is_v_edge(t) || t.eq_ignore_ascii_case(b"center")
}

fn from_edge(edge: &[u8], offset: Option<&[u8]>) -> Vec<u8> {
    let far = edge.eq_ignore_ascii_case(b"right") || edge.eq_ignore_ascii_case(b"bottom");
    let Some(offset) = offset else {
        return if far {
            b"100%".to_vec()
        } else {
            b"0%".to_vec()
        };
    };
    if !far {
        return offset.to_vec();
    }
    let text = CString::new(offset).unwrap_or_default();
    let (v, end) = ffi::strtod(&text, 0);
    if &offset[end..] == b"%" {
        let mut out = ffi::format_double(c"%g", 100.0 - v);
        out.push(b'%');
        return out;
    }
    let mut out = b"calc(100% - ".to_vec();
    out.extend_from_slice(offset);
    out.push(b')');
    out
}

pub(crate) fn split(text: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let tokens = split_ws_limit(text, 4);
    let n = tokens.len();
    let mut used = [false; 4];
    let mut x: Option<Vec<u8>> = None;
    let mut y: Option<Vec<u8>> = None;
    for (edge_test, slot) in [
        (is_h_edge as fn(&[u8]) -> bool, &mut x),
        (is_v_edge as fn(&[u8]) -> bool, &mut y),
    ] {
        for i in 0..n {
            if used[i] || !edge_test(tokens[i]) {
                continue;
            }
            let offset = (n >= 3 && i + 1 < n && !is_keyword(tokens[i + 1])).then(|| tokens[i + 1]);
            *slot = Some(from_edge(tokens[i], offset));
            used[i] = true;
            if offset.is_some() {
                used[i + 1] = true;
            }
        }
    }
    for i in 0..n {
        if used[i] {
            continue;
        }
        let value = if tokens[i].eq_ignore_ascii_case(b"center") {
            b"50%".to_vec()
        } else {
            tokens[i].to_vec()
        };
        if x.is_none() {
            x = Some(value);
        } else if y.is_none() {
            y = Some(value);
        }
    }
    (
        x.unwrap_or_else(|| b"50%".to_vec()),
        y.unwrap_or_else(|| b"50%".to_vec()),
    )
}

fn join(parts: &[&[u8]]) -> Vec<u8> {
    parts.join(&b' ')
}

pub(crate) fn canonical_ex(text: &[u8], expand_single: bool, allow_three: bool) -> Option<Vec<u8>> {
    let tok = split_ws_paren(text, 8);
    let m = tok.len();
    if m == 0 || m > 4 || (m == 3 && !allow_three) {
        return None;
    }
    let h0 = is_h_edge(tok[0]);
    let v0 = is_v_edge(tok[0]);
    let c0 = tok[0].eq_ignore_ascii_case(b"center");
    match m {
        1 => {
            if !(h0 || v0 || c0 || token_is_length_pct(tok[0])) {
                return None;
            }
            Some(if !expand_single {
                tok[0].to_vec()
            } else if v0 {
                join(&[b"center", tok[0]])
            } else {
                join(&[tok[0], b"center"])
            })
        }
        3 => {
            let off_after_first = token_is_length_pct(tok[1]);
            let (edge, offset, other) = if off_after_first {
                (tok[0], tok[1], tok[2])
            } else {
                (tok[1], tok[2], tok[0])
            };
            let eh = is_h_edge(edge);
            let ev = is_v_edge(edge);
            let oh = is_h_edge(other);
            let ov = is_v_edge(other);
            let oc = other.eq_ignore_ascii_case(b"center");
            if !(eh || ev) || !token_is_length_pct(offset) || !(oh || ov || oc) {
                return None;
            }
            if (eh && oh) || (ev && ov) {
                return None;
            }
            Some(if eh {
                join(&[edge, offset, other])
            } else {
                join(&[other, edge, offset])
            })
        }
        2 => {
            let h1 = is_h_edge(tok[1]);
            let v1 = is_v_edge(tok[1]);
            let c1 = tok[1].eq_ignore_ascii_case(b"center");
            let k0 = h0 || v0 || c0;
            let k1 = h1 || v1 || c1;
            if k0 && k1 {
                if (h0 && h1) || (v0 && v1) {
                    return None;
                }
                if (v0 && (h1 || c1)) || (c0 && h1) {
                    Some(join(&[tok[1], tok[0]]))
                } else {
                    Some(join(&[tok[0], tok[1]]))
                }
            } else if k0 {
                (!v0 && token_is_length_pct(tok[1])).then(|| join(&[tok[0], tok[1]]))
            } else if k1 {
                (!h1 && token_is_length_pct(tok[0])).then(|| join(&[tok[0], tok[1]]))
            } else {
                (token_is_length_pct(tok[0]) && token_is_length_pct(tok[1]))
                    .then(|| join(&[tok[0], tok[1]]))
            }
        }
        _ => {
            let h2 = is_h_edge(tok[2]);
            let v2 = is_v_edge(tok[2]);
            if !token_is_length_pct(tok[1]) || !token_is_length_pct(tok[3]) {
                return None;
            }
            if h0 && v2 {
                Some(join(&[tok[0], tok[1], tok[2], tok[3]]))
            } else if v0 && h2 {
                Some(join(&[tok[2], tok[3], tok[0], tok[1]]))
            } else {
                None
            }
        }
    }
}

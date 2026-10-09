//! Southstar — a @keyframes stop whose declarations waited on var(): the transform its resolved translate, rotate, scale and transform declarations give the stop.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::transform::{
    OPS_MAX, Op, Transform, parse_rotate_prop, parse_scale_prop, parse_transform,
    parse_translate_prop,
};

fn push(tf: &mut Transform, op: &Op) -> bool {
    let n = tf.n_ops as usize;
    if n >= OPS_MAX {
        return false;
    }
    tf.ops[n] = *op;
    tf.n_ops += 1;
    true
}

pub(crate) fn resolved_transform(resolved: &[u8], existing: Transform) -> Option<Transform> {
    let mut individual = Transform::default();
    let mut list = existing;
    for decl in resolved.split(|&c| c == b';') {
        let Some(colon) = decl.iter().position(|&c| c == b':') else {
            continue;
        };
        let prop = decl[..colon].trim_ascii();
        let value = decl[colon + 1..].trim_ascii();
        if prop.eq_ignore_ascii_case(b"transform") {
            if let Some(tf) = parse_transform(value) {
                list = tf;
            }
            continue;
        }
        let parsed = if prop.eq_ignore_ascii_case(b"translate") {
            parse_translate_prop(value)
        } else if prop.eq_ignore_ascii_case(b"rotate") {
            parse_rotate_prop(value)
        } else if prop.eq_ignore_ascii_case(b"scale") {
            parse_scale_prop(value)
        } else {
            None
        };
        if let Some(tf) = parsed {
            push(&mut individual, &tf.ops[0]);
        }
    }
    let mut merged = individual;
    let n_list = usize::try_from(list.n_ops).unwrap_or(0).min(OPS_MAX);
    for op in &list.ops[..n_list] {
        if !push(&mut merged, op) {
            break;
        }
    }
    (merged.n_ops > 0).then_some(merged)
}

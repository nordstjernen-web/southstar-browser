//! Southstar — the CharacterData and Text methods: substringData, appendData, insertData, deleteData, replaceData and splitText in UTF-16 offsets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi;
use crate::{Element, JsResult};

const INDEX_SIZE_ERR: i32 = 1;

fn sequence_len(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        0xf8..=0xfb => 5,
        0xfc..=0xfd => 6,
        _ => 1,
    }
}

fn unit_width(lead: u8) -> u64 {
    if (0xf0..=0xfd).contains(&lead) { 2 } else { 1 }
}

fn utf16_length(text: &[u8]) -> u64 {
    let mut units = 0;
    let mut p = 0;
    while p < text.len() {
        units += unit_width(text[p]);
        p += sequence_len(text[p]);
    }
    units
}

fn utf16_offset(text: &[u8], offset: u64) -> usize {
    let mut units = 0;
    let mut p = 0;
    while p < text.len() && units < offset {
        let width = unit_width(text[p]);
        if units + width > offset {
            break;
        }
        units += width;
        p += sequence_len(text[p]);
    }
    p.min(text.len())
}

fn current(node: Element) -> &'static [u8] {
    node.text().map_or(&[], |t| t.to_bytes())
}

fn uint_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<u64> {
    scope.to_int32(value).map(|n| u64::from(n as u32))
}

fn string_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<Vec<u8>> {
    let mut bytes = scope.to_bytes(value)?;
    if let Some(nul) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(nul);
    }
    Ok(bytes)
}

fn arity(scope: &mut Scope<'_>, args: &[Value], required: usize) -> JsResult<()> {
    if args.len() >= required {
        return Ok(());
    }
    let message = match required {
        1 => "1 argument required, but only 0 present".to_owned(),
        _ => format!(
            "{required} arguments required, but only {} present",
            args.len()
        ),
    };
    Err(scope.type_error(&message))
}

fn check_offset(scope: &mut Scope<'_>, offset: u64, total: u64) -> JsResult<()> {
    if offset > total {
        return Err(ffi::dom_exception(
            scope,
            "IndexSizeError",
            INDEX_SIZE_ERR,
            "offset is greater than length",
        ));
    }
    Ok(())
}

fn clamp_count(offset: u64, count: u64, total: u64) -> u64 {
    if offset + count > total {
        total - offset
    } else {
        count
    }
}

fn splice(node: Element, offset: u64, count: u64, insert: &[u8]) {
    let cur = current(node);
    let start = utf16_offset(cur, offset);
    let end = start + utf16_offset(&cur[start..], count);
    let mut merged = Vec::with_capacity(cur.len() + insert.len());
    merged.extend_from_slice(&cur[..start]);
    merged.extend_from_slice(insert);
    merged.extend_from_slice(&cur[end..]);
    ffi::replace_text(node, &merged);
}

fn record(scope: &Scope<'_>, node: Element, old: &[u8]) {
    let js = ffi::js_of(scope);
    ffi::mark_mutated(js);
    ffi::record_character_data(js, node, old);
}

pub(crate) fn substring_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(scope.string(""));
    };
    arity(scope, args, 2)?;
    let offset = uint_arg(scope, &args[0])?;
    let count = uint_arg(scope, &args[1])?;
    let cur = current(node);
    let total = utf16_length(cur);
    check_offset(scope, offset, total)?;
    let end_units = (offset + count).min(total);
    let start = utf16_offset(cur, offset);
    let end = utf16_offset(cur, end_units);
    Ok(scope.string_from_bytes(&cur[start..end.max(start)]))
}

pub(crate) fn append_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    arity(scope, args, 1)?;
    let data = string_arg(scope, &args[0])?;
    let old = current(node).to_vec();
    let mut merged = old.clone();
    merged.extend_from_slice(&data);
    ffi::replace_text(node, &merged);
    record(scope, node, &old);
    Ok(Value::undefined())
}

pub(crate) fn delete_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    arity(scope, args, 2)?;
    let offset = uint_arg(scope, &args[0])?;
    let count = uint_arg(scope, &args[1])?;
    let total = utf16_length(current(node));
    check_offset(scope, offset, total)?;
    let old = current(node).to_vec();
    splice(node, offset, clamp_count(offset, count, total), &[]);
    record(scope, node, &old);
    Ok(Value::undefined())
}

pub(crate) fn insert_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    arity(scope, args, 2)?;
    let offset = uint_arg(scope, &args[0])?;
    let data = string_arg(scope, &args[1])?;
    let total = utf16_length(current(node));
    check_offset(scope, offset, total)?;
    let old = current(node).to_vec();
    splice(node, offset, 0, &data);
    record(scope, node, &old);
    Ok(Value::undefined())
}

pub(crate) fn replace_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    arity(scope, args, 3)?;
    let offset = uint_arg(scope, &args[0])?;
    let count = uint_arg(scope, &args[1])?;
    let data = string_arg(scope, &args[2])?;
    let total = utf16_length(current(node));
    check_offset(scope, offset, total)?;
    let old = current(node).to_vec();
    splice(node, offset, clamp_count(offset, count, total), &data);
    record(scope, node, &old);
    Ok(Value::undefined())
}

pub(crate) fn split_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this).filter(|n| n.is_text() && n.text().is_some()) else {
        return Ok(Value::null());
    };
    arity(scope, args, 1)?;
    let offset = uint_arg(scope, &args[0])?;
    let cur = current(node);
    check_offset(scope, offset, utf16_length(cur))?;
    let split = utf16_offset(cur, offset);
    let tail = ffi::new_text(&cur[split..]);
    let head = cur[..split].to_vec();
    ffi::replace_text(node, &head);
    let js = ffi::js_of(scope);
    match node.parent() {
        Some(parent) => {
            match node.next_sibling() {
                Some(next) => ffi::insert_sibling_before(next, tail),
                None => parent.append(tail),
            }
            ffi::record_child_change(
                js,
                parent,
                Some(tail),
                None,
                tail.prev_sibling(),
                tail.next_sibling(),
            );
        }
        None if !js.is_null() => ffi::orphan_node(js, tail),
        None => {}
    }
    ffi::mark_mutated(js);
    Ok(ffi::wrap_node(scope, tail))
}

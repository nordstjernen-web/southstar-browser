//! Southstar — btoa and atob, and the base64 coding they and FileReader share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const INVALID_CHARACTER_ERR: i32 = 5;

pub(crate) fn encode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[usize::from(b0 >> 2)]);
        out.push(ALPHABET[usize::from(((b0 & 0x03) << 4) | (b1 >> 4))]);
        out.push(if chunk.len() > 1 {
            ALPHABET[usize::from(((b1 & 0x0f) << 2) | (b2 >> 6))]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[usize::from(b2 & 0x3f)]
        } else {
            b'='
        });
    }
    out
}

fn sextet(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_clean(clean: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(clean.len() / 4 * 3 + 3);
    for chunk in clean.chunks(4) {
        let values: Vec<u32> = chunk.iter().filter_map(|&c| sextet(c)).collect();
        let mut group = 0u32;
        for (i, v) in values.iter().enumerate() {
            group |= v << (18 - 6 * i as u32);
        }
        let produced = match values.len() {
            4 => 3,
            3 => 2,
            2 => 1,
            _ => 0,
        };
        let group_bytes = [(group >> 16) as u8, (group >> 8) as u8, group as u8];
        out.extend_from_slice(&group_bytes[..produced]);
    }
    out
}

fn invalid(scope: &mut Scope<'_>, message: &core::ffi::CStr) -> Value {
    ffi::dom_exception(
        scope,
        c"InvalidCharacterError",
        INVALID_CHARACTER_ERR,
        message,
    )
}

pub(crate) fn btoa(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(input) = args.first() else {
        return Ok(scope.string(""));
    };
    let bytes = scope.to_bytes(input)?;
    let latin1 = match String::from_utf8(bytes) {
        Ok(text) => text
            .chars()
            .map(|c| u8::try_from(u32::from(c)).ok())
            .collect::<Option<Vec<u8>>>(),
        Err(_) => None,
    };
    let Some(latin1) = latin1 else {
        return Err(invalid(scope, c"String contains an invalid character"));
    };
    Ok(scope.string_from_bytes(&encode(&latin1)))
}

pub(crate) fn atob(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(input) = args.first() else {
        return Ok(scope.string(""));
    };
    let bytes = scope.to_bytes(input)?;
    let mut clean: Vec<u8> = bytes
        .into_iter()
        .filter(|c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
        .collect();
    if clean.len().is_multiple_of(4) {
        if clean.ends_with(b"==") {
            clean.truncate(clean.len() - 2);
        } else if clean.ends_with(b"=") {
            clean.truncate(clean.len() - 1);
        }
    }
    if clean.len() % 4 == 1 || clean.iter().any(|&c| sextet(c).is_none()) {
        return Err(invalid(
            scope,
            c"The string to be decoded is not correctly encoded",
        ));
    }
    let decoded = decode_clean(&clean);
    Ok(crate::latin1_string(scope, &decoded))
}

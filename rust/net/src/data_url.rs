//! Southstar — data: URL decoding (percent-encoded and base64 payloads) into a response body under a memory budget.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::budget::{Budgeted, Sink};

const PERCENT_CHUNK: usize = 8192;

pub enum Failure {
    Malformed,
    TooLarge,
}

pub fn is_ascii_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|v| v as u8)
}

fn base64_value(c: u8) -> i32 {
    match c {
        b'A'..=b'Z' => i32::from(c - b'A'),
        b'a'..=b'z' => i32::from(c - b'a') + 26,
        b'0'..=b'9' => i32::from(c - b'0') + 52,
        b'+' => 62,
        b'/' => 63,
        _ => -1,
    }
}

fn percent_byte(data: &[u8], i: usize) -> Option<u8> {
    let hi = hex(*data.get(i + 1)?)?;
    let lo = hex(*data.get(i + 2)?)?;
    Some((hi << 4) | lo)
}

fn decode_base64<S: Sink>(data: &[u8], out: &mut Budgeted<'_, S>) -> Result<(), Failure> {
    let mut q = [0i32; 4];
    let mut qn = 0;
    let mut ended = false;
    let mut i = 0;
    while i < data.len() {
        let mut c = data[i];
        if c == b'%'
            && let Some(b) = percent_byte(data, i)
        {
            c = b;
            i += 2;
        }
        i += 1;
        if is_ascii_space(c) {
            continue;
        }
        if ended {
            return Err(Failure::Malformed);
        }
        if c == b'=' {
            q[qn] = -2;
        } else {
            let v = base64_value(c);
            if v < 0 {
                return Err(Failure::Malformed);
            }
            q[qn] = v;
        }
        qn += 1;
        if qn < 4 {
            continue;
        }
        if q[0] < 0 || q[1] < 0 {
            return Err(Failure::Malformed);
        }
        let mut chunk = [((q[0] << 2) | (q[1] >> 4)) as u8, 0, 0];
        let mut n = 1;
        if q[2] == -2 {
            if q[3] != -2 {
                return Err(Failure::Malformed);
            }
            ended = true;
        } else {
            chunk[1] = (((q[1] & 15) << 4) | (q[2] >> 2)) as u8;
            n = 2;
            if q[3] == -2 {
                ended = true;
            } else {
                if q[3] < 0 {
                    return Err(Failure::Malformed);
                }
                chunk[2] = (((q[2] & 3) << 6) | q[3]) as u8;
                n = 3;
            }
        }
        if !out.append(&chunk[..n]) {
            return Err(Failure::TooLarge);
        }
        qn = 0;
    }
    if qn == 0 {
        return Ok(());
    }
    if qn == 1 || q[0] < 0 || q[1] < 0 {
        return Err(Failure::Malformed);
    }
    let mut chunk = [((q[0] << 2) | (q[1] >> 4)) as u8, 0];
    let mut n = 1;
    if qn == 3 {
        if q[2] < 0 {
            return Err(Failure::Malformed);
        }
        chunk[1] = (((q[1] & 15) << 4) | (q[2] >> 2)) as u8;
        n = 2;
    }
    if !out.append(&chunk[..n]) {
        return Err(Failure::TooLarge);
    }
    Ok(())
}

fn decode_percent<S: Sink>(data: &[u8], out: &mut Budgeted<'_, S>) -> Result<(), Failure> {
    let mut buf = Vec::with_capacity(PERCENT_CHUNK);
    let mut i = 0;
    while i < data.len() {
        let b = if data[i] == b'%' {
            let Some(b) = percent_byte(data, i) else {
                return Err(Failure::Malformed);
            };
            i += 2;
            b
        } else {
            data[i]
        };
        i += 1;
        buf.push(b);
        if buf.len() == PERCENT_CHUNK {
            if !out.append(&buf) {
                return Err(Failure::TooLarge);
            }
            buf.clear();
        }
    }
    if !out.append(&buf) {
        return Err(Failure::TooLarge);
    }
    Ok(())
}

pub struct Decoded {
    pub content_type: Vec<u8>,
    pub result: Result<(), Failure>,
}

pub fn decode<S: Sink>(url: &[u8], out: &mut Budgeted<'_, S>) -> Option<Decoded> {
    let rest = url.strip_prefix(b"data:")?;
    let comma = rest.iter().position(|&c| c == b',')?;
    let mut meta = &rest[..comma];
    while meta.last().copied().is_some_and(is_ascii_space) {
        meta = &meta[..meta.len() - 1];
    }
    let base64 = meta.len() >= 7 && meta[meta.len() - 7..].eq_ignore_ascii_case(b";base64");
    if base64 {
        meta = &meta[..meta.len() - 7];
    }
    let content_type = if meta.is_empty() {
        b"text/plain;charset=UTF-8".to_vec()
    } else {
        meta.to_vec()
    };
    let data = &rest[comma + 1..];
    let result = if base64 {
        decode_base64(data, out)
    } else {
        decode_percent(data, out)
    };
    Some(Decoded {
        content_type,
        result,
    })
}

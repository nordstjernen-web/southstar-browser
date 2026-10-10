//! Southstar — per-process thread dumps: the thread list with CPU times, and a SIGQUIT trigger that prints one to stderr.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::io::Write;

pub struct ThreadStat {
    pub state: u8,
    pub cpu: f64,
    pub comm: Vec<u8>,
}

fn atol(text: &[u8]) -> i64 {
    let text = text.trim_ascii_start();
    let (negative, digits) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0i64, |acc, &byte| {
            acc.wrapping_mul(10).wrapping_add(i64::from(byte - b'0'))
        });
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

pub fn parse_stat(line: &[u8], tick: f64) -> ThreadStat {
    let mut stat = ThreadStat {
        state: b'?',
        cpu: -1.0,
        comm: Vec::new(),
    };
    let open = line.iter().position(|&byte| byte == b'(');
    let close = line.iter().rposition(|&byte| byte == b')');
    if let (Some(open), Some(close)) = (open, close)
        && close > open + 1
    {
        let comm = &line[open + 1..close];
        stat.comm = comm[..comm.len().min(63)].to_vec();
    }
    let Some(close) = close else {
        return stat;
    };
    let rest = &line[close + 1..];
    if rest.len() < 2 {
        return stat;
    }
    stat.state = rest[1];
    let mut utime = 0;
    let mut stime = 0;
    for (index, token) in rest[1..]
        .split(|&byte| byte == b' ')
        .filter(|token| !token.is_empty())
        .enumerate()
    {
        if index == 11 {
            utime = atol(token);
        } else if index == 12 {
            stime = atol(token);
            break;
        }
    }
    stat.cpu = (utime + stime) as f64 / tick;
    stat
}

pub fn first_line(bytes: &[u8]) -> &[u8] {
    let limited = &bytes[..bytes.len().min(1023)];
    match limited.iter().position(|&byte| byte == b'\n') {
        Some(end) => &limited[..=end],
        None => limited,
    }
}

pub fn dump_text(pid: i32, label: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"===== thread dump: ");
    out.extend_from_slice(label.unwrap_or(b"process"));
    let _ = writeln!(out, " (pid {pid}) =====");
    ffi::list_threads(pid, &mut out);
    out
}

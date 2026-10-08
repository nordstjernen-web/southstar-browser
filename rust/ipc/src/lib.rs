//! Southstar — the renderer protocol's framing: HTTP/1.1 heads and bodies, the X-* reply headers and the flat JSON bodies.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::{c_int, c_long};
use std::io;

pub const MAX_BODY: c_long = 64 * 1024 * 1024;
pub const MAX_HEADERS: usize = 64;
const LINE_CAP: usize = 20480;
const REQUEST_HEAD_CAP: usize = 512;

#[repr(C)]
pub struct Conn {
    pub fd: c_int,
    pub buf: [u8; 16384],
    pub start: usize,
    pub len: usize,
}

#[repr(C)]
pub struct Head {
    pub method: [u8; 8],
    pub path: [u8; 128],
    pub status: c_int,
    pub content_length: c_long,
    pub x_w: c_long,
    pub x_h: c_long,
    pub x_stride: c_long,
    pub x_anim: c_long,
    pub x_unchanged: c_long,
    pub x_render_rc: c_long,
    pub x_page_w: c_long,
    pub x_page_h: c_long,
    pub x_scroll_y: c_long,
    pub x_scroll_x: c_long,
    pub x_clipboard: c_long,
    pub x_tiles: c_long,
    pub x_nav: [u8; 2048],
    pub x_webgl: [u8; 2048],
    pub x_camera: [u8; 2048],
    pub x_download: [u8; 3072],
    pub x_audio: [u8; 16384],
    pub x_window_action: [u8; 32],
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

fn copy_truncated(dst: &mut [u8], src: &[u8]) {
    let n = src.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&src[..n]);
    dst[n] = 0;
}

fn c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

pub fn atol(s: &[u8]) -> c_long {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut i = 0;
    while c_space(at(i)) {
        i += 1;
    }
    let negative = at(i) == b'-';
    if matches!(at(i), b'-' | b'+') {
        i += 1;
    }
    let mut magnitude: i128 = 0;
    while at(i).is_ascii_digit() {
        magnitude = (magnitude * 10 + i128::from(at(i) - b'0')).min(1 << 80);
        i += 1;
    }
    let value = if negative { -magnitude } else { magnitude };
    value.clamp(i128::from(c_long::MIN), i128::from(c_long::MAX)) as c_long
}

pub fn atoi(s: &[u8]) -> c_int {
    atol(s) as c_int
}

impl Head {
    pub fn clear(&mut self) {
        self.method.fill(0);
        self.path.fill(0);
        self.status = 0;
        self.content_length = 0;
        self.x_w = -1;
        self.x_h = -1;
        self.x_stride = -1;
        self.x_anim = -1;
        self.x_unchanged = 0;
        self.x_render_rc = 0;
        self.x_page_w = -1;
        self.x_page_h = -1;
        self.x_scroll_y = -1;
        self.x_scroll_x = -1;
        self.x_clipboard = 0;
        self.x_tiles = 0;
        self.x_nav.fill(0);
        self.x_webgl.fill(0);
        self.x_camera.fill(0);
        self.x_download.fill(0);
        self.x_audio.fill(0);
        self.x_window_action.fill(0);
    }

    fn start_line(&mut self, line: &[u8]) -> bool {
        let space = line.iter().position(|&b| b == b' ');
        if line.starts_with(b"HTTP/") {
            self.status = space.map_or(0, |sp| atoi(&line[sp + 1..]));
            return true;
        }
        let Some(sp) = space else {
            return false;
        };
        copy_truncated(&mut self.method, &line[..sp]);
        let target = &line[sp + 1..];
        let end = target.iter().position(|&b| b == b' ');
        copy_truncated(&mut self.path, &target[..end.unwrap_or(target.len())]);
        true
    }

    fn header(&mut self, name: &[u8], value: &[u8]) {
        let is = |known: &str| name.eq_ignore_ascii_case(known.as_bytes());
        let number = if is("Content-Length") {
            Some(&mut self.content_length)
        } else if is("X-W") {
            Some(&mut self.x_w)
        } else if is("X-H") {
            Some(&mut self.x_h)
        } else if is("X-Stride") {
            Some(&mut self.x_stride)
        } else if is("X-Anim") {
            Some(&mut self.x_anim)
        } else if is("X-PageW") {
            Some(&mut self.x_page_w)
        } else if is("X-PageH") {
            Some(&mut self.x_page_h)
        } else if is("X-ScrollY") {
            Some(&mut self.x_scroll_y)
        } else if is("X-ScrollX") {
            Some(&mut self.x_scroll_x)
        } else if is("X-Unchanged") {
            Some(&mut self.x_unchanged)
        } else if is("X-Render-RC") {
            Some(&mut self.x_render_rc)
        } else if is("X-Clipboard") {
            Some(&mut self.x_clipboard)
        } else if is("X-Tiles") {
            Some(&mut self.x_tiles)
        } else {
            None
        };
        if let Some(slot) = number {
            *slot = atol(value);
            return;
        }
        let text: Option<&mut [u8]> = if is("X-Nav") {
            Some(&mut self.x_nav)
        } else if is("X-WebGL") {
            Some(&mut self.x_webgl)
        } else if is("X-Camera") {
            Some(&mut self.x_camera)
        } else if is("X-Download") {
            Some(&mut self.x_download)
        } else if is("X-Audio") {
            Some(&mut self.x_audio)
        } else if is("X-Window-Action") {
            Some(&mut self.x_window_action)
        } else {
            None
        };
        if let Some(slot) = text {
            copy_truncated(slot, value);
        }
    }
}

impl Conn {
    fn fill(&mut self) -> bool {
        if self.len > 0 {
            return true;
        }
        self.start = 0;
        loop {
            match ffi::read(self.fd, &mut self.buf) {
                Ok(0) => return false,
                Ok(n) => {
                    self.len = n;
                    return true;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return false,
            }
        }
    }

    fn take(&mut self, n: usize) -> &[u8] {
        let taken = &self.buf[self.start..self.start + n];
        self.start += n;
        self.len -= n;
        taken
    }

    fn read_line(&mut self, out: &mut Vec<u8>) -> Option<usize> {
        out.clear();
        loop {
            if self.len == 0 && !self.fill() {
                return None;
            }
            let ch = self.take(1)[0];
            if ch == b'\n' {
                while out.last() == Some(&b'\r') {
                    out.pop();
                }
                return Some(out.len());
            }
            if out.len() + 1 < LINE_CAP {
                out.push(ch);
            }
        }
    }

    pub fn read_body(&mut self, dst: &mut [u8]) -> bool {
        let mut got = 0;
        while got < dst.len() {
            if self.len == 0 && !self.fill() {
                return false;
            }
            let n = self.len.min(dst.len() - got);
            dst[got..got + n].copy_from_slice(self.take(n));
            got += n;
        }
        true
    }

    pub fn skip_body(&mut self, total: u64) -> bool {
        let mut got = 0u64;
        while got < total {
            if self.len == 0 && !self.fill() {
                return false;
            }
            let n = self
                .len
                .min(usize::try_from(total - got).unwrap_or(usize::MAX));
            self.take(n);
            got += n as u64;
        }
        true
    }

    pub fn read_head(&mut self, out: &mut Head) -> bool {
        out.clear();
        let mut line = Vec::with_capacity(256);
        match self.read_line(&mut line) {
            Some(n) if n > 0 => {}
            _ => return false,
        }
        if !out.start_line(until_nul(&line)) {
            return false;
        }
        let mut headers = 0;
        loop {
            match self.read_line(&mut line) {
                None => return false,
                Some(0) => break,
                Some(_) => {}
            }
            headers += 1;
            if headers > MAX_HEADERS {
                return false;
            }
            let text = until_nul(&line);
            let Some(colon) = text.iter().position(|&b| b == b':') else {
                continue;
            };
            let value = &text[colon + 1..];
            let skip = value.iter().take_while(|&&b| b == b' ').count();
            out.header(&text[..colon], &value[skip..]);
        }
        (0..=MAX_BODY).contains(&out.content_length)
    }
}

pub fn write_all(fd: c_int, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        match ffi::write(fd, bytes) {
            Ok(n) if n > 0 => bytes = &bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            _ => return false,
        }
    }
    true
}

pub fn request_head(
    method: &[u8],
    path: &[u8],
    content_type: &[u8],
    length: usize,
) -> Option<Vec<u8>> {
    let mut head = Vec::with_capacity(REQUEST_HEAD_CAP);
    head.extend_from_slice(method);
    head.push(b' ');
    head.extend_from_slice(path);
    head.extend_from_slice(b" HTTP/1.1\r\nContent-Type: ");
    head.extend_from_slice(content_type);
    head.extend_from_slice(format!("\r\nContent-Length: {length}\r\n\r\n").as_bytes());
    (head.len() < REQUEST_HEAD_CAP).then_some(head)
}

pub fn response_head(
    status: c_int,
    content_type: &[u8],
    extra_headers: &[u8],
    length: usize,
) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {status} OK\r\nContent-Type: ").into_bytes();
    head.extend_from_slice(content_type);
    head.extend_from_slice(b"\r\n");
    head.extend_from_slice(extra_headers);
    head.extend_from_slice(format!("Content-Length: {length}\r\n\r\n").as_bytes());
    head
}

pub fn write_message(fd: c_int, head: &[u8], body: &[u8]) -> bool {
    write_all(fd, head) && (body.is_empty() || write_all(fd, body))
}

pub fn json_escape(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 2);
    for &ch in s {
        match ch {
            b'"' | b'\\' => out.extend_from_slice(&[b'\\', ch]),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0..=0x1f => out.extend_from_slice(format!("\\u{ch:04x}").as_bytes()),
            _ => out.push(ch),
        }
    }
    out
}

pub fn json_value<'a>(body: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut from = 0;
    while let Some(offset) = body[from..].iter().position(|&b| b == b'"') {
        let quote = from + offset;
        let name_end = quote + 1 + key.len();
        if body.get(quote + 1..name_end) == Some(key) && body.get(name_end) == Some(&b'"') {
            let mut value = name_end + 1;
            while matches!(body.get(value), Some(b' ' | b':')) {
                value += 1;
            }
            return Some(&body[value..]);
        }
        from = quote + 1;
    }
    None
}

pub fn json_double(v: &[u8]) -> f64 {
    let at = |i: usize| v.get(i).copied().unwrap_or(0);
    let mut i = 0;
    let mut sign = 1.0;
    if at(0) == b'-' {
        sign = -1.0;
        i = 1;
    } else if at(0) == b'+' {
        i = 1;
    }
    let mut val = 0.0f64;
    while at(i).is_ascii_digit() {
        val = val * 10.0 + f64::from(at(i) - b'0');
        i += 1;
    }
    if at(i) == b'.' {
        i += 1;
        let mut frac = 0.1;
        while at(i).is_ascii_digit() {
            val += f64::from(at(i) - b'0') * frac;
            frac *= 0.1;
            i += 1;
        }
    }
    if matches!(at(i), b'e' | b'E') {
        i += 1;
        let mut negative_exponent = false;
        if at(i) == b'-' {
            negative_exponent = true;
            i += 1;
        } else if at(i) == b'+' {
            i += 1;
        }
        let mut e = 0;
        while at(i).is_ascii_digit() {
            if e < 1000 {
                e = e * 10 + i32::from(at(i) - b'0');
            }
            i += 1;
        }
        let mut p = 1.0f64;
        for _ in 0..e.min(308) {
            p *= 10.0;
        }
        val = if negative_exponent { val / p } else { val * p };
    }
    sign * val
}

fn hex4(v: &[u8], from: usize) -> Option<u32> {
    let mut value = 0;
    for i in from..from + 4 {
        let digit = char::from(v.get(i).copied().unwrap_or(0)).to_digit(16)?;
        value = (value << 4) | digit;
    }
    Some(value)
}

fn utf8_encode(cp: u32, out: &mut Vec<u8>) {
    if cp < 0x80 {
        out.push(cp as u8);
    } else if cp < 0x800 {
        out.extend_from_slice(&[0xC0 | (cp >> 6) as u8, 0x80 | (cp & 0x3F) as u8]);
    } else if cp < 0x10000 {
        out.extend_from_slice(&[
            0xE0 | (cp >> 12) as u8,
            0x80 | ((cp >> 6) & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
        ]);
    } else {
        out.extend_from_slice(&[
            0xF0 | (cp >> 18) as u8,
            0x80 | ((cp >> 12) & 0x3F) as u8,
            0x80 | ((cp >> 6) & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
        ]);
    }
}

pub fn json_string(v: &[u8]) -> Option<Vec<u8>> {
    if v.first() != Some(&b'"') {
        return None;
    }
    let at = |i: usize| v.get(i).copied().unwrap_or(0);
    let mut out = Vec::with_capacity(v.len());
    let mut i = 1;
    while at(i) != 0 && at(i) != b'"' {
        if at(i) != b'\\' || at(i + 1) == 0 {
            out.push(at(i));
            i += 1;
            continue;
        }
        i += 1;
        match at(i) {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'u' => {
                if let Some(high) = hex4(v, i + 1) {
                    i += 4;
                    let mut cp = high;
                    if (0xD800..=0xDBFF).contains(&cp) && at(i + 1) == b'\\' && at(i + 2) == b'u' {
                        if let Some(low) =
                            hex4(v, i + 3).filter(|low| (0xDC00..=0xDFFF).contains(low))
                        {
                            cp = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                            i += 6;
                        }
                    }
                    utf8_encode(cp, &mut out);
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    Some(out)
}

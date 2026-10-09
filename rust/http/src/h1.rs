//! Southstar — HTTP/1.1 messages: the request head, the status line, header lines and the chunked transfer coding.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub fn request_head(
    method: &[u8],
    path: &[u8],
    authority: &[u8],
    fields: &[(&[u8], &[u8])],
    extra_lines: &[&[u8]],
    body_len: usize,
    keep_alive: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    out.extend_from_slice(method);
    out.push(b' ');
    out.extend_from_slice(path);
    out.extend_from_slice(b" HTTP/1.1\r\nHost: ");
    out.extend_from_slice(authority);
    out.extend_from_slice(b"\r\n");
    for (name, value) in fields {
        out.extend_from_slice(name);
        out.extend_from_slice(b": ");
        out.extend_from_slice(value);
        out.extend_from_slice(b"\r\n");
    }
    for line in extra_lines {
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    if body_len > 0 {
        out.extend_from_slice(format!("Content-Length: {body_len}\r\n").as_bytes());
    }
    out.extend_from_slice(if keep_alive {
        b"Connection: keep-alive\r\n\r\n".as_slice()
    } else {
        b"Connection: close\r\n\r\n".as_slice()
    });
    out
}

pub fn strtoll(text: &[u8], radix: u32) -> i64 {
    let mut i = 0;
    while i < text.len() && matches!(text[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let negative = text.get(i) == Some(&b'-');
    if matches!(text.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    if radix == 16 && text.len() > i + 1 && text[i] == b'0' && matches!(text[i + 1], b'x' | b'X') {
        i += 2;
    }
    let mut value: i64 = 0;
    while let Some(d) = text.get(i).and_then(|&c| (c as char).to_digit(radix)) {
        value = value.saturating_mul(radix as i64).saturating_add(d as i64);
        i += 1;
    }
    if negative { -value } else { value }
}

pub fn status_code(line: &[u8]) -> Option<i64> {
    let space = line.iter().position(|&b| b == b' ')?;
    Some(strtoll(&line[space + 1..], 10))
}

pub fn split_header(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let colon = line.iter().position(|&b| b == b':')?;
    let mut value = &line[colon + 1..];
    while let [b' ' | b'\t', rest @ ..] = value {
        value = rest;
    }
    Some((&line[..colon], value))
}

pub fn until_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

enum ChunkState {
    Size(Vec<u8>),
    Data(u64),
    DataEnd(u8),
    Trailer(Vec<u8>),
    Done,
}

pub struct Chunked {
    state: ChunkState,
}

#[derive(Debug)]
pub struct ChunkError;

impl Chunked {
    pub fn new() -> Chunked {
        Chunked {
            state: ChunkState::Size(Vec::new()),
        }
    }

    pub fn done(&self) -> bool {
        matches!(self.state, ChunkState::Done)
    }

    pub fn feed(&mut self, mut input: &[u8], out: &mut Vec<u8>) -> Result<(), ChunkError> {
        while !input.is_empty() {
            match &mut self.state {
                ChunkState::Size(line) => {
                    let byte = input[0];
                    input = &input[1..];
                    if byte != b'\n' {
                        if line.len() > 4096 {
                            return Err(ChunkError);
                        }
                        line.push(byte);
                        continue;
                    }
                    let size = strtoll(until_nul(line), 16);
                    self.state = if size <= 0 {
                        ChunkState::Trailer(Vec::new())
                    } else {
                        ChunkState::Data(size as u64)
                    };
                }
                ChunkState::Data(remaining) => {
                    let n = (*remaining).min(input.len() as u64) as usize;
                    out.extend_from_slice(&input[..n]);
                    input = &input[n..];
                    *remaining -= n as u64;
                    if *remaining == 0 {
                        self.state = ChunkState::DataEnd(0);
                    }
                }
                ChunkState::DataEnd(seen) => {
                    let byte = input[0];
                    input = &input[1..];
                    if byte == b'\n' {
                        self.state = ChunkState::Size(Vec::new());
                    } else {
                        *seen += 1;
                        if *seen > 2 {
                            return Err(ChunkError);
                        }
                    }
                }
                ChunkState::Trailer(line) => {
                    let byte = input[0];
                    input = &input[1..];
                    if byte != b'\n' {
                        if line.len() > 8192 {
                            return Err(ChunkError);
                        }
                        line.push(byte);
                        continue;
                    }
                    if line.is_empty() || line.as_slice() == b"\r" {
                        self.state = ChunkState::Done;
                    } else {
                        line.clear();
                    }
                }
                ChunkState::Done => return Ok(()),
            }
        }
        Ok(())
    }
}

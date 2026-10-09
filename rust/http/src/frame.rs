//! Southstar — HTTP/2 frame types, flags and the 9-byte frame header (RFC 9113 §4 and §6).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
pub const HEADER_LEN: usize = 9;
pub const DEFAULT_MAX_FRAME: usize = 16_384;
pub const DEFAULT_WINDOW: i64 = 65_535;
pub const MAX_WINDOW: i64 = (1 << 31) - 1;

pub const DATA: u8 = 0;
pub const HEADERS: u8 = 1;
pub const RST_STREAM: u8 = 3;
pub const SETTINGS: u8 = 4;
pub const PUSH_PROMISE: u8 = 5;
pub const PING: u8 = 6;
pub const GOAWAY: u8 = 7;
pub const WINDOW_UPDATE: u8 = 8;
pub const CONTINUATION: u8 = 9;

pub const FLAG_END_STREAM: u8 = 0x1;
pub const FLAG_ACK: u8 = 0x1;
pub const FLAG_END_HEADERS: u8 = 0x4;
pub const FLAG_PADDED: u8 = 0x8;
pub const FLAG_PRIORITY: u8 = 0x20;

pub const SETTINGS_HEADER_TABLE_SIZE: u16 = 1;
pub const SETTINGS_ENABLE_PUSH: u16 = 2;
pub const SETTINGS_MAX_CONCURRENT_STREAMS: u16 = 3;
pub const SETTINGS_INITIAL_WINDOW_SIZE: u16 = 4;
pub const SETTINGS_MAX_FRAME_SIZE: u16 = 5;

pub const NO_ERROR: u32 = 0;
pub const PROTOCOL_ERROR: u32 = 1;
pub const INTERNAL_ERROR: u32 = 2;
pub const FLOW_CONTROL_ERROR: u32 = 3;
pub const FRAME_SIZE_ERROR: u32 = 6;
pub const REFUSED_STREAM: u32 = 7;
pub const CANCEL: u32 = 8;
pub const COMPRESSION_ERROR: u32 = 9;

pub struct Header {
    pub len: usize,
    pub kind: u8,
    pub flags: u8,
    pub stream: u32,
}

pub fn parse_header(b: &[u8]) -> Header {
    Header {
        len: ((b[0] as usize) << 16) | ((b[1] as usize) << 8) | b[2] as usize,
        kind: b[3],
        flags: b[4],
        stream: u32::from_be_bytes([b[5], b[6], b[7], b[8]]) & 0x7fff_ffff,
    }
}

pub fn write(out: &mut Vec<u8>, kind: u8, flags: u8, stream: u32, payload: &[u8]) {
    let len = payload.len();
    out.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8, kind, flags]);
    out.extend_from_slice(&(stream & 0x7fff_ffff).to_be_bytes());
    out.extend_from_slice(payload);
}

pub fn write_settings(out: &mut Vec<u8>, settings: &[(u16, u32)]) {
    let mut payload = Vec::with_capacity(settings.len() * 6);
    for &(id, value) in settings {
        payload.extend_from_slice(&id.to_be_bytes());
        payload.extend_from_slice(&value.to_be_bytes());
    }
    write(out, SETTINGS, 0, 0, &payload);
}

pub fn write_window_update(out: &mut Vec<u8>, stream: u32, increment: u32) {
    write(out, WINDOW_UPDATE, 0, stream, &increment.to_be_bytes());
}

pub fn write_rst(out: &mut Vec<u8>, stream: u32, code: u32) {
    write(out, RST_STREAM, 0, stream, &code.to_be_bytes());
}

pub fn write_goaway(out: &mut Vec<u8>, last_stream: u32, code: u32) {
    let mut payload = last_stream.to_be_bytes().to_vec();
    payload.extend_from_slice(&code.to_be_bytes());
    write(out, GOAWAY, 0, 0, &payload);
}

pub fn strip_padding(flags: u8, payload: &[u8]) -> Option<&[u8]> {
    if flags & FLAG_PADDED == 0 {
        return Some(payload);
    }
    let (&pad, rest) = payload.split_first()?;
    rest.len().checked_sub(pad as usize).map(|end| &rest[..end])
}

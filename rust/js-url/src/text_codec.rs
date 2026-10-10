//! Southstar — TextEncoder and TextDecoder over UTF-8, UTF-16LE/BE and windows-1252, with streaming and BOM handling.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::quickjs;
use southstar_js_engine::{ElementType, Scope, Value};

use crate::ffi::{self, HO_TEXT_DECODER, HO_TEXT_ENCODER};
use crate::{JsResult, arg, bind_fn, get, illegal_invocation, set, truthy};

const MAX_ARRAY_LIKE: u32 = 1 << 24;

const UTF8_LABELS: &[&str] = &[
    "unicode-1-1-utf-8",
    "unicode11utf8",
    "unicode20utf8",
    "utf-8",
    "utf8",
    "x-unicode20utf8",
];
const UTF16LE_LABELS: &[&str] = &[
    "csunicode",
    "iso-10646-ucs-2",
    "ucs-2",
    "unicode",
    "unicodefeff",
    "utf-16",
    "utf-16le",
];
const UTF16BE_LABELS: &[&str] = &["unicodefffe", "utf-16be"];
const WINDOWS1252_LABELS: &[&str] = &[
    "ansi_x3.4-1968",
    "ascii",
    "cp1252",
    "cp819",
    "csisolatin1",
    "ibm819",
    "iso-8859-1",
    "iso-ir-100",
    "iso8859-1",
    "iso88591",
    "iso_8859-1",
    "iso_8859-1:1987",
    "l1",
    "latin1",
    "us-ascii",
    "windows-1252",
    "x-cp1252",
];

const WINDOWS1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl Mode {
    fn code(self) -> i32 {
        match self {
            Mode::Utf8 => 0,
            Mode::Utf16Le => 1,
            Mode::Utf16Be => 2,
            Mode::Windows1252 => 3,
        }
    }

    fn from_code(code: i32) -> Mode {
        match code {
            1 => Mode::Utf16Le,
            2 => Mode::Utf16Be,
            3 => Mode::Windows1252,
            _ => Mode::Utf8,
        }
    }

    fn from_label(label: &str) -> Option<(Mode, &'static str)> {
        if UTF8_LABELS.contains(&label) {
            Some((Mode::Utf8, "utf-8"))
        } else if UTF16LE_LABELS.contains(&label) {
            Some((Mode::Utf16Le, "utf-16le"))
        } else if UTF16BE_LABELS.contains(&label) {
            Some((Mode::Utf16Be, "utf-16be"))
        } else if WINDOWS1252_LABELS.contains(&label) {
            Some((Mode::Windows1252, "windows-1252"))
        } else {
            None
        }
    }
}

fn replace_lone_surrogates(bytes: Vec<u8>) -> Vec<u8> {
    let found = bytes.windows(3).any(|w| w[0] == 0xED && w[1] >= 0xA0);
    if !found {
        return bytes;
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 2 < bytes.len() && bytes[i] == 0xED && (0xA0..=0xBF).contains(&bytes[i + 1]) {
            out.extend_from_slice(&[0xEF, 0xBF, 0xBD]);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn encode(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !ffi::host_is(this, HO_TEXT_ENCODER) {
        return Err(illegal_invocation(scope));
    }
    let input = arg(args, 0);
    let bytes = if input.is_undefined() {
        Vec::new()
    } else {
        replace_lone_surrogates(scope.to_bytes(&input)?)
    };
    let buffer = scope.new_array_buffer(&bytes)?;
    let global = scope.global();
    let uint8 = get(scope, &global, "Uint8Array");
    scope.construct(&uint8, &[buffer])
}

fn copy_into(src: &[u8], dst: &mut [u8]) -> (usize, usize) {
    let (mut read, mut written) = (0usize, 0usize);
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        let mut len = match c {
            0..0x80 => 1,
            0x80..0xE0 => 2,
            0xE0..0xF0 => 3,
            _ => 4,
        };
        if i + len > src.len() {
            len = src.len() - i;
        }
        if written + len > dst.len() {
            break;
        }
        dst[written..written + len].copy_from_slice(&src[i..i + len]);
        written += len;
        read += if len == 4 { 2 } else { 1 };
        i += len;
    }
    (read, written)
}

enum Target {
    Bytes(usize, usize),
    Empty,
    NotUint8,
}

fn encode_into_target(scope: &mut Scope<'_>, view: &Value) -> (Target, Option<Value>) {
    if scope.typed_array_element(view) != Some(ElementType::Uint8) {
        return (Target::NotUint8, None);
    }
    match quickjs::typed_array_parts(scope, view) {
        Ok(parts) => (
            Target::Bytes(parts.byte_offset, parts.byte_length),
            Some(parts.buffer),
        ),
        Err(_) => (Target::Empty, None),
    }
}

fn not_uint8(scope: &mut Scope<'_>) -> Value {
    scope.type_error(
        "Failed to execute 'encodeInto' on 'TextEncoder': parameter 2 is not of type 'Uint8Array'.",
    )
}

fn encode_into(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !ffi::host_is(this, HO_TEXT_ENCODER) {
        return Err(illegal_invocation(scope));
    }
    if args.len() < 2 {
        let message = format!(
            "Failed to execute 'encodeInto' on 'TextEncoder': 2 arguments required, but only {} present.",
            args.len()
        );
        return Err(scope.type_error(&message));
    }
    let src = replace_lone_surrogates(scope.to_bytes(&args[0])?);
    let (read, written) = match encode_into_target(scope, &args[1]) {
        (Target::NotUint8, _) => return Err(not_uint8(scope)),
        (Target::Empty, _) => (0, 0),
        (Target::Bytes(offset, length), Some(buffer)) => {
            let copied = scope.with_buffer_bytes_mut(&buffer, |bytes| {
                offset
                    .checked_add(length)
                    .filter(|&end| end <= bytes.len())
                    .map(|end| copy_into(&src, &mut bytes[offset..end]))
            });
            match copied.flatten() {
                Some(counts) => counts,
                None => return Err(not_uint8(scope)),
            }
        }
        (Target::Bytes(..), None) => return Err(not_uint8(scope)),
    };
    let result = scope.new_object();
    set(scope, &result, "read", Value::int64(read as i64));
    set(scope, &result, "written", Value::int64(written as i64));
    Ok(result)
}

pub(crate) fn encoder_ctor(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let object = ffi::host_construct(scope, this, HO_TEXT_ENCODER)?;
    crate::set_str(scope, &object, "encoding", b"utf-8");
    Ok(object)
}

fn label_of(scope: &mut Scope<'_>, value: &Value) -> JsResult<Option<String>> {
    if value.is_undefined() {
        return Ok(None);
    }
    let text = scope.to_string(value)?;
    let label = text
        .to_ascii_lowercase()
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\x0b')
        .to_owned();
    Ok(Some(label))
}

pub(crate) fn decoder_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let object = ffi::host_construct(scope, this, HO_TEXT_DECODER)?;
    let (mode, encoding) = match label_of(scope, &arg(args, 0))? {
        None => (Mode::Utf8, "utf-8"),
        Some(label) => match Mode::from_label(&label) {
            Some(found) => found,
            None => {
                return Err(scope.range_error("TextDecoder: the encoding label is not supported"));
            }
        },
    };
    let options = arg(args, 1);
    let (fatal, ignore_bom) = if options.is_object() {
        (
            truthy(scope, &options, "fatal"),
            truthy(scope, &options, "ignoreBOM"),
        )
    } else {
        (false, false)
    };
    crate::set_str(scope, &object, "encoding", encoding.as_bytes());
    set(scope, &object, "_mode", Value::int(mode.code()));
    set(scope, &object, "fatal", Value::boolean(fatal));
    set(scope, &object, "ignoreBOM", Value::boolean(ignore_bom));
    Ok(object)
}

struct Decoded {
    text: String,
    pending: usize,
}

fn decode_utf8(data: &[u8], fatal: bool, stream: bool) -> Option<Decoded> {
    let mut out = String::with_capacity(data.len());
    let (mut codep, mut needed, mut seen) = (0u32, 0u32, 0u32);
    let (mut lower, mut upper) = (0x80u8, 0xBFu8);
    let mut seq_start = 0usize;
    let mut i = 0usize;
    let push = |out: &mut String, cp: u32| out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
    while i < data.len() {
        let b = data[i];
        if needed == 0 {
            match b {
                0x00..=0x7F => {
                    push(&mut out, u32::from(b));
                    i += 1;
                }
                0xC2..=0xDF => {
                    seq_start = i;
                    needed = 1;
                    codep = u32::from(b & 0x1F);
                    i += 1;
                }
                0xE0..=0xEF => {
                    seq_start = i;
                    if b == 0xE0 {
                        lower = 0xA0;
                    }
                    if b == 0xED {
                        upper = 0x9F;
                    }
                    needed = 2;
                    codep = u32::from(b & 0x0F);
                    i += 1;
                }
                0xF0..=0xF4 => {
                    seq_start = i;
                    if b == 0xF0 {
                        lower = 0x90;
                    }
                    if b == 0xF4 {
                        upper = 0x8F;
                    }
                    needed = 3;
                    codep = u32::from(b & 0x07);
                    i += 1;
                }
                _ => {
                    if fatal {
                        return None;
                    }
                    out.push('\u{FFFD}');
                    i += 1;
                }
            }
        } else if b < lower || b > upper {
            codep = 0;
            needed = 0;
            seen = 0;
            lower = 0x80;
            upper = 0xBF;
            if fatal {
                return None;
            }
            out.push('\u{FFFD}');
        } else {
            lower = 0x80;
            upper = 0xBF;
            codep = (codep << 6) | u32::from(b & 0x3F);
            seen += 1;
            i += 1;
            if seen == needed {
                push(&mut out, codep);
                codep = 0;
                needed = 0;
                seen = 0;
            }
        }
    }
    let mut pending = 0;
    if needed != 0 {
        if stream {
            pending = data.len() - seq_start;
        } else {
            if fatal {
                return None;
            }
            out.push('\u{FFFD}');
        }
    }
    Some(Decoded { text: out, pending })
}

fn decode_utf16(data: &[u8], fatal: bool, big_endian: bool, stream: bool) -> Option<Decoded> {
    let mut out = String::with_capacity(data.len());
    let mut high: Option<u32> = None;
    let mut i = 0usize;
    while i + 2 <= data.len() {
        let unit = if big_endian {
            u16::from_be_bytes([data[i], data[i + 1]])
        } else {
            u16::from_le_bytes([data[i], data[i + 1]])
        };
        let unit = u32::from(unit);
        i += 2;
        if let Some(hi) = high.take() {
            if (0xDC00..=0xDFFF).contains(&unit) {
                let cp = 0x10000 + ((hi - 0xD800) << 10) + (unit - 0xDC00);
                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                continue;
            }
            if fatal {
                return None;
            }
            out.push('\u{FFFD}');
        }
        if (0xD800..=0xDBFF).contains(&unit) {
            high = Some(unit);
        } else if (0xDC00..=0xDFFF).contains(&unit) {
            if fatal {
                return None;
            }
            out.push('\u{FFFD}');
        } else {
            out.push(char::from_u32(unit).unwrap_or('\u{FFFD}'));
        }
    }
    let tail = data.len() - i;
    if stream {
        let pending = tail + if high.is_some() { 2 } else { 0 };
        return Some(Decoded { text: out, pending });
    }
    if high.is_some() {
        if fatal {
            return None;
        }
        out.push('\u{FFFD}');
    }
    if tail != 0 {
        if fatal {
            return None;
        }
        out.push('\u{FFFD}');
    }
    Some(Decoded {
        text: out,
        pending: 0,
    })
}

fn decode_windows1252(data: &[u8]) -> Decoded {
    let text = data
        .iter()
        .map(|&b| match b {
            0x80..0xA0 => char::from_u32(u32::from(WINDOWS1252_HIGH[usize::from(b - 0x80)]))
                .unwrap_or('\u{FFFD}'),
            _ => char::from(b),
        })
        .collect();
    Decoded { text, pending: 0 }
}

fn input_bytes(scope: &mut Scope<'_>, input: &Value) -> Vec<u8> {
    if let Some(bytes) = ffi::bytes_view(scope, input) {
        return bytes;
    }
    let length = get(scope, input, "length");
    let length = scope
        .to_number(&length)
        .ok()
        .map_or(0, |n| if n.is_finite() { n as i64 as u32 } else { 0 })
        .min(MAX_ARRAY_LIKE);
    (0..length)
        .map(|i| {
            let value = scope
                .get_index(input, i)
                .unwrap_or_else(|_| Value::undefined());
            scope.to_int32(&value).unwrap_or(0) as u8
        })
        .collect()
}

fn decode(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if !ffi::host_is(this, HO_TEXT_DECODER) {
        return Err(illegal_invocation(scope));
    }
    let options = arg(args, 1);
    let stream = options.is_object() && truthy(scope, &options, "stream");
    let mode_value = get(scope, this, "_mode");
    let mode = if mode_value.is_number() {
        Mode::from_code(scope.to_int32(&mode_value).unwrap_or(0))
    } else {
        Mode::Utf8
    };
    let fatal = truthy(scope, this, "fatal");
    let mut buf = Vec::new();
    let tail = get(scope, this, "_tail");
    if tail.is_object()
        && let Some(bytes) = scope.array_buffer_bytes(&tail)
    {
        buf.extend_from_slice(&bytes);
    }
    let input = arg(args, 0);
    if !input.is_undefined() {
        buf.extend(input_bytes(scope, &input));
    }
    let decoded = match mode {
        Mode::Utf16Le => decode_utf16(&buf, fatal, false, stream),
        Mode::Utf16Be => decode_utf16(&buf, fatal, true, stream),
        Mode::Windows1252 => Some(decode_windows1252(&buf)),
        Mode::Utf8 => decode_utf8(&buf, fatal, stream),
    };
    let Some(Decoded { mut text, pending }) = decoded else {
        if stream {
            set(scope, this, "_tail", Value::undefined());
        }
        return Err(scope.type_error("The encoded data was not valid"));
    };
    if !truthy(scope, this, "_bomChecked") && !text.is_empty() {
        if !truthy(scope, this, "ignoreBOM") && text.starts_with('\u{FEFF}') {
            text.remove(0);
        }
        set(scope, this, "_bomChecked", Value::boolean(true));
    }
    let result = scope.string(&text);
    let n = buf.len();
    let held = if stream && pending > 0 && pending <= n {
        scope.new_array_buffer(&buf[n - pending..])?
    } else {
        Value::undefined()
    };
    set(scope, this, "_tail", held);
    if !stream {
        set(scope, this, "_bomChecked", Value::boolean(false));
    }
    Ok(result)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let encoder = crate::proto_of(scope, global, "TextEncoder");
    if encoder.is_object() {
        bind_fn(scope, &encoder, "encode", 0, encode);
        bind_fn(scope, &encoder, "encodeInto", 2, encode_into);
    }
    let decoder = crate::proto_of(scope, global, "TextDecoder");
    if decoder.is_object() {
        bind_fn(scope, &decoder, "decode", 0, decode);
    }
}

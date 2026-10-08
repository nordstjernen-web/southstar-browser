//! Southstar — HTML helper utilities: escaping, number and charset parsing, body decoding, and the image, JSON and XML viewer pages.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod tables;

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

fn html_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

pub fn parse_float(s: &[u8]) -> Option<f64> {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut p = s.iter().take_while(|&&c| html_space(c)).count();
    let mut num = Vec::new();
    if at(p) == b'-' {
        num.push(b'-');
        p += 1;
    } else if at(p) == b'+' {
        p += 1;
    }
    if !(at(p).is_ascii_digit() || (at(p) == b'.' && at(p + 1).is_ascii_digit())) {
        return None;
    }
    let digits = |num: &mut Vec<u8>, p: &mut usize| {
        while at(*p).is_ascii_digit() {
            num.push(at(*p));
            *p += 1;
        }
    };
    digits(&mut num, &mut p);
    if at(p) == b'.' {
        if at(p + 1).is_ascii_digit() {
            num.push(b'.');
            p += 1;
            digits(&mut num, &mut p);
        } else if matches!(at(p + 1), b'e' | b'E') {
            p += 1;
        }
    }
    if matches!(at(p), b'e' | b'E') {
        let mut q = p + 1;
        let negative = at(q) == b'-';
        if matches!(at(q), b'-' | b'+') {
            q += 1;
        }
        if at(q).is_ascii_digit() {
            num.extend_from_slice(if negative { b"e-" } else { b"e" });
            digits(&mut num, &mut q);
        }
    }
    let v = ffi::ascii_strtod(&num);
    if !v.is_finite() {
        return None;
    }
    Some(if v == 0.0 { 0.0 } else { v })
}

pub fn is_void(tag: &[u8]) -> bool {
    [
        "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
        "keygen", "link", "meta", "param", "source", "track", "wbr",
    ]
    .iter()
    .any(|v| v.as_bytes() == tag)
}

pub fn is_raw_text(tag: &[u8]) -> bool {
    [
        "script",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "noscript",
        "plaintext",
    ]
    .iter()
    .any(|v| tag.eq_ignore_ascii_case(v.as_bytes()))
}

pub fn escape_append(out: &mut Vec<u8>, s: &[u8], escape_quotes: bool) {
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'"' if escape_quotes => out.extend_from_slice(b"&quot;"),
            0xc2 if s.get(i + 1) == Some(&0xa0) => {
                out.extend_from_slice(b"&nbsp;");
                i += 1;
            }
            c => out.push(c),
        }
        i += 1;
    }
}

pub fn escape_text(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    escape_append(&mut out, s, true);
    out
}

pub fn image_document(url: &[u8]) -> Vec<u8> {
    let escaped = escape_text(url);
    let mut name = ffi::path_basename(url);
    if let Some(query) = name.iter().position(|&c| c == b'?') {
        name.truncate(query);
    }
    let name = escape_text(if name.is_empty() { b"image" } else { &name });
    [
        b"<!DOCTYPE html><html><head><title>".as_slice(),
        &name,
        b"</title><style>html,body{margin:0;min-height:100vh}body{background:#1c1d1e;text-align:center}img{max-width:100vw;max-height:100vh}</style></head><body><img src=\"",
        &escaped,
        b"\" alt=\"\"></body></html>",
    ]
    .concat()
}

const VIEWER_STYLE: &str = "<style>\
body{margin:0;background:#fbfbfd;color:#1a1a1a;\
font:13px/1.55 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}\
pre{margin:0;padding:14px;white-space:pre;tab-size:2}\
.k{color:#9b2393}.s{color:#1a7f37}.n{color:#0b69c7}.b{color:#b35900}\
.p{color:#6e7781}.tag{color:#116329}.at{color:#6f42c1}.av{color:#1a7f37}\
.cm{color:#6a737d;font-style:italic}.pi{color:#6e7781}\
</style>";

fn doc_escaped(out: &mut Vec<u8>, s: &[u8]) {
    for &c in s {
        match c {
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'&' => out.extend_from_slice(b"&amp;"),
            c => out.push(c),
        }
    }
}

fn doc_indent(out: &mut Vec<u8>, depth: i32) {
    for _ in 0..depth.clamp(0, 64) {
        out.extend_from_slice(b"  ");
    }
}

fn viewer_page(url: Option<&[u8]>, body: &[u8]) -> Vec<u8> {
    let url = escape_text(until_nul(url.unwrap_or_default()));
    [
        b"<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>".as_slice(),
        &url,
        b"</title>",
        VIEWER_STYLE.as_bytes(),
        b"</head><body><pre>",
        until_nul(body),
        b"</pre></body></html>",
    ]
    .concat()
}

struct Json<'a> {
    s: &'a [u8],
    p: usize,
    out: Vec<u8>,
    ok: bool,
}

impl Json<'_> {
    fn at(&self) -> Option<u8> {
        self.s.get(self.p).copied()
    }

    fn ws(&mut self) {
        while matches!(self.at(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.p += 1;
        }
    }

    fn string(&mut self, class: &str) {
        let start = self.p;
        self.p += 1;
        while self.p < self.s.len() && self.s[self.p] != b'"' {
            if self.s[self.p] == b'\\' && self.p + 1 < self.s.len() {
                self.p += 1;
            }
            self.p += 1;
        }
        if self.p >= self.s.len() {
            self.ok = false;
            return;
        }
        self.p += 1;
        self.out
            .extend_from_slice(format!("<span class={class}>").as_bytes());
        doc_escaped(&mut self.out, &self.s[start..self.p]);
        self.out.extend_from_slice(b"</span>");
    }

    fn literal(&mut self, lit: &str) -> bool {
        if self.s[self.p..].starts_with(lit.as_bytes()) {
            self.out
                .extend_from_slice(format!("<span class=b>{lit}</span>").as_bytes());
            self.p += lit.len();
            return true;
        }
        false
    }

    fn punct(&mut self, c: u8) {
        self.out.extend_from_slice(b"<span class=p>");
        self.out.push(c);
        self.out.extend_from_slice(b"</span>");
    }

    fn value(&mut self, depth: i32) {
        if depth > 256 {
            self.ok = false;
            return;
        }
        self.ws();
        let Some(ch) = self.at() else {
            self.ok = false;
            return;
        };
        if ch == b'{' || ch == b'[' {
            let close = if ch == b'{' { b'}' } else { b']' };
            let object = ch == b'{';
            self.punct(ch);
            self.p += 1;
            self.ws();
            if self.at() == Some(close) {
                self.p += 1;
                self.punct(close);
                return;
            }
            loop {
                self.out.push(b'\n');
                doc_indent(&mut self.out, depth + 1);
                self.ws();
                if object {
                    if self.at() != Some(b'"') {
                        self.ok = false;
                        return;
                    }
                    self.string("k");
                    self.ws();
                    if self.at() != Some(b':') {
                        self.ok = false;
                        return;
                    }
                    self.p += 1;
                    self.out.extend_from_slice(b"<span class=p>: </span>");
                }
                self.value(depth + 1);
                if !self.ok {
                    return;
                }
                self.ws();
                if self.at() == Some(b',') {
                    self.p += 1;
                    self.out.extend_from_slice(b"<span class=p>,</span>");
                    continue;
                }
                break;
            }
            self.out.push(b'\n');
            doc_indent(&mut self.out, depth);
            if self.at() != Some(close) {
                self.ok = false;
                return;
            }
            self.p += 1;
            self.punct(close);
            return;
        }
        if ch == b'"' {
            self.string("s");
            return;
        }
        if ch == b'-' || ch.is_ascii_digit() {
            let start = self.p;
            if ch == b'-' {
                self.p += 1;
            }
            while matches!(
                self.at(),
                Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
            ) {
                self.p += 1;
            }
            if self.p == start {
                self.ok = false;
                return;
            }
            self.out.extend_from_slice(b"<span class=n>");
            doc_escaped(&mut self.out, &self.s[start..self.p]);
            self.out.extend_from_slice(b"</span>");
            return;
        }
        if self.literal("true") || self.literal("false") || self.literal("null") {
            return;
        }
        self.ok = false;
    }
}

pub fn json_document(url: Option<&[u8]>, json: &[u8]) -> Option<Vec<u8>> {
    let mut parser = Json {
        s: json,
        p: 0,
        out: Vec::new(),
        ok: true,
    };
    parser.ws();
    parser.value(0);
    parser.ok.then(|| viewer_page(url, &parser.out))
}

fn xml_tag_end(s: &[u8], mut p: usize) -> Option<usize> {
    let mut quote = 0u8;
    while p < s.len() {
        let c = s[p];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return Some(p);
        }
        p += 1;
    }
    None
}

fn find_before_nul(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    until_nul(haystack)
        .windows(needle.len())
        .position(|w| w == needle)
}

fn xml_line(out: &mut Vec<u8>, depth: i32, class: Option<&str>, text: &[u8]) {
    if !out.is_empty() {
        out.push(b'\n');
    }
    doc_indent(out, depth);
    if let Some(class) = class {
        out.extend_from_slice(format!("<span class={class}>").as_bytes());
    }
    doc_escaped(out, text);
    if class.is_some() {
        out.extend_from_slice(b"</span>");
    }
}

pub fn xml_document(url: Option<&[u8]>, xml: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let end = xml.len();
    let mut p = 0;
    let mut depth = 0;
    while p < end {
        if xml[p] != b'<' {
            let t = p;
            while p < end && xml[p] != b'<' {
                p += 1;
            }
            let text = xml[t..p].trim_ascii();
            if !text.is_empty() {
                xml_line(&mut out, depth, None, text);
            }
            continue;
        }
        let rest = &xml[p..];
        if rest.starts_with(b"<!--") || rest.starts_with(b"<![CDATA[") {
            let (close, class): (&[u8], &str) = if rest.starts_with(b"<!--") {
                (b"-->", "cm")
            } else {
                (b"]]>", "s")
            };
            let te = find_before_nul(rest, close).map_or(end, |i| p + i + 3);
            xml_line(&mut out, depth, Some(class), &xml[p..te]);
            p = te;
            continue;
        }
        if p + 1 < end && (xml[p + 1] == b'!' || xml[p + 1] == b'?') {
            let te = xml_tag_end(xml, p).map_or(end, |e| e + 1);
            xml_line(&mut out, depth, Some("pi"), &xml[p..te]);
            p = te;
            continue;
        }
        let Some(e) = xml_tag_end(xml, p) else {
            xml_line(&mut out, depth, Some("tag"), &xml[p..]);
            break;
        };
        let is_end = p + 1 < end && xml[p + 1] == b'/';
        let self_close = e > p && xml[e - 1] == b'/';
        if is_end && depth > 0 {
            depth -= 1;
        }
        xml_line(&mut out, depth, Some("tag"), &xml[p..=e]);
        if !is_end && !self_close {
            depth += 1;
        }
        p = e + 1;
    }
    viewer_page(url, &out)
}

fn charset_normalize(name: &[u8]) -> Vec<u8> {
    tables::ICONV_NAMES
        .iter()
        .find(|(label, _)| name.eq_ignore_ascii_case(label.as_bytes()))
        .map_or_else(
            || name.to_ascii_uppercase(),
            |(_, iconv)| iconv.as_bytes().to_vec(),
        )
}

fn charset_value_in(s: &[u8]) -> Option<Vec<u8>> {
    let len = s.len();
    let mut i = 0;
    while i + 7 <= len {
        if !s[i..i + 7].eq_ignore_ascii_case(b"charset") {
            i += 1;
            continue;
        }
        let mut p = i + 7;
        while p < len && s[p].is_ascii_whitespace() {
            p += 1;
        }
        if p >= len || s[p] != b'=' {
            i += 1;
            continue;
        }
        p += 1;
        while p < len && s[p].is_ascii_whitespace() {
            p += 1;
        }
        if p < len && (s[p] == b'"' || s[p] == b'\'') {
            p += 1;
        }
        let start = p;
        while p < len && (s[p].is_ascii_alphanumeric() || matches!(s[p], b'-' | b'_' | b':' | b'.'))
        {
            p += 1;
        }
        if p > start && p - start < 40 {
            return Some(s[start..p].to_vec());
        }
        i += 1;
    }
    None
}

fn encoding_label_to_name(label: &[u8]) -> Option<&'static str> {
    let start = label.iter().take_while(|&&c| html_space(c)).count();
    let mut end = label.len();
    while end > start && html_space(label[end - 1]) {
        end -= 1;
    }
    let label = &label[start..end];
    tables::ENCODING_LABELS
        .iter()
        .find(|(l, _)| l.len() == label.len() && l.as_bytes().eq_ignore_ascii_case(label))
        .map(|(_, name)| *name)
}

pub fn declared_charset(body: Option<&[u8]>, content_type: Option<&[u8]>) -> Option<&'static str> {
    let mut from_meta = false;
    let mut label = content_type.and_then(|ct| charset_value_in(until_nul(ct)));
    if label.is_none() {
        if let Some(body) = body {
            label = charset_value_in(&body[..body.len().min(1024)]);
            from_meta = label.is_some();
        }
    }
    let name = encoding_label_to_name(&label?)?;
    Some(if from_meta && name.starts_with("UTF-16") {
        "UTF-8"
    } else {
        name
    })
}

fn charset_is_dangerous(charset: &[u8]) -> bool {
    if charset.is_empty() {
        return true;
    }
    let up = charset.to_ascii_uppercase();
    let has = |needle: &[u8]| up.windows(needle.len()).any(|w| w == needle);
    has(b"UTF-7")
        || has(b"UTF7")
        || has(b"REPLACEMENT")
        || up.starts_with(b"HZ")
        || has(b"2022-CN")
        || has(b"2022CN")
        || has(b"2022-KR")
        || has(b"2022KR")
        || has(b"IMAP")
        || has(b"CESU")
        || has(b"BOCU")
        || has(b"SCSU")
}

pub enum Decoded {
    Glib(ffi::GlibString),
    Bytes(Vec<u8>),
}

pub fn decode_body(body: &[u8], content_type: Option<&[u8]>) -> (Decoded, Option<Vec<u8>>) {
    if body.is_empty() {
        return (Decoded::Bytes(Vec::new()), None);
    }
    let utf8 = || Some(b"UTF-8".to_vec());
    if body.starts_with(b"\xef\xbb\xbf") {
        return ((ffi::utf8_make_valid(&body[3..])), utf8());
    }
    for (bom, from) in [(b"\xff\xfe", "UTF-16LE"), (b"\xfe\xff", "UTF-16BE")] {
        if body.starts_with(bom) {
            if let Some(out) = ffi::convert(&body[2..], from.as_bytes()) {
                return (Decoded::Glib(out), Some(from.as_bytes().to_vec()));
            }
        }
    }
    let mut declared = content_type.and_then(|ct| charset_value_in(until_nul(ct)));
    if declared.is_none() {
        declared = charset_value_in(&body[..body.len().min(1024)]);
    }
    let mut declared_utf8 = false;
    if let Some(declared) = declared {
        let charset = charset_normalize(&declared);
        if charset.eq_ignore_ascii_case(b"UTF-8") {
            declared_utf8 = true;
        } else if !charset_is_dangerous(&charset) {
            if let Some(out) = ffi::convert(body, &charset) {
                return (Decoded::Glib(out), Some(charset));
            }
        }
    }
    if ffi::utf8_validate(body) {
        return (Decoded::Bytes(body.to_vec()), utf8());
    }
    if declared_utf8 {
        return ((ffi::utf8_make_valid(body)), utf8());
    }
    if let Some(charset) = ffi::detect_charset(&body[..body.len().min(1024 * 1024)]) {
        if !charset_is_dangerous(&charset) {
            if let Some(out) = ffi::convert(body, &charset) {
                return (Decoded::Glib(out), Some(charset));
            }
        }
    }
    if let Some(out) = ffi::convert(body, b"WINDOWS-1252") {
        return (Decoded::Glib(out), Some(b"WINDOWS-1252".to_vec()));
    }
    ((ffi::utf8_make_valid(body)), utf8())
}

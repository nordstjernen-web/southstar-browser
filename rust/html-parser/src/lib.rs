//! Southstar — the lexbor-backed HTML parser's tree post-processing: declarative shadow roots, standard media metadata, inline script positions and XML well-formedness.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use ffi::dom;
use southstar_dom::{Node, children};

const MAX_DEPTH: i32 = 512;
const SCRIPT_TEXT_DEPTH: i32 = 64;
const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";
const MEDIA_SRC_ATTR: &CStr = c"data-nd-media-src";
const MEDIA_POSTER_ATTR: &CStr = c"data-nd-media-poster";
const MEDIA_STREAM_ATTR: &CStr = c"data-nd-media-stream";
const SHADOW_HOSTS: [&str; 18] = [
    "article",
    "aside",
    "blockquote",
    "body",
    "div",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "main",
    "nav",
    "p",
    "section",
    "span",
];
const SHADOW_TEMPLATE_ATTRS: [&CStr; 5] = [
    c"shadowrootmode",
    c"shadowroot",
    c"shadowrootdelegatesfocus",
    c"shadowrootserializable",
    c"shadowrootclonable",
];
const PLAYABLE_EXTENSIONS: [&[u8]; 6] = [b".webm", b".mpg", b".mpeg", b".m1v", b".ogv", b".ogg"];

fn name_is(node: Node, name: &str) -> bool {
    node.element_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(name.as_bytes()))
}

fn valid_shadow_host(name: &[u8]) -> bool {
    SHADOW_HOSTS
        .iter()
        .any(|host| name.eq_ignore_ascii_case(host.as_bytes()))
        || name.contains(&b'-')
}

fn ieq(value: &CStr, word: &str) -> bool {
    value.to_bytes().eq_ignore_ascii_case(word.as_bytes())
}

fn adopt_template_content(template: Node) {
    let Some(fragment) = template.take_tpl_content() else {
        return;
    };
    let mut child = fragment.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        dom::detach(c);
        dom::append_child(template, c);
    }
    dom::free(fragment);
}

fn attach_declarative_shadow(template: Node, mode: &CStr) {
    let closed = ieq(mode, "closed");
    let delegates = template.attr(c"shadowrootdelegatesfocus").is_some();
    let serializable = template.attr(c"shadowrootserializable").is_some();
    let clonable = template.attr(c"shadowrootclonable").is_some();
    dom::rename_static(template, c"div");
    for attr in SHADOW_TEMPLATE_ATTRS {
        dom::remove_attr(template, attr);
    }
    adopt_template_content(template);
    dom::set_attr(
        template,
        SHADOW_ATTR,
        if closed { c"closed" } else { c"open" },
    );
    dom::set_attr(template, c"data-nd-shadow-declarative", c"1");
    for (set, attr) in [
        (delegates, c"data-nd-shadow-delegates"),
        (serializable, c"data-nd-shadow-serializable"),
        (clonable, c"data-nd-shadow-clonable"),
    ] {
        if set {
            dom::set_attr(template, attr, c"1");
        }
    }
}

pub(crate) fn convert_declarative_shadow(node: Option<Node>, depth: i32) {
    let Some(node) = node else { return };
    if depth >= MAX_DEPTH {
        return;
    }
    let host_ok = node.element_name().is_some_and(valid_shadow_host);
    let mut shadow_done = false;
    for child in children(node) {
        if host_ok && !shadow_done && name_is(child, "template") {
            let mode = child
                .attr(c"shadowrootmode")
                .or_else(|| child.attr(c"shadowroot"));
            if let Some(mode) = mode.filter(|m| ieq(m, "open") || ieq(m, "closed")) {
                shadow_done = true;
                let mode = mode.to_owned();
                attach_declarative_shadow(child, &mode);
            }
        }
        convert_declarative_shadow(Some(child), depth + 1);
    }
    if let Some(content) = node.tpl_content() {
        convert_declarative_shadow(Some(content), depth + 1);
    }
}

fn append_text_descendants(node: Node, out: &mut Vec<u8>, depth: i32) {
    if depth >= SCRIPT_TEXT_DEPTH {
        return;
    }
    if node.is_text()
        && let Some(text) = node.text()
    {
        out.extend_from_slice(text.to_bytes());
    }
    for child in children(node) {
        append_text_descendants(child, out, depth + 1);
    }
}

fn script_text(node: Node) -> Vec<u8> {
    let mut out = Vec::new();
    append_text_descendants(node, &mut out, 0);
    out
}

fn push_utf8(out: &mut Vec<u8>, ch: u32) {
    if ch < 0x80 {
        out.push(ch as u8);
    } else if ch < 0x800 {
        out.extend_from_slice(&[0xC0 | (ch >> 6) as u8, 0x80 | (ch & 0x3F) as u8]);
    } else {
        out.extend_from_slice(&[
            0xE0 | (ch >> 12) as u8,
            0x80 | ((ch >> 6) & 0x3F) as u8,
            0x80 | (ch & 0x3F) as u8,
        ]);
    }
}

fn json_string_unescape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() && text[i] != b'"' {
        let c = text[i];
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&e) = text.get(i) else { break };
        match e {
            b'/' | b'"' | b'\\' => out.push(e),
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'u' if text
                .get(i + 1..i + 5)
                .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) =>
            {
                let hex = core::str::from_utf8(&text[i + 1..i + 5]).unwrap_or("0");
                let ch = u32::from_str_radix(hex, 16).unwrap_or(0);
                if ch != 0 {
                    push_utf8(&mut out, ch);
                }
                i += 4;
            }
            _ => out.push(e),
        }
        i += 1;
    }
    out
}

fn json_string_value_for_key<'a>(text: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    if key.is_empty() {
        return None;
    }
    let mut from = 0;
    while let Some(found) = find(&text[from..], key) {
        let p = from + found;
        let mut q = p + key.len();
        while text.get(q).is_some_and(u8::is_ascii_whitespace) {
            q += 1;
        }
        if text.get(q) == Some(&b':') {
            q += 1;
            while text.get(q).is_some_and(u8::is_ascii_whitespace) {
                q += 1;
            }
            if text.get(q) == Some(&b'"') {
                return Some(&text[q + 1..]);
            }
        }
        from = p + key.len();
    }
    None
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn json_first_url_for_key(text: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    json_string_value_for_key(text, key).map(json_string_unescape)
}

fn find_element_with_attr_contains<'a>(
    node: Node<'a>,
    attr: &CStr,
    word: &[u8],
    depth: i32,
) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if node.is_element()
        && node
            .attr(attr)
            .is_some_and(|v| find(v.to_bytes(), word).is_some())
    {
        return Some(node);
    }
    children(node).find_map(|child| find_element_with_attr_contains(child, attr, word, depth + 1))
}

fn document_media_target(root: Node) -> Option<Node> {
    dom::find_first_element(root, c"video")
        .or_else(|| dom::find_by_id(root, c"player"))
        .or_else(|| find_element_with_attr_contains(root, c"class", b"player", 0))
}

fn find_meta_property<'a>(node: Node<'a>, property: &str, depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if node.element_name() == Some(b"meta") {
        let name = node.attr(c"property").or_else(|| node.attr(c"name"));
        let content = node.attr(c"content");
        if name.is_some_and(|p| ieq(p, property)) && content.is_some_and(|c| !c.is_empty()) {
            return Some(node);
        }
    }
    children(node).find_map(|child| find_meta_property(child, property, depth + 1))
}

fn meta_property_content(root: Node, property: &str) -> Option<Vec<u8>> {
    find_meta_property(root, property, 0)
        .and_then(|m| m.attr(c"content"))
        .filter(|c| !c.is_empty())
        .map(|c| c.to_bytes().to_vec())
}

fn jsonld_video_object_url(node: Node, key: &[u8], depth: i32) -> Option<Vec<u8>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if node.element_name() == Some(b"script")
        && node
            .attr(c"type")
            .is_some_and(|t| ieq(t, "application/ld+json"))
    {
        let text = script_text(node);
        if find(&text, b"VideoObject").is_some()
            && let Some(url) = json_first_url_for_key(&text, key).filter(|u| !u.is_empty())
        {
            return Some(url);
        }
    }
    children(node).find_map(|child| jsonld_video_object_url(child, key, depth + 1))
}

fn media_url_is_direct_playable(url: &[u8]) -> bool {
    if !url.starts_with(b"http") {
        return false;
    }
    let end = url
        .iter()
        .position(|&b| b == b'?' || b == b'#')
        .unwrap_or(url.len());
    let Some(dot) = url[..end].iter().rposition(|&b| b == b'.') else {
        return false;
    };
    let extension = url[dot..end].to_ascii_lowercase();
    PLAYABLE_EXTENSIONS.contains(&extension.as_slice())
}

fn first_of(candidates: impl IntoIterator<Item = Option<Vec<u8>>>) -> Option<Vec<u8>> {
    candidates.into_iter().flatten().next()
}

fn c_value(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap_or_default()
}

pub(crate) fn extract_standard_media(root: Node) {
    let Some(target) = document_media_target(root) else {
        return;
    };
    if target.attr(MEDIA_SRC_ATTR).is_some() || target.attr(MEDIA_STREAM_ATTR).is_some() {
        return;
    }
    let direct = jsonld_video_object_url(root, b"\"contentUrl\"", 0)
        .filter(|u| media_url_is_direct_playable(u));
    let og_video = first_of([
        meta_property_content(root, "og:video:secure_url"),
        meta_property_content(root, "og:video:url"),
        meta_property_content(root, "og:video"),
    ]);
    let twitter_player = meta_property_content(root, "twitter:player");
    let poster = first_of([
        meta_property_content(root, "og:image:secure_url"),
        meta_property_content(root, "og:image"),
        jsonld_video_object_url(root, b"\"thumbnailUrl\"", 0),
        meta_property_content(root, "twitter:image"),
    ]);
    let embed = jsonld_video_object_url(root, b"\"embedUrl\"", 0);
    if direct.is_none() && og_video.is_none() && twitter_player.is_none() && embed.is_none() {
        return;
    }
    if let Some(direct) = &direct {
        dom::set_attr(target, MEDIA_SRC_ATTR, &c_value(direct));
    } else if let Some(video) = og_video
        .as_ref()
        .filter(|v| media_url_is_direct_playable(v))
    {
        dom::set_attr(target, MEDIA_SRC_ATTR, &c_value(video));
    } else {
        dom::set_attr(target, MEDIA_STREAM_ATTR, c"1");
    }
    if let Some(poster) = poster.filter(|p| !p.is_empty()) {
        dom::set_attr(target, MEDIA_POSTER_ATTR, &c_value(&poster));
    }
}

fn skip_until(input: &[u8], from: usize, delimiter: &[u8]) -> Option<usize> {
    find(&input[from.min(input.len())..], delimiter).map(|at| from + at + delimiter.len())
}

fn truncate_at_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

fn root_default_namespace(tag: &[u8]) -> Option<Vec<u8>> {
    for p in 0..tag.len() {
        if p != 0 && !tag[p - 1].is_ascii_whitespace() {
            continue;
        }
        if !tag[p..].starts_with(b"xmlns") {
            continue;
        }
        let mut a = p + 5;
        while a < tag.len() && tag[a].is_ascii_whitespace() {
            a += 1;
        }
        if tag.get(a) != Some(&b'=') {
            continue;
        }
        a += 1;
        while a < tag.len() && tag[a].is_ascii_whitespace() {
            a += 1;
        }
        let Some(&quote) = tag.get(a).filter(|&&q| q == b'"' || q == b'\'') else {
            continue;
        };
        let start = a + 1;
        let end = tag[start..]
            .iter()
            .position(|&b| b == quote)
            .map_or(tag.len(), |e| start + e);
        return Some(truncate_at_nul(&tag[start..end]).to_vec());
    }
    None
}

pub fn xml_well_formed(input: &[u8]) -> (bool, Option<Vec<u8>>) {
    let end = input.len();
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut root_seen = false;
    let mut root_closed = false;
    let mut root_ns = None;
    let mut p = 0;
    while p < end {
        if input[p] != b'<' {
            p += 1;
            continue;
        }
        let mut q = p + 1;
        if input.get(q) == Some(&b'!') {
            let rest = &input[q..];
            let skipped = if rest.starts_with(b"!--") {
                skip_until(input, q + 3, b"-->")
            } else if rest.starts_with(b"![CDATA[") {
                skip_until(input, q + 8, b"]]>")
            } else {
                input[q..]
                    .iter()
                    .position(|&b| b == b'>')
                    .map(|e| q + e + 1)
            };
            match skipped {
                Some(next) => p = next,
                None => return (false, None),
            }
            continue;
        }
        if input.get(q) == Some(&b'?') {
            match skip_until(input, q + 1, b"?>") {
                Some(next) => p = next,
                None => return (false, None),
            }
            continue;
        }
        let is_end = input.get(q) == Some(&b'/');
        if is_end {
            q += 1;
        }
        let mut te = q;
        let mut quote = 0u8;
        while te < end {
            let c = input[te];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'>' {
                break;
            }
            te += 1;
        }
        if te >= end {
            return (false, None);
        }
        let mut self_close = false;
        let mut inner_end = te;
        if !is_end {
            let mut t = te - 1;
            while t > q && input[t].is_ascii_whitespace() {
                t -= 1;
            }
            if input[t] == b'/' {
                self_close = true;
                inner_end = t;
            }
        }
        let mut ne = q;
        while ne < inner_end && !input[ne].is_ascii_whitespace() && input[ne] != b'/' {
            ne += 1;
        }
        if ne == q {
            return (false, None);
        }
        let name = truncate_at_nul(&input[q..ne]).to_vec();
        if is_end {
            if stack.last() != Some(&name) {
                return (false, None);
            }
            stack.pop();
            if stack.is_empty() {
                root_closed = true;
            }
        } else {
            if root_closed {
                return (false, None);
            }
            if !root_seen {
                root_seen = true;
                root_ns = root_default_namespace(&input[ne..inner_end]);
            }
            if self_close {
                if stack.is_empty() {
                    root_closed = true;
                }
            } else {
                stack.push(name);
            }
        }
        p = te + 1;
    }
    if !stack.is_empty() || !root_seen {
        return (false, None);
    }
    (true, root_ns)
}

fn is_html_whitespace(text: &CStr) -> bool {
    text.to_bytes()
        .iter()
        .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c'))
}

pub(crate) fn prune_html_interelement_whitespace(root: Node) {
    let Some(html) = children(root).find(|c| name_is(*c, "html")) else {
        return;
    };
    let has_head = children(html).any(|c| name_is(c, "head"));
    let has_body = children(html).any(|c| name_is(c, "body"));
    if !has_head || !has_body {
        return;
    }
    let mut child = html.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if c.is_text() && c.text().is_none_or(is_html_whitespace) {
            dom::detach(c);
            dom::free(c);
        }
    }
}

fn collect_scripts<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in children(node) {
        if name_is(child, "script") {
            out.push(child);
        }
        collect_scripts(child, out, depth + 1);
    }
}

fn script_positions(input: &[u8]) -> Vec<(c_int, c_int)> {
    let mut positions = Vec::new();
    let (mut line, mut col): (c_int, c_int) = (1, 1);
    let mut i = 0;
    let len = input.len();
    while i < len {
        if input[i] == b'<'
            && i + 7 <= len
            && input[i..i + 7].eq_ignore_ascii_case(b"<script")
            && (i + 7 == len || !input[i + 7].is_ascii_alphanumeric())
        {
            let (mut l, mut c) = (line, col);
            let mut j = i;
            while j < len && input[j] != b'>' {
                if input[j] == b'\n' {
                    l += 1;
                    c = 1;
                } else {
                    c += 1;
                }
                j += 1;
            }
            if j < len {
                c += 1;
                j += 1;
                positions.push((l, c));
                line = l;
                col = c;
                i = j;
                continue;
            }
        }
        if input[i] == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        i += 1;
    }
    positions
}

pub(crate) fn assign_script_positions(root: Node, input: &[u8]) {
    let positions = script_positions(input);
    let mut scripts = Vec::new();
    collect_scripts(root, &mut scripts, 0);
    for (script, (line, col)) in scripts.into_iter().zip(positions) {
        script.set_source_position(line, col);
    }
}

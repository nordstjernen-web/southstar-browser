//! Southstar — a style sheet's rules read from its flattened text: style rules with their scopes, @media, @supports, @container, @layer and @scope blocks, @import with layer() and media, @font-face, @keyframes, @property and @page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::atomic::{AtomicI32, Ordering};

use crate::color;
use crate::container;
use crate::content;
use crate::ffi::{self, PageRule, SheetBuilder, SyntaxDef};
use crate::lex::read_ident;
use crate::nesting::{block_body_end, skip_invalid_qualified_rule};
use crate::prop::Prop;
use crate::property::{self, Body};
use crate::scan::{
    is_ident, is_ws, scan_until, skip_comment, skip_to_block_end, skip_ws_comments, strip,
    trim_range,
};
use crate::selector::{self, PE_NONE};
use crate::supports;
use crate::transform::{self, OPS_MAX, Transform};
use crate::units::PX;

const MAX_AT_NESTING: i32 = 32;

pub(crate) const SLANT_AUTO: i32 = 0;
const SLANT_ROMAN: i32 = 1;
const SLANT_ITALIC: i32 = 2;
const SLANT_OBLIQUE: i32 = 3;

static AT_DEPTH: AtomicI32 = AtomicI32::new(0);

#[derive(Clone)]
pub(crate) struct ScopeText {
    pub start: Option<Vec<u8>>,
    pub end: Option<Vec<u8>>,
}

pub(crate) struct FontFace {
    pub family: Vec<u8>,
    pub src_url: Vec<u8>,
    pub unicode_range: Option<Vec<u8>>,
    pub weight: i32,
    pub slant: i32,
}

pub(crate) struct Stop {
    pub pct: f64,
    pub opacity: f64,
    pub has_opacity: bool,
    pub transform: Transform,
    pub has_transform: bool,
    pub color: [u8; 4],
    pub has_color: bool,
    pub bg_color: [u8; 4],
    pub has_bg_color: bool,
    pub raw_props: Option<Vec<u8>>,
}

fn eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn starts_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn scan_segment(s: &[u8], p: usize) -> (usize, u8) {
    scan_until(s, p, s.len(), b"{;}")
}

fn skip_at_rule(s: &[u8], p: usize) -> usize {
    let (seg, term) = scan_segment(s, p);
    match term {
        b';' => seg + 1,
        b'{' => skip_to_block_end(s, seg, s.len()),
        _ => seg,
    }
}

fn descriptors(body: &[u8]) -> Vec<(&[u8], &[u8])> {
    let end = body.len();
    let mut out = Vec::new();
    let mut p = 0;
    while p < end {
        let (decl_end, term) = scan_until(body, p, end, b";}");
        let line = strip(&body[p..decl_end]);
        let (colon, colon_term) = scan_until(line, 0, line.len(), b":");
        if colon_term == b':' {
            out.push((strip(&line[..colon]), strip(&line[colon + 1..])));
        }
        if term == 0 {
            break;
        }
        p = decl_end + 1;
    }
    out
}

fn font_face_weight(value: &[u8]) -> i32 {
    if eq(value, b"normal") {
        return 400;
    }
    if eq(value, b"bold") {
        return 700;
    }
    let text = std::ffi::CString::new(value).unwrap_or_default();
    let (weight, end) = ffi::strtod(&text, 0);
    if end == 0 || end != value.len() || !(1.0..=1000.0).contains(&weight) {
        return 0;
    }
    (weight + 0.5) as i32
}

fn font_face_slant(value: &[u8]) -> i32 {
    if eq(value, b"normal") {
        SLANT_ROMAN
    } else if eq(value, b"italic") {
        SLANT_ITALIC
    } else if starts_ci(value, b"oblique") {
        SLANT_OBLIQUE
    } else {
        SLANT_AUTO
    }
}

fn font_url_suffix_eq(url: &[u8], suffix: &[u8]) -> bool {
    url.len() >= suffix.len() && url[url.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn has(text: &[u8], needle: &[u8]) -> bool {
    text.windows(needle.len()).any(|w| w == needle)
}

fn font_src_score(url: &[u8]) -> i32 {
    if url.is_empty() {
        return -1;
    }
    if url.starts_with(b"data:") {
        return if has(url, b"font/woff2") {
            80
        } else if has(url, b"font/woff") {
            70
        } else if has(url, b"font/") {
            40
        } else {
            20
        };
    }
    let end = [b'?', b'#']
        .iter()
        .filter_map(|c| url.iter().position(|b| b == c))
        .min()
        .unwrap_or(url.len());
    let path = &url[..end];
    if font_url_suffix_eq(path, b".woff2") {
        80
    } else if font_url_suffix_eq(path, b".woff") {
        70
    } else if [&b".otf"[..], b".ttf", b".ttc"]
        .iter()
        .any(|s| font_url_suffix_eq(path, s))
    {
        60
    } else if font_url_suffix_eq(path, b".eot") || font_url_suffix_eq(path, b".svg") {
        -1
    } else {
        10
    }
}

fn font_src_consider(best: &mut Option<Vec<u8>>, candidate: &[u8]) {
    if candidate.is_empty() {
        return;
    }
    let score = font_src_score(candidate);
    if score < 0 {
        return;
    }
    let old_score = best.as_deref().map_or(-1, font_src_score);
    if best.is_none() || score > old_score {
        *best = Some(candidate.to_vec());
    }
}

fn font_src_consider_urls(best: &mut Option<Vec<u8>>, value: &[u8]) {
    let end = value.len();
    let mut p = 0;
    while p < end {
        if p + 4 <= end && value[p..p + 4].eq_ignore_ascii_case(b"url(") {
            p = skip_ws_comments(value, p + 4, end);
            let mut quote = 0u8;
            if p < end && (value[p] == b'"' || value[p] == b'\'') {
                quote = value[p];
                p += 1;
            }
            let start = p;
            if quote != 0 {
                while p < end {
                    if value[p] == b'\\' && p + 1 < end {
                        p += 2;
                    } else if value[p] == quote {
                        break;
                    } else {
                        p += 1;
                    }
                }
            } else {
                while p < end && value[p] != b')' && !is_ws(value[p]) {
                    p += if value[p] == b'\\' && p + 1 < end {
                        2
                    } else {
                        1
                    };
                }
            }
            if p > start {
                font_src_consider(best, &value[start..p.min(end)]);
            }
            while p < end && value[p] != b')' {
                p += 1;
            }
            if p < end {
                p += 1;
            }
            continue;
        }
        if value[p] == b'"' || value[p] == b'\'' {
            let q = value[p];
            p += 1;
            while p < end {
                if value[p] == b'\\' && p + 1 < end {
                    p += 2;
                } else {
                    p += 1;
                    if value[p - 1] == q {
                        break;
                    }
                }
            }
        } else if p + 1 < end && value[p] == b'/' && value[p + 1] == b'*' {
            p = skip_comment(value, p, end);
        } else {
            p += 1;
        }
    }
}

fn string_descriptor(value: &[u8]) -> Option<Vec<u8>> {
    let (items, valid) = southstar_css_syntax::parse(value);
    if !valid {
        return None;
    }
    let mut only = None;
    for item in &items {
        if item.kind == southstar_css_syntax::Kind::Whitespace {
            continue;
        }
        if only.is_some() {
            return None;
        }
        only = Some(item);
    }
    let only = only.filter(|c| c.kind == southstar_css_syntax::Kind::String)?;
    Some(
        only.value
            .clone()
            .map(|range| value[range].to_vec())
            .unwrap_or_default(),
    )
}

fn keyframes_name(prelude: &[u8]) -> Vec<u8> {
    let name = trim_range(prelude, 0, prelude.len());
    let n = name.len();
    if n >= 2 && (name[0] == b'"' || name[0] == b'\'') && name[n - 1] == name[0] {
        let mut out = Vec::with_capacity(n);
        let mut i = 1;
        while i + 1 < n {
            if name[i] == b'\\' && i + 1 < n - 1 {
                i += 1;
            }
            out.push(name[i]);
            i += 1;
        }
        return out;
    }
    name.to_vec()
}

fn keyframe_stop_pct(sel: &[u8]) -> Option<f64> {
    let start = sel.iter().position(|&c| c != b' ').unwrap_or(sel.len());
    let sel = &sel[start..];
    if eq(sel, b"from") {
        return Some(0.0);
    }
    if eq(sel, b"to") {
        return Some(100.0);
    }
    let text = std::ffi::CString::new(sel).unwrap_or_default();
    let (v, mut end) = ffi::strtod(&text, 0);
    if end == 0 {
        return None;
    }
    while end < sel.len() && sel[end] == b' ' {
        end += 1;
    }
    (end == sel.len() || sel[end] == b'%').then_some(v)
}

fn color_into(value: &[u8], out: &mut [u8; 4]) -> bool {
    let text = std::ffi::CString::new(value).unwrap_or_default();
    let mut channels: color::Channels = [None; 4];
    let ok = color::parse_into(&text, &mut channels);
    for (slot, channel) in out.iter_mut().zip(channels) {
        if let Some(channel) = channel {
            *slot = channel;
        }
    }
    ok
}

fn push_op(tf: &mut Transform, parsed: Option<Transform>) {
    if let Some(parsed) = parsed {
        if (tf.n_ops as usize) < OPS_MAX {
            tf.ops[tf.n_ops as usize] = parsed.ops[0];
            tf.n_ops += 1;
        }
    }
}

fn keyframe_block(body: &[u8]) -> (Stop, Option<Vec<u8>>) {
    let mut stop = Stop {
        pct: 0.0,
        opacity: 0.0,
        has_opacity: false,
        transform: Transform::default(),
        has_transform: false,
        color: [0; 4],
        has_color: false,
        bg_color: [0; 4],
        has_bg_color: false,
        raw_props: None,
    };
    let mut individual = Transform::default();
    let mut raw: Option<Vec<u8>> = None;
    for (prop, val) in descriptors(body) {
        let transform_prop = [&b"transform"[..], b"translate", b"rotate", b"scale"]
            .iter()
            .any(|p| eq(prop, p));
        let raw = raw.get_or_insert_with(Vec::new);
        if !raw.is_empty() {
            raw.push(b';');
        }
        raw.extend_from_slice(prop);
        raw.push(b':');
        raw.extend_from_slice(val);
        if transform_prop && has(val, b"var(") {
        } else if eq(prop, b"opacity") {
            let text = std::ffi::CString::new(val).unwrap_or_default();
            stop.opacity = ffi::strtod(&text, 0).0;
            stop.has_opacity = true;
        } else if eq(prop, b"transform") {
            if let Some(tf) = transform::parse_transform(val) {
                stop.transform = tf;
                stop.has_transform = true;
            }
        } else if eq(prop, b"translate") {
            push_op(&mut individual, transform::parse_translate_prop(val));
        } else if eq(prop, b"rotate") {
            push_op(&mut individual, transform::parse_rotate_prop(val));
        } else if eq(prop, b"scale") {
            push_op(&mut individual, transform::parse_scale_prop(val));
        } else if eq(prop, b"color") {
            stop.has_color |= color_into(val, &mut stop.color);
        } else if eq(prop, b"background-color") || eq(prop, b"background") {
            stop.has_bg_color |= color_into(val, &mut stop.bg_color);
        }
    }
    if individual.n_ops > 0 {
        let mut merged = individual;
        for k in 0..stop.transform.n_ops as usize {
            if merged.n_ops as usize >= OPS_MAX {
                break;
            }
            merged.ops[merged.n_ops as usize] = stop.transform.ops[k];
            merged.n_ops += 1;
        }
        stop.transform = merged;
        stop.has_transform = true;
    }
    (stop, raw.filter(|raw| !raw.is_empty()))
}

fn page_named_size(name: &[u8]) -> Option<(f64, f64)> {
    const SIZES: [(&[u8], f64, f64); 7] = [
        (b"a3", 297.0, 420.0),
        (b"a4", 210.0, 297.0),
        (b"a5", 148.0, 210.0),
        (b"b4", 250.0, 353.0),
        (b"b5", 176.0, 250.0),
        (b"jis-b4", 257.0, 364.0),
        (b"jis-b5", 182.0, 257.0),
    ];
    if eq(name, b"letter") {
        return Some((8.5 * 96.0, 11.0 * 96.0));
    }
    if eq(name, b"legal") {
        return Some((8.5 * 96.0, 14.0 * 96.0));
    }
    if eq(name, b"ledger") {
        return Some((11.0 * 96.0, 17.0 * 96.0));
    }
    SIZES
        .iter()
        .find(|(known, _, _)| eq(name, known))
        .map(|&(_, w, h)| (w * (96.0 / 25.4), h * (96.0 / 25.4)))
}

fn page_length_px(text: &[u8]) -> Option<f64> {
    match property::parse(Prop::Width, text)?.body {
        Body::Length(v, PX) => Some(v),
        _ => None,
    }
}

fn page_words(text: &[u8]) -> impl Iterator<Item = &[u8]> {
    text.split(|&c| matches!(c, b' ' | b'\t' | b'\r' | b'\n'))
        .filter(|part| !part.is_empty())
}

fn page_apply_size(pr: &mut PageRule, text: &[u8]) {
    let (mut w, mut h, mut n1, mut n2) = (0.0, 0.0, 0.0, 0.0);
    let mut lengths = 0;
    let (mut named, mut portrait, mut landscape, mut bad) = (false, false, false, false);
    for part in page_words(text) {
        if let Some((nw, nh)) = page_named_size(part) {
            (w, h) = (nw, nh);
            named = true;
        } else if eq(part, b"portrait") {
            portrait = true;
        } else if eq(part, b"landscape") {
            landscape = true;
        } else if eq(part, b"auto") {
            continue;
        } else if let Some(px) = page_length_px(part) {
            if lengths == 0 {
                n1 = px;
            } else if lengths == 1 {
                n2 = px;
            }
            lengths += 1;
        } else {
            bad = true;
        }
    }
    if bad || lengths > 2 {
        return;
    }
    if lengths == 1 {
        (w, h) = (n1, n1);
    } else if lengths == 2 {
        (w, h) = (n1, n2);
    } else if !named {
        if !portrait && !landscape {
            return;
        }
        (w, h) = page_named_size(b"a4").unwrap_or((0.0, 0.0));
    }
    if !(w > 0.0 && h > 0.0) {
        return;
    }
    if landscape && w < h {
        (w, h) = (h, w);
    }
    if portrait && w > h {
        (w, h) = (h, w);
    }
    pr.width = w;
    pr.height = h;
    pr.has_size = 1;
    pr.landscape = i32::from(w > h);
}

fn page_apply_margin(pr: &mut PageRule, text: &[u8]) {
    let mut v = [0.0; 4];
    let mut n: i32 = 0;
    for part in page_words(text) {
        if n >= 4 {
            break;
        }
        match page_length_px(part) {
            Some(px) => {
                v[n as usize] = px;
                n += 1;
            }
            None => {
                n = -1;
                break;
            }
        }
    }
    if n < 1 {
        return;
    }
    let top = v[0];
    let right = if n > 1 { v[1] } else { top };
    let bottom = if n > 2 { v[2] } else { top };
    let left = if n > 3 { v[3] } else { right };
    pr.margin = [top, right, bottom, left];
    pr.has_margin = [1; 4];
}

fn page_block(pr: &mut PageRule, body: &[u8]) {
    const SIDES: [&[u8]; 4] = [
        b"margin-top",
        b"margin-right",
        b"margin-bottom",
        b"margin-left",
    ];
    let end = body.len();
    let mut p = 0;
    while p < end {
        let (decl_end, term) = scan_until(body, p, end, b";}");
        let line = strip(&body[p..decl_end]);
        let (colon, colon_term) = scan_until(line, 0, line.len(), b":");
        if colon_term == b':' && line.first() != Some(&b'@') {
            let name = strip(&line[..colon]);
            let value = strip(&line[colon + 1..]);
            if eq(name, b"size") {
                page_apply_size(pr, value);
            } else if eq(name, b"margin") {
                page_apply_margin(pr, value);
            } else {
                for (i, side) in SIDES.iter().enumerate() {
                    if !eq(name, side) {
                        continue;
                    }
                    if let Some(px) = page_length_px(value) {
                        pr.margin[i] = px;
                        pr.has_margin[i] = 1;
                    }
                }
            }
        }
        if term == 0 {
            break;
        }
        p = decl_end + 1;
    }
}

pub(crate) fn layer_join(parent: Option<&[u8]>, child: &[u8]) -> Vec<u8> {
    match parent {
        Some(parent) if !parent.is_empty() && !child.is_empty() => [parent, b".", child].concat(),
        Some(parent) if !parent.is_empty() => parent.to_vec(),
        _ => child.to_vec(),
    }
}

fn layer_anonymous(sheet: &mut SheetBuilder, current: Option<&[u8]>) -> Vec<u8> {
    let leaf = format!("@anon:{}:{}", sheet.serial(), sheet.layer_count()).into_bytes();
    let full = layer_join(current, &leaf);
    sheet.layer_register(&full);
    full
}

fn layer_name_from_range(
    sheet: &mut SheetBuilder,
    current: Option<&[u8]>,
    range: &[u8],
) -> Vec<u8> {
    let name = trim_range(range, 0, range.len());
    if name.is_empty() {
        return layer_anonymous(sheet, current);
    }
    let full = match current {
        Some(current) => layer_join(Some(current), name),
        None => name.to_vec(),
    };
    sheet.layer_register(&full);
    full
}

fn layer_register_list(sheet: &mut SheetBuilder, current: Option<&[u8]>, range: &[u8]) {
    let end = range.len();
    let mut p = 0;
    while p < end {
        let (item_end, term) = scan_until(range, p, end, b",");
        let name = trim_range(range, p, item_end);
        if !name.is_empty() {
            let full = match current {
                Some(current) => layer_join(Some(current), name),
                None => name.to_vec(),
            };
            sheet.layer_register(&full);
        }
        p = if term == b',' { item_end + 1 } else { item_end };
    }
}

fn at_keyword(s: &[u8], p: usize, kw: &[u8]) -> bool {
    let end = s.len();
    if end - p.min(end) < kw.len() || !s[p..p + kw.len()].eq_ignore_ascii_case(kw) {
        return false;
    }
    if p + kw.len() == end {
        return true;
    }
    let c = s[p + kw.len()];
    is_ws(c) || matches!(c, b'(' | b';' | b',')
}

fn import_url(s: &[u8], p: &mut usize) -> Option<Vec<u8>> {
    let end = s.len();
    let mut q = skip_ws_comments(s, *p, end);
    let url;
    if q + 4 <= end && s[q..q + 4].eq_ignore_ascii_case(b"url(") {
        q = skip_ws_comments(s, q + 4, end);
        let mut quote = 0u8;
        if q < end && (s[q] == b'"' || s[q] == b'\'') {
            quote = s[q];
            q += 1;
        }
        let start = q;
        if quote != 0 {
            while q < end {
                if s[q] == b'\\' && q + 1 < end {
                    q += 2;
                } else if s[q] == quote {
                    break;
                } else {
                    q += 1;
                }
            }
        } else {
            while q < end && s[q] != b')' && !is_ws(s[q]) {
                q += 1;
            }
        }
        url = Some(s[start..q.min(end)].to_vec());
        if quote != 0 && q < end && s[q] == quote {
            q += 1;
        }
        q = skip_ws_comments(s, q, end);
        if q < end && s[q] == b')' {
            q += 1;
        }
    } else if q < end && (s[q] == b'"' || s[q] == b'\'') {
        let quote = s[q];
        q += 1;
        let start = q;
        while q < end {
            if s[q] == b'\\' && q + 1 < end {
                q += 2;
            } else if s[q] == quote {
                break;
            } else {
                q += 1;
            }
        }
        url = Some(s[start..q.min(end)].to_vec());
        if q < end && s[q] == quote {
            q += 1;
        }
    } else {
        url = None;
    }
    *p = q;
    url.map(|url| strip(&url).to_vec())
}

fn layer_function(sheet: &mut SheetBuilder, s: &[u8], p: &mut usize) -> Option<Vec<u8>> {
    let end = s.len();
    if !at_keyword(s, *p, b"layer") {
        return None;
    }
    let mut q = skip_ws_comments(s, *p + 5, end);
    if q < end && s[q] == b'(' {
        q += 1;
        let start = q;
        let mut depth = 1;
        while q < end && depth > 0 {
            if s[q] == b'(' {
                depth += 1;
            } else if s[q] == b')' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            q += 1;
        }
        let name = layer_name_from_range(sheet, None, &s[start..q]);
        if q < end && s[q] == b')' {
            q += 1;
        }
        *p = q;
        return Some(name);
    }
    *p = q;
    Some(layer_anonymous(sheet, None))
}

fn import_prelude(sheet: &mut SheetBuilder, current: Option<&[u8]>, prelude: &[u8]) {
    let mut p = 0;
    let Some(url) = import_url(prelude, &mut p).filter(|url| !url.is_empty()) else {
        return;
    };
    let mut layer: Option<Vec<u8>> = None;
    while p < prelude.len() {
        p = skip_ws_comments(prelude, p, prelude.len());
        if !at_keyword(prelude, p, b"layer") {
            break;
        }
        if let Some(parsed) = layer_function(sheet, prelude, &mut p) {
            layer = Some(parsed);
        }
    }
    if let Some(current) = current {
        layer = Some(match layer {
            Some(layer) => layer_join(Some(current), &layer),
            None => current.to_vec(),
        });
    }
    let media = trim_range(prelude, p.min(prelude.len()), prelude.len());
    sheet.add_import(
        &url,
        layer.as_deref(),
        Some(media).filter(|m| !m.is_empty()),
    );
}

fn scope_keyword_at(s: &[u8], p: usize, kw: &[u8]) -> bool {
    let end = s.len();
    end - p.min(end) >= kw.len()
        && s[p..p + kw.len()].eq_ignore_ascii_case(kw)
        && (p + kw.len() == end || !is_ident(s[p + kw.len()]))
}

pub(crate) fn scope_group_valid(text: &[u8]) -> Option<Vec<selector::Selector>> {
    let group = selector::parse_group(text, 0, false);
    (!group.is_empty() && group.iter().all(|sel| sel.pseudo_element == PE_NONE)).then_some(group)
}

fn scope_text_valid(scope: &ScopeText) -> bool {
    if scope_group_valid(scope.start.as_deref().unwrap_or(b":root")).is_none() {
        return false;
    }
    scope
        .end
        .as_deref()
        .is_none_or(|end| scope_group_valid(end).is_some())
}

fn scope_paren(s: &[u8], p: usize) -> Option<(Vec<u8>, usize)> {
    let (close, term) = scan_until(s, p + 1, s.len(), b")");
    if term != b')' {
        return None;
    }
    let inner = trim_range(s, p + 1, close);
    (!inner.is_empty()).then(|| (inner.to_vec(), close + 1))
}

fn scope_from_prelude(prelude: &[u8]) -> Option<ScopeText> {
    let end = prelude.len();
    let mut p = skip_ws_comments(prelude, 0, end);
    let mut scope = ScopeText {
        start: None,
        end: None,
    };
    if p < end && prelude[p] == b'(' {
        let (start, next) = scope_paren(prelude, p)?;
        scope.start = Some(start);
        p = next;
    }
    p = skip_ws_comments(prelude, p, end);
    if scope_keyword_at(prelude, p, b"to") {
        p = skip_ws_comments(prelude, p + 2, end);
        if p >= end || prelude[p] != b'(' {
            return None;
        }
        let (limit, next) = scope_paren(prelude, p)?;
        scope.end = Some(limit);
        p = next;
    }
    p = skip_ws_comments(prelude, p, end);
    (p >= end && scope_text_valid(&scope)).then_some(scope)
}

fn segment_has_scope_marker(s: &[u8]) -> bool {
    let end = s.len();
    let mut quote = 0u8;
    let (mut paren, mut bracket) = (0u32, 0u32);
    let mut p = 0;
    while p < end {
        let c = s[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                p += 2;
            } else {
                if c == quote {
                    quote = 0;
                }
                p += 1;
            }
            continue;
        }
        if c == b'/' && p + 1 < end && s[p + 1] == b'*' {
            p = skip_comment(s, p, end);
            continue;
        }
        if c == b'\\' && p + 1 < end {
            p += 2;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
            p += 1;
            continue;
        }
        match c {
            b'[' => bracket += 1,
            b']' if bracket > 0 => bracket -= 1,
            b'(' => paren += 1,
            b')' if paren > 0 => paren -= 1,
            _ => {}
        }
        if bracket == 0 && c == b'&' {
            return true;
        }
        if bracket == 0
            && c == b':'
            && end - p >= 6
            && s[p + 1..p + 6].eq_ignore_ascii_case(b"scope")
            && (p + 6 == end || !is_ident(s[p + 6]))
        {
            return true;
        }
        p += 1;
    }
    false
}

fn append_amp_as_scope(out: &mut Vec<u8>, s: &[u8]) {
    let end = s.len();
    let mut quote = 0u8;
    let mut bracket = 0u32;
    let mut p = 0;
    while p < end {
        let c = s[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                out.extend_from_slice(&s[p..p + 2]);
                p += 2;
                continue;
            }
            out.push(c);
            if c == quote {
                quote = 0;
            }
            p += 1;
            continue;
        }
        if c == b'/' && p + 1 < end && s[p + 1] == b'*' {
            let q = skip_comment(s, p, end);
            out.extend_from_slice(&s[p..q]);
            p = q;
            continue;
        }
        if c == b'\\' && p + 1 < end {
            out.extend_from_slice(&s[p..p + 2]);
            p += 2;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
            out.push(c);
            p += 1;
            continue;
        }
        if c == b'[' {
            bracket += 1;
        } else if c == b']' && bracket > 0 {
            bracket -= 1;
        }
        if c == b'&' && bracket == 0 {
            out.extend_from_slice(b":scope");
            p += 1;
            continue;
        }
        out.push(c);
        p += 1;
    }
}

fn scoped_selector_list(list: &[u8]) -> Vec<u8> {
    let end = list.len();
    let mut out = Vec::new();
    let mut first = true;
    let mut p = 0;
    while p < end {
        let (seg_end, term) = scan_until(list, p, end, b",");
        let seg = trim_range(list, p, seg_end);
        if !seg.is_empty() {
            if !first {
                out.extend_from_slice(b", ");
            }
            first = false;
            if segment_has_scope_marker(seg) {
                append_amp_as_scope(&mut out, seg);
            } else {
                out.extend_from_slice(b":where(:scope) ");
                out.extend_from_slice(seg);
            }
        }
        p = if term == b',' { seg_end + 1 } else { seg_end };
    }
    out
}

struct Parser<'a> {
    s: &'a [u8],
    sheet: &'a mut SheetBuilder,
    source_order: i32,
    scopes: Vec<ScopeText>,
}

impl Parser<'_> {
    fn condition_block(
        &mut self,
        p: usize,
        layer: Option<&[u8]>,
        matches: impl FnOnce(&[u8]) -> bool,
    ) -> usize {
        let s = self.s;
        let (cond_end, _) = scan_segment(s, p);
        let cond = strip(&s[p..cond_end]);
        let mut p = cond_end;
        if p < s.len() && s[p] == b'{' {
            p += 1;
            if matches(cond) {
                p = self.rules_until(p, b'}', layer);
            } else {
                p = skip_to_block_end(s, p - 1, s.len());
            }
        } else if p < s.len() && s[p] == b';' {
            p += 1;
        }
        p
    }

    fn font_face(&mut self, p: usize) -> usize {
        let s = self.s;
        let end = s.len();
        let (prelude_end, term) = scan_segment(s, p);
        if term != b'{' {
            return if term == b';' {
                prelude_end + 1
            } else {
                prelude_end
            };
        }
        let block_end = skip_to_block_end(s, prelude_end, end);
        let body_start = prelude_end + 1;
        let body = &s[body_start..block_body_end(s, body_start, block_end).max(body_start)];
        let mut family: Option<Vec<u8>> = None;
        let mut src_url: Option<Vec<u8>> = None;
        let mut unicode_range: Option<Vec<u8>> = None;
        let (mut weight, mut slant) = (0, SLANT_AUTO);
        for (prop, val) in descriptors(body) {
            if eq(prop, b"font-family") && family.is_none() {
                let start = val
                    .iter()
                    .position(|&c| !matches!(c, b' ' | b'\'' | b'"'))
                    .unwrap_or(val.len());
                let stop = val
                    .iter()
                    .rposition(|&c| !matches!(c, b' ' | b'\'' | b'"'))
                    .map_or(start, |i| (i + 1).max(start));
                if stop > start {
                    family = Some(val[start..stop].to_vec());
                }
            } else if eq(prop, b"src") {
                font_src_consider_urls(&mut src_url, val);
            } else if eq(prop, b"unicode-range") {
                unicode_range = Some(val.to_vec());
            } else if eq(prop, b"font-weight") {
                weight = font_face_weight(val);
            } else if eq(prop, b"font-style") {
                slant = font_face_slant(val);
            }
        }
        self.sheet.ensure_font_faces();
        if let (Some(family), Some(src_url)) = (family, src_url) {
            if !family.is_empty() && !src_url.is_empty() {
                self.sheet.push_font_face(FontFace {
                    family,
                    src_url,
                    unicode_range,
                    weight,
                    slant,
                });
            }
        }
        block_end
    }

    fn keyframes(&mut self, p: usize) -> usize {
        let s = self.s;
        let end = s.len();
        let (prelude_end, term) = scan_segment(s, p);
        let mut name = keyframes_name(&s[p..prelude_end]);
        let first = p + s[p..prelude_end].iter().take_while(|&&c| is_ws(c)).count();
        let quoted = first < prelude_end && (s[first] == b'"' || s[first] == b'\'');
        if !name.is_empty()
            && !quoted
            && (content::wide_keyword_or_default(&name)
                || eq(&name, b"none")
                || !content::ident_valid(&name))
        {
            name.clear();
        }
        let mut p = prelude_end;
        if term != b'{' {
            return if term == b';' { p + 1 } else { p };
        }
        p += 1;
        let mut stops = Vec::new();
        while p < end {
            p = skip_ws_comments(s, p, end);
            if p < end && s[p] == b'}' {
                p += 1;
                break;
            }
            let sel_start = p;
            let (sel_end, sel_term) = scan_segment(s, p);
            if sel_term != b'{' {
                break;
            }
            let body_start = sel_end + 1;
            let block_end = skip_to_block_end(s, sel_end, end);
            let body_end = block_body_end(s, body_start, block_end).max(body_start);
            p = block_end;
            let sel = strip(&s[sel_start..sel_end]);
            let (base, raw) = keyframe_block(&s[body_start..body_end]);
            let sel_len = sel.len();
            let mut q = 0;
            while q < sel_len {
                let (one_end, term) = scan_until(sel, q, sel_len, b",");
                let one = trim_range(sel, q, one_end);
                if let Some(pct) = keyframe_stop_pct(one) {
                    stops.push(Stop {
                        pct,
                        raw_props: raw.clone(),
                        ..base.clone_data()
                    });
                }
                q = if term == b',' { one_end + 1 } else { one_end };
            }
        }
        if !name.is_empty() {
            self.sheet.push_keyframes(&name, stops);
        }
        p
    }

    fn property(&mut self, p: usize) -> usize {
        let s = self.s;
        let end = s.len();
        let (name_end, term) = scan_segment(s, p);
        let name = trim_range(s, p, name_end);
        let p = name_end;
        if term == b'{' && name.starts_with(b"--") && name.len() > 2 {
            let block_end = skip_to_block_end(s, p, end);
            let body_start = p + 1;
            let body = &s[body_start..block_body_end(s, body_start, block_end).max(body_start)];
            let mut initial: Option<Vec<u8>> = None;
            let mut syntax_text: Option<Vec<u8>> = None;
            let (mut inherits, mut has_inherits, mut has_initial) = (true, false, false);
            for (dprop, dval) in descriptors(body) {
                if eq(dprop, b"inherits") {
                    if eq(dval, b"true") || eq(dval, b"false") {
                        inherits = !eq(dval, b"false");
                        has_inherits = true;
                    }
                } else if eq(dprop, b"initial-value") {
                    initial = Some(dval.to_vec());
                    has_initial = true;
                } else if eq(dprop, b"syntax") {
                    syntax_text = string_descriptor(dval);
                }
            }
            let syntax = syntax_text.as_deref().and_then(SyntaxDef::parse);
            let mut valid = syntax.is_some() && has_inherits;
            if let (true, Some(syntax)) = (valid, &syntax) {
                if !syntax.universal() {
                    valid = has_initial && syntax.initial_valid(initial.as_deref());
                } else if has_initial {
                    valid = syntax.initial_valid(initial.as_deref());
                }
            }
            if let (true, Some(syntax)) = (valid, syntax) {
                self.sheet.push_property_rule(
                    name,
                    initial.as_deref(),
                    syntax_text.as_deref(),
                    syntax,
                    inherits,
                    has_initial,
                );
            }
            return block_end;
        }
        if term == b';' && p < end {
            return p + 1;
        }
        if term == b'{' {
            return skip_to_block_end(s, p, end);
        }
        p
    }

    fn at_rule(&mut self, at_start: usize, layer: Option<&[u8]>) -> usize {
        let s = self.s;
        let end = s.len();
        let mut p = at_start + 1;
        let name = read_ident(s, &mut p, end);
        if name.is_empty() {
            return skip_at_rule(s, at_start);
        }
        if eq(&name, b"import") {
            let (prelude_end, term) = scan_segment(s, p);
            if term == b';' {
                import_prelude(self.sheet, layer, &s[p..prelude_end]);
                return prelude_end + 1;
            }
            return skip_at_rule(s, at_start);
        }
        if eq(&name, b"supports") {
            return self.condition_block(p, layer, |cond| supports::condition(cond, false));
        }
        if eq(&name, b"font-face") {
            return self.font_face(p);
        }
        if eq(&name, b"keyframes") || eq(&name, b"-webkit-keyframes") {
            return self.keyframes(p);
        }
        if eq(&name, b"media") {
            return self.condition_block(p, layer, ffi::media_query_matches);
        }
        if eq(&name, b"container") {
            let (cond_end, _) = scan_segment(s, p);
            let cond = strip(&s[p..cond_end]);
            let canon = container::condition_canonical(cond);
            let mut p = cond_end;
            if p < end && s[p] == b'{' {
                let Some(canon) = canon else {
                    return skip_to_block_end(s, p, end);
                };
                p += 1;
                let before = self.sheet.rules_len();
                p = self.rules_until(p, b'}', layer);
                self.sheet.set_has_container_rules();
                self.sheet.join_container_condition(before, &canon);
            } else if p < end && s[p] == b';' {
                p += 1;
            }
            return p;
        }
        if eq(&name, b"scope") {
            let (prelude_end, term) = scan_segment(s, p);
            if term == b'{' {
                let scope = scope_from_prelude(&s[p..prelude_end]);
                let body = prelude_end + 1;
                return match scope {
                    Some(scope) => {
                        self.scopes.push(scope);
                        let after = self.rules_until(body, b'}', layer);
                        self.scopes.pop();
                        after
                    }
                    None => skip_to_block_end(s, prelude_end, end),
                };
            }
            return if term == b';' {
                prelude_end + 1
            } else {
                prelude_end
            };
        }
        if eq(&name, b"layer") {
            let (prelude_end, term) = scan_segment(s, p);
            return match term {
                b'{' => {
                    let name = layer_name_from_range(self.sheet, layer, &s[p..prelude_end]);
                    self.rules_until(prelude_end + 1, b'}', Some(&name))
                }
                b';' => {
                    layer_register_list(self.sheet, layer, &s[p..prelude_end]);
                    prelude_end + 1
                }
                _ => prelude_end,
            };
        }
        if eq(&name, b"property") {
            return self.property(p);
        }
        if eq(&name, b"page") {
            let (prelude_end, term) = scan_segment(s, p);
            if term == b'{' {
                let block_end = skip_to_block_end(s, prelude_end, end);
                let body_start = prelude_end + 1;
                let body = &s[body_start..block_body_end(s, body_start, block_end).max(body_start)];
                page_block(self.sheet.page_rule(), body);
                return block_end;
            }
            return if term == b';' && prelude_end < end {
                prelude_end + 1
            } else {
                prelude_end
            };
        }
        skip_at_rule(s, at_start)
    }

    fn style_rule(&mut self, p: usize, layer: Option<&[u8]>, nested: bool) -> usize {
        let s = self.s;
        let end = s.len();
        let mut rule = self.sheet.new_rule(layer, self.source_order);
        self.source_order += 1;
        if !rule.apply_scopes(&self.scopes) {
            let (skip_to, term) = scan_segment(s, p);
            return match term {
                b'{' => skip_to_block_end(s, skip_to, end),
                b';' => skip_to + 1,
                _ => skip_to,
            };
        }
        let (sel_end, term) = scan_segment(s, p);
        if term != b'{' {
            return if term == b';' {
                skip_invalid_qualified_rule(s, sel_end + 1, end, nested)
            } else {
                sel_end
            };
        }
        let scoped = rule
            .has_scopes()
            .then(|| scoped_selector_list(&s[p..sel_end]));
        let parsed = rule.parse_selectors(scoped.as_deref().unwrap_or(&s[p..sel_end]));
        if parsed.ok && parsed.has_hover {
            self.sheet.set_has_hover_rules();
        }
        if parsed.ok && parsed.has_active {
            self.sheet.set_has_active_rules();
        }
        if !parsed.ok {
            return skip_to_block_end(s, sel_end, end);
        }
        let after = rule.parse_declarations(s, sel_end + 1);
        self.sheet.push_rule(rule);
        after
    }

    fn rules_until(&mut self, mut p: usize, close_at: u8, layer: Option<&[u8]>) -> usize {
        let s = self.s;
        let end = s.len();
        let nested = close_at == b'}';
        if nested {
            if AT_DEPTH.load(Ordering::Relaxed) >= MAX_AT_NESTING {
                let (seg, term) = scan_until(s, p, end, b"}");
                return if term == b'}' { seg + 1 } else { seg };
            }
            AT_DEPTH.fetch_add(1, Ordering::Relaxed);
        }
        while p < end {
            p = skip_ws_comments(s, p, end);
            if p >= end {
                break;
            }
            if !nested && p + 4 <= end && &s[p..p + 4] == b"<!--" {
                p += 4;
                continue;
            }
            if !nested && p + 3 <= end && &s[p..p + 3] == b"-->" {
                p += 3;
                continue;
            }
            if s[p] == b'}' {
                if nested {
                    p += 1;
                    break;
                }
                p = skip_invalid_qualified_rule(s, p + 1, end, false);
                continue;
            }
            if s[p] == b'@' {
                p = self.at_rule(p, layer);
                continue;
            }
            p = self.style_rule(p, layer, nested);
        }
        if nested {
            AT_DEPTH.fetch_sub(1, Ordering::Relaxed);
        }
        p
    }
}

impl Stop {
    fn clone_data(&self) -> Stop {
        Stop {
            pct: self.pct,
            opacity: self.opacity,
            has_opacity: self.has_opacity,
            transform: self.transform,
            has_transform: self.has_transform,
            color: self.color,
            has_color: self.has_color,
            bg_color: self.bg_color,
            has_bg_color: self.has_bg_color,
            raw_props: None,
        }
    }
}

pub(crate) fn parse_into(sheet: &mut SheetBuilder, flattened: &[u8]) {
    let mut parser = Parser {
        s: flattened,
        sheet,
        source_order: 0,
        scopes: Vec::new(),
    };
    parser.rules_until(0, 0, None);
}

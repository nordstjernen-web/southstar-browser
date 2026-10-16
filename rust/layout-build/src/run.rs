//! Southstar — building an inline run box: collecting its nodes, collapsing white space, remapping links and attributes onto the collapsed text, and the parent's text-decoration and ::first-letter styling.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_layout::{InlineAttr, NsBox, inline_kind};
use southstar_layout_content::controls::{FontKeyword, font_kerning, font_keyword};
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef};

use crate::collect::{
    Collector, LINE_SEPARATOR, WS_COLLAPSE, WS_PRE_LINE, WS_PRESERVE, contains, has_small_caps,
    white_space_mode,
};
use crate::ffi::{self, RawLink, RunParts};
use southstar_layout_content::text_input::utf8_skip;

struct Collapsed {
    text: Vec<u8>,
    map: Vec<usize>,
}

fn collapse(c: &Collector<'_>, run_ws_mode: i32) -> Collapsed {
    let buf = &c.out;
    let mut text = Vec::with_capacity(buf.len());
    let mut map = vec![0; buf.len() + 1];
    let mut prev_ws = run_ws_mode != WS_PRESERVE;
    let mut trailing_preserved = false;
    let mut ends_in_collapsed_space = false;
    let mut separator_bytes_left = 0;
    let mut ri = 0;
    for (i, &ch) in buf.iter().enumerate() {
        if separator_bytes_left > 0 {
            separator_bytes_left -= 1;
            map[i] = text.len();
            text.push(ch);
            if separator_bytes_left == 0 {
                prev_ws = true;
            }
            continue;
        }
        while ri < c.ws_ranges.len() && c.ws_ranges[ri].end <= i {
            ri += 1;
        }
        let mut ws_mode = run_ws_mode;
        if ri < c.ws_ranges.len() && c.ws_ranges[ri].start <= i {
            ws_mode = c.ws_ranges[ri].mode;
        }
        trailing_preserved = ws_mode == WS_PRESERVE;
        let forced_break = (ws_mode == WS_PRE_LINE && ch == b'\n')
            || (ws_mode != WS_PRESERVE && buf[i..].starts_with(LINE_SEPARATOR));
        if forced_break && ends_in_collapsed_space {
            text.pop();
            let len = text.len();
            for m in map[..i].iter_mut().rev().take_while(|m| **m > len) {
                *m = len;
            }
        }
        map[i] = text.len();
        if ws_mode == WS_PRESERVE {
            text.push(ch);
            prev_ws = ch == b'\n';
            ends_in_collapsed_space = false;
            continue;
        }
        if ws_mode == WS_PRE_LINE && ch == b'\n' {
            text.push(b'\n');
            prev_ws = true;
            ends_in_collapsed_space = false;
            continue;
        }
        if forced_break {
            text.push(ch);
            separator_bytes_left = LINE_SEPARATOR.len() - 1;
            ends_in_collapsed_space = false;
            continue;
        }
        if matches!(ch, b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
            if !prev_ws {
                text.push(b' ');
                ends_in_collapsed_space = true;
            }
            prev_ws = true;
        } else {
            text.push(ch);
            prev_ws = false;
            ends_in_collapsed_space = false;
        }
    }
    map[buf.len()] = text.len();
    if !trailing_preserved && text.last() == Some(&b' ') {
        text.pop();
    }
    Collapsed { text, map }
}

fn remap(start: usize, len: usize, buf_len: usize, c: &Collapsed) -> Option<(usize, usize)> {
    let start = start.min(buf_len);
    let end = (start + len).min(buf_len);
    let ns = c.map[start];
    let ne = c.map[end].min(c.text.len());
    (ne > ns).then(|| (ns, ne - ns))
}

fn decoration_attrs(ps: StyleRef<'_>, len: usize, out: &mut Vec<InlineAttr>) {
    let Some(kw) = ps
        .get(PropId::TextDecoration)
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes)
    else {
        return;
    };
    let invisible = ps
        .get(PropId::TextDecorationColor)
        .and_then(ValueRef::color)
        .is_some_and(|c| c[3] == 0);
    if contains(kw, b"none") || invisible {
        return;
    }
    for (word, kind) in [
        (&b"underline"[..], inline_kind::UNDERLINE),
        (b"overline", inline_kind::OVERLINE),
        (b"line-through", inline_kind::STRIKETHROUGH),
    ] {
        if contains(kw, word) {
            out.push(InlineAttr::new(kind, 0, len).with_style(ps.as_ptr()));
        }
    }
}

fn first_letter_attrs(fl: StyleRef<'_>, text: &[u8], out: &mut Vec<InlineAttr>) {
    let mut i = 0;
    while i < text.len() && matches!(text[i], b' ' | b'\t' | b'\n') {
        i += 1;
    }
    while i < text.len() {
        if ffi::unichar_isalnum(ffi::utf8_get_char(text, i)) {
            break;
        }
        i += utf8_skip(text[i]);
    }
    if i >= text.len() {
        return;
    }
    let start = i;
    let end = (i + utf8_skip(text[i])).min(text.len());
    let len = end - start;
    if let Some((v, 0)) = fl.get(PropId::FontSize).and_then(ValueRef::length) {
        let mut a = InlineAttr::new(inline_kind::FONT_SIZE, start, len);
        a.font_size_px = v;
        out.push(a);
    }
    for (prop, kind) in [
        (PropId::Color, inline_kind::COLOR),
        (PropId::BackgroundColor, inline_kind::BG_COLOR),
    ] {
        if let Some(c) = fl.get(prop).and_then(ValueRef::color) {
            let mut a = InlineAttr::new(kind, start, len);
            [a.r, a.g, a.b, a.a] = c;
            out.push(a);
        }
    }
    let weight = fl
        .get(PropId::FontWeight)
        .map_or(-1, |v| v.font_weight_or(-1));
    if weight > 0 {
        let mut a = InlineAttr::new(inline_kind::FONT_WEIGHT, start, len);
        a.font_weight = weight;
        out.push(a);
    }
    let fst = fl.get(PropId::FontStyle);
    if fst.is_some_and(ValueRef::font_slanted) {
        out.push(InlineAttr::new(inline_kind::ITALIC, start, len));
    }
    if let Some(fam) = fl
        .get(PropId::FontFamily)
        .filter(|v| v.kind() == southstar_style::Kind::Keyword)
    {
        out.push(
            InlineAttr::new(inline_kind::FONT_FAMILY, start, len)
                .with_family(fam.keyword_text().map_or(ptr::null(), CStr::as_ptr)),
        );
    }
    let kerning = font_kerning(Some(fl));
    let ligatures = font_keyword(Some(fl), FontKeyword::Ligatures);
    let features = font_keyword(Some(fl), FontKeyword::Features);
    if kerning >= 0 || ligatures.is_some() || features.is_some() {
        let mut a = InlineAttr::new(inline_kind::FONT_FEATURES, start, len).with_font_settings(
            ligatures.map_or(ptr::null(), CStr::as_ptr),
            features.map_or(ptr::null(), CStr::as_ptr),
            ptr::null(),
        );
        a.font_kerning = kerning;
        out.push(a);
    }
    if let Some(v) = font_keyword(Some(fl), FontKeyword::Variations) {
        out.push(
            InlineAttr::new(inline_kind::FONT_VARIATIONS, start, len).with_font_settings(
                ptr::null(),
                ptr::null(),
                v.as_ptr(),
            ),
        );
    }
}

pub fn build_inline_run(
    first: Option<Node<'_>>,
    last_excl: *const NsNode,
    styles: StyleTable,
    abs_placeholders: bool,
) -> *mut NsBox {
    let mut c = Collector::new(styles, abs_placeholders);
    let parent = first.and_then(Node::parent);
    let parent_style = parent.and_then(|p| styles.get(p));
    if let Some(kw) = parent_style
        .and_then(|ps| ps.get(PropId::TextTransform))
        .and_then(ValueRef::keyword_text)
        .filter(|kw| kw.to_bytes() != b"none")
    {
        c.text_transform = Some(kw);
    }
    let first_child_of_parent = first.zip(parent).is_some_and(|(f, p)| {
        p.first_child()
            .is_some_and(|fc| ptr::eq(fc.as_ptr(), f.as_ptr()))
    });
    if let Some(p) = parent.filter(|p| p.is_element() && first_child_of_parent)
        && let Some(marker) = ffi::li_marker(p, parent_style)
    {
        c.push(&marker);
    }
    let depth = c.state.inline_collect_depth;
    let body_start = c.out.len();
    let mut n = first;
    while let Some(node) = n.filter(|node| !ptr::eq(node.as_ptr(), last_excl)) {
        n = node.next_sibling();
        c.walk(node, depth);
    }
    if let Some(ps) = parent_style
        && ps
            .get(PropId::FontVariant)
            .and_then(ValueRef::keyword_text)
            .is_some_and(|kw| has_small_caps(kw.to_bytes()))
        && let Some((size, 0)) = ps.get(PropId::FontSize).and_then(ValueRef::length)
    {
        let end = c.out.len();
        c.synthesize_small_caps(body_start, end, size);
    }

    let run_ws_mode = first.map_or(WS_COLLAPSE, |f| white_space_mode(f, styles));
    let collapsed = collapse(&c, run_ws_mode);
    let buf_len = c.out.len();

    let links = c
        .links
        .iter()
        .filter_map(|r| {
            remap(r.start, r.len, buf_len, &collapsed).map(|(start, len)| RawLink {
                start,
                len,
                href: r.href,
                target: r.target,
                dom: r.dom,
            })
        })
        .collect();

    let mut attrs = Vec::with_capacity(c.attrs.len());
    for mut a in core::mem::take(&mut c.attrs) {
        if let Some((start, len)) = remap(a.start, a.len, buf_len, &collapsed) {
            a.start = start;
            a.len = len;
            attrs.push(a);
        }
    }

    if let Some(ps) = parent_style.filter(|_| !collapsed.text.is_empty()) {
        decoration_attrs(ps, collapsed.text.len(), &mut attrs);
        if first_child_of_parent && let Some(fl) = ps.first_letter() {
            first_letter_attrs(fl, &collapsed.text, &mut attrs);
        }
    }

    let atomics = c
        .atomics
        .iter()
        .map(|&(start, b)| {
            (
                collapsed.map[start.min(buf_len)].min(collapsed.text.len()),
                b,
            )
        })
        .collect();

    ffi::new_inline_run(RunParts {
        text: collapsed.text,
        links,
        attrs,
        atomics,
    })
}

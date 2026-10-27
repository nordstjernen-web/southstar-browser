//! Southstar — the boxes ::before and ::after generate: inline runs with their colour and font attributes and edge spacers, atomic and block-level pseudo boxes, absolutely positioned pseudos, and generated text appended to a block's last run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::ptr;

use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxPool, BoxRef, InlineAtomic, InlineAttr, inline_kind};
use southstar_layout_content::controls::{
    FontKeyword, font_kerning, font_keyword, keyword_is, length_or,
};
use southstar_layout_content::pseudo;
use southstar_layout_geom::{OUTER_INLINE, is_atomic_inline, style_is_absolute_or_fixed};
use southstar_layout_tree::append_child;
use southstar_style::{PropId, StyleRef, ValueRef, display_of};

use crate::collect::OBJECT_REPLACEMENT;
use crate::ffi;
use crate::media::{KIND_BLOCK, collect_box_bg_image};
use crate::state::{self, AbsEntry};

fn text_collapses(ps: StyleRef<'_>) -> bool {
    ps.keyword_of(PropId::WhiteSpace)
        .is_none_or(|ws| matches!(ws.to_bytes(), b"normal" | b"nowrap"))
}

fn block_text(ps: StyleRef<'_>, txt: &[u8]) -> Vec<u8> {
    if !text_collapses(ps) {
        return txt.to_vec();
    }
    let mut out = Vec::with_capacity(txt.len());
    for word in txt
        .split(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
        .filter(|w| !w.is_empty())
    {
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(word);
    }
    out
}

pub fn pseudo_block_with_text(ps: StyleRef<'_>, txt: Option<&[u8]>) -> BoxRef<'static> {
    let block = BoxPool::new_box(KIND_BLOCK);
    block.set_style(ps.as_ptr());
    collect_box_bg_image(block, Some(ps));
    let text = txt.map(|t| block_text(ps, t)).unwrap_or_default();
    if !text.is_empty() {
        let run = BoxPool::new_inline();
        run.set_text_bytes(&text);
        append_child(block, run);
    }
    block
}

fn add_edge_spacers(b: BoxRef<'_>, ps: StyleRef<'_>) {
    let edges = [
        length_or(ps.get(PropId::MarginLeft), 0.0),
        length_or(ps.get(PropId::PaddingLeft), 0.0),
        length_or(ps.get(PropId::PaddingRight), 0.0),
        length_or(ps.get(PropId::MarginRight), 0.0),
    ];
    let slen = OBJECT_REPLACEMENT.len();
    let lead = if edges[0] > 0.0 { slen } else { 0 } + if edges[1] > 0.0 { slen } else { 0 };
    if lead == 0 && !positive(edges[2]) && !positive(edges[3]) {
        return;
    }
    let mut attrs = b.take_attrs().unwrap_or_default();
    for a in &mut attrs {
        a.start += lead;
    }
    let body = b.text_bytes().to_vec();
    let mut text = Vec::with_capacity(body.len() + 4 * slen);
    for (e, &w) in edges.iter().enumerate() {
        if e == 2 {
            text.extend_from_slice(&body);
        }
        if !positive(w) {
            continue;
        }
        let mut a = InlineAttr::new(inline_kind::SPACER, text.len(), slen);
        a.box_w = w;
        text.extend_from_slice(OBJECT_REPLACEMENT);
        attrs.push(a);
    }
    b.put_attrs(Some(&attrs));
    b.set_text_bytes(&text);
}

enum Content {
    Text(Vec<u8>),
    Suppressed,
}

fn resolve_with_quotes(ps: StyleRef<'_>, resolved: Vec<u8>) -> Content {
    match resolved.as_slice() {
        b"open-quote" => Content::Text(pseudo::quotes_string(Some(ps), 0, false)),
        b"close-quote" => Content::Text(pseudo::quotes_string(Some(ps), 0, true)),
        b"no-open-quote" | b"no-close-quote" => Content::Suppressed,
        _ => Content::Text(resolved),
    }
}

fn content_keyword<'a>(ps: StyleRef<'a>) -> Option<&'a CStr> {
    ps.get(PropId::Content).and_then(ValueRef::keyword_text)
}

pub fn build_pseudo_inline_for(
    ps: Option<StyleRef<'_>>,
    host: Option<Node<'_>>,
) -> Option<BoxRef<'static>> {
    let ps = ps?;
    if !pseudo::generates_box(Some(ps)) {
        return None;
    }
    let kw = content_keyword(ps)?;
    let inline_atomic = is_atomic_inline(display_of(Some(ps)));
    let resolved = match pseudo::resolve_content(kw.to_bytes(), host) {
        Some(r) => r,
        None if inline_atomic => Vec::new(),
        None => return None,
    };
    let Content::Text(txt) = resolve_with_quotes(ps, resolved) else {
        return None;
    };
    if inline_atomic {
        let inner = pseudo_block_with_text(ps, Some(&txt));
        let run = BoxPool::new_inline();
        run.set_style(ps.as_ptr());
        run.set_text_bytes(OBJECT_REPLACEMENT);
        inner.set_parent(Some(run));
        run.put_atomics(Some(&[InlineAtomic::new(0, inner)]));
        return Some(run);
    }
    let b = BoxPool::new_inline();
    b.set_text_bytes(&txt);
    b.set_style(ps.as_ptr());
    let tlen = txt.iter().position(|&c| c == 0).unwrap_or(txt.len());
    let mut attrs = b.take_attrs().unwrap_or_default();
    let invisible = ps
        .keyword_of(PropId::Visibility)
        .is_some_and(|v| matches!(v.to_bytes(), b"hidden" | b"collapse"));
    let color = ps.text_fill_color().and_then(ValueRef::color);
    if invisible || color.is_some() {
        let c = color.unwrap_or([0, 0, 0, 0]);
        let mut a = InlineAttr::new(inline_kind::COLOR, 0, tlen);
        [a.r, a.g, a.b] = [c[0], c[1], c[2]];
        a.a = if invisible { 0 } else { c[3] };
        attrs.push(a);
    }
    if let Some(c) = ps.get(PropId::BackgroundColor).and_then(ValueRef::color) {
        let mut a = InlineAttr::new(inline_kind::BG_COLOR, 0, tlen);
        [a.r, a.g, a.b, a.a] = c;
        attrs.push(a);
    }
    if let Some((v, 0)) = ps.get(PropId::FontSize).and_then(ValueRef::length) {
        let mut a = InlineAttr::new(inline_kind::FONT_SIZE, 0, tlen);
        a.font_size_px = v;
        attrs.push(a);
    }
    let weight = ps
        .get(PropId::FontWeight)
        .map_or(-1, |v| v.font_weight_or(-1));
    if weight > 0 {
        let mut a = InlineAttr::new(inline_kind::FONT_WEIGHT, 0, tlen);
        a.font_weight = weight;
        attrs.push(a);
    }
    if let Some(st) = ps.get(PropId::FontStretch) {
        let mut a = InlineAttr::new(inline_kind::FONT_STRETCH, 0, tlen);
        a.font_stretch = ffi::font_stretch_rank(Some(st));
        attrs.push(a);
    }
    let kerning = font_kerning(Some(ps));
    let ligatures = font_keyword(Some(ps), FontKeyword::Ligatures);
    let features = font_keyword(Some(ps), FontKeyword::Features);
    if kerning >= 0 || ligatures.is_some() || features.is_some() {
        let mut a = InlineAttr::new(inline_kind::FONT_FEATURES, 0, tlen)
            .with_font_settings(ligatures, features, None);
        a.font_kerning = kerning;
        attrs.push(a);
    }
    if let Some(v) = font_keyword(Some(ps), FontKeyword::Variations) {
        attrs.push(
            InlineAttr::new(inline_kind::FONT_VARIATIONS, 0, tlen).with_font_settings(
                None,
                None,
                Some(v),
            ),
        );
    }
    let fst = ps.get(PropId::FontStyle);
    if fst.is_some_and(ValueRef::font_slanted) {
        attrs.push(InlineAttr::new(inline_kind::ITALIC, 0, tlen));
    }
    b.put_attrs(Some(&attrs));
    add_edge_spacers(b, ps);
    Some(b)
}

pub fn pseudo_item_block(run: BoxRef<'static>, ps: Option<StyleRef<'_>>) -> BoxRef<'static> {
    let item = BoxPool::new_box(KIND_BLOCK);
    item.set_style(ps.map_or(ptr::null(), StyleRef::as_ptr));
    collect_box_bg_image(item, ps);
    if let Some(mut attrs) = run.take_attrs() {
        attrs.retain(|a| a.kind != inline_kind::BG_COLOR);
        run.put_attrs(Some(&attrs));
    }
    append_child(item, run);
    item
}

pub fn build_pseudo_block_for(
    ps: Option<StyleRef<'_>>,
    host: Option<Node<'_>>,
) -> Option<BoxRef<'static>> {
    let ps = ps?;
    if style_is_absolute_or_fixed(Some(ps)) || !pseudo::generates_box(Some(ps)) {
        return None;
    }
    let kw = content_keyword(ps)?;
    let d = display_of(Some(ps));
    if ps.get(PropId::Display).is_none() || d.is_none() || d.outer == OUTER_INLINE {
        return None;
    }
    let resolved = if kw.is_empty() {
        Vec::new()
    } else {
        pseudo::resolve_content(kw.to_bytes(), host)?
    };
    let txt = match resolve_with_quotes(ps, resolved) {
        Content::Text(t) => t,
        Content::Suppressed => Vec::new(),
    };
    Some(pseudo_block_with_text(ps, Some(&txt)))
}

pub fn register_abs_pseudo(host: Option<Node<'_>>, ps: Option<StyleRef<'_>>) {
    let Some(ps) = ps else {
        return;
    };
    if state::with(|st| st.abs_pending.is_none()) || !style_is_absolute_or_fixed(Some(ps)) {
        return;
    }
    if !pseudo::generates_box(Some(ps)) {
        return;
    }
    let fixed = keyword_is(ps.get(PropId::Position), c"fixed");
    let host_ptr = Node::ptr_or_null(host);
    state::with(|st| {
        if let Some(pending) = st.abs_pending.as_mut()
            && !pending
                .iter()
                .any(|e| e.dom == host_ptr && e.pseudo == ps.as_ptr())
        {
            pending.push(AbsEntry {
                dom: Node::ptr_or_null(host),
                pseudo: ps.as_ptr(),
                fixed,
            });
        }
    });
}

pub fn append_generated_after(block: BoxRef<'_>, generated: BoxRef<'static>) {
    let Some(last) = block.last_child().filter(|l| l.kind() == BoxKind::Inline) else {
        append_child(block, generated);
        return;
    };
    let ll = last.text().map_or(0, |t| t.to_bytes().len());
    let mut combined = last.text_bytes().to_vec();
    combined.extend_from_slice(&generated.text_bytes());
    last.set_text_bytes(&combined);
    let gen_attrs = generated.attrs().to_vec();
    if !gen_attrs.is_empty() {
        let mut attrs = last.take_attrs().unwrap_or_default();
        for mut a in gen_attrs {
            a.start += ll;
            attrs.push(a);
        }
        last.put_attrs(Some(&attrs));
    }
    if let Some(gen_atomics) = generated.take_atomics() {
        let mut atomics = last.take_atomics().unwrap_or_default();
        for mut ia in gen_atomics {
            ia.set_byte_off(ia.byte_off() + ll);
            if let Some(child) = ia.box_handle() {
                child.set_parent(Some(last));
            }
            atomics.push(ia);
        }
        last.put_atomics(Some(&atomics));
    }
    BoxPool::free_tree(Some(generated));
}

fn positive(x: f64) -> bool {
    x > 0.0
}

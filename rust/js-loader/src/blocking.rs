//! Southstar — whether a script or stylesheet link blocks rendering, and the frame whose document an element belongs to, for resource timing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Kind, Node};

use crate::ffi::is_named;
use crate::{FLAG_NOT_PARSER_INSERTED, type_is_module};

const FLAG_FRAGMENT: u32 = 1 << 2;

fn has_token(el: Node<'_>, attr: &CStr, token: &[u8]) -> bool {
    el.attr(attr).is_some_and(|value| {
        value
            .to_bytes()
            .split(|c| b" \t\n\x0c\r".contains(c))
            .any(|part| part.eq_ignore_ascii_case(token))
    })
}

fn in_head(el: Node<'_>) -> bool {
    southstar_dom::ancestors(el)
        .take_while(|p| p.kind() == Kind::Element)
        .any(|p| is_named(p, c"head"))
}

fn media_applies_to_screen(media: Option<&CStr>) -> bool {
    match media.map(CStr::to_bytes) {
        None | Some(b"") => true,
        Some(media) => media.eq_ignore_ascii_case(b"all") || media.eq_ignore_ascii_case(b"screen"),
    }
}

pub(crate) fn render_blocking(el: Node<'_>) -> bool {
    if el.kind() != Kind::Element || !in_head(el) {
        return false;
    }
    if has_token(el, c"blocking", b"render") {
        return true;
    }
    if el.flags() & FLAG_NOT_PARSER_INSERTED != 0 {
        return false;
    }
    if is_named(el, c"script") {
        return !type_is_module(el) && el.attr(c"async").is_none() && el.attr(c"defer").is_none();
    }
    if is_named(el, c"link") {
        return has_token(el, c"rel", b"stylesheet")
            && !has_token(el, c"rel", b"alternate")
            && media_applies_to_screen(el.attr(c"media"));
    }
    false
}

pub(crate) fn content_frame(node: Node<'_>) -> Option<Node<'_>> {
    let doc = southstar_dom::ancestors_and_self(node)
        .find(|p| p.kind() == Kind::Document && p.flags() & FLAG_FRAGMENT == 0)?;
    doc.parent().filter(|frame| {
        [c"iframe", c"frame", c"object"]
            .iter()
            .any(|tag| is_named(*frame, tag))
    })
}

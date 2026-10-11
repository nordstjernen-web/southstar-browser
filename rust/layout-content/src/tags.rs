//! Southstar — the element names inline collection styles by default and the elements it never renders.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{FLAG_SCRIPTING_DISABLED, Node};

fn name_in(name: Option<&CStr>, set: &[&[u8]]) -> bool {
    name.is_some_and(|n| set.contains(&n.to_bytes()))
}

pub fn is_bold(name: Option<&CStr>) -> bool {
    name_in(name, &[b"b", b"strong"])
}

pub fn is_italic(name: Option<&CStr>) -> bool {
    name_in(name, &[b"i", b"em", b"cite", b"dfn"])
}

pub fn is_monospace(name: Option<&CStr>) -> bool {
    name_in(name, &[b"code", b"tt", b"kbd", b"samp", b"pre"])
}

pub fn is_underline(name: Option<&CStr>) -> bool {
    name_in(name, &[b"u", b"ins"])
}

pub fn is_strike(name: Option<&CStr>) -> bool {
    name_in(name, &[b"s", b"del", b"strike"])
}

pub fn is_non_rendering(name: Option<&CStr>) -> bool {
    name_in(
        name,
        &[
            b"area",
            b"base",
            b"head",
            b"link",
            b"meta",
            b"noscript",
            b"param",
            b"script",
            b"source",
            b"style",
            b"template",
            b"title",
            b"track",
        ],
    )
}

pub fn node_is_non_rendering(n: Option<Node<'_>>) -> bool {
    let Some(n) = n.filter(|n| is_non_rendering(n.name())) else {
        return false;
    };
    let is_noscript = n
        .name()
        .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"noscript"));
    !(is_noscript && n.root().flags() & FLAG_SCRIPTING_DISABLED != 0)
}

//! Southstar — script, module and stylesheet loading: which scripts run when, import maps, module fetching and the load and error events of script and link elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod fetch;
mod ffi;
mod hold;
mod import_map;
mod page;
mod scan;
mod schedule;

use core::ffi::CStr;

use southstar_dom::Node;

pub(crate) const ALREADY_STARTED: &CStr = c"data-nd-script-already-started";
pub(crate) const EMPTY_SOURCE: &CStr = c"data-nd-script-empty-source";
pub(crate) const MAX_SCRIPT_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const FLAG_LINK_LOAD_FIRED: u32 = 1 << 8;
pub(crate) const FLAG_NOT_PARSER_INSERTED: u32 = 1 << 13;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Schedule {
    Blocking,
    Deferred,
    Async,
}

impl Schedule {
    pub(crate) fn from_raw(raw: i32) -> Option<Schedule> {
        match raw {
            0 => Some(Schedule::Blocking),
            1 => Some(Schedule::Deferred),
            2 => Some(Schedule::Async),
            _ => None,
        }
    }

    pub(crate) fn raw(self) -> i32 {
        match self {
            Schedule::Blocking => 0,
            Schedule::Deferred => 1,
            Schedule::Async => 2,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Task<'a> {
    pub node: Node<'a>,
    pub schedule: Schedule,
}

fn eq_ignore_case(value: Option<&CStr>, want: &[u8]) -> bool {
    value.is_some_and(|value| value.to_bytes().eq_ignore_ascii_case(want))
}

pub(crate) fn type_is_module(n: Node) -> bool {
    eq_ignore_case(n.attr(c"type"), b"module")
}

pub(crate) fn type_supported(n: Node) -> bool {
    match n.attr(c"type").map(CStr::to_bytes) {
        None | Some(b"") => true,
        Some(ty) => [
            &b"text/javascript"[..],
            b"application/javascript",
            b"module",
        ]
        .iter()
        .any(|ok| ty.eq_ignore_ascii_case(ok)),
    }
}

pub(crate) fn content_type_is_javascript(ct: &[u8]) -> bool {
    const OK: [&[u8]; 10] = [
        b"text/javascript",
        b"application/javascript",
        b"application/ecmascript",
        b"text/ecmascript",
        b"application/x-javascript",
        b"text/x-javascript",
        b"application/x-ecmascript",
        b"text/x-ecmascript",
        b"text/jscript",
        b"text/livescript",
    ];
    let start = ct
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(ct.len());
    let ct = &ct[start..];
    let end = ct
        .iter()
        .position(|&b| matches!(b, b';' | b' ' | b'\t'))
        .unwrap_or(ct.len());
    let essence = &ct[..end];
    !essence.is_empty() && OK.iter().any(|ok| essence.eq_ignore_ascii_case(ok))
}

pub(crate) fn skipped_by_nomodule(n: Node) -> bool {
    !type_is_module(n) && n.attr(c"nomodule").is_some()
}

pub(crate) fn schedule_for(n: Node) -> Schedule {
    let is_module = type_is_module(n);
    let has_src = n.attr(c"src").is_some();
    let has_async = n.attr(c"async").is_some();
    let has_defer = n.attr(c"defer").is_some();
    if has_async && (has_src || is_module) {
        Schedule::Async
    } else if is_module || (has_src && has_defer) {
        Schedule::Deferred
    } else {
        Schedule::Blocking
    }
}

pub(crate) fn link_is_loadable_stylesheet(n: Node) -> bool {
    if !ffi::is_named(n, c"link") || n.flags() & FLAG_LINK_LOAD_FIRED != 0 {
        return false;
    }
    let (Some(rel), Some(href)) = (n.attr(c"rel"), n.attr(c"href")) else {
        return false;
    };
    if href.is_empty() {
        return false;
    }
    let mut is_sheet = false;
    let mut is_alt = false;
    for token in rel
        .to_bytes()
        .split(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c))
    {
        if token.eq_ignore_ascii_case(b"stylesheet") {
            is_sheet = true;
        } else if token.eq_ignore_ascii_case(b"alternate") {
            is_alt = true;
        }
    }
    is_sheet && !is_alt
}

pub(crate) fn mark(n: Node, name: &CStr) {
    southstar_dom::attrs::set_len(n, name, Some(b"1"), 1);
}

pub(crate) fn unmark(n: Node, name: &CStr) {
    southstar_dom::attrs::remove(n, name);
}

pub(crate) fn source_is_empty(n: Node) -> bool {
    southstar_dom::children(n).all(|c| !c.is_text() || c.text().is_none_or(CStr::is_empty))
}

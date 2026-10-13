//! Southstar — the page globals a frame's scripts leave behind when they run in the page realm, recorded per frame and scrubbed when the frame's document goes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::Value;

use crate::ffi::ctx::{Ctx, Keys};
use crate::ffi::{self, Js};
use crate::page;
use crate::realm::frame_of;

fn global_names(ctx: Ctx) -> Vec<CString> {
    let global = ctx.global();
    ctx.own_keys(&global, Keys::Strings)
        .unwrap_or_default()
        .iter()
        .filter_map(|key| key.name())
        .collect()
}

pub(crate) fn snapshot(js: Js) -> HashSet<CString> {
    global_names(ffi::current_realm(js)).into_iter().collect()
}

pub(crate) fn record(js: Js, iframe: Node<'_>, before: &HashSet<CString>) {
    let added: Vec<CString> = global_names(ffi::current_realm(js))
        .into_iter()
        .filter(|name| !before.contains(name))
        .collect();
    if added.is_empty() {
        return;
    }
    page::with(js, |page| {
        page.iframe_globals
            .entry(frame_of(iframe))
            .or_default()
            .extend(added)
    });
}

pub(crate) fn scrub(js: Js, iframe: Node<'_>) {
    if js.is_null() {
        return;
    }
    let Some(names) = page::peek(js, |page| {
        page.iframe_globals.get(&frame_of(iframe)).cloned()
    })
    .flatten() else {
        return;
    };
    let ctx = ffi::current_realm(js);
    let global = ctx.global();
    for name in &names {
        if ctx.delete(&global, name) == 0 {
            ctx.set(&global, &name.to_string_lossy(), Value::undefined());
        }
    }
    page::peek_mut(js, |page| page.iframe_globals.remove(&frame_of(iframe)));
}

//! Southstar — the style text a <style> element or adopted shadow sheet contributes: its text joined, dropped when its media query fails, and scoped to the shadow host or framed document it sits in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::sync::atomic::{AtomicI32, Ordering};

use southstar_dom::{Kind, Node, ancestors_and_self, attrs, children};

use crate::ffi::{media_query_matches, scoped_css};

const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";
const HOST_SCOPE_ATTR: &CStr = c"data-nd-host";
const ADOPTED_CSS_ATTR: &CStr = c"data-nd-adopted-css";
const MAX_DEPTH: u32 = 512;

static SCOPE_COUNTER: AtomicI32 = AtomicI32::new(0);

fn append_text_children(node: Node, out: &mut Vec<u8>, depth: u32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in children(node) {
        match (child.kind(), child.text()) {
            (Kind::Text, Some(text)) => out.extend_from_slice(text.to_bytes()),
            (Kind::Element, _) => append_text_children(child, out, depth + 1),
            _ => {}
        }
    }
}

fn scope_id(owner: Node) -> Vec<u8> {
    if let Some(existing) = owner.attr(HOST_SCOPE_ATTR) {
        return existing.to_bytes().to_vec();
    }
    let id = SCOPE_COUNTER
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
        .to_string()
        .into_bytes();
    if owner.is_element() {
        attrs::set_len(owner, HOST_SCOPE_ATTR, Some(&id), id.len());
    }
    id
}

fn shadow_host_scope_id(root: Node) -> Option<Vec<u8>> {
    root.parent().map(scope_id)
}

fn shadow_scope_id(style: Node) -> Option<Vec<u8>> {
    ancestors_and_self(style)
        .find(|a| a.is_element() && a.attr(SHADOW_ATTR).is_some())
        .and_then(shadow_host_scope_id)
}

fn frame_scope_id(style: Node) -> Option<Vec<u8>> {
    ancestors_and_self(style)
        .find(|a| {
            a.is_element()
                && a.parent()
                    .is_some_and(|p| p.kind() == Kind::Document && p.parent().is_some())
        })
        .map(scope_id)
}

fn until_nul(mut text: Vec<u8>) -> Vec<u8> {
    if let Some(end) = text.iter().position(|&c| c == 0) {
        text.truncate(end);
    }
    text
}

pub(crate) fn style_element_text(style: Node) -> Option<Vec<u8>> {
    if style.element_name() != Some(b"style") {
        return None;
    }
    if let Some(media) = style.attr(c"media")
        && !media.is_empty()
        && !media_query_matches(media.to_bytes())
    {
        return None;
    }
    let mut text = Vec::new();
    append_text_children(style, &mut text, 0);
    if text.is_empty() {
        return None;
    }
    if let Some(host_id) = shadow_scope_id(style) {
        return Some(until_nul(scoped_css(&text, &host_id, false)));
    }
    if let Some(frame_id) = frame_scope_id(style) {
        return Some(until_nul(scoped_css(&text, &frame_id, true)));
    }
    Some(text)
}

pub(crate) fn shadow_adopted_text(root: Node) -> Option<Vec<u8>> {
    let css = root.attr(ADOPTED_CSS_ATTR).filter(|css| !css.is_empty())?;
    let host_id = shadow_host_scope_id(root)?;
    Some(until_nul(scoped_css(css.to_bytes(), &host_id, false)))
}

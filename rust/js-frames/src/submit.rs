//! Southstar — form submissions that navigate a frame: the frame a form's target names and the request queued for it to load.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::{CStr, CString};

use southstar_dom::{Kind, Node, ancestors_and_self, children};

use crate::ffi::{self, Js, form, load};
use crate::page::{self, Navigation};
use crate::realm::frame_of;
use crate::schedule;

const MAX_DEPTH: usize = 512;

fn named(node: Node<'_>, tag: &[u8]) -> bool {
    node.element_name() == Some(tag)
}

fn non_empty(text: Option<&CStr>) -> Option<&CStr> {
    text.filter(|text| !text.is_empty())
}

fn is_submit_button(node: Node<'_>) -> bool {
    let kind = node.attr(c"type").map(CStr::to_bytes);
    let kind_is = |name: &[u8]| kind.is_some_and(|kind| kind.eq_ignore_ascii_case(name));
    (named(node, b"button") && (kind.is_none() || kind_is(b"submit")))
        || (named(node, b"input") && (kind_is(b"submit") || kind_is(b"image")))
}

fn document_of(node: Node<'_>) -> Option<Node<'_>> {
    ancestors_and_self(node).find(|node| node.kind() == Kind::Document)
}

fn frame_holding(doc: Node<'_>) -> Option<Node<'_>> {
    doc.parent().filter(|owner| named(*owner, b"iframe"))
}

fn first_in_document<'a>(
    node: Node<'a>,
    depth: usize,
    found: &impl Fn(Node<'a>) -> bool,
) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    for child in children(node) {
        if child.kind() == Kind::Document {
            continue;
        }
        if found(child) {
            return Some(child);
        }
        if let Some(hit) = first_in_document(child, depth + 1, found) {
            return Some(hit);
        }
    }
    None
}

fn frame_named<'a>(node: Node<'a>, name: &[u8], depth: usize) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    for child in children(node) {
        if named(child, b"iframe") && child.attr(c"name").map(CStr::to_bytes) == Some(name) {
            return Some(child);
        }
        if let Some(hit) = frame_named(child, name, depth + 1) {
            return Some(hit);
        }
    }
    None
}

fn target_name<'a>(form: Node<'a>, button: Option<Node<'a>>, doc: Node<'a>) -> &'a [u8] {
    let target = button
        .and_then(|button| non_empty(button.attr(c"formtarget")))
        .or_else(|| form.attr(c"target"))
        .or_else(|| {
            first_in_document(doc, 0, &|node| {
                named(node, b"base") && node.attr(c"target").is_some()
            })
            .and_then(|base| base.attr(c"target"))
        });
    target.map_or(&[][..], |target| target.to_bytes().trim_ascii())
}

fn target_frame<'a>(page: Node<'a>, doc: Node<'a>, target: &[u8]) -> Option<Node<'a>> {
    let own = frame_holding(doc);
    if target.is_empty() || target.eq_ignore_ascii_case(b"_self") {
        return own;
    }
    if target.eq_ignore_ascii_case(b"_parent") {
        return own.and_then(document_of).and_then(frame_holding);
    }
    if target.starts_with(b"_") {
        return None;
    }
    frame_named(page, target, 0)
}

fn document_url(js: Js, doc: Node<'_>) -> Option<CString> {
    match frame_holding(doc) {
        Some(owner) => page::frame_entry(js, frame_of(owner), |entry| entry.url.clone()).flatten(),
        None => load::page_url(js),
    }
}

fn without_query_or_fragment(url: &CStr) -> &[u8] {
    let bytes = url.to_bytes();
    let end = bytes
        .iter()
        .position(|&byte| byte == b'?' || byte == b'#')
        .unwrap_or(bytes.len());
    &bytes[..end]
}

fn navigation_for(
    js: Js,
    form_node: Node<'_>,
    submitter: Option<Node<'_>>,
    doc: Node<'_>,
) -> Option<Navigation> {
    let button = submitter.filter(|submitter| is_submit_button(*submitter));
    let attr = |name: &CStr, form_name: &CStr| {
        button
            .and_then(|button| non_empty(button.attr(name)))
            .or_else(|| form_node.attr(form_name))
    };
    let is_post = attr(c"formmethod", c"method")
        .is_some_and(|method| method.to_bytes().eq_ignore_ascii_case(b"post"));
    let action = match non_empty(attr(c"formaction", c"action")) {
        Some(action) => {
            let base = load::node_doc_base_ptr(js, form_node);
            let base = (!base.is_null()).then(|| {
                // SAFETY: a non-null document base is a NUL-terminated string owned by the page.
                unsafe { CStr::from_ptr(base) }
            })?;
            ffi::resolve_url(base, action)?
        }
        None => document_url(js, doc)?,
    };
    if !load::is_http_or_https(Some(&action)) || !form::csp_form_action_allowed(js, &action) {
        return None;
    }
    let charset = non_empty(form_node.attr(c"accept-charset"));
    let data = form::urlencoded(form_node, doc, submitter, charset);
    if is_post {
        return Some(Navigation {
            url: action,
            form_body: Some(data),
        });
    }
    let mut url = without_query_or_fragment(&action).to_vec();
    url.push(b'?');
    url.extend_from_slice(&data);
    Some(Navigation {
        url: CString::new(url).ok()?,
        form_body: None,
    })
}

pub(crate) fn submit_into_frame(js: Js, form_node: Node<'_>, submitter: Option<Node<'_>>) -> bool {
    if js.is_null() || ffi::halted(js) || !load::node_in_page(js, form_node) {
        return false;
    }
    let submitter = submitter.filter(|submitter| *submitter != form_node);
    let (Some(doc), Some(page)) = (document_of(form_node), ffi::current_document(js)) else {
        return false;
    };
    let button = submitter.filter(|submitter| is_submit_button(*submitter));
    let target = target_name(form_node, button, doc);
    let Some(frame) = target_frame(page, doc, target) else {
        return false;
    };
    let Some(navigation) = navigation_for(js, form_node, submitter, doc) else {
        return false;
    };
    page::update_frame(js, frame_of(frame), |entry| {
        entry.navigation = Some(navigation);
    });
    schedule::schedule(js, frame, true);
    true
}

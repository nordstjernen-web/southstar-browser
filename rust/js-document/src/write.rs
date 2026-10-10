//! Southstar — document.open, write, writeln and close on the page's document: the script-inserted markup buffer and where it is parsed into the tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::cell::RefCell;
use std::ffi::CString;

use southstar_dom::{FLAG_SCRIPTING_DISABLED, Kind, Node, NsNode};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, JsResult, until_nul};

const INVALID_STATE_ERR: i32 = 11;

#[derive(Default)]
struct Page {
    buffer: Option<Vec<u8>>,
    script: usize,
    parser_open: bool,
    dynamic_markup: i32,
}

thread_local! {
    static PAGES: RefCell<Vec<(usize, Page)>> = const { RefCell::new(Vec::new()) };
}

fn with_page<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let index = match pages.iter().position(|(owner, _)| *owner == js.key()) {
            Some(index) => index,
            None => {
                pages.push((js.key(), Page::default()));
                pages.len() - 1
            }
        };
        f(&mut pages[index].1)
    })
}

pub(crate) fn teardown(js: Js) {
    PAGES.with(|pages| pages.borrow_mut().retain(|(owner, _)| *owner != js.key()));
}

pub(crate) fn dynamic_markup_active(js: Js) -> bool {
    !js.is_null() && with_page(js, |page| page.dynamic_markup > 0)
}

fn adjust_dynamic_markup(js: Js, delta: i32) {
    with_page(js, |page| page.dynamic_markup += delta);
}

pub(crate) fn throw_during_construction(scope: &mut Scope<'_>, method: &str) -> Value {
    ffi::dom_exception(
        scope,
        "InvalidStateError",
        INVALID_STATE_ERR,
        &format!("document.{method} during parser-created element construction"),
    )
}

fn node_at(addr: usize) -> Option<Element> {
    unsafe { Node::from_ptr(addr as *const NsNode) }
}

fn script_addr(node: Option<Element>) -> usize {
    node.map_or(0, |n| n.as_ptr() as usize)
}

fn write_parent(js: Js) -> Option<(Element, Option<Element>)> {
    let doc = ffi::current_document(js)?;
    let script = node_at(with_page(js, |page| page.script));
    if let Some(script) = script
        && let Some(parent) = script.parent()
    {
        return Some((parent, Some(script)));
    }
    let body = southstar_dom::index::find_first_element(doc, c"body")?;
    Some((body, body.last_child()))
}

pub(crate) fn flush(js: Js) {
    let pending = with_page(js, |page| {
        let buffer = page.buffer.as_mut()?;
        if buffer.is_empty() {
            page.script = 0;
            return None;
        }
        Some(())
    });
    if pending.is_none() {
        return;
    }
    let target = write_parent(js);
    let html = with_page(js, |page| {
        page.script = 0;
        page.buffer
            .as_mut()
            .map(core::mem::take)
            .unwrap_or_default()
    });
    let Some((parent, mut reference)) = target else {
        return;
    };
    let context = (parent.kind() == Kind::Element)
        .then(|| parent.name())
        .flatten()
        .map(CStr::to_owned);
    let scripting = parent.root().flags() & FLAG_SCRIPTING_DISABLED == 0;
    let Some(fragment) = ffi::parse_fragment(context.as_deref(), &html, scripting) else {
        return;
    };
    let mut inserted = Vec::new();
    adjust_dynamic_markup(js, 1);
    let mut child = fragment.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        southstar_dom::node::own_strings_deep(c);
        ffi::remove_node(c);
        match reference.filter(|r| r.parent() == Some(parent)) {
            Some(r) => r.insert_after(c),
            None => ffi::append_child(parent, c),
        }
        ffi::record_child_change(
            js,
            parent,
            Some(c),
            None,
            c.prev_sibling(),
            c.next_sibling(),
        );
        reference = Some(c);
        inserted.push(c);
    }
    ffi::free_node(fragment);
    if inserted.is_empty() {
        adjust_dynamic_markup(js, -1);
        return;
    }
    ffi::with_main_context(js, |scope| {
        let global = scope.global();
        let document = scope
            .get(&global, "document")
            .unwrap_or_else(|_| Value::undefined());
        for node in &inserted {
            ffi::expose_legacy_named(scope, *node, &document);
        }
    });
    ffi::mark_mutated(js);
    ffi::ce_upgrade_subtree_all(js, parent);
    adjust_dynamic_markup(js, -1);
    ffi::run_inserted_scripts(js, parent);
}

fn open_current(js: Js) {
    let Some(doc) = ffi::current_document(js) else {
        return;
    };
    ffi::orphan_children(js, doc);
    let html = ffi::new_element(b"html");
    let head = ffi::new_element(b"head");
    let body = ffi::new_element(b"body");
    ffi::append_child(html, head);
    ffi::append_child(html, body);
    ffi::append_child(doc, html);
    southstar_dom::index::tag_build(doc);
    southstar_dom::index::id_build(doc);
    southstar_dom::index::class_build(doc);
    with_page(js, |page| {
        if let Some(buffer) = page.buffer.as_mut() {
            buffer.clear();
        }
        page.script = 0;
        page.parser_open = true;
    });
    ffi::mark_mutated(js);
    ffi::arm_js_invalidate(doc);
}

fn targets_other_document(js: Js, this: &Value) -> bool {
    !js.is_null()
        && ffi::unwrap_node(this).is_some_and(|target| Some(target) != ffi::current_document(js))
}

pub(crate) fn open(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if targets_other_document(js, this) {
        return crate::realm::open(scope, this, args);
    }
    if dynamic_markup_active(js) {
        return Err(throw_during_construction(scope, "open"));
    }
    if args.len() >= 3 {
        return ffi::window_open(scope, this, args);
    }
    if js.is_null() {
        return Ok(this.clone());
    }
    if ffi::current_script(js).is_some() && ffi::ready_state(js, None) < 2 {
        return Ok(this.clone());
    }
    open_current(js);
    Ok(this.clone())
}

pub(crate) fn close(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if targets_other_document(js, this) {
        return crate::realm::close(scope, this, args);
    }
    if dynamic_markup_active(js) {
        return Err(throw_during_construction(scope, "close"));
    }
    if !js.is_null() {
        flush(js);
        with_page(js, |page| page.parser_open = false);
    }
    Ok(Value::undefined())
}

fn append_to_buffer(js: Js, text: &[u8]) {
    with_page(js, |page| {
        page.buffer
            .get_or_insert_with(Vec::new)
            .extend_from_slice(text)
    });
}

pub(crate) fn append_args(scope: &mut Scope<'_>, out: &mut Vec<u8>, args: &[Value]) {
    for arg in args {
        if let Ok(text) = scope.to_bytes(arg) {
            out.extend_from_slice(until_nul(&text));
        }
    }
}

fn write_common(scope: &mut Scope<'_>, args: &[Value], newline: bool) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() || ffi::current_document(js).is_none() {
        return Ok(Value::undefined());
    }
    if dynamic_markup_active(js) {
        return Err(throw_during_construction(scope, "write"));
    }
    let script = ffi::current_script(js);
    if script.is_none() && ffi::ignore_destructive_writes(js) {
        return Ok(Value::undefined());
    }
    if script.is_none()
        && ffi::ready_state(js, None) >= 2
        && !with_page(js, |page| page.parser_open)
    {
        open_current(js);
    }
    let script = script_addr(script);
    if with_page(js, |page| page.buffer.is_some() && page.script != script) {
        flush(js);
    }
    with_page(js, |page| {
        page.buffer.get_or_insert_with(Vec::new);
        page.script = script;
    });
    for arg in args {
        if let Ok(text) = scope.to_bytes(arg) {
            append_to_buffer(js, until_nul(&text));
        }
    }
    if newline {
        append_to_buffer(js, b"\n");
    }
    let complete = with_page(js, |page| {
        script == 0 || page.buffer.as_deref().is_some_and(markup_is_complete)
    });
    if complete {
        flush(js);
    }
    Ok(Value::undefined())
}

pub(crate) fn write(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if targets_other_document(js, this) {
        return crate::realm::write(scope, this, args);
    }
    write_common(scope, args, false)
}

pub(crate) fn writeln(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if targets_other_document(js, this) {
        return crate::realm::writeln(scope, this, args);
    }
    write_common(scope, args, true)
}

const VOID_TAGS: [&[u8]; 14] = [
    b"area", b"base", b"br", b"col", b"embed", b"hr", b"img", b"input", b"link", b"meta", b"param",
    b"source", b"track", b"wbr",
];

const RAW_TAGS: [&[u8]; 8] = [
    b"script",
    b"style",
    b"textarea",
    b"title",
    b"xmp",
    b"iframe",
    b"noembed",
    b"noframes",
];

fn find_byte(s: &[u8], from: usize, byte: u8) -> Option<usize> {
    s.get(from..)?
        .iter()
        .position(|&b| b == byte)
        .map(|i| from + i)
}

fn find_bytes(s: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    s.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| from + i)
}

fn is_glib_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn at(s: &[u8], index: usize) -> u8 {
    s.get(index).copied().unwrap_or(0)
}

fn starts_with_caseless(s: &[u8], from: usize, prefix: &[u8]) -> bool {
    s.get(from..from + prefix.len())
        .is_some_and(|w| w.eq_ignore_ascii_case(prefix))
}

fn raw_text_end(s: &[u8], from: usize, tag: &[u8]) -> Option<usize> {
    let mut end_tag = b"</".to_vec();
    end_tag.extend_from_slice(tag);
    let mut e = from;
    loop {
        e = find_byte(s, e, b'<')?;
        if starts_with_caseless(s, e, &end_tag) {
            return find_byte(s, e, b'>').map(|close| close + 1);
        }
        e += 1;
    }
}

fn markup_is_complete(s: &[u8]) -> bool {
    let mut open: Vec<Vec<u8>> = Vec::new();
    let mut p = 0;
    while let Some(lt) = find_byte(s, p, b'<') {
        p = lt;
        if s[p..].starts_with(b"<!--") {
            match find_bytes(s, p + 4, b"-->") {
                Some(end) => p = end + 3,
                None => return false,
            }
            continue;
        }
        let closing = at(s, p + 1) == b'/';
        let name = p + if closing { 2 } else { 1 };
        let first = at(s, name);
        if !first.is_ascii_alphabetic() {
            if first == b'!' || first == b'?' {
                match find_byte(s, name, b'>') {
                    Some(end) => p = end + 1,
                    None => return false,
                }
            } else {
                p += 1;
            }
            continue;
        }
        let mut q = name;
        while q < s.len() && (s[q].is_ascii_alphanumeric() || s[q] == b'-' || s[q] == b':') {
            q += 1;
        }
        let tag = s[name..q].to_ascii_lowercase();
        let mut quote = 0u8;
        let mut self_closing = false;
        let mut has_src = false;
        while q < s.len() && (quote != 0 || s[q] != b'>') {
            let c = s[q];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'/' && at(s, q + 1) == b'>' {
                self_closing = true;
            } else if starts_with_caseless(s, q, b"src")
                && (at(s, q + 3) == b'=' || is_glib_space(at(s, q + 3)))
                && is_glib_space(s[q - 1])
            {
                has_src = true;
            }
            q += 1;
        }
        if q >= s.len() {
            return false;
        }
        p = q + 1;
        if closing {
            if let Some(i) = open.iter().rposition(|t| *t == tag) {
                open.truncate(i);
            }
            continue;
        }
        let is_void = self_closing || VOID_TAGS.contains(&tag.as_slice());
        let is_raw = RAW_TAGS.contains(&tag.as_slice());
        if tag == b"script" && has_src {
            return false;
        }
        if is_raw && !is_void {
            match raw_text_end(s, p, &tag) {
                Some(end) => p = end,
                None => return false,
            }
            continue;
        }
        if !is_void {
            open.push(tag);
        }
    }
    open.is_empty()
}

pub(crate) fn c_string(bytes: &[u8]) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}

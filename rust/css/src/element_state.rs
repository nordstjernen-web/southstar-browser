//! Southstar — the element state behind pseudo-classes: form controls (:checked, :default, :indeterminate, :valid, :invalid, :in-range, :read-write, :placeholder-shown, :blank, :required, :disabled), links and :visited, :target, :lang(), :dir(), :empty, :heading() and the media, popover and dialog states.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::{Mutex, MutexGuard};

use southstar_dom::{
    FLAG_FOREIGN_NS, FLAG_SVG_NS, Kind, MAX_DEPTH, Node, attrs, controls, select, serialize,
};

use crate::ffi;
use crate::scan::{is_gspace, is_ws, scan_until, strip, trim_range, utf8_char};
use crate::selector::anb_int_strict;

const PC_EMPTY: u32 = 6;
const PC_CHECKED: u32 = 8;
const PC_DISABLED: u32 = 9;
const PC_ENABLED: u32 = 10;
const PC_REQUIRED: u32 = 11;
const PC_OPTIONAL: u32 = 12;
const PC_VALID: u32 = 13;
const PC_INVALID: u32 = 14;
const PC_IN_RANGE: u32 = 15;
const PC_OUT_OF_RANGE: u32 = 16;
const PC_DEFAULT: u32 = 17;
const PC_INDETERMINATE: u32 = 18;
const PC_LINK: u32 = 23;
const PC_VISITED: u32 = 24;
const PC_ANY_LINK: u32 = 25;
const PC_TARGET: u32 = 31;
const PC_TARGET_WITHIN: u32 = 32;
const PC_PLACEHOLDER_SHOWN: u32 = 35;
const PC_READ_ONLY: u32 = 36;
const PC_READ_WRITE: u32 = 37;
const PC_BLANK: u32 = 38;
const PC_LANG: u32 = 39;
const PC_DIR: u32 = 40;
const PC_OPEN: u32 = 41;
const PC_POPOVER_OPEN: u32 = 42;
const PC_MODAL: u32 = 43;
const PC_HEADING: u32 = 45;
const PC_USER_VALID: u32 = 46;
const PC_USER_INVALID: u32 = 47;
const PC_AUTOFILL: u32 = 48;
const PC_PLAYING: u32 = 49;
const PC_PAUSED: u32 = 50;
const PC_MUTED: u32 = 51;
const PC_SEEKING: u32 = 52;
const PC_BUFFERING: u32 = 53;
const PC_STALLED: u32 = 54;

const DIR_DEPTH_MAX: i32 = 256;
const PRAGMA_LANG_MAX: usize = 127;
const XML_NS: &CStr = c"http://www.w3.org/XML/1998/namespace";
const CUSTOM_VALIDITY_ATTR: &CStr = c"data-nd-custom-validity";

static TARGET_FRAGMENT: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static VISITED: Mutex<Option<HashSet<Vec<u8>>>> = Mutex::new(None);
static DOC_BASE: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static DOC_LANGUAGE: Mutex<Option<Vec<u8>>> = Mutex::new(None);

#[derive(Default)]
struct Pragma {
    valid: bool,
    doc: usize,
    lang: Option<Vec<u8>>,
}

thread_local! {
    static PRAGMA: RefCell<Pragma> = RefCell::new(Pragma::default());
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn non_empty(value: Option<&[u8]>) -> Option<Vec<u8>> {
    value.filter(|v| !v.is_empty()).map(<[u8]>::to_vec)
}

pub(crate) fn set_target_fragment(fragment: Option<&[u8]>) {
    *lock(&TARGET_FRAGMENT) = non_empty(fragment);
}

pub(crate) fn mark_visited(url: &[u8]) {
    if url.is_empty() {
        return;
    }
    lock(&VISITED)
        .get_or_insert_with(HashSet::new)
        .insert(url.to_vec());
}

pub(crate) fn set_doc_base(base: Option<&[u8]>) {
    *lock(&DOC_BASE) = non_empty(base);
}

pub(crate) fn set_doc_language(lang: Option<&[u8]>) {
    *lock(&DOC_LANGUAGE) = non_empty(lang);
}

pub(crate) fn reset_language_cache() {
    PRAGMA.with_borrow_mut(|pragma| pragma.valid = false);
}

fn until_nul(text: &[u8]) -> &[u8] {
    &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())]
}

fn eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn attr<'a>(el: Node<'a>, name: &CStr) -> Option<&'a CStr> {
    attrs::get(el, name)
}

fn has(el: Node<'_>, name: &CStr) -> bool {
    attr(el, name).is_some()
}

fn attr_bytes<'a>(el: Node<'a>, name: &CStr) -> Option<&'a [u8]> {
    attr(el, name).map(CStr::to_bytes)
}

fn named(node: Option<Node<'_>>, tag: &[u8]) -> bool {
    node.and_then(Node::element_name) == Some(tag)
}

fn raw_name(el: Node<'_>) -> Option<&[u8]> {
    el.name().map(CStr::to_bytes)
}

fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.first_child(), |child| child.next_sibling())
}

fn all_ws(text: &[u8]) -> bool {
    until_nul(text).iter().all(|&c| is_ws(c))
}

fn input_is_text_entry(el: Node<'_>) -> bool {
    match attr_bytes(el, c"type") {
        None | Some(b"") => true,
        Some(ty) => [
            &b"text"[..],
            b"search",
            b"url",
            b"tel",
            b"email",
            b"password",
            b"number",
        ]
        .iter()
        .any(|t| eq(ty, t)),
    }
}

fn is_read_write(el: Node<'_>) -> bool {
    let Some(name) = raw_name(el) else {
        return false;
    };
    if name == b"input" {
        return controls::type_supports_readonly(attr(el, c"type"))
            && !has(el, c"readonly")
            && !controls::effectively_disabled(el);
    }
    if name == b"textarea" {
        return !has(el, c"readonly") && !controls::effectively_disabled(el);
    }
    attr_bytes(el, c"contenteditable")
        .is_some_and(|ce| ce.is_empty() || eq(ce, b"true") || eq(ce, b"plaintext-only"))
}

fn placeholder_shown(el: Node<'_>) -> bool {
    let Some(name) = raw_name(el) else {
        return false;
    };
    if !has(el, c"placeholder") {
        return false;
    }
    if name == b"input" {
        return input_is_text_entry(el) && attr_bytes(el, c"value").is_none_or(<[u8]>::is_empty);
    }
    name == b"textarea" && all_ws(&serialize::collect_text(Some(el)))
}

fn is_checked(el: Node<'_>) -> bool {
    let some = Some(el);
    if named(some, b"option") {
        if has(el, c"selected") {
            return true;
        }
        let mut select = el.parent();
        if named(select, b"optgroup") {
            select = select.and_then(Node::parent);
        }
        return select.is_some_and(|s| {
            named(Some(s), b"select")
                && !has(s, c"multiple")
                && select::chosen_option(s) == Some(el)
        });
    }
    if !named(some, b"input") {
        return false;
    }
    attr_bytes(el, c"type").is_some_and(|ty| eq(ty, b"checkbox") || eq(ty, b"radio"))
        && controls::is_checked(el)
}

fn is_submit_button(el: Node<'_>) -> bool {
    let some = Some(el);
    let ty = attr_bytes(el, c"type");
    if named(some, b"input") {
        return ty.is_some_and(|t| eq(t, b"submit") || eq(t, b"image"));
    }
    named(some, b"button") && ty.is_none_or(|t| t.is_empty() || eq(t, b"submit") || eq(t, b"auto"))
}

fn first_submit_button_for<'a>(scan: Node<'a>, owner: Node<'a>, depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if scan.is_element()
        && is_submit_button(scan)
        && !controls::effectively_disabled(scan)
        && controls::form_owner(scan) == Some(owner)
    {
        return Some(scan);
    }
    if named(Some(scan), b"template") {
        return None;
    }
    children(scan).find_map(|c| first_submit_button_for(c, owner, depth + 1))
}

fn is_default(el: Node<'_>) -> bool {
    let some = Some(el);
    if named(some, b"option") {
        return has(el, c"selected");
    }
    if named(some, b"input")
        && attr_bytes(el, c"type").is_some_and(|t| eq(t, b"checkbox") || eq(t, b"radio"))
    {
        return has(el, c"checked");
    }
    if !is_submit_button(el) {
        return false;
    }
    let Some(owner) = controls::form_owner(el) else {
        return false;
    };
    first_submit_button_for(el.root(), owner, 0) == Some(el)
}

fn radio_group_has_checked(
    scan: Node<'_>,
    owner: Option<Node<'_>>,
    name: &[u8],
    depth: i32,
) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    if named(Some(scan), b"input")
        && attr_bytes(scan, c"type").is_some_and(|t| eq(t, b"radio"))
        && attr_bytes(scan, c"name").unwrap_or(b"") == name
        && controls::form_owner(scan) == owner
        && controls::is_checked(scan)
    {
        return true;
    }
    if named(Some(scan), b"template") {
        return false;
    }
    children(scan).any(|c| radio_group_has_checked(c, owner, name, depth + 1))
}

fn is_indeterminate(el: Node<'_>) -> bool {
    let some = Some(el);
    if named(some, b"progress") {
        return !has(el, c"value");
    }
    if !named(some, b"input") || !attr_bytes(el, c"type").is_some_and(|t| eq(t, b"radio")) {
        return false;
    }
    let name = attr_bytes(el, c"name").unwrap_or(b"");
    !radio_group_has_checked(el.root(), controls::form_owner(el), name, 0)
}

fn range_state(el: Node<'_>) -> Option<(bool, bool)> {
    if !named(Some(el), b"input") || !controls::type_has_number_value(attr(el, c"type")) {
        return None;
    }
    if !has(el, c"min") && !has(el, c"max") {
        return None;
    }
    let value = attr(el, c"value").filter(|v| !v.is_empty())?;
    controls::value_range_state(el, Some(value))
}

fn is_blank(el: Node<'_>) -> bool {
    let some = Some(el);
    if named(some, b"input") {
        return input_is_text_entry(el) && attr_bytes(el, c"value").is_none_or(<[u8]>::is_empty);
    }
    named(some, b"textarea") && all_ws(&serialize::collect_text(Some(el)))
}

fn is_empty(el: Node<'_>) -> bool {
    children(el).all(|c| match c.kind() {
        Kind::Element => false,
        Kind::Text => c.text().is_none_or(|t| t.is_empty()),
        _ => true,
    })
}

fn is_link(el: Node<'_>) -> bool {
    has(el, c"href") && (named(Some(el), b"a") || named(Some(el), b"area"))
}

fn is_visited_link(el: Node<'_>) -> bool {
    let visited = lock(&VISITED);
    let Some(visited) = visited.as_ref() else {
        return false;
    };
    let base = lock(&DOC_BASE);
    let Some(base) = base.as_deref() else {
        return false;
    };
    if !is_link(el) {
        return false;
    }
    let Some(href) = attr_bytes(el, c"href").filter(|h| !h.is_empty()) else {
        return false;
    };
    ffi::url_resolve(base, href).is_some_and(|abs| visited.contains(&abs))
}

fn is_target(el: Node<'_>, fragment: &[u8]) -> bool {
    if attr_bytes(el, c"id") == Some(fragment) {
        return true;
    }
    raw_name(el).is_some_and(|n| eq(n, b"a")) && attr_bytes(el, c"name") == Some(fragment)
}

fn has_target_within(el: Node<'_>, fragment: &[u8], depth: i32) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    if el.is_element() && is_target(el, fragment) {
        return true;
    }
    if named(Some(el), b"template") {
        return false;
    }
    children(el).any(|c| has_target_within(c, fragment, depth + 1))
}

fn target_matches(el: Node<'_>, within: bool) -> bool {
    let fragment = lock(&TARGET_FRAGMENT);
    let Some(fragment) = fragment.as_deref() else {
        return false;
    };
    if within {
        has_target_within(el, fragment, 0)
    } else {
        is_target(el, fragment)
    }
}

fn pragma_language_scan(n: Node<'_>, found: &mut Option<Vec<u8>>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for c in children(n) {
        if c.is_element() && raw_name(c).is_some_and(|name| eq(name, b"meta")) {
            let content = attr_bytes(c, c"http-equiv")
                .filter(|he| eq(he, b"content-language"))
                .and_then(|_| attr_bytes(c, c"content"));
            if let Some(content) = content.filter(|c| !c.contains(&b',')) {
                let start = content
                    .iter()
                    .position(|&b| !is_gspace(b))
                    .unwrap_or(content.len());
                let rest = &content[start..];
                let word = &rest[..rest
                    .iter()
                    .position(|&b| is_gspace(b))
                    .unwrap_or(rest.len())];
                if !word.is_empty() {
                    *found = Some(word[..word.len().min(PRAGMA_LANG_MAX)].to_vec());
                }
            }
        }
        pragma_language_scan(c, found, depth + 1);
    }
}

fn with_language<R>(el: Node<'_>, f: impl FnOnce(Option<&[u8]>) -> R) -> R {
    let mut node = Some(el);
    while let Some(n) = node {
        node = n.parent();
        if !n.is_element() {
            continue;
        }
        if let Some(xml) = attrs::find_ns(n, Some(XML_NS), c"lang") {
            return f(Some(xml.value().map_or(&b""[..], CStr::to_bytes)));
        }
        if let Some(lang) = attrs::find_ns(n, None, c"lang")
            && n.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0
        {
            return f(Some(lang.value().map_or(&b""[..], CStr::to_bytes)));
        }
    }
    let mut root = el;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let doc = root.as_ptr() as usize;
    PRAGMA.with_borrow_mut(|pragma| {
        if !pragma.valid || pragma.doc != doc {
            let mut found = None;
            pragma_language_scan(root, &mut found, 0);
            *pragma = Pragma {
                valid: true,
                doc,
                lang: found,
            };
        }
        match pragma.lang.as_deref() {
            Some(lang) => f(Some(lang)),
            None => f(lock(&DOC_LANGUAGE).as_deref()),
        }
    })
}

fn lang_one_matches(lang: &[u8], want: &[u8]) -> bool {
    let start = want
        .iter()
        .position(|&c| !matches!(c, b' ' | b'\'' | b'"'))
        .unwrap_or(want.len());
    let want = &want[start..];
    let wlen = want
        .iter()
        .rposition(|&c| !(is_ws(c) || c == b'\'' || c == b'"'))
        .map_or(0, |i| i + 1);
    let want = &want[..wlen];
    if want.is_empty() {
        return false;
    }
    if want == b"*" {
        return true;
    }
    let prefix_matches = |p: &[u8], needle: &[u8]| {
        p.len() >= needle.len()
            && eq(&p[..needle.len()], needle)
            && (p.len() == needle.len() || p[needle.len()] == b'-')
    };
    if want.len() >= 2 && want[0] == b'*' && want[1] == b'-' {
        let needle = &want[2..];
        return lang
            .iter()
            .enumerate()
            .filter(|&(_, &c)| c == b'-')
            .any(|(i, _)| prefix_matches(&lang[i + 1..], needle));
    }
    prefix_matches(lang, want)
}

fn lang_matches(el: Node<'_>, arg: Option<&[u8]>) -> bool {
    let Some(arg) = arg else {
        return false;
    };
    with_language(el, |lang| {
        lang.is_some_and(|lang| lang_list_matches(lang, arg))
    })
}

fn lang_list_matches(lang: &[u8], arg: &[u8]) -> bool {
    let end = arg.len();
    let mut p = 0;
    while p < end {
        let (seg, term) = scan_until(arg, p, end, b",");
        let want = trim_range(arg, p, seg);
        if !want.is_empty() && lang_one_matches(lang, want) {
            return true;
        }
        p = if term == b',' { seg + 1 } else { seg };
    }
    false
}

fn strong_direction(text: &[u8]) -> Option<&'static CStr> {
    let text = until_nul(text);
    let mut p = 0;
    while p < text.len() {
        let (c, next) = utf8_char(text, p);
        if ffi::unichar_is_rtl_script(c) {
            return Some(c"rtl");
        }
        if ffi::unichar_is_alpha(c) {
            return Some(c"ltr");
        }
        p = next;
    }
    None
}

fn first_strong(n: Node<'_>, depth: i32) -> Option<&'static CStr> {
    if depth > DIR_DEPTH_MAX {
        return None;
    }
    match n.kind() {
        Kind::Text => return n.text().and_then(|t| strong_direction(t.to_bytes())),
        Kind::Element => {}
        _ => return None,
    }
    for c in children(n) {
        if c.is_element() {
            let html = c.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0;
            let skipped = raw_name(c).is_some_and(|name| {
                (html
                    && [&b"script"[..], b"style", b"textarea", b"bdi"]
                        .iter()
                        .any(|t| eq(name, t)))
                    || has(c, c"dir")
            });
            if skipped {
                continue;
            }
        }
        if let Some(d) = first_strong(c, depth + 1) {
            return Some(d);
        }
    }
    None
}

fn form_control_value(n: Node<'_>) -> Option<&CStr> {
    let name = raw_name(n)?;
    if eq(name, b"textarea") {
        return Some(controls::editable_value(n));
    }
    if !eq(name, b"input") {
        return None;
    }
    if let Some(ty) = attr_bytes(n, c"type") {
        const USES: [&[u8]; 10] = [
            b"hidden",
            b"text",
            b"search",
            b"tel",
            b"url",
            b"email",
            b"password",
            b"submit",
            b"reset",
            b"button",
        ];
        if !USES.iter().any(|u| eq(ty, u)) {
            return None;
        }
    }
    Some(controls::editable_value(n))
}

fn dir_auto_resolve(n: Node<'_>) -> &'static CStr {
    match form_control_value(n) {
        Some(value) => strong_direction(value.to_bytes()),
        None => first_strong(n, 0),
    }
    .unwrap_or(c"ltr")
}

pub(crate) fn node_dir(el: Node<'_>) -> &'static CStr {
    let mut node = Some(el);
    while let Some(n) = node {
        node = n.parent();
        if !n.is_element() {
            continue;
        }
        let name = raw_name(n);
        match attr_bytes(n, c"dir") {
            Some(dir) if eq(dir, b"ltr") => return c"ltr",
            Some(dir) if eq(dir, b"rtl") => return c"rtl",
            Some(dir) if eq(dir, b"auto") => return dir_auto_resolve(n),
            Some(_) => {}
            None if name.is_some_and(|nm| eq(nm, b"bdi")) => {
                return first_strong(n, 0).unwrap_or(c"ltr");
            }
            None => {}
        }
        if n == el
            && name.is_some_and(|nm| eq(nm, b"input"))
            && attr_bytes(n, c"type").is_some_and(|t| eq(t, b"tel"))
        {
            return c"ltr";
        }
    }
    c"ltr"
}

fn will_validate(el: Node<'_>) -> bool {
    let Some(name) = el.element_name() else {
        return false;
    };
    let is_input = name == b"input";
    if !is_input && name != b"textarea" && name != b"select" {
        return false;
    }
    if controls::effectively_disabled(el) || controls::readonly_bars_validation(el) {
        return false;
    }
    let ty = if is_input {
        attr_bytes(el, c"type")
    } else {
        None
    };
    !ty.is_some_and(|t| {
        [&b"submit"[..], b"button", b"reset", b"image", b"hidden"]
            .iter()
            .any(|x| eq(t, x))
    })
}

fn control_value(el: Node<'_>) -> Vec<u8> {
    match raw_name(el) {
        None => Vec::new(),
        Some(b"textarea") => serialize::collect_text(Some(el)),
        Some(b"select") => {
            let option = if has(el, c"multiple") {
                select::first_selected_option(el)
            } else {
                select::chosen_option(el)
            };
            option.map_or(Vec::new(), select::option_value)
        }
        Some(_) => attr_bytes(el, c"value").unwrap_or(b"").to_vec(),
    }
}

fn utf8_len(text: &[u8]) -> usize {
    let mut p = 0;
    let mut n = 0;
    while p < text.len() {
        p = utf8_char(text, p).1;
        n += 1;
    }
    n
}

fn control_is_valid(el: Node<'_>) -> bool {
    if !will_validate(el) {
        return false;
    }
    if attr(el, CUSTOM_VALIDITY_ATTR).is_some_and(|c| !c.is_empty()) {
        return false;
    }
    let owned = control_value(el);
    let value = CString::new(until_nul(&owned)).unwrap_or_default();
    let bytes = value.to_bytes();
    let is_input = raw_name(el) == Some(b"input");
    let ty = if is_input { attr(el, c"type") } else { None };
    let mut valid = !(controls::supports_required(el)
        && has(el, c"required")
        && controls::value_missing(el, Some(&value), Some(el.root())));
    if valid
        && !bytes.is_empty()
        && let Some(t) = ty.map(CStr::to_bytes)
    {
        if eq(t, b"email") {
            valid = controls::email_value_valid(Some(el), Some(&value));
        } else if eq(t, b"url") {
            valid = ffi::url_is_valid_absolute(&value);
        } else if controls::type_has_number_value(ty) {
            valid = controls::value_to_number(ty, Some(&value)).is_some();
        }
        if valid && controls::value_range_state(el, Some(&value)).is_some_and(|(u, o)| u || o) {
            valid = false;
        }
        if valid && controls::value_step_mismatch(el, Some(&value)) {
            valid = false;
        }
    }
    if valid
        && !bytes.is_empty()
        && is_input
        && controls::type_supports_text_constraints(ty)
        && !ffi::regex_matches_whole(attr_bytes(el, c"pattern"), bytes)
    {
        valid = false;
    }
    if valid && !bytes.is_empty() && controls::length_limits_apply(el) {
        let len = utf8_len(bytes) as i64;
        let limit = |name: &CStr| {
            attr(el, name).map(|v| i64::from(controls::parse_int(Some(v), 0, 0, 1_000_000)))
        };
        if limit(c"minlength").is_some_and(|min| len < min) {
            valid = false;
        }
        if limit(c"maxlength").is_some_and(|max| len > max) {
            valid = false;
        }
    }
    valid
}

fn heading_matches(el: Node<'_>, arg: Option<&[u8]>) -> bool {
    let level = match el.element_name() {
        Some([b'h', d @ b'1'..=b'6']) => i32::from(d - b'0'),
        _ => return false,
    };
    arg.is_none_or(|arg| {
        arg.split(|&c| c == b',')
            .any(|item| anb_int_strict(strip(item)) == Some(level))
    })
}

fn is_media(el: Node<'_>) -> bool {
    named(Some(el), b"video") || named(Some(el), b"audio")
}

pub(crate) fn matches(el: Node<'_>, kind: u32, arg: Option<&[u8]>) -> bool {
    match kind {
        PC_EMPTY => is_empty(el),
        PC_CHECKED => is_checked(el),
        PC_DISABLED => controls::supports_disabled(el) && controls::effectively_disabled(el),
        PC_ENABLED => controls::supports_disabled(el) && !controls::effectively_disabled(el),
        PC_REQUIRED => controls::supports_required(el) && has(el, c"required"),
        PC_OPTIONAL => controls::supports_required(el) && !has(el, c"required"),
        PC_VALID => will_validate(el) && control_is_valid(el),
        PC_INVALID => will_validate(el) && !control_is_valid(el),
        PC_IN_RANGE => range_state(el) == Some((false, false)),
        PC_OUT_OF_RANGE => range_state(el).is_some_and(|(under, over)| under || over),
        PC_DEFAULT => is_default(el),
        PC_INDETERMINATE => is_indeterminate(el),
        PC_ANY_LINK => is_link(el),
        PC_LINK => is_link(el) && !is_visited_link(el),
        PC_VISITED => is_visited_link(el),
        PC_TARGET => target_matches(el, false),
        PC_TARGET_WITHIN => target_matches(el, true),
        PC_PLACEHOLDER_SHOWN => placeholder_shown(el),
        PC_READ_ONLY => !is_read_write(el),
        PC_READ_WRITE => is_read_write(el),
        PC_BLANK => is_blank(el),
        PC_LANG => lang_matches(el, arg),
        PC_DIR => arg == Some(node_dir(el).to_bytes()),
        PC_OPEN => (named(Some(el), b"details") || named(Some(el), b"dialog")) && has(el, c"open"),
        PC_POPOVER_OPEN => has(el, c"popover") && has(el, c"data-nd-popover-open"),
        PC_MODAL => has(el, c"data-nd-modal"),
        PC_HEADING => heading_matches(el, arg),
        PC_USER_VALID => has(el, c"data-nd-vdirty") && will_validate(el) && control_is_valid(el),
        PC_USER_INVALID => has(el, c"data-nd-vdirty") && will_validate(el) && !control_is_valid(el),
        PC_AUTOFILL => has(el, c"autofill") || has(el, c"data-nd-autofill"),
        PC_PLAYING => is_media(el) && has(el, c"data-nd-playing"),
        PC_PAUSED => is_media(el) && !has(el, c"data-nd-playing"),
        PC_MUTED => is_media(el) && (has(el, c"muted") || has(el, c"data-nd-muted")),
        PC_SEEKING => is_media(el) && has(el, c"data-nd-seeking"),
        PC_BUFFERING => is_media(el) && has(el, c"data-nd-buffering"),
        PC_STALLED => is_media(el) && has(el, c"data-nd-stalled"),
        _ => false,
    }
}

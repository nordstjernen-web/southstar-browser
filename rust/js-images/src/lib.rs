//! Southstar — image readiness for script: each page's tracked <img> loads, their load and error events, and the image a node shows.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_uint};
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::Node;
use southstar_image::ImageRef;

use ffi::{Js, Realm};

type Element = Node<'static>;

const IMG_LOAD_FIRED: u32 = 1 << 3;
const MAX_DEPTH: usize = 512;
const PUMP_RETRY_MS: c_uint = 4;

pub(crate) struct Load {
    js: Js,
    realm: Realm,
    el: Element,
    img: Cell<Option<ImageRef<'static>>>,
    requested_url: CString,
    ready_idle: Cell<c_uint>,
    seq: u64,
}

impl Drop for Load {
    fn drop(&mut self) {
        let idle = self.ready_idle.replace(0);
        if idle != 0 {
            ffi::source_remove(idle);
        }
        ffi::cancel_image_cb(self.js, self);
    }
}

#[derive(Default)]
struct Page {
    loads: HashMap<usize, Rc<Load>>,
    next_seq: u64,
}

thread_local! {
    static PAGES: RefCell<Vec<(Js, Page)>> = const { RefCell::new(Vec::new()) };
}

fn with_page<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let index = match pages.iter().position(|(owner, _)| *owner == js) {
            Some(index) => index,
            None => {
                pages.push((js, Page::default()));
                pages.len() - 1
            }
        };
        f(&mut pages[index].1)
    })
}

fn peek_page<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    PAGES.with(|pages| {
        pages
            .borrow()
            .iter()
            .find(|(owner, _)| *owner == js)
            .map(|(_, page)| f(page))
    })
}

fn key(el: Element) -> usize {
    el.as_ptr() as usize
}

fn lookup(js: Js, el: Element) -> Option<Rc<Load>> {
    peek_page(js, |page| page.loads.get(&key(el)).cloned()).flatten()
}

fn is_current(load: &Rc<Load>) -> bool {
    lookup(load.js, load.el).is_some_and(|cur| Rc::ptr_eq(&cur, load))
}

fn remove(js: Js, el: Element) {
    let old = PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let (_, page) = pages.iter_mut().find(|(owner, _)| *owner == js)?;
        page.loads.remove(&key(el))
    });
    drop(old);
}

fn has_loads(js: Js) -> bool {
    peek_page(js, |page| !page.loads.is_empty()).unwrap_or(false)
}

fn is_img(el: Element) -> bool {
    el.name().is_some_and(|name| name.to_bytes() == b"img")
}

fn nonempty(src: Option<&CStr>) -> Option<&CStr> {
    src.filter(|s| !s.is_empty())
}

fn resolve_or_copy(base: Option<&CStr>, src: &CStr) -> Option<CString> {
    match base {
        Some(base) => ffi::url_resolve(base, src),
        None => Some(src.to_owned()),
    }
}

fn settled(img: ImageRef<'_>) -> bool {
    img.loaded() || img.failed()
}

fn schedule_ready(load: &Rc<Load>) {
    if load.ready_idle.get() == 0 {
        load.ready_idle.set(ffi::idle_add_ready(load));
    }
}

pub(crate) fn start_image_load(js: Js, el: Element, src: Option<&CStr>) {
    if js.is_null() || !ffi::has_image_cache(js) || !is_img(el) {
        return;
    }
    let Some(src) = nonempty(src) else {
        remove(js, el);
        return;
    };
    let base = ffi::doc_base(js, el);
    let Some(abs_url) = resolve_or_copy(base, src) else {
        remove(js, el);
        return;
    };
    let realm = ffi::image_load_realm(js);
    let seq = with_page(js, |page| {
        page.next_seq += 1;
        page.next_seq
    });
    let load = Rc::new(Load {
        js,
        realm,
        el,
        img: Cell::new(None),
        requested_url: abs_url,
        ready_idle: Cell::new(0),
        seq,
    });
    let old = with_page(js, |page| page.loads.insert(key(el), load.clone()));
    drop(old);
    let top_url = base.unwrap_or(&load.requested_url);
    let img = ffi::cache_get(js, &load.requested_url, top_url, &load);
    load.img.set(img);
    if img.is_some_and(settled) {
        schedule_ready(&load);
    }
}

pub(crate) fn on_image_ready(load: Rc<Load>, img: Option<ImageRef<'static>>) {
    if ffi::halted(load.js) || !is_current(&load) {
        return;
    }
    load.img.set(img);
    schedule_ready(&load);
}

fn log_failure(js: Js, img: ImageRef<'_>) {
    if !img.failed() || !ffi::log_enabled(js) || img.http_status() == 204 {
        return;
    }
    let url = ffi::c_str(img.url()).map_or(&b"(no url)"[..], CStr::to_bytes);
    let error = ffi::c_str(img.error()).map_or(&b"(no error msg)"[..], CStr::to_bytes);
    let mut line = b"[image] error: ".to_vec();
    line.extend_from_slice(url);
    line.extend_from_slice(" — ".as_bytes());
    line.extend_from_slice(error);
    line.extend_from_slice(format!(" (HTTP {})", img.http_status()).as_bytes());
    ffi::log_line(js, &line);
}

pub(crate) fn ready_idle(load: Rc<Load>) {
    let js = load.js;
    if !ffi::halted(js) && (ffi::in_pump(js) || ffi::in_blocking_fetch()) {
        load.ready_idle
            .set(ffi::timeout_add_ready(PUMP_RETRY_MS, &load));
        return;
    }
    load.ready_idle.set(0);
    if ffi::halted(js) || !is_current(&load) {
        return;
    }
    let el = load.el;
    let cur_src = el.attr(c"src");
    let abs_url = match (nonempty(cur_src), ffi::doc_base(js, el)) {
        (Some(src), Some(base)) => ffi::url_resolve(base, src),
        _ => cur_src.map(CStr::to_owned),
    };
    if abs_url.as_deref() != Some(load.requested_url.as_c_str()) {
        if let Some(src) = nonempty(cur_src) {
            start_image_load(js, el, Some(src));
        }
        return;
    }
    if let Some(img) = load.img.get() {
        log_failure(js, img);
        if settled(img) {
            ffi::in_realm_scope(js, load.realm, || fire_img_load_once(js, el, img.failed()));
            if img.loaded() {
                ffi::mark_mutated(js);
            }
        }
    }
    ffi::request_repaint(js);
}

pub(crate) fn fire_img_load_once(js: Js, node: Element, failed: bool) {
    if js.is_null() || node.flags() & IMG_LOAD_FIRED != 0 {
        return;
    }
    if ffi::halted(js) || ffi::in_pump(js) {
        return;
    }
    node.add_flags(IMG_LOAD_FIRED);
    if node.element_name() == Some(b"img")
        && let Some(img) = image_for_node(js, node)
    {
        ffi::record_image_timing(js, node, img);
    }
    let kind = if failed { c"error" } else { c"load" };
    ffi::dispatch_resource_event(js, node, kind);
}

pub(crate) fn image_for_node(js: Js, el: Element) -> Option<ImageRef<'static>> {
    if js.is_null() {
        return None;
    }
    if let Some(img) = lookup(js, el).and_then(|load| load.img.get()) {
        return Some(img);
    }
    if ffi::has_image_cache(js)
        && is_img(el)
        && let Some(src) = nonempty(el.attr(c"src"))
        && let Some(abs_url) = resolve_or_copy(ffi::doc_base(js, el), src)
        && let Some(img) = ffi::cache_peek(js, &abs_url)
    {
        return Some(img);
    }
    ffi::layout_image(js, el)
}

pub(crate) fn loads_pending(js: Js) -> bool {
    peek_page(js, |page| {
        page.loads
            .values()
            .any(|load| load.el.flags() & IMG_LOAD_FIRED == 0)
    })
    .unwrap_or(false)
}

pub(crate) fn flush_ready_images(js: Js) {
    if js.is_null() || ffi::halted(js) || ffi::in_pump(js) {
        return;
    }
    let mut ready = peek_page(js, |page| {
        page.loads
            .values()
            .filter(|load| {
                load.el.flags() & IMG_LOAD_FIRED == 0 && load.img.get().is_some_and(settled)
            })
            .map(|load| (load.seq, load.el))
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    ready.sort_unstable_by_key(|(seq, _)| *seq);
    for (_, el) in ready {
        let Some(load) = lookup(js, el) else {
            continue;
        };
        let Some(img) = load.img.get() else {
            continue;
        };
        ffi::in_realm_scope(js, load.realm, || fire_img_load_once(js, el, img.failed()));
    }
}

fn rescan_subtree(js: Js, root: Element, depth: usize) {
    if depth >= MAX_DEPTH {
        return;
    }
    if root.element_name() == Some(b"img")
        && let Some(src) = nonempty(root.attr(c"src"))
        && let Some(load) = lookup(js, root)
        && let Some(abs_url) = ffi::doc_base(js, root).and_then(|base| ffi::url_resolve(base, src))
        && abs_url != load.requested_url
    {
        root.remove_flags(IMG_LOAD_FIRED);
        start_image_load(js, root, Some(src));
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        rescan_subtree(js, c, depth + 1);
        child = c.next_sibling();
    }
}

pub(crate) fn rescan_pending_images(js: Js, root: Option<Element>) {
    if js.is_null() || !has_loads(js) {
        return;
    }
    if let Some(root) = root {
        rescan_subtree(js, root, 0);
    }
}

pub(crate) fn is_tracked(js: Js, el: Element) -> bool {
    lookup(js, el).is_some()
}

pub(crate) fn forget_node(js: Js, el: Element) {
    remove(js, el);
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let index = pages.iter().position(|(owner, _)| *owner == js)?;
        Some(pages.swap_remove(index).1)
    });
    drop(page);
}

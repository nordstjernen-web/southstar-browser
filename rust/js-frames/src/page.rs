//! Southstar — per-page frame state: the frame table (each frame's realm, window, URL and referrer), the realm cloners, the initial about:blank realms and the frame load queues.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;

use southstar_js_engine::Value;

use crate::cloner::Shared;
use crate::ffi::Js;
use crate::ffi::qjs::Ctx;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Frame(pub usize);

#[derive(Default)]
pub(crate) struct FrameEntry {
    pub realm: Option<Ctx>,
    pub window: Option<Value>,
    pub url: Option<CString>,
    pub referrer: Option<CString>,
}

impl FrameEntry {
    fn is_empty(&self) -> bool {
        self.realm.is_none()
            && self.window.is_none()
            && self.url.is_none()
            && self.referrer.is_none()
    }
}

#[derive(Default)]
pub(crate) struct Page {
    pub frames: HashMap<Frame, FrameEntry>,
    pub cloners: HashMap<Ctx, Shared>,
    pub cloners_made: bool,
    pub initial_blank: HashMap<Frame, Ctx>,
    pub pending_loads: Vec<Frame>,
    pub deferred_loads: Vec<Frame>,
}

thread_local! {
    static PAGES: RefCell<Vec<(Js, Page)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn with<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
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

pub(crate) fn peek<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    PAGES.with(|pages| {
        pages
            .borrow()
            .iter()
            .find(|(owner, _)| *owner == js)
            .map(|(_, page)| f(page))
    })
}

pub(crate) fn peek_mut<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> Option<R> {
    PAGES.with(|pages| {
        pages
            .borrow_mut()
            .iter_mut()
            .find(|(owner, _)| *owner == js)
            .map(|(_, page)| f(page))
    })
}

pub(crate) fn take(js: Js) -> Option<Page> {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let index = pages.iter().position(|(owner, _)| *owner == js)?;
        Some(pages.swap_remove(index).1)
    })
}

pub(crate) fn frame_entry<R>(js: Js, frame: Frame, f: impl FnOnce(&FrameEntry) -> R) -> Option<R> {
    peek(js, |page| page.frames.get(&frame).map(f)).flatten()
}

pub(crate) fn update_frame<R>(js: Js, frame: Frame, f: impl FnOnce(&mut FrameEntry) -> R) -> R {
    with(js, |page| {
        let entry = page.frames.entry(frame).or_default();
        let result = f(entry);
        if entry.is_empty() {
            page.frames.remove(&frame);
        }
        result
    })
}

pub(crate) fn frame_of_realm(js: Js, realm: Ctx) -> Option<Frame> {
    peek(js, |page| {
        page.frames
            .iter()
            .find(|(_, entry)| entry.realm == Some(realm))
            .map(|(frame, _)| *frame)
    })
    .flatten()
}

pub(crate) fn realms(js: Js) -> Vec<(Frame, Ctx)> {
    peek(js, |page| {
        page.frames
            .iter()
            .filter_map(|(frame, entry)| entry.realm.map(|realm| (*frame, realm)))
            .collect()
    })
    .unwrap_or_default()
}

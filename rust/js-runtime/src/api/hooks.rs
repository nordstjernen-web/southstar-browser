//! Southstar — the embedder hooks a page calls for binding crates: navigation, downloads, window actions, repaints, scrolling, layout flushes, form submission, the load delay, the clipboard and the selection.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::Node;

use crate::ffi::state::PageState;
use crate::ffi::{Js, NsJs, glue};

pub use crate::ffi::state::{
    DataFn, DelayFn, DownloadFn, FormSubmitFn, LogFn, NavigateFn, NodeFn, TextQueryFn, UrlFn,
    ViewportScrollFn,
};

pub struct Selection {
    pub text: Option<CString>,
    pub rect: [f64; 4],
    pub has_range: bool,
}

fn state<R>(js: *const NsJs, fallback: R, f: impl FnOnce(&PageState) -> R) -> R {
    Js::of(js).map_or(fallback, |js| f(js.st()))
}

macro_rules! hook_setter {
    ($name:ident, $field:ident, $ty:ty) => {
        pub fn $name(js: *const NsJs, hook: Option<Rc<$ty>>) {
            state(js, (), |state| state.$field.set(hook));
        }
    };
}

hook_setter!(set_form_submit_hook, form_submit, FormSubmitFn);
hook_setter!(set_download_hook, download, DownloadFn);
hook_setter!(set_clipboard_write_hook, clipboard_write, TextQueryFn);
hook_setter!(set_selection_cmd_hook, selection_cmd, TextQueryFn);
hook_setter!(set_scroll_to_hook, scroll_to, NodeFn);
hook_setter!(set_fragment_nav_hook, fragment_nav, UrlFn);
hook_setter!(set_soft_nav_hook, soft_nav, NavigateFn);
hook_setter!(set_window_action_hook, window_action, UrlFn);
hook_setter!(set_viewport_scroll_hook, viewport_scroll, ViewportScrollFn);
hook_setter!(set_layout_flush_hook, layout_flush, DataFn);
hook_setter!(set_load_delay_hook, load_delay, DelayFn);

pub fn navigate(js: *const NsJs, url: &CStr, reload: bool) -> bool {
    match state(js, None, |state| state.navigate.get()) {
        Some(navigate) => {
            navigate(url, reload);
            true
        }
        None => false,
    }
}

pub fn can_navigate(js: *const NsJs) -> bool {
    state(js, false, |state| state.navigate.is_set())
}

pub fn soft_navigate(js: *const NsJs, url: &CStr, replace: bool) {
    if let Some(navigate) = state(js, None, |state| state.soft_nav.get()) {
        navigate(url, replace);
    }
}

pub fn fragment_navigated(js: *const NsJs, url: &CStr) {
    if let Some(navigated) = state(js, None, |state| state.fragment_nav.get()) {
        navigated(url);
    }
}

pub fn request_download(js: *const NsJs, url: &CStr, filename: Option<&CStr>) -> bool {
    match state(js, None, |state| state.download.get()) {
        Some(download) => {
            download(url, filename);
            true
        }
        None => false,
    }
}

pub fn window_action(js: *const NsJs, action: &CStr) {
    if let Some(window_action) = state(js, None, |state| state.window_action.get()) {
        window_action(action);
    }
}

pub fn has_window_action(js: *const NsJs) -> bool {
    state(js, false, |state| state.window_action.is_set())
}

pub fn request_repaint(js: *const NsJs) {
    if let Some(repaint) = state(js, None, |state| state.repaint.get()) {
        repaint();
    }
}

pub fn notify_scroll_to(js: *const NsJs, target: Node<'_>) {
    if let Some(scroll) = state(js, None, |state| state.scroll_to.get()) {
        scroll(target);
    }
}

pub fn scroll_viewport(js: *const NsJs, x: f64, y: f64) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let x = if !x.is_finite() || x < 0.0 { 0.0 } else { x };
    let y = if !y.is_finite() || y < 0.0 { 0.0 } else { y };
    let (x, y) = match js.st().viewport_scroll.get() {
        Some(scroll) => scroll(x, y),
        None => (x, y),
    };
    glue::note_viewport_scroll(js, x, y);
}

pub fn flush_layout(js: *const NsJs) {
    state(js, (), |state| {
        let Some(flush) = state.layout_flush.get() else {
            return;
        };
        if state.in_layout_flush.get() {
            return;
        }
        state.in_layout_flush.set(true);
        flush();
        state.in_layout_flush.set(false);
    });
}

pub fn submit_form(js: *const NsJs, form: Node<'_>, submitter: Option<Node<'_>>) {
    if (super::frames_hooks().submit_form)(js, form, submitter) {
        return;
    }
    if let Some(submit) = state(js, None, |state| state.form_submit.get()) {
        submit(form, submitter);
    }
}

pub fn load_delayed(js: *const NsJs) -> bool {
    state(js, None, |state| state.load_delay.get()).is_some_and(|delayed| delayed())
}

pub fn clipboard_write(js: *const NsJs, text: &CStr) -> Option<bool> {
    let write = state(js, None, |state| state.clipboard_write.get())?;
    Some(write(text))
}

pub fn selection_copyable(js: *const NsJs) -> bool {
    state(js, false, |state| {
        state.selection_has_range.get() && state.clipboard_write.is_set()
    })
}

pub fn copy_selection(js: *const NsJs) -> bool {
    if !selection_copyable(js) {
        return false;
    }
    let Some((write, text)) = state(js, None, |state| {
        let write = state.clipboard_write.get()?;
        Some((
            write,
            state.selection_text.borrow().clone().unwrap_or_default(),
        ))
    }) else {
        return false;
    };
    write(&text)
}

pub fn has_selection_commands(js: *const NsJs) -> bool {
    state(js, false, |state| state.selection_cmd.is_set())
}

pub fn selection_command(js: *const NsJs, cmd: &CStr) -> bool {
    state(js, None, |state| state.selection_cmd.get()).is_some_and(|command| command(cmd))
}

pub fn selection_state(js: *const NsJs) -> Selection {
    state(
        js,
        Selection {
            text: None,
            rect: [0.0; 4],
            has_range: false,
        },
        |state| Selection {
            text: state.selection_text.borrow().clone(),
            rect: state.selection_rect.get(),
            has_range: state.selection_has_range.get(),
        },
    )
}

pub fn set_selection(js: *const NsJs, text: Option<&CStr>, has_range: bool, rect: [f64; 4]) {
    state(js, (), |state| {
        state
            .selection_text
            .replace(Some(text.map_or_else(CString::default, CStr::to_owned)));
        state.selection_has_range.set(has_range);
        state.selection_rect.set(rect);
    });
}

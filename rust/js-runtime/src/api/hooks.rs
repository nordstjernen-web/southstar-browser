//! Southstar — the embedder hooks a page calls for binding crates: navigation, downloads, window actions, repaints, scrolling, layout flushes, form submission, the load delay, the clipboard and the selection.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean};

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
        pub fn $name(js: *const NsJs, hook: Option<$ty>, data: *mut c_void) {
            state(js, (), |state| state.$field.set(hook, data));
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
hook_setter!(set_style_flush_hook, style_flush, DataFn);
hook_setter!(set_load_delay_hook, load_delay, DelayFn);

pub fn navigate(js: *const NsJs, url: &CStr, reload: bool) -> bool {
    state(js, false, |state| match state.navigate.get() {
        Some((navigate, data)) => {
            unsafe { navigate(url.as_ptr(), GBoolean::from(reload), data) };
            true
        }
        None => false,
    })
}

pub fn can_navigate(js: *const NsJs) -> bool {
    state(js, false, |state| state.navigate.is_set())
}

pub fn soft_navigate(js: *const NsJs, url: &CStr, replace: bool) {
    state(js, (), |state| {
        if let Some((navigate, data)) = state.soft_nav.get() {
            unsafe { navigate(url.as_ptr(), GBoolean::from(replace), data) };
        }
    });
}

pub fn fragment_navigated(js: *const NsJs, url: &CStr) {
    state(js, (), |state| {
        if let Some((navigated, data)) = state.fragment_nav.get() {
            unsafe { navigated(url.as_ptr(), data) };
        }
    });
}

pub fn request_download(js: *const NsJs, url: &CStr, filename: Option<&CStr>) -> bool {
    let filename = filename.map_or(core::ptr::null(), CStr::as_ptr);
    state(js, false, |state| match state.download.get() {
        Some((download, data)) => {
            unsafe { download(url.as_ptr(), filename, data) };
            true
        }
        None => false,
    })
}

pub fn window_action(js: *const NsJs, action: &CStr) {
    state(js, (), |state| {
        if let Some((window_action, data)) = state.window_action.get() {
            unsafe { window_action(action.as_ptr(), data) };
        }
    });
}

pub fn has_window_action(js: *const NsJs) -> bool {
    state(js, false, |state| state.window_action.is_set())
}

pub fn request_repaint(js: *const NsJs) {
    state(js, (), |state| {
        if let Some((repaint, data)) = state.repaint.get() {
            unsafe { repaint(data) };
        }
    });
}

pub fn notify_scroll_to(js: *const NsJs, target: Node<'_>) {
    state(js, (), |state| {
        if let Some((scroll, data)) = state.scroll_to.get() {
            unsafe { scroll(target.as_ptr(), data) };
        }
    });
}

pub fn scroll_viewport(js: *const NsJs, x: f64, y: f64) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let mut x = if !x.is_finite() || x < 0.0 { 0.0 } else { x };
    let mut y = if !y.is_finite() || y < 0.0 { 0.0 } else { y };
    if let Some((scroll, data)) = js.st().viewport_scroll.get() {
        unsafe { scroll(&mut x, &mut y, data) };
    }
    glue::note_viewport_scroll(js, x, y);
}

pub fn flush_style(js: *const NsJs) {
    let restyled = state(js, false, |state| {
        let Some((flush, data)) = state.style_flush.get() else {
            return false;
        };
        if state.in_layout_flush.get() {
            return true;
        }
        state.in_layout_flush.set(true);
        unsafe { flush(data) };
        state.in_layout_flush.set(false);
        true
    });
    if !restyled {
        flush_layout(js);
    }
}

pub fn flush_layout(js: *const NsJs) {
    state(js, (), |state| {
        let Some((flush, data)) = state.layout_flush.get() else {
            return;
        };
        if state.in_layout_flush.get() {
            return;
        }
        state.in_layout_flush.set(true);
        unsafe { flush(data) };
        state.in_layout_flush.set(false);
    });
}

pub fn submit_form(js: *const NsJs, form: Node<'_>, submitter: Option<Node<'_>>) {
    if (super::frames_hooks().submit_form)(js, form, submitter) {
        return;
    }
    let submitter = Node::ptr_or_null(submitter);
    state(js, (), |state| {
        if let Some((submit, data)) = state.form_submit.get() {
            unsafe { submit(form.as_ptr(), submitter, data) };
        }
    });
}

pub fn load_delayed(js: *const NsJs) -> bool {
    state(js, false, |state| match state.load_delay.get() {
        Some((delayed, data)) => (unsafe { delayed(data) }) != 0,
        None => false,
    })
}

pub fn clipboard_write(js: *const NsJs, text: &CStr) -> Option<bool> {
    state(js, None, |state| {
        let (write, data) = state.clipboard_write.get()?;
        Some(unsafe { write(text.as_ptr(), data) } != 0)
    })
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
    state(js, false, |state| {
        let Some((write, data)) = state.clipboard_write.get() else {
            return false;
        };
        let text = state.selection_text.get();
        let text = if text.is_null() {
            c"".as_ptr()
        } else {
            text.cast_const()
        };
        unsafe { write(text, data) != 0 }
    })
}

pub fn has_selection_commands(js: *const NsJs) -> bool {
    state(js, false, |state| state.selection_cmd.is_set())
}

pub fn selection_command(js: *const NsJs, cmd: &CStr) -> bool {
    state(js, false, |state| match state.selection_cmd.get() {
        Some((command, data)) => (unsafe { command(cmd.as_ptr(), data) }) != 0,
        None => false,
    })
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
            text: glue::text(state.selection_text.get()).map(CStr::to_owned),
            rect: state.selection_rect.get(),
            has_range: state.selection_has_range.get(),
        },
    )
}

pub fn set_selection(js: *const NsJs, text: Option<&CStr>, has_range: bool, rect: [f64; 4]) {
    state(js, (), |state| {
        let text = glib::strdup(text.map_or(&b""[..], CStr::to_bytes));
        PageState::replace_owned(&state.selection_text, text);
        state.selection_has_range.set(has_range);
        state.selection_rect.set(rect);
    });
}

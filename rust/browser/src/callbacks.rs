//! Southstar — what a page's script engine and media ask of the embedder: console lines, navigations, downloads, window actions, the clipboard, selection commands, audio and media control.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};

use southstar_glib::GStr;

use crate::ffi::{self, NsBrowser};

const CONSOLE_MAX: usize = 256 * 1024;
const CONSOLE_KEEP: usize = 192 * 1024;
const PENDING_CLIPBOARD_MAX: usize = 1048576;
const PENDING_AUDIO_MAX: usize = 15000;

pub fn js_log(b: &NsBrowser, line: &CStr) {
    ffi::debug_log_console(line);
    b.console_buf.create();
    if b.console_buf.len() > CONSOLE_MAX {
        b.console_buf
            .erase_front(b.console_buf.len() - CONSOLE_KEEP);
    }
    b.console_buf.append(line.to_bytes());
    b.console_buf.append(b"\n");
}

pub fn allows_navigation_url(b: &NsBrowser, url: Option<&CStr>) -> bool {
    if !url.is_some_and(|u| u.to_bytes().starts_with(b"file:")) {
        return true;
    }
    b.base_url
        .get()
        .is_some_and(|base| base.to_bytes().starts_with(b"file:"))
}

pub fn resolve_navigation(b: &NsBrowser, href: &CStr) -> Option<GStr> {
    let abs = ffi::url_resolve(b.base_url.get(), href);
    if !allows_navigation_url(b, abs.as_deref()) {
        return None;
    }
    abs
}

pub fn js_navigate(b: &NsBrowser, url: &CStr) {
    if url.is_empty() {
        return;
    }
    b.pending_nav.clear();
    let nav = resolve_navigation(b, url);
    b.pending_nav.adopt(nav);
}

pub fn js_download(b: &NsBrowser, url: &CStr, filename: Option<&CStr>) {
    if url.is_empty() {
        return;
    }
    let abs = ffi::url_resolve(b.base_url.get(), url);
    let target = abs.as_deref().unwrap_or(url);
    if !allows_navigation_url(b, Some(target)) {
        return;
    }
    let mut entry = target.to_bytes().to_vec();
    entry.push(b'\t');
    entry.extend_from_slice(filename.map_or(&b""[..], CStr::to_bytes));
    b.pending_download.adopt(ffi::gstr_from(&entry));
}

pub fn window_action(b: &NsBrowser, action: &CStr) {
    if action.is_empty() {
        return;
    }
    b.pending_window_action.set(Some(action));
}

pub fn clipboard_write(b: &NsBrowser, text: &CStr) -> bool {
    if text.to_bytes().len() > PENDING_CLIPBOARD_MAX {
        return false;
    }
    b.pending_clipboard.set(Some(text));
    true
}

pub fn sync_js_selection(b: &NsBrowser) {
    let Some(js) = b.js() else {
        return;
    };
    let text = b.selection_text();
    let (ok, mut rect) = b.selection_bounds();
    if ok {
        rect[0] -= b.cur_scroll_x.get();
        rect[1] -= b.cur_scroll_y.get();
    }
    js.set_selection(text.as_deref(), b.selection_has_range(), rect);
}

pub fn selection_cmd(b: &NsBrowser, command: &CStr) -> bool {
    let Some(layout) = b.layout() else {
        return false;
    };
    let ok = match command.to_bytes() {
        b"selectAll" => b.selection_select_all(layout),
        b"unselect" => {
            b.selection_clear();
            true
        }
        _ => false,
    };
    if ok {
        sync_js_selection(b);
        b.dirty.set(true);
    }
    ok
}

pub fn js_audio(b: &NsBrowser, command: &CStr) {
    if command.is_empty() {
        return;
    }
    if ffi::env_set(c"NS_DBG_AUDIO") {
        let mut line = b"[audio-cmd] ".to_vec();
        line.extend_from_slice(command.to_bytes());
        line.push(b'\n');
        ffi::printerr(&line);
    }
    b.pending_audio.create();
    if b.pending_audio.len() >= PENDING_AUDIO_MAX {
        return;
    }
    b.pending_audio.append(command.to_bytes());
    b.pending_audio.append(b"\n");
}

pub fn media_seek(b: Option<&NsBrowser>, node: *const c_void, seconds: f64) -> bool {
    if ffi::env_set(c"NS_DBG_AUDIO") {
        ffi::printerr_double(c"[media-seek] to=%.3f\n", seconds);
    }
    let Some(videos) = b.and_then(NsBrowser::videos) else {
        return false;
    };
    videos.seek_node(node, seconds, ffi::monotonic_us())
}

pub fn media_play(b: &NsBrowser, node: *const c_void, play: bool) {
    let Some(videos) = b.videos() else {
        return;
    };
    videos.set_node_playing(node, play, ffi::monotonic_us());
    if let Some(js) = b.js() {
        js.request_repaint();
    }
}

pub fn media_muted(b: &NsBrowser, node: *const c_void, muted: bool) {
    let Some(videos) = b.videos() else {
        return;
    };
    videos.set_node_muted(node, muted);
    if let Some(js) = b.js() {
        js.request_repaint();
    }
}

pub fn media_volume(b: &NsBrowser, node: *const c_void, volume: f64) {
    let Some(videos) = b.videos() else {
        return;
    };
    videos.set_node_volume(node, volume);
    if let Some(js) = b.js() {
        js.request_repaint();
    }
}

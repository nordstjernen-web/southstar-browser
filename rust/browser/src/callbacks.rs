//! Southstar — what a page's script engine and media ask of the embedder, and its camera answer: console lines, navigations, downloads, window actions, the clipboard, selection commands, audio and media control.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use std::ffi::CString;

use southstar_dom::Node;

use crate::ffi::{self, NsBrowser};
use crate::open::UNSAFE_CONTINUE_SCHEME;

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

fn continues_interstitial(url: &[u8], from: Option<&[u8]>) -> bool {
    let mut target = url;
    while let Some(rest) = target.strip_prefix(UNSAFE_CONTINUE_SCHEME) {
        target = rest;
    }
    let host = |url: &[u8]| southstar_url::bytes::host_from(url).filter(|h| !h.is_empty());
    match (host(target), from.and_then(host)) {
        (Some(target), Some(from)) => target.eq_ignore_ascii_case(&from),
        _ => false,
    }
}

pub fn allows_navigation_url(b: &NsBrowser, url: Option<&CStr>) -> bool {
    let Some(url) = url.map(CStr::to_bytes) else {
        return true;
    };
    let base = b.base_url.get();
    let from = base.as_deref().map(CStr::to_bytes);
    if url.starts_with(b"file:") {
        return from.is_some_and(|f| f.starts_with(b"file:"));
    }
    if southstar_net::is_privileged_page(url) {
        return from.is_some_and(southstar_net::is_chrome_page);
    }
    if url.starts_with(UNSAFE_CONTINUE_SCHEME) {
        return continues_interstitial(url, from);
    }
    true
}

pub fn resolve_navigation(b: &NsBrowser, href: &CStr) -> Option<CString> {
    let abs = ffi::url_resolve(b.base_url.get().as_deref(), href);
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
    let abs = ffi::url_resolve(b.base_url.get().as_deref(), url);
    let target = abs.as_deref().unwrap_or(url);
    if !allows_navigation_url(b, Some(target)) {
        return;
    }
    let mut entry = target.to_bytes().to_vec();
    entry.push(b'\t');
    entry.extend_from_slice(filename.map_or(&b""[..], CStr::to_bytes));
    b.pending_download.adopt(ffi::owned_text(&entry));
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
    if ffi::env_set("NS_DBG_AUDIO") {
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

pub fn media_seek(b: Option<&NsBrowser>, node: Node<'_>, seconds: f64) -> bool {
    if ffi::env_set("NS_DBG_AUDIO") {
        ffi::printerr(format!("[media-seek] to={seconds:.3}\n").as_bytes());
    }
    let Some(videos) = b.and_then(NsBrowser::videos) else {
        return false;
    };
    videos.seek_node(node, seconds, ffi::monotonic_us())
}

pub fn media_play(b: &NsBrowser, node: Node<'_>, play: bool) {
    let Some(videos) = b.videos() else {
        return;
    };
    videos.set_node_playing(node, play, ffi::monotonic_us());
    if let Some(js) = b.js() {
        js.request_repaint();
    }
}

pub fn media_muted(b: &NsBrowser, node: Node<'_>, muted: bool) {
    let Some(videos) = b.videos() else {
        return;
    };
    videos.set_node_muted(node, muted);
    if let Some(js) = b.js() {
        js.request_repaint();
    }
}

pub fn media_volume(b: &NsBrowser, node: Node<'_>, volume: f64) {
    let Some(videos) = b.videos() else {
        return;
    };
    videos.set_node_volume(node, volume);
    if let Some(js) = b.js() {
        js.request_repaint();
    }
}

pub fn resolve_camera(b: Option<&NsBrowser>, origin: Option<&CStr>, allow: bool) {
    if let Some(origin) = origin {
        southstar_camera::set_decision(origin.to_bytes(), allow);
    }
    if let Some(js) = b.and_then(NsBrowser::js) {
        let src = if allow {
            c"__nd_camera_resolve_pending(true)"
        } else {
            c"__nd_camera_resolve_pending(false)"
        };
        drop(js.eval(src, c"camera-resolve"));
    }
}

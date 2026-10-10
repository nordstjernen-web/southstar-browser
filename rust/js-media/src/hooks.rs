//! Southstar — the per-page media hooks the embedder installs: the audio side-channel, seek/play/mute/volume notifications and the MSE stream sinks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ffi::{
    AudioFn, Element, Js, MseBufferedFn, MseBytesFn, MseFn, MseRemoveFn, SeekFn, ToggleFn, VolumeFn,
};

#[derive(Clone, Copy)]
pub(crate) struct Hook<F: Copy> {
    pub f: F,
    pub data: usize,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Hooks {
    pub audio: Option<Hook<AudioFn>>,
    pub seek: Option<Hook<SeekFn>>,
    pub play: Option<Hook<ToggleFn>>,
    pub muted: Option<Hook<ToggleFn>>,
    pub volume: Option<Hook<VolumeFn>>,
    pub mse: Option<Hook<MseFn>>,
    pub mse_buffered: Option<Hook<MseBufferedFn>>,
    pub mse_remove: Option<Hook<MseRemoveFn>>,
    pub mse_bytes: Option<Hook<MseBytesFn>>,
}

#[derive(Clone, Copy, Default)]
struct Page {
    hooks: Hooks,
    next_audio_token: u32,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn init(js: Js) {
    if js.is_null() {
        return;
    }
    PAGES.with(|pages| {
        pages.borrow_mut().entry(js).or_default();
    });
}

pub(crate) fn teardown(js: Js) {
    let _ = PAGES.try_with(|pages| pages.borrow_mut().remove(&js));
}

pub(crate) fn hooks(js: Js) -> Hooks {
    PAGES
        .try_with(|pages| pages.borrow().get(&js).map(|page| page.hooks))
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn update(js: Js, f: impl FnOnce(&mut Hooks)) {
    if js.is_null() {
        return;
    }
    PAGES.with(|pages| f(&mut pages.borrow_mut().entry(js).or_default().hooks));
}

pub(crate) fn next_audio_token(js: Js) -> u32 {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let page = pages.entry(js).or_default();
        page.next_audio_token = page.next_audio_token.wrapping_add(1);
        page.next_audio_token
    })
}

impl Hooks {
    pub fn emit_audio(&self, command: &str) {
        if let Some(hook) = self.audio {
            crate::ffi::call_audio(hook, command);
        }
    }

    pub fn notify_play(&self, element: Element, play: bool) {
        if let Some(hook) = self.play {
            crate::ffi::call_toggle(hook, element, play);
        }
    }

    pub fn notify_muted(&self, element: Element, muted: bool) {
        if let Some(hook) = self.muted {
            crate::ffi::call_toggle(hook, element, muted);
        }
    }

    pub fn notify_volume(&self, element: Element, volume: f64) {
        if let Some(hook) = self.volume {
            crate::ffi::call_volume(hook, element, volume);
        }
    }

    pub fn seek(&self, element: Element, seconds: f64) -> bool {
        self.seek
            .is_some_and(|hook| crate::ffi::call_seek(hook, element, seconds))
    }
}

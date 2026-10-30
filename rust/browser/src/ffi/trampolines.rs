//! Southstar — the callbacks a page hands the script engine, the video cache, image sessions and GLib timeouts, each forwarding to the lifecycle.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::ops::ControlFlow;
use std::rc::Rc;

use southstar_dom::Node;
use southstar_glib::{self as glib, SourceId};
use southstar_js_runtime::api::{self, NavigationTiming};

use super::{Js, NsBrowser, Session, Videos, browser_for, monotonic_us};
use crate::{build, callbacks, images, page, scroll, settle};

#[derive(Clone, Copy)]
struct PageRef(*const NsBrowser);

impl PageRef {
    fn of(b: &NsBrowser) -> PageRef {
        PageRef(b)
    }

    fn get(self) -> Option<&'static NsBrowser> {
        browser_for(self.0.cast_mut().cast())
    }

    fn with(self, f: impl FnOnce(&NsBrowser)) {
        if let Some(b) = self.get() {
            f(b);
        }
    }

    fn videos(self) -> Option<Videos> {
        self.get().and_then(NsBrowser::videos)
    }
}

fn audio_command(page: PageRef, command: &CStr) {
    page.with(|b| callbacks::js_audio(b, command));
}

fn video_event(page: PageRef, node: Node<'_>, kind: &CStr, value: f64) {
    if let Some(js) = page.get().and_then(NsBrowser::js) {
        southstar_js_media::video_event(js.page(), node, kind, value);
    }
}

pub fn new_js(b: &NsBrowser, timing: Option<NavigationTiming>) -> Option<Js> {
    let page = PageRef::of(b);
    let callbacks = api::PageCallbacks {
        log: Some(Rc::new(move |line: &CStr| {
            page.with(|b| callbacks::js_log(b, line));
        })),
        mutated: Some(Rc::new(move || page.with(|b| b.dirty.set(true)))),
        navigate: Some(Rc::new(move |url: &CStr, _reload| {
            page.with(|b| callbacks::js_navigate(b, url));
        })),
    };
    southstar_js_page::install_runtime_hooks();
    api::new_page(callbacks, timing).map(Js::new)
}

pub fn wire_js(b: &NsBrowser, js: Js) {
    let page = PageRef::of(b);
    let js_page = js.page();
    api::set_load_delay_hook(
        js_page,
        Some(Rc::new(move || {
            page.get().is_some_and(images::load_waits_for_images)
        })),
    );
    js.set_style_table(b.styles());
    js.set_image_cache(b.images());
    js.set_anim(b.anim());
    api::set_form_submit_hook(
        js_page,
        Some(Rc::new(
            move |form: Node<'_>, submitter: Option<Node<'_>>| {
                page.with(|b| build::js_form_submit(b, form, submitter));
            },
        )),
    );
    api::set_layout_flush_hook(js_page, Some(Rc::new(move || page.with(page::flush))));
    api::set_viewport_scroll_hook(
        js_page,
        Some(Rc::new(move |x, y| match page.get() {
            Some(b) => scroll::js_viewport_scroll(b, x, y),
            None => (x, y),
        })),
    );
    api::set_scroll_to_hook(
        js_page,
        Some(Rc::new(move |target: Node<'_>| {
            page.with(|b| scroll::js_scroll_to(b, Some(target)));
        })),
    );
    api::set_fragment_nav_hook(
        js_page,
        Some(Rc::new(move |url: &CStr| {
            page.with(|b| scroll::js_fragment_navigate(b, url));
        })),
    );
    api::set_soft_nav_hook(
        js_page,
        Some(Rc::new(move |url: &CStr, replace| {
            page.with(|b| scroll::js_soft_navigate(b, url, replace));
        })),
    );
    api::set_download_hook(
        js_page,
        Some(Rc::new(move |url: &CStr, filename: Option<&CStr>| {
            page.with(|b| callbacks::js_download(b, url, filename));
        })),
    );
    api::set_clipboard_write_hook(
        js_page,
        Some(Rc::new(move |text: &CStr| {
            page.get()
                .is_some_and(|b| callbacks::clipboard_write(b, text))
        })),
    );
    api::set_selection_cmd_hook(
        js_page,
        Some(Rc::new(move |command: &CStr| {
            page.get()
                .is_some_and(|b| callbacks::selection_cmd(b, command))
        })),
    );
    southstar_js_media::set_audio_hook(
        js_page,
        Some(Rc::new(move |command: &CStr| audio_command(page, command))),
    );
    southstar_js_media::set_seek_hook(
        js_page,
        Some(Rc::new(move |node, seconds| {
            callbacks::media_seek(page.get(), node, seconds)
        })),
    );
    southstar_js_media::set_play_hook(
        js_page,
        Some(Rc::new(move |node, play| {
            page.with(|b| callbacks::media_play(b, node, play));
        })),
    );
    southstar_js_media::set_muted_hook(
        js_page,
        Some(Rc::new(move |node, muted| {
            page.with(|b| callbacks::media_muted(b, node, muted));
        })),
    );
    southstar_js_media::set_mse_hook(
        js_page,
        Some(Rc::new(move |stream_id, kind, data: Option<&[u8]>| {
            let Some(videos) = page.videos() else {
                return false;
            };
            match data {
                Some(data) => videos.mse_append(stream_id, kind, data),
                None => {
                    videos.mse_eos(stream_id);
                    true
                }
            }
        })),
    );
    southstar_js_media::set_mse_buffered_hook(
        js_page,
        Some(Rc::new(move |stream_id, kind| {
            page.videos()
                .map_or((0.0, 0.0), |videos| videos.mse_buffered(stream_id, kind))
        })),
    );
    southstar_js_media::set_mse_remove_hook(
        js_page,
        Some(Rc::new(move |stream_id, kind, start, end| {
            page.videos()
                .is_some_and(|videos| videos.mse_remove(stream_id, kind, start, end))
        })),
    );
    southstar_js_media::set_mse_bytes_hook(
        js_page,
        Some(Rc::new(move |stream_id, kind| {
            page.videos()
                .map_or(0, |videos| videos.mse_bytes(stream_id, kind))
        })),
    );
    southstar_js_media::set_volume_hook(
        js_page,
        Some(Rc::new(move |node, volume| {
            page.with(|b| callbacks::media_volume(b, node, volume));
        })),
    );
    api::set_window_action_hook(
        js_page,
        Some(Rc::new(move |action: &CStr| {
            page.with(|b| callbacks::window_action(b, action));
        })),
    );
}

pub fn wire_videos(b: &NsBrowser, videos: Videos) {
    let page = PageRef::of(b);
    videos.set_js_sink(Some(Rc::new(move |node, kind, value| {
        video_event(page, node, kind, value);
    })));
    videos.set_audio_sink(Some(Rc::new(move |command| audio_command(page, command))));
}

pub fn add_media_events_timeout(b: &NsBrowser) -> SourceId {
    let page = PageRef::of(b);
    glib::timeout_add_local_once(0, move || page.with(images::fire_media_events))
}

pub fn start_image_session(b: &NsBrowser, viewport_h: f64) -> (Option<Session>, bool) {
    let base = b.base_url.get();
    let (Some(root), Some(base), Some(cache)) = (b.layout(), base.as_deref(), b.images()) else {
        return (None, false);
    };
    let fetch = southstar_engine::ImageFetch {
        root,
        base,
        cache,
        scroll_y: b.cur_scroll_y.get(),
        viewport_h,
    };
    let page = PageRef::of(b);
    let arrived = Rc::new(move || page.with(images::image_arrived));
    southstar_engine::start_image_session(&fetch, &mut b.requested_images(), arrived)
}

pub fn run_settle_loop(b: &NsBrowser, settle_ms: c_int) {
    let main_loop = glib::MainLoop::new();
    let mut state = settle::SettleState::new(monotonic_us() + i64::from(settle_ms) * 1000);
    glib::scope(|scope| {
        scope.timeout_add(settle_ms.max(0).unsigned_abs(), || {
            main_loop.quit();
            ControlFlow::Continue(())
        });
        scope.timeout_add(16, || {
            if settle::settle_tick(b, &mut state) {
                main_loop.quit();
            }
            ControlFlow::Continue(())
        });
        main_loop.run();
    });
}

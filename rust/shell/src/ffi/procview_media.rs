//! Southstar — a tab's audio and video helper processes, their shared-memory clock and frame queue, and the video overlay drawn over the page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{c_char, c_int};
use core::ops::ControlFlow;
use core::sync::atomic::Ordering;
use std::collections::HashMap;
use std::ffi::{CString, OsString};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use southstar_cairo::{Context, ContextRef, Format, Surface, SurfaceRef};
use southstar_glib::{self as glib, MainContext};
use southstar_gtk::{self as gtk, Widget};
use southstar_gtk_ffi::{GrapheneRect, GtkSnapshot};
use southstar_media_shm::{
    AUDIO_CLOCK_MAGIC, AUDIO_CLOCK_SLOTS, AUDIO_CLOCK_VERSION, Geometry, SharedMemory,
    VIDEO_RING_MAGIC, VIDEO_RING_VERSION, VideoRing, VideoRingHdr,
};

use super::procview_result::{ns_proc_view_local_page, ns_proc_view_push_video_event};
use super::procview_state::NsProcView;
use super::procview_view::{
    ns_proc_view_area, ns_proc_view_media, ns_proc_view_ref, ns_proc_view_unref,
};
use super::procview_worker::{is_executable, sibling_program};
use crate::procview::media::{
    self, FrameRing, HelperLine, Rect, SlotView, VideoCommand, clock_position, pick_frame,
};

const RESYNC_INTERVAL_US: i64 = 950_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pipe {
    Audio,
    Video,
}

#[derive(Clone, Copy)]
struct Reader {
    view: *mut NsProcView,
    pipe: Pipe,
}

thread_local! {
    static COMPOSITE: Cell<(i64, u32, u32)> = const { Cell::new((0, 0, 0)) };
    static READERS: RefCell<HashMap<u64, Reader>> = RefCell::new(HashMap::new());
    static NEXT_READER: Cell<u64> = const { Cell::new(1) };
}

struct Helper {
    child: Child,
    stdin: ChildStdin,
    reader: u64,
}

impl FrameRing for VideoRing<'_> {
    fn slot(&self, index: u32) -> SlotView {
        let slot = VideoRing::slot(self, index);
        SlotView {
            sequence: slot.sequence,
            generation: slot.generation,
            pts: slot.pts,
        }
    }
}

fn valid_header(ring: &VideoRing<'_>) -> bool {
    ring.magic() == VIDEO_RING_MAGIC
        && ring.version() == VIDEO_RING_VERSION
        && ring.nslots() == media::RING_SLOTS
        && core::mem::size_of::<VideoRingHdr>()
            + ring.nslots() as usize * ring.geometry().frame_bytes as usize
            <= ring.mapped_len()
}

pub struct Media {
    audio: Option<Helper>,
    audio_clock: Option<SharedMemory>,
    audio_clock_name: Vec<u8>,
    video: Option<Helper>,
    vring: Option<SharedMemory>,
    fallback: Option<Surface>,
    token: Vec<u8>,
    shm_name: Vec<u8>,
    rect: Rect,
    playing: bool,
    tick_armed: bool,
    tick_count: u32,
    resync_us: i64,
    sequence: u32,
    generation: u32,
    slot: u32,
    pts: f64,
    presented: u64,
    dropped: u64,
}

fn debug_audio() -> bool {
    env_set("NS_DBG_AUDIO")
}

fn env_set(name: &str) -> bool {
    glib::env_bytes(name).is_some()
}

fn err_line(bytes: &[u8]) {
    glib::stderr_write(bytes);
}

fn audio_helper_path() -> PathBuf {
    sibling_program(if cfg!(windows) {
        "southstar-audio.exe"
    } else {
        "southstar-audio"
    })
}

fn video_helper_path() -> PathBuf {
    sibling_program(if cfg!(windows) {
        "southstar-video.exe"
    } else {
        "southstar-video"
    })
}

fn env_value(value: Option<Vec<u8>>) -> Option<OsString> {
    let value = value.filter(|v| !v.is_empty() && !v.contains(&0))?;
    glib::path_from_bytes(&value).map(PathBuf::into_os_string)
}

fn apply_proxy_env(command: &mut Command) {
    if let Some(proxy) = env_value(southstar_net::proxy_override()) {
        command.env("all_proxy", &proxy);
        command.env("http_proxy", &proxy);
        command.env("https_proxy", &proxy);
    } else {
        if let Some(http) = env_value(southstar_net::http_proxy()) {
            command.env("http_proxy", http);
        }
        if let Some(https) = env_value(southstar_net::https_proxy()) {
            command.env("https_proxy", https);
        }
    }
    if let Some(no_proxy) = env_value(southstar_net::no_proxy()) {
        command.env("no_proxy", no_proxy);
    }
    if let Some(ca) = env_value(southstar_net::ca_bundle_path()) {
        command.env("CURL_CA_BUNDLE", ca);
    }
}

fn next_reader() -> u64 {
    NEXT_READER.with(|next| {
        let key = next.get();
        next.set(key + 1);
        key
    })
}

fn read_lines(stdout: ChildStdout, context: &MainContext, key: u64) {
    let mut input = BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        if !matches!(input.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            break;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if let Some(nul) = line.iter().position(|&b| b == 0) {
            line.truncate(nul);
        }
        context.defer(move || deliver(key, Some(line)));
    }
    context.defer(move || deliver(key, None));
}

fn watch(v: *mut NsProcView, pipe: Pipe, stdout: ChildStdout) -> u64 {
    let key = next_reader();
    let view = ns_proc_view_ref(v);
    READERS.with(|readers| readers.borrow_mut().insert(key, Reader { view, pipe }));
    let context = MainContext::global();
    let started = std::thread::Builder::new()
        .name("ns-media-pipe".into())
        .spawn(move || read_lines(stdout, &context, key));
    if started.is_err() {
        finish(key);
    }
    key
}

fn finish(key: u64) {
    if let Some(reader) = READERS.with(|readers| readers.borrow_mut().remove(&key)) {
        ns_proc_view_unref(reader.view);
    }
}

fn deliver(key: u64, line: Option<Vec<u8>>) {
    let Some(reader) = READERS.with(|readers| readers.borrow().get(&key).copied()) else {
        return;
    };
    let v = reader.view;
    match (reader.pipe, line) {
        (Pipe::Audio, Some(line)) => audio_feedback(v, &line),
        (Pipe::Video, Some(line)) => {
            if let Some(m) = media_of(v)
                && m.video.as_ref().is_some_and(|helper| helper.reader == key)
            {
                video_handle_line(v, m, &line);
            }
        }
        (pipe, None) => {
            if pipe == Pipe::Audio
                && let Some(m) = media_of(v)
            {
                m.clock_unmap();
            }
            finish(key);
        }
    }
}

impl Helper {
    fn spawn(v: *mut NsProcView, path: &Path, proxy: bool, pipe: Pipe) -> Result<Helper, Vec<u8>> {
        let mut command = Command::new(path);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if proxy {
            apply_proxy_env(&mut command);
        }
        let mut child = command
            .spawn()
            .map_err(|error| error.to_string().into_bytes())?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(b"no pipes".to_vec());
        };
        let reader = watch(v, pipe, stdout);
        Ok(Helper {
            child,
            stdin,
            reader,
        })
    }

    fn write(&mut self, line: &[u8]) {
        let _ = self.stdin.write_all(line);
    }

    fn flush(&mut self) {
        let _ = self.stdin.flush();
    }

    fn send(&mut self, line: &[u8]) {
        self.write(line);
        self.flush();
    }

    fn pid(&mut self) -> c_int {
        match self.child.try_wait() {
            Ok(None) => c_int::try_from(self.child.id()).unwrap_or(-1),
            _ => -1,
        }
    }

    fn shutdown(mut self) {
        self.send(b"quit\n");
        let _ = self.child.kill();
        let Helper { mut child, .. } = self;
        let _ = std::thread::Builder::new()
            .name("ns-media-reap".into())
            .spawn(move || child.wait());
    }
}

fn stream_url_allowed(url: &[u8], subdir: &str) -> bool {
    let Some((dir, name)) = media::stream_file_parts(url) else {
        return false;
    };
    let Some(dir) = glib::path_from_bytes(dir).filter(|d| d.is_absolute()) else {
        return false;
    };
    let Some(cache) = glib::path_from_bytes(&glib::user_cache_dir()) else {
        return false;
    };
    let streams = cache.join("southstar").join(subdir);
    let (Ok(streams), Ok(dir)) = (std::fs::canonicalize(streams), std::fs::canonicalize(dir))
    else {
        return false;
    };
    let Some(name) = glib::path_from_bytes(name) else {
        return false;
    };
    dir == streams
        && std::fs::symlink_metadata(dir.join(name)).is_ok_and(|m| m.file_type().is_file())
}

fn audio_url_allowed(v: *mut NsProcView, url: &[u8]) -> bool {
    if url.starts_with(b"http://") || url.starts_with(b"https://") || url.starts_with(b"data:") {
        return true;
    }
    if !url.starts_with(b"file://") {
        return false;
    }
    if ns_proc_view_local_page(v) {
        return true;
    }
    stream_url_allowed(url, "msaudio")
}

fn audio_command_allowed(v: *mut NsProcView, cmd: &[u8]) -> bool {
    media::line_ok(cmd) && media::open_url(cmd).is_none_or(|url| audio_url_allowed(v, url))
}

fn video_command_allowed(cmd: &[u8]) -> bool {
    media::line_ok(cmd) && media::open_url(cmd).is_none_or(|url| stream_url_allowed(url, "msvideo"))
}

fn media_of(v: *mut NsProcView) -> Option<&'static mut Media> {
    // SAFETY: the view's media is null or the Box ns_pv_media_new made, freed only with the view; it is used on the main thread alone.
    unsafe { ns_proc_view_media(v).cast::<Media>().as_mut() }
}

fn area(v: *mut NsProcView) -> Option<Widget> {
    // SAFETY: the view's area is the live drawing area it holds, or null once that was destroyed.
    unsafe { Widget::from_ptr(ns_proc_view_area(v)) }
}

fn queue_draw(v: *mut NsProcView) {
    if let Some(area) = area(v) {
        area.queue_draw();
    }
}

fn helper_pid(helper: Option<&mut Helper>) -> c_int {
    helper.map_or(-1, Helper::pid)
}

impl Media {
    fn new() -> Media {
        Media {
            audio: None,
            audio_clock: None,
            audio_clock_name: Vec::new(),
            video: None,
            vring: None,
            fallback: None,
            token: Vec::new(),
            shm_name: Vec::new(),
            rect: Rect::default(),
            playing: false,
            tick_armed: false,
            tick_count: 0,
            resync_us: 0,
            sequence: 0,
            generation: 0,
            slot: 0,
            pts: 0.0,
            presented: 0,
            dropped: 0,
        }
    }

    fn ring(&self) -> Option<VideoRing<'_>> {
        self.vring.as_ref()?.video_ring()
    }

    fn clock_unmap(&mut self) {
        self.audio_clock = None;
        self.audio_clock_name.clear();
    }

    fn clock_adopt(&mut self, name: &[u8]) {
        self.clock_unmap();
        let Some(map) = SharedMemory::open(name, false) else {
            return;
        };
        let valid = map.audio_clock().is_some_and(|clock| {
            clock.magic() == AUDIO_CLOCK_MAGIC
                && clock.version() == AUDIO_CLOCK_VERSION
                && clock.nslots() == AUDIO_CLOCK_SLOTS
        });
        if valid {
            self.audio_clock = Some(map);
            self.audio_clock_name = media::truncated(name);
        }
    }

    fn clock_position(&self, token: &[u8]) -> Option<f64> {
        let clock = self.audio_clock.as_ref()?.audio_clock()?;
        if token.is_empty() {
            return None;
        }
        for index in 0..AUDIO_CLOCK_SLOTS {
            let Some(sample) = clock.sample(index) else {
                continue;
            };
            let mut name = sample.token;
            name[63] = 0;
            let len = name.iter().position(|&b| b == 0).unwrap_or(64);
            if sample.flags & media::CLOCK_USED == 0 || &name[..len] != token {
                continue;
            }
            return clock_position(
                sample.flags,
                sample.monotonic_us,
                sample.position,
                glib::monotonic_us(),
                100_000,
            );
        }
        None
    }

    fn snapshot_current(&mut self) {
        let Some(ring) = self.ring() else {
            return;
        };
        if self.sequence == 0 {
            return;
        }
        let geometry = ring.geometry();
        if ring.magic() != VIDEO_RING_MAGIC || ring.version() != VIDEO_RING_VERSION {
            return;
        }
        let Some(frame) = ring.frame(self.slot, geometry) else {
            return;
        };
        let meta = VideoRing::slot(&ring, self.slot);
        if meta.sequence != self.sequence || meta.generation != self.generation {
            return;
        }
        let Some(copy) = Surface::image(
            Format::Rgb24,
            geometry.width as c_int,
            geometry.height as c_int,
        ) else {
            return;
        };
        copy.as_ref()
            .with_data_mut(|data, stride| frame.copy_rows(data, stride));
        self.fallback = Some(copy);
    }

    fn vring_unmap(&mut self) {
        self.vring = None;
        self.shm_name.clear();
        self.rect.valid = false;
        self.playing = false;
        self.sequence = 0;
        self.generation = 0;
        self.slot = 0;
        self.pts = 0.0;
    }

    fn adopt_ring(&mut self, token: &[u8], name: &[u8]) {
        let was_playing = self.playing;
        let had_rect = self.rect.valid;
        self.snapshot_current();
        if cfg!(unix) && debug_audio() {
            let mut line = b"[shm-adopt] ".to_vec();
            line.extend_from_slice(token);
            line.push(b' ');
            line.extend_from_slice(name);
            line.push(b'\n');
            err_line(&line);
        }
        self.vring_unmap();
        self.playing = was_playing;
        self.rect.valid = had_rect;
        let Some(map) = SharedMemory::open(name, true) else {
            return;
        };
        let valid = map.len() > core::mem::size_of::<VideoRingHdr>()
            && map.video_ring().is_some_and(|ring| valid_header(&ring));
        if valid {
            self.vring = Some(map);
            self.shm_name = media::truncated(name);
            self.token = media::truncated(token);
        }
    }

    fn audio_shutdown(&mut self) {
        if let Some(helper) = self.audio.take() {
            helper.shutdown();
        }
        self.clock_unmap();
    }

    fn video_shutdown(&mut self) {
        self.vring_unmap();
        self.fallback = None;
        if let Some(helper) = self.video.take() {
            helper.shutdown();
        }
    }

    fn video_playing(&self) -> bool {
        self.vring.is_some() && self.playing
    }

    fn draw_surface(&self, cr: &ContextRef<'_>, surface: SurfaceRef<'_>) {
        let fw = surface.width();
        let fh = surface.height();
        if fw <= 0 || fh <= 0 {
            return;
        }
        let (x, y, w, h) = self.rect.draw_box(fw, fh);
        let r = &self.rect;
        cr.save();
        cr.rectangle(r.x, r.y, r.w, r.h);
        cr.clip();
        cr.rectangle(r.clip_x, r.clip_y, r.clip_w, r.clip_h);
        cr.clip();
        cr.translate(x, y);
        cr.scale(w / f64::from(fw), h / f64::from(fh));
        cr.set_source_surface(surface, 0.0, 0.0);
        cr.paint();
        cr.restore();
    }

    fn draw_frame(&self, cr: &ContextRef<'_>, slot: u32, geometry: Geometry) -> bool {
        let Some(frame) = self.ring().and_then(|ring| ring.frame(slot, geometry)) else {
            return false;
        };
        // SAFETY: frame() checked that stride * height pixel bytes lie inside the ring mapping, which self.vring keeps mapped through this call; cairo only reads them as a paint source, and the surface is dropped before returning.
        let surface = unsafe {
            Surface::for_data(
                frame.as_ptr(),
                Format::Rgb24,
                frame.width as c_int,
                frame.height as c_int,
                frame.stride as c_int,
            )
        };
        if surface.as_ref().status().is_err() {
            return false;
        }
        self.draw_surface(cr, surface.as_ref());
        true
    }

    fn draw(&mut self, cr: &ContextRef<'_>, fs: f64) {
        let Some(ring) = self.ring().filter(|_| self.rect.valid) else {
            return;
        };
        let nslots = ring.nslots();
        let geometry = ring.geometry();
        let magic_ok = ring.magic() == VIDEO_RING_MAGIC;
        let version_ok = ring.version() == VIDEO_RING_VERSION;
        let mut frame_drawn = false;
        cr.save();
        cr.scale(1.0 / fs, 1.0 / fs);
        let r = &self.rect;
        cr.rectangle(r.clip_x, r.clip_y, r.clip_w, r.clip_h);
        cr.clip();
        cr.set_source_rgb(0.10, 0.10, 0.10);
        cr.rectangle(r.x, r.y, r.w, r.h);
        cr.fill();
        let mut shown = (0u32, 0u32, 0.0f64);
        let pick = if magic_ok && version_ok {
            self.pick_frame()
        } else {
            None
        };
        if let Some(pick) = pick {
            shown = (pick.slot, pick.sequence, pick.pts);
            if pick.slot < nslots && self.draw_frame(cr, pick.slot, geometry) {
                frame_drawn = true;
                self.fallback = None;
                if pick.sequence != self.sequence {
                    let gap = pick.sequence.wrapping_sub(self.sequence);
                    if self.sequence != 0 && gap > 1 && gap < u32::MAX / 2 {
                        self.dropped += u64::from(gap - 1);
                    }
                    self.sequence = pick.sequence;
                    self.slot = pick.slot;
                    self.pts = pick.pts;
                    self.presented += 1;
                }
                if let Some(ring) = self.ring() {
                    let released = ring.released().load(Ordering::Acquire);
                    let target = pick.sequence.wrapping_sub(1);
                    if target > released {
                        ring.released().store(target, Ordering::Release);
                    }
                }
            }
        }
        if !frame_drawn && let Some(fallback) = &self.fallback {
            self.draw_surface(cr, fallback.as_ref());
            frame_drawn = true;
        }
        cr.restore();
        if env_set("NS_DBG_COMPOSITE") {
            self.debug_composite(frame_drawn, magic_ok, geometry, shown);
        }
    }

    fn debug_composite(
        &self,
        frame_drawn: bool,
        magic_ok: bool,
        geometry: Geometry,
        shown: (u32, u32, f64),
    ) {
        COMPOSITE.with(|cell| {
            let (mut last_us, mut drawn, mut black) = cell.get();
            if frame_drawn {
                drawn += 1;
            } else {
                black += 1;
            }
            let now = glib::monotonic_us();
            if now - last_us > 1_000_000 {
                last_us = now;
                let line = format!(
                    "[composite] drawn={}/s black={}/s slot={} magic={} {}x{} seq={} pts={:.3} rect={:.0},{:.0} {:.0}x{:.0}\n",
                    drawn,
                    black,
                    shown.0,
                    if magic_ok { "ok" } else { "BAD" },
                    geometry.width,
                    geometry.height,
                    shown.1,
                    shown.2,
                    self.rect.x,
                    self.rect.y,
                    self.rect.w,
                    self.rect.h
                );
                err_line(line.as_bytes());
                drawn = 0;
                black = 0;
            }
            cell.set((last_us, drawn, black));
        });
    }

    fn pick_frame(&mut self) -> Option<media::Pick> {
        let generation = self.ring()?.generation().load(Ordering::Acquire);
        if generation != self.generation {
            self.generation = generation;
            self.sequence = 0;
            self.pts = 0.0;
        }
        let ring = self.ring()?;
        let position = self.clock_position(&self.token).unwrap_or_else(|| {
            ring.clock()
                .and_then(|clock| {
                    clock_position(
                        clock.flags,
                        clock.monotonic_us,
                        clock.position,
                        glib::monotonic_us(),
                        500_000,
                    )
                })
                .unwrap_or(self.pts)
        });
        let published = ring.published().load(Ordering::Acquire);
        let current = (self.sequence != 0).then_some((self.sequence, self.slot));
        pick_frame(
            &ring,
            generation,
            published,
            position,
            current,
            self.playing,
        )
    }

    fn snapshot(&mut self, fs: f64, snapshot: *mut GtkSnapshot) {
        if self.vring.is_none() || !self.rect.valid {
            return;
        }
        let r = &self.rect;
        let clip = GrapheneRect::new(
            (r.clip_x / fs) as f32,
            (r.clip_y / fs) as f32,
            (r.clip_w.max(0.0) / fs) as f32,
            (r.clip_h.max(0.0) / fs) as f32,
        );
        // SAFETY: snapshot is the GtkSnapshot of the snapshot vfunc running now, and GTK copies the bounds.
        let cr = unsafe { gtk::sys::gtk_snapshot_append_cairo(snapshot, &clip) };
        // SAFETY: gtk_snapshot_append_cairo returns a new cairo_t reference, handed to the Context and used on this thread only.
        if let Some(cr) = unsafe { Context::from_raw_full(cr.cast()) } {
            self.draw(&cr.as_ref(), fs);
        }
    }
}

fn ensure_tick(v: *mut NsProcView, m: &mut Media) {
    if !m.tick_armed
        && m.video_playing()
        && let Some(area) = area(v)
    {
        m.tick_armed = true;
        area.add_tick_callback(move |widget| video_tick(v, widget));
    }
}

fn video_tick(v: *mut NsProcView, widget: &Widget) -> ControlFlow<()> {
    let Some(m) = media_of(v) else {
        return ControlFlow::Break(());
    };
    if !m.video_playing() {
        m.tick_armed = false;
        return ControlFlow::Break(());
    }
    widget.queue_draw();
    m.tick_count = m.tick_count.wrapping_add(1);
    let count = m.tick_count;
    super::procview_draw::request_tick(v);
    if count % 300 == 0 {
        super::procview_draw::request_render(v);
    }
    if count % 60 == 0
        && let Some(audio) = media_of(v).and_then(|m| m.audio.as_mut())
    {
        audio.send(b"poll\n");
    }
    ControlFlow::Continue(())
}

fn audio_feedback(v: *mut NsProcView, line: &[u8]) {
    if line.starts_with(b"error ") || debug_audio() {
        let mut out = b"[audio-helper] ".to_vec();
        out.extend_from_slice(line);
        out.push(b'\n');
        err_line(&out);
    }
    let Some(m) = media_of(v) else {
        return;
    };
    if let Some(rest) = line.strip_prefix(b"clock ")
        && let Some((name, _)) = media::scan_token(rest)
    {
        m.clock_adopt(name);
    }
    if m.video.is_some()
        && m.playing
        && !m.token.is_empty()
        && let Some(rest) = line.strip_prefix(b"pos ")
        && let Some((token, rest)) = media::scan_token(rest)
        && let Some((value, _)) = media::scan_token(rest)
        && token == m.token.as_slice()
    {
        let seconds = glib::ascii_strtod(value);
        let now = glib::monotonic_us();
        if seconds >= 0.0 && now - m.resync_us > RESYNC_INTERVAL_US {
            m.resync_us = now;
            let mut cmd = b"resync ".to_vec();
            cmd.extend_from_slice(token);
            cmd.extend_from_slice(format!(" {seconds:.3}").as_bytes());
            video_send(v, m, &cmd);
        }
    }
}

fn audio_pump(v: *mut NsProcView, m: &mut Media, commands: &[&[u8]]) {
    if m.audio.is_none() {
        let path = audio_helper_path();
        match Helper::spawn(v, &path, true, Pipe::Audio) {
            Ok(helper) => m.audio = Some(helper),
            Err(message) => {
                let mut line = b"southstar: audio helper ".to_vec();
                line.extend_from_slice(&glib::path_bytes(&path));
                line.extend_from_slice(b" failed to start: ");
                line.extend_from_slice(&message);
                line.push(b'\n');
                err_line(&line);
                return;
            }
        }
        if debug_audio() {
            let mut line = b"[audio-pump] spawn ".to_vec();
            line.extend_from_slice(&glib::path_bytes(&path));
            line.extend_from_slice(b" -> ok\n");
            err_line(&line);
        }
    }
    for cmd in commands {
        if cmd.is_empty() {
            continue;
        }
        if !audio_command_allowed(v, cmd) {
            if debug_audio() {
                let mut line = b"[audio-pump] refused: ".to_vec();
                line.extend_from_slice(cmd);
                line.push(b'\n');
                err_line(&line);
            }
            continue;
        }
        let mut line = cmd.to_vec();
        line.push(b'\n');
        if debug_audio() {
            let mut out = b"[audio-pump] cmd: ".to_vec();
            out.extend_from_slice(&line);
            err_line(&out);
        }
        if let Some(audio) = m.audio.as_mut() {
            audio.write(&line);
        }
    }
    if let Some(audio) = m.audio.as_mut() {
        audio.flush();
    }
}

fn video_handle_line(v: *mut NsProcView, m: &mut Media, line: &[u8]) {
    let clean = media::chomp(line);
    if debug_audio() {
        let mut out = b"[video-helper] ".to_vec();
        out.extend_from_slice(clean);
        out.push(b'\n');
        err_line(&out);
    }
    match media::helper_line(clean) {
        HelperLine::Shm { token, .. } if token != m.token.as_slice() => {
            if debug_audio() {
                let mut out = b"[shm-reject] line-tok=".to_vec();
                out.extend_from_slice(token);
                out.extend_from_slice(b" cur-tok=");
                out.extend_from_slice(&m.token);
                out.push(b'\n');
                err_line(&out);
            }
        }
        HelperLine::Shm { token, name } => {
            m.adopt_ring(token, name);
            ensure_tick(v, m);
        }
        HelperLine::Closed(token) => {
            if token == m.token.as_slice() {
                m.vring_unmap();
                m.fallback = None;
            }
        }
        HelperLine::Playing(token) => {
            if token == m.token.as_slice() {
                m.playing = true;
                ensure_tick(v, m);
            }
        }
        HelperLine::Paused(token) => {
            if token == m.token.as_slice() {
                m.playing = false;
                queue_draw(v);
            }
        }
        HelperLine::Waiting { token, event } => {
            if token == m.token.as_slice() {
                m.playing = true;
                ensure_tick(v, m);
                queue_draw(v);
                let token = CString::new(token).unwrap_or_default();
                let event = CString::new(event).unwrap_or_default();
                ns_proc_view_push_video_event(v, &token, &event);
            }
        }
        HelperLine::Ignored => {}
    }
}

fn video_send(v: *mut NsProcView, m: &mut Media, cmd: &[u8]) {
    if m.video.is_none() {
        let path = video_helper_path();
        let spawned = Helper::spawn(v, &path, false, Pipe::Video);
        if debug_audio() {
            let (status, detail) = match &spawned {
                Ok(_) => ("ok", b"-".to_vec()),
                Err(message) => ("FAIL", message.clone()),
            };
            let mut line = b"[video-pump] spawn ".to_vec();
            line.extend_from_slice(&glib::path_bytes(&path));
            line.extend_from_slice(format!(" -> {status} (").as_bytes());
            line.extend_from_slice(&detail);
            line.extend_from_slice(b")\n");
            err_line(&line);
        }
        let Ok(helper) = spawned else {
            return;
        };
        m.video = Some(helper);
    }
    if debug_audio() {
        let mut line = b"[video-pump] cmd: ".to_vec();
        line.extend_from_slice(cmd);
        line.push(b'\n');
        err_line(&line);
    }
    let mut line = cmd.to_vec();
    line.push(b'\n');
    if let Some(video) = m.video.as_mut() {
        video.send(&line);
    }
}

fn video_dispatch(v: *mut NsProcView, m: &mut Media, cmd: &[u8]) {
    match media::video_command(cmd) {
        VideoCommand::Rect(rect) => {
            if rect.fields >= 5 && (rect.token == m.token.as_slice() || m.token.is_empty()) {
                m.rect = Rect::from_command(&rect);
                if debug_audio() {
                    let r = &m.rect;
                    let line = format!(
                        "[video-rect] {},{} {}x{} clip={},{} {}x{} valid={}\n",
                        rect.values[0],
                        rect.values[1],
                        rect.values[2],
                        rect.values[3],
                        rect.values[5],
                        rect.values[6],
                        rect.values[7],
                        rect.values[8],
                        i32::from(r.valid)
                    );
                    err_line(line.as_bytes());
                }
                queue_draw(v);
            }
            return;
        }
        VideoCommand::Open(token) => {
            if let Some(token) = token {
                m.token = media::truncated(token);
            }
            m.rect.valid = false;
        }
        VideoCommand::Play(token) => {
            if token.unwrap_or_default() == m.token.as_slice() {
                m.playing = true;
            }
        }
        VideoCommand::Pause(token) => {
            if token.unwrap_or_default() == m.token.as_slice() {
                m.playing = false;
            }
        }
        VideoCommand::Other => {}
    }
    video_send(v, m, cmd);
    ensure_tick(v, m);
}

#[cfg(target_os = "linux")]
fn append_threads(out: &mut Vec<u8>, pid: c_int) {
    let taskdir = format!("/proc/{pid}/task");
    let Ok(entries) = std::fs::read_dir(&taskdir) else {
        return;
    };
    let mut hz = southstar_renderer_client::clock_ticks();
    if hz <= 0 {
        hz = 100;
    }
    for entry in entries.flatten() {
        let tid = entry.file_name();
        let tid = tid.to_string_lossy();
        let Ok(stat) = std::fs::read(format!("{taskdir}/{tid}/stat")) else {
            continue;
        };
        let (Some(open), Some(close)) = (
            stat.iter().position(|&b| b == b'('),
            stat.iter().rposition(|&b| b == b')'),
        ) else {
            continue;
        };
        if close <= open {
            continue;
        }
        let name_len = (close - open).min(32) - 1;
        let name = String::from_utf8_lossy(&stat[open + 1..open + 1 + name_len]).into_owned();
        let rest = String::from_utf8_lossy(&stat[close + 1..]).into_owned();
        let mut fields = rest.split_ascii_whitespace();
        let Some(state) = fields.next().and_then(|s| s.chars().next()) else {
            continue;
        };
        let numbers: Vec<u64> = fields.take(12).map(|f| f.parse().unwrap_or(0)).collect();
        let utime = numbers.get(10).copied().unwrap_or(0);
        let stime = numbers.get(11).copied().unwrap_or(0);
        let line = format!(
            "    tid {:<7} {:<16} {}  cpu {:.2}s\n",
            tid,
            name,
            state,
            (utime + stime) as f64 / hz as f64
        );
        out.extend_from_slice(line.as_bytes());
    }
}

#[cfg(not(target_os = "linux"))]
fn append_threads(_out: &mut Vec<u8>, _pid: c_int) {}

fn stats(m: &mut Media) -> Vec<u8> {
    let mut out = Vec::new();
    let procs = [
        (
            "audio helper (southstar-audio)",
            helper_pid(m.audio.as_mut()),
        ),
        (
            "video helper (southstar-video)",
            helper_pid(m.video.as_mut()),
        ),
    ];
    let mut any = false;
    for (label, pid) in procs {
        if pid <= 0 {
            continue;
        }
        if !any {
            out.extend_from_slice(b"\n\n== Media helper processes ==\n");
            any = true;
        }
        let info = southstar_renderer_client::proc_info(pid);
        let state = info.state.unwrap_or("running");
        let rss = info.rss_kb.unwrap_or(-1);
        let mb = if rss >= 0 { rss as f64 / 1024.0 } else { 0.0 };
        out.extend_from_slice(format!("{label}  pid {pid}  {state}  rss {mb:.1} MB\n").as_bytes());
        append_threads(&mut out, pid);
    }
    if any && let Some(ring) = m.ring() {
        let published = ring.published().load(Ordering::Acquire);
        let released = ring.released().load(Ordering::Acquire);
        let geometry = ring.geometry();
        let line = format!(
            "video queue {}x{} stride {} queued {} frame {} pts {:.2}s shown {} dropped {}\n",
            geometry.width,
            geometry.height,
            geometry.stride,
            published.wrapping_sub(released),
            m.sequence,
            m.pts,
            m.presented,
            m.dropped
        );
        out.extend_from_slice(line.as_bytes());
    }
    out
}

pub(super) fn ns_pv_media_new() -> *mut Media {
    Box::into_raw(Box::new(Media::new()))
}

/// # Safety
/// `m` is null or media `ns_pv_media_new` returned that has not been freed.
pub(super) unsafe fn ns_pv_media_free(m: *mut Media) {
    if m.is_null() {
        return;
    }
    // SAFETY: the caller hands over the Box ns_pv_media_new leaked, exactly once.
    let mut m = unsafe { Box::from_raw(m) };
    m.audio_shutdown();
    m.video_shutdown();
}

pub(super) fn ns_pv_media_pump(v: *mut NsProcView, commands: *const c_char) {
    // SAFETY: commands is null or the reply's NUL-terminated audio side channel, alive for this call.
    let Some(commands) = unsafe { glib::bytes(commands) }.filter(|c| !c.is_empty()) else {
        return;
    };
    let Some(m) = media_of(v) else {
        return;
    };
    let mut audio: Vec<&[u8]> = Vec::new();
    for line in commands.split(|&b| b == 0x1f) {
        if line.is_empty() {
            continue;
        }
        if let Some(cmd) = line.strip_prefix(b"video ") {
            if video_command_allowed(cmd) {
                video_dispatch(v, m, cmd);
            } else if debug_audio() {
                let mut out = b"[video-pump] refused: ".to_vec();
                out.extend_from_slice(cmd);
                out.push(b'\n');
                err_line(&out);
            }
        } else {
            audio.push(line);
        }
    }
    if !audio.is_empty() {
        audio_pump(v, m, &audio);
    }
}

pub(super) fn ns_pv_media_audio_shutdown(v: *mut NsProcView) {
    if let Some(m) = media_of(v) {
        m.audio_shutdown();
    }
}

pub(super) fn ns_pv_media_video_shutdown(v: *mut NsProcView) {
    if let Some(m) = media_of(v) {
        m.video_shutdown();
    }
}

pub(super) fn ns_pv_media_audio_pid(v: *mut NsProcView) -> c_int {
    media_of(v).map_or(-1, |m| helper_pid(m.audio.as_mut()))
}

pub(super) fn ns_pv_media_video_pid(v: *mut NsProcView) -> c_int {
    media_of(v).map_or(-1, |m| helper_pid(m.video.as_mut()))
}

pub(super) fn ns_pv_media_video_playing(v: *mut NsProcView) -> bool {
    media_of(v).is_some_and(|m| m.video_playing())
}

pub(super) fn ns_pv_media_forget_rect(v: *mut NsProcView) {
    if let Some(m) = media_of(v) {
        m.rect.valid = false;
    }
}

pub(super) fn ns_pv_media_on_page(v: *mut NsProcView) -> bool {
    media_of(v).is_some_and(|m| m.rect.page)
}

pub(super) fn ns_pv_media_snapshot(v: *mut NsProcView, fs: f64, snapshot: *mut GtkSnapshot) {
    if let Some(m) = media_of(v) {
        m.snapshot(fs, snapshot);
    }
}

pub(super) fn ns_pv_media_stats(v: *mut NsProcView) -> Vec<u8> {
    media_of(v).map(stats).unwrap_or_default()
}

pub(super) fn ns_proc_video_helper_available() -> bool {
    let path = video_helper_path();
    if path.is_absolute() {
        return is_executable(&path);
    }
    std::env::var_os("PATH")
        .is_some_and(|dirs| std::env::split_paths(&dirs).any(|dir| is_executable(&dir.join(&path))))
}

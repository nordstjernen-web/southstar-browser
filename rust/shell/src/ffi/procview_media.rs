//! Southstar — a tab's audio and video helper processes, their shared-memory clock and frame queue, and the video overlay drawn over the page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use core::sync::atomic::Ordering;
use std::ffi::CString;

use southstar_media_shm::{
    AUDIO_CLOCK_MAGIC, AUDIO_CLOCK_SLOTS, AUDIO_CLOCK_VERSION, AudioClockHdr, VIDEO_RING_MAGIC,
    VIDEO_RING_VERSION, VideoRingHdr,
};

use southstar_glib::{self as glib, GBoolean, GError, GStr};
use southstar_gtk_ffi::{
    self as gtk, Cairo, CairoSurface, GdkFrameClock, GrapheneRect, GtkSnapshot, GtkWidget,
};

use super::appmain::ns_app_self_exe;
use super::procview_result::{ns_proc_view_push_video_event, ns_proc_view_url};
use super::procview_state::NsProcView;
use super::procview_view::{
    ns_proc_view_area, ns_proc_view_media, ns_proc_view_ref, ns_proc_view_unref,
};
use crate::procview::media::{
    self, FrameRing, HelperLine, Rect, SlotView, VideoCommand, clock_position, pick_frame,
};

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {
        $(
            #[repr(C)]
            struct $name {
                _private: [u8; 0],
            }
        )*
    };
}

opaque!(
    GSubprocess,
    GSubprocessLauncher,
    GOutputStream,
    GInputStream,
    GDataInputStream,
    GAsyncResult,
    GString,
);

type AsyncReady =
    unsafe extern "C" fn(source: *mut c_void, result: *mut GAsyncResult, data: *mut c_void);

const SUBPROCESS_FLAGS: c_uint = (1 << 0) | (1 << 2) | (1 << 5);
const FILE_TEST_IS_EXECUTABLE: c_uint = 1 << 3;
const RESYNC_INTERVAL_US: i64 = 950_000;

unsafe extern "C" {
    fn ns_net_proxy_override() -> *const c_char;
    fn ns_net_http_proxy() -> *const c_char;
    fn ns_net_https_proxy() -> *const c_char;
    fn ns_net_no_proxy() -> *const c_char;
    fn ns_net_ca_bundle_path() -> *const c_char;

    fn g_subprocess_launcher_new(flags: c_uint) -> *mut GSubprocessLauncher;
    fn g_subprocess_launcher_setenv(
        launcher: *mut GSubprocessLauncher,
        variable: *const c_char,
        value: *const c_char,
        overwrite: GBoolean,
    );
    fn g_subprocess_launcher_spawnv(
        launcher: *mut GSubprocessLauncher,
        argv: *const *const c_char,
        error: *mut *mut GError,
    ) -> *mut GSubprocess;
    fn g_subprocess_get_stdin_pipe(proc_: *mut GSubprocess) -> *mut GOutputStream;
    fn g_subprocess_get_stdout_pipe(proc_: *mut GSubprocess) -> *mut GInputStream;
    fn g_subprocess_force_exit(proc_: *mut GSubprocess);
    fn g_subprocess_get_identifier(proc_: *mut GSubprocess) -> *const c_char;
    fn g_output_stream_write_all(
        stream: *mut GOutputStream,
        buffer: *const c_void,
        count: usize,
        bytes_written: *mut usize,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_output_stream_flush(
        stream: *mut GOutputStream,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_data_input_stream_new(base: *mut GInputStream) -> *mut GDataInputStream;
    fn g_data_input_stream_read_line_async(
        stream: *mut GDataInputStream,
        io_priority: c_int,
        cancellable: *mut c_void,
        callback: AsyncReady,
        user_data: *mut c_void,
    );
    fn g_data_input_stream_read_line_finish(
        stream: *mut GDataInputStream,
        result: *mut GAsyncResult,
        length: *mut usize,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_filename_from_uri(
        uri: *const c_char,
        hostname: *mut *mut c_char,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_canonicalize_filename(filename: *const c_char, relative_to: *const c_char) -> *mut c_char;
    fn g_file_test(filename: *const c_char, test: c_uint) -> GBoolean;
    fn g_path_is_absolute(file_name: *const c_char) -> GBoolean;
    fn g_find_program_in_path(program: *const c_char) -> *mut c_char;
    fn g_path_get_dirname(file_name: *const c_char) -> *mut c_char;
    fn g_get_monotonic_time() -> i64;
    fn g_string_append_len(string: *mut GString, val: *const c_char, len: isize) -> *mut GString;
}

#[cfg(unix)]
unsafe extern "C" {
    fn shm_open(name: *const c_char, oflag: c_int, ...) -> c_int;
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: c_int,
        flags: c_int,
        fd: c_int,
        offset: i64,
    ) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> c_int;
}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn sysconf(name: c_int) -> c_long;
}

#[cfg(windows)]
#[repr(C)]
struct MemoryBasicInformation {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    partition_id: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

#[cfg(windows)]
unsafe extern "system" {
    fn OpenFileMappingA(access: u32, inherit: c_int, name: *const c_char) -> *mut c_void;
    fn MapViewOfFile(
        mapping: *mut c_void,
        access: u32,
        offset_high: u32,
        offset_low: u32,
        bytes: usize,
    ) -> *mut c_void;
    fn VirtualQuery(
        address: *const c_void,
        info: *mut MemoryBasicInformation,
        length: usize,
    ) -> usize;
    fn UnmapViewOfFile(address: *const c_void) -> c_int;
    fn CloseHandle(handle: *mut c_void) -> c_int;
}

#[cfg(windows)]
const FILE_MAP_READ: u32 = 4;
#[cfg(windows)]
const FILE_MAP_ALL_ACCESS: u32 = 0xF001F;

struct Mapping {
    base: *mut u8,
    #[cfg(windows)]
    handle: *mut c_void,
    bytes: usize,
}

impl Mapping {
    const fn empty() -> Mapping {
        Mapping {
            base: ptr::null_mut(),
            #[cfg(windows)]
            handle: ptr::null_mut(),
            bytes: 0,
        }
    }

    fn is_mapped(&self) -> bool {
        !self.base.is_null()
    }

    #[cfg(windows)]
    fn open(name: &[u8], writable: bool, min_bytes: usize, strict: bool) -> Option<Mapping> {
        let name = CString::new(name).ok()?;
        let access = if writable {
            FILE_MAP_ALL_ACCESS
        } else {
            FILE_MAP_READ
        };
        let handle = unsafe { OpenFileMappingA(access, 0, name.as_ptr()) };
        if handle.is_null() {
            return None;
        }
        let base = unsafe { MapViewOfFile(handle, access, 0, 0, 0) };
        let mut info = core::mem::MaybeUninit::<MemoryBasicInformation>::zeroed();
        let bytes = if !base.is_null()
            && unsafe {
                VirtualQuery(
                    base,
                    info.as_mut_ptr(),
                    core::mem::size_of::<MemoryBasicInformation>(),
                )
            } != 0
        {
            unsafe { info.assume_init() }.region_size
        } else {
            0
        };
        let large_enough = if strict {
            bytes > min_bytes
        } else {
            bytes >= min_bytes
        };
        if base.is_null() || !large_enough {
            if !base.is_null() {
                unsafe { UnmapViewOfFile(base) };
            }
            unsafe { CloseHandle(handle) };
            return None;
        }
        Some(Mapping {
            base: base.cast(),
            handle,
            bytes,
        })
    }

    #[cfg(unix)]
    fn open(name: &[u8], writable: bool, min_bytes: usize, strict: bool) -> Option<Mapping> {
        use std::os::fd::FromRawFd;
        let name = CString::new(name).ok()?;
        let fd = unsafe { shm_open(name.as_ptr(), if writable { 2 } else { 0 }, 0 as c_uint) };
        if fd < 0 {
            return None;
        }
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let size = file.metadata().ok()?.len() as usize;
        let large_enough = if strict {
            size > min_bytes
        } else {
            size >= min_bytes
        };
        if !large_enough {
            return None;
        }
        let prot = if writable { 1 | 2 } else { 1 };
        let base = unsafe { mmap(ptr::null_mut(), size, prot, 1, fd, 0) };
        drop(file);
        if base as isize == -1 {
            return None;
        }
        Some(Mapping {
            base: base.cast(),
            bytes: size,
        })
    }

    fn unmap(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.base.is_null() {
                UnmapViewOfFile(self.base.cast());
            }
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
        #[cfg(unix)]
        if !self.base.is_null() {
            unsafe { munmap(self.base.cast(), self.bytes) };
        }
        *self = Mapping::empty();
    }
}

struct Ring<'a> {
    hdr: &'a VideoRingHdr,
}

impl Ring<'_> {
    fn field(&self, f: impl FnOnce(*const VideoRingHdr) -> *const u32) -> u32 {
        unsafe { ptr::read_volatile(f(self.hdr)) }
    }

    fn magic(&self) -> u32 {
        self.field(|h| unsafe { &raw const (*h).magic })
    }

    fn version(&self) -> u32 {
        self.field(|h| unsafe { &raw const (*h).version })
    }

    fn width(&self) -> u32 {
        self.field(|h| unsafe { &raw const (*h).width })
    }

    fn height(&self) -> u32 {
        self.field(|h| unsafe { &raw const (*h).height })
    }

    fn stride(&self) -> u32 {
        self.field(|h| unsafe { &raw const (*h).stride })
    }

    fn nslots(&self) -> u32 {
        self.field(|h| unsafe { &raw const (*h).nslots })
    }

    fn frame_bytes(&self) -> u32 {
        self.field(|h| unsafe { &raw const (*h).frame_bytes })
    }

    fn valid_header(&self, mapped: usize) -> bool {
        self.magic() == VIDEO_RING_MAGIC
            && self.version() == VIDEO_RING_VERSION
            && self.nslots() == media::RING_SLOTS
            && core::mem::size_of::<VideoRingHdr>()
                + self.nslots() as usize * self.frame_bytes() as usize
                <= mapped
    }

    fn clock(&self, fallback: f64) -> f64 {
        let seq1 = self.hdr.clock_sequence.load(Ordering::Acquire);
        if seq1 == 0 || seq1 & 1 != 0 {
            return fallback;
        }
        let flags = self.field(|h| unsafe { &raw const (*h).clock_flags });
        let sample_us = unsafe { ptr::read_volatile(&raw const self.hdr.clock_monotonic_us) };
        let position = unsafe { ptr::read_volatile(&raw const self.hdr.clock_position) };
        let seq2 = self.hdr.clock_sequence.load(Ordering::Acquire);
        if seq1 != seq2 {
            return fallback;
        }
        clock_position(flags, sample_us, position, monotonic_us(), 500_000).unwrap_or(fallback)
    }
}

impl FrameRing for Ring<'_> {
    fn slot(&self, index: u32) -> SlotView {
        let slot = &self.hdr.slots[index as usize];
        SlotView {
            sequence: slot.sequence.load(Ordering::Acquire),
            generation: unsafe { ptr::read_volatile(&raw const slot.generation) },
            pts: unsafe { ptr::read_volatile(&raw const slot.pts) },
        }
    }
}

pub struct Media {
    audio_proc: *mut GSubprocess,
    audio_in: *mut GOutputStream,
    audio_clock: Mapping,
    audio_clock_name: Vec<u8>,
    video_proc: *mut GSubprocess,
    video_in: *mut GOutputStream,
    video_out: *mut GDataInputStream,
    vring: Mapping,
    fallback: *mut CairoSurface,
    token: Vec<u8>,
    shm_name: Vec<u8>,
    rect: Rect,
    playing: bool,
    tick_id: c_uint,
    tick_count: c_uint,
    resync_us: i64,
    sequence: u32,
    generation: u32,
    slot: u32,
    pts: f64,
    presented: u64,
    dropped: u64,
}

thread_local! {
    static COMPOSITE: Cell<(i64, u32, u32)> = const { Cell::new((0, 0, 0)) };
}

fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

fn debug_audio() -> bool {
    env_set(c"NS_DBG_AUDIO")
}

fn env_set(name: &CStr) -> bool {
    !unsafe { glib::g_getenv(name.as_ptr()) }.is_null()
}

fn text(p: *const c_char) -> Option<&'static [u8]> {
    unsafe { glib::bytes(p) }
}

fn err_line(bytes: &[u8]) {
    glib::stderr_write(bytes);
}

fn write_line(stream: *mut GOutputStream, line: &[u8]) {
    unsafe {
        g_output_stream_write_all(
            stream,
            line.as_ptr().cast(),
            line.len(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        g_output_stream_flush(stream, ptr::null_mut(), ptr::null_mut());
    }
}

fn helper_path(name: &CStr) -> GStr {
    let exe = ns_app_self_exe();
    if !exe.is_null()
        && let Some(dir) = unsafe { GStr::take(g_path_get_dirname(exe)) }
    {
        let parent = build_filename(&[c"..", name]);
        for rel in [name, &parent] {
            let candidate = build_filename(&[&dir, rel]);
            if unsafe { g_file_test(candidate.as_ptr(), FILE_TEST_IS_EXECUTABLE) } != 0 {
                return candidate;
            }
        }
    }
    unsafe { GStr::take(glib::g_strdup(name.as_ptr())) }.expect("g_strdup")
}

fn build_filename(parts: &[&CStr]) -> GStr {
    let mut argv: Vec<*mut c_char> = parts.iter().map(|p| p.as_ptr().cast_mut()).collect();
    argv.push(ptr::null_mut());
    unsafe { GStr::take(glib::g_build_filenamev(argv.as_mut_ptr())) }.expect("g_build_filenamev")
}

fn audio_helper_path() -> GStr {
    if cfg!(windows) {
        helper_path(c"southstar-audio.exe")
    } else {
        helper_path(c"southstar-audio")
    }
}

fn video_helper_path() -> GStr {
    if cfg!(windows) {
        helper_path(c"southstar-video.exe")
    } else {
        helper_path(c"southstar-video")
    }
}

fn apply_proxy_env(launcher: *mut GSubprocessLauncher) {
    let set = |name: &CStr, value: *const c_char| unsafe {
        g_subprocess_launcher_setenv(launcher, name.as_ptr(), value, glib::TRUE)
    };
    let non_empty = |p: *const c_char| text(p).is_some_and(|t| !t.is_empty());
    let proxy = unsafe { ns_net_proxy_override() };
    if non_empty(proxy) {
        set(c"all_proxy", proxy);
        set(c"http_proxy", proxy);
        set(c"https_proxy", proxy);
    } else {
        let http = unsafe { ns_net_http_proxy() };
        let https = unsafe { ns_net_https_proxy() };
        if non_empty(http) {
            set(c"http_proxy", http);
        }
        if non_empty(https) {
            set(c"https_proxy", https);
        }
    }
    let no_proxy = unsafe { ns_net_no_proxy() };
    if non_empty(no_proxy) {
        set(c"no_proxy", no_proxy);
    }
    let ca = unsafe { ns_net_ca_bundle_path() };
    if non_empty(ca) {
        set(c"CURL_CA_BUNDLE", ca);
    }
}

fn spawn(path: &CStr, proxy: bool) -> Result<*mut GSubprocess, Vec<u8>> {
    let launcher = unsafe { g_subprocess_launcher_new(SUBPROCESS_FLAGS) };
    if proxy {
        apply_proxy_env(launcher);
    }
    let argv = [path.as_ptr(), ptr::null()];
    let mut error: *mut GError = ptr::null_mut();
    let process = unsafe { g_subprocess_launcher_spawnv(launcher, argv.as_ptr(), &mut error) };
    unsafe { gtk::g_object_unref(launcher.cast()) };
    if !process.is_null() {
        return Ok(process);
    }
    let message = if error.is_null() {
        b"unknown error".to_vec()
    } else {
        let message = text(unsafe { (*error).message })
            .unwrap_or_default()
            .to_vec();
        unsafe { glib::g_error_free(error) };
        message
    };
    Err(message)
}

fn stream_url_allowed(url: &[u8], subdir: &CStr) -> bool {
    let Some(rest) = url.strip_prefix(b"file://") else {
        return false;
    };
    let Ok(curl) = CString::new(url) else {
        return false;
    };
    let path = unsafe {
        GStr::take(g_filename_from_uri(
            curl.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        ))
    };
    let path = path.map_or_else(
        || {
            let rest = rest.strip_prefix(b"localhost").unwrap_or(rest);
            CString::new(rest).unwrap_or_default()
        },
        |p| p.to_owned(),
    );
    let Some(canon) = (unsafe { GStr::take(g_canonicalize_filename(path.as_ptr(), ptr::null())) })
    else {
        return false;
    };
    let cache = unsafe { glib::g_get_user_cache_dir() };
    let cache = unsafe { CStr::from_ptr(cache) };
    let streams = build_filename(&[cache, c"southstar", subdir, c""]);
    let slashes = |s: &[u8]| -> Vec<u8> {
        s.iter()
            .map(|&b| if b == b'\\' { b'/' } else { b })
            .collect()
    };
    slashes(canon.to_bytes()).starts_with(&slashes(streams.to_bytes()))
}

fn audio_url_allowed(v: *mut NsProcView, url: &[u8]) -> bool {
    if url.starts_with(b"http://") || url.starts_with(b"https://") || url.starts_with(b"data:") {
        return true;
    }
    if !url.starts_with(b"file://") {
        return false;
    }
    if text(ns_proc_view_url(v)).is_some_and(|u| u.starts_with(b"file:")) {
        return true;
    }
    stream_url_allowed(url, c"msaudio")
}

fn audio_command_allowed(v: *mut NsProcView, cmd: &[u8]) -> bool {
    media::line_ok(cmd) && media::open_url(cmd).is_none_or(|url| audio_url_allowed(v, url))
}

fn video_command_allowed(cmd: &[u8]) -> bool {
    media::line_ok(cmd)
        && media::open_url(cmd).is_none_or(|url| stream_url_allowed(url, c"msvideo"))
}

fn media_of(v: *mut NsProcView) -> Option<&'static mut Media> {
    unsafe { ns_proc_view_media(v).cast::<Media>().as_mut() }
}

fn area(v: *mut NsProcView) -> *mut GtkWidget {
    ns_proc_view_area(v)
}

fn subprocess_pid(process: *mut GSubprocess) -> c_int {
    if process.is_null() {
        return -1;
    }
    let id = unsafe { g_subprocess_get_identifier(process) };
    text(id).map_or(-1, atoi)
}

fn atoi(bytes: &[u8]) -> c_int {
    let s = bytes
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .copied()
        .collect::<Vec<_>>();
    let (negative, digits) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, &s[..]),
    };
    let mut value: i64 = 0;
    for &b in digits.iter().take_while(|b| b.is_ascii_digit()) {
        value = value.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    (if negative { -value } else { value }) as c_int
}

impl Media {
    fn new() -> Media {
        Media {
            audio_proc: ptr::null_mut(),
            audio_in: ptr::null_mut(),
            audio_clock: Mapping::empty(),
            audio_clock_name: Vec::new(),
            video_proc: ptr::null_mut(),
            video_in: ptr::null_mut(),
            video_out: ptr::null_mut(),
            vring: Mapping::empty(),
            fallback: ptr::null_mut(),
            token: Vec::new(),
            shm_name: Vec::new(),
            rect: Rect::default(),
            playing: false,
            tick_id: 0,
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

    fn ring(&self) -> Option<Ring<'_>> {
        self.vring.is_mapped().then(|| Ring {
            hdr: unsafe { &*self.vring.base.cast::<VideoRingHdr>() },
        })
    }

    fn clock_unmap(&mut self) {
        self.audio_clock.unmap();
        self.audio_clock_name.clear();
    }

    fn clock_adopt(&mut self, name: &[u8]) {
        self.clock_unmap();
        let Some(map) = Mapping::open(name, false, core::mem::size_of::<AudioClockHdr>(), false)
        else {
            return;
        };
        self.audio_clock = map;
        let hdr = self.audio_clock.base.cast::<AudioClockHdr>();
        let (magic, version, nslots) = unsafe {
            (
                ptr::read_volatile(&raw const (*hdr).magic),
                ptr::read_volatile(&raw const (*hdr).version),
                ptr::read_volatile(&raw const (*hdr).nslots),
            )
        };
        if magic != AUDIO_CLOCK_MAGIC
            || version != AUDIO_CLOCK_VERSION
            || nslots != AUDIO_CLOCK_SLOTS
        {
            self.clock_unmap();
            return;
        }
        self.audio_clock_name = media::truncated(name);
    }

    fn clock_position(&self, token: &[u8]) -> Option<f64> {
        if !self.audio_clock.is_mapped() || token.is_empty() {
            return None;
        }
        let hdr = unsafe { &*self.audio_clock.base.cast::<AudioClockHdr>() };
        for slot in &hdr.slots {
            let seq1 = slot.sequence.load(Ordering::Acquire);
            if seq1 == 0 || seq1 & 1 != 0 {
                continue;
            }
            let flags = unsafe { ptr::read_volatile(&raw const slot.flags) };
            let sample_us = unsafe { ptr::read_volatile(&raw const slot.monotonic_us) };
            let position = unsafe { ptr::read_volatile(&raw const slot.position) };
            let mut sample = unsafe { ptr::read_volatile(&raw const slot.token) };
            sample[63] = 0;
            let seq2 = slot.sequence.load(Ordering::Acquire);
            let len = sample.iter().position(|&b| b == 0).unwrap_or(64);
            if seq1 != seq2 || flags & media::CLOCK_USED == 0 || &sample[..len] != token {
                continue;
            }
            return clock_position(flags, sample_us, position, monotonic_us(), 100_000);
        }
        None
    }

    fn fallback_clear(&mut self) {
        if !self.fallback.is_null() {
            unsafe { gtk::cairo_surface_destroy(self.fallback) };
            self.fallback = ptr::null_mut();
        }
    }

    fn snapshot_current(&mut self) {
        let Some(ring) = self.ring() else {
            return;
        };
        if self.sequence == 0 {
            return;
        }
        let slot = self.slot;
        let (width, height, stride, frame_bytes) = (
            ring.width(),
            ring.height(),
            ring.stride(),
            ring.frame_bytes(),
        );
        if ring.magic() != VIDEO_RING_MAGIC
            || ring.version() != VIDEO_RING_VERSION
            || slot >= media::RING_SLOTS
            || width == 0
            || height == 0
            || u64::from(stride) < u64::from(width) * 4
            || u64::from(stride) * u64::from(height) > u64::from(frame_bytes)
            || core::mem::size_of::<VideoRingHdr>() + (slot as usize + 1) * frame_bytes as usize
                > self.vring.bytes
        {
            return;
        }
        let meta = ring.slot(slot);
        if meta.sequence != self.sequence || meta.generation != self.generation {
            return;
        }
        let copy = unsafe {
            gtk::cairo_image_surface_create(
                gtk::CAIRO_FORMAT_RGB24,
                width as c_int,
                height as c_int,
            )
        };
        if unsafe { gtk::cairo_surface_status(copy) } != gtk::CAIRO_STATUS_SUCCESS {
            unsafe { gtk::cairo_surface_destroy(copy) };
            return;
        }
        unsafe {
            let src = self
                .vring
                .base
                .add(core::mem::size_of::<VideoRingHdr>() + slot as usize * frame_bytes as usize);
            let dst = gtk::cairo_image_surface_get_data(copy);
            let dst_stride = gtk::cairo_image_surface_get_stride(copy) as usize;
            for y in 0..height as usize {
                ptr::copy_nonoverlapping(
                    src.add(y * stride as usize),
                    dst.add(y * dst_stride),
                    width as usize * 4,
                );
            }
            gtk::cairo_surface_mark_dirty(copy);
        }
        self.fallback_clear();
        self.fallback = copy;
    }

    fn vring_unmap(&mut self) {
        self.vring.unmap();
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
        if let Some(map) = Mapping::open(name, true, core::mem::size_of::<VideoRingHdr>(), true) {
            let ring = Ring {
                hdr: unsafe { &*map.base.cast::<VideoRingHdr>() },
            };
            if ring.valid_header(map.bytes) {
                self.vring = map;
                self.shm_name = media::truncated(name);
                self.token = media::truncated(token);
            } else {
                let mut map = map;
                map.unmap();
            }
        }
    }

    fn audio_shutdown(&mut self) {
        if self.audio_proc.is_null() {
            self.clock_unmap();
            return;
        }
        if !self.audio_in.is_null() {
            write_line(self.audio_in, b"quit\n");
        }
        unsafe {
            g_subprocess_force_exit(self.audio_proc);
            gtk::g_object_unref(self.audio_proc.cast());
        }
        self.audio_proc = ptr::null_mut();
        self.audio_in = ptr::null_mut();
        self.clock_unmap();
    }

    fn video_shutdown(&mut self) {
        self.vring_unmap();
        self.fallback_clear();
        if self.video_proc.is_null() {
            return;
        }
        if !self.video_in.is_null() {
            write_line(self.video_in, b"quit\n");
        }
        unsafe {
            if !self.video_out.is_null() {
                gtk::g_object_unref(self.video_out.cast());
            }
            self.video_out = ptr::null_mut();
            g_subprocess_force_exit(self.video_proc);
            gtk::g_object_unref(self.video_proc.cast());
        }
        self.video_proc = ptr::null_mut();
        self.video_in = ptr::null_mut();
    }

    fn video_playing(&self) -> bool {
        self.vring.is_mapped() && self.playing
    }

    fn draw_surface(&self, cr: *mut Cairo, surface: *mut CairoSurface) {
        let fw = unsafe { gtk::cairo_image_surface_get_width(surface) };
        let fh = unsafe { gtk::cairo_image_surface_get_height(surface) };
        if fw <= 0 || fh <= 0 {
            return;
        }
        let (x, y, w, h) = self.rect.draw_box(fw, fh);
        let r = &self.rect;
        unsafe {
            gtk::cairo_save(cr);
            gtk::cairo_rectangle(cr, r.x, r.y, r.w, r.h);
            gtk::cairo_clip(cr);
            gtk::cairo_rectangle(cr, r.clip_x, r.clip_y, r.clip_w, r.clip_h);
            gtk::cairo_clip(cr);
            gtk::cairo_translate(cr, x, y);
            gtk::cairo_scale(cr, w / f64::from(fw), h / f64::from(fh));
            gtk::cairo_set_source_surface(cr, surface, 0.0, 0.0);
            gtk::cairo_paint(cr);
            gtk::cairo_restore(cr);
        }
    }

    fn draw(&mut self, cr: *mut Cairo, fs: f64) {
        if !self.vring.is_mapped() || !self.rect.valid {
            return;
        }
        let mut frame_drawn = false;
        unsafe {
            gtk::cairo_save(cr);
            gtk::cairo_scale(cr, 1.0 / fs, 1.0 / fs);
            let r = &self.rect;
            gtk::cairo_rectangle(cr, r.clip_x, r.clip_y, r.clip_w, r.clip_h);
            gtk::cairo_clip(cr);
            gtk::cairo_set_source_rgb(cr, 0.10, 0.10, 0.10);
            gtk::cairo_rectangle(cr, r.x, r.y, r.w, r.h);
            gtk::cairo_fill(cr);
        }
        let ring = self.ring().expect("mapped ring");
        let (nslots, fw, fh, fstride, fbytes) = (
            ring.nslots(),
            ring.width(),
            ring.height(),
            ring.stride(),
            ring.frame_bytes(),
        );
        let magic_ok = ring.magic() == VIDEO_RING_MAGIC;
        let mut shown = (0u32, 0u32, 0.0f64);
        let pick = if magic_ok && ring.version() == VIDEO_RING_VERSION {
            self.pick_frame()
        } else {
            None
        };
        if let Some(pick) = pick {
            shown = (pick.slot, pick.sequence, pick.pts);
            let fits = pick.slot < nslots
                && fw > 0
                && fh > 0
                && u64::from(fstride) >= u64::from(fw) * 4
                && u64::from(fstride) * u64::from(fh) <= u64::from(fbytes)
                && core::mem::size_of::<VideoRingHdr>()
                    + (pick.slot as usize + 1) * fbytes as usize
                    <= self.vring.bytes;
            if fits {
                let pixels = unsafe {
                    self.vring.base.add(
                        core::mem::size_of::<VideoRingHdr>() + pick.slot as usize * fbytes as usize,
                    )
                };
                let surface = unsafe {
                    gtk::cairo_image_surface_create_for_data(
                        pixels,
                        gtk::CAIRO_FORMAT_RGB24,
                        fw as c_int,
                        fh as c_int,
                        fstride as c_int,
                    )
                };
                if unsafe { gtk::cairo_surface_status(surface) } == gtk::CAIRO_STATUS_SUCCESS {
                    self.draw_surface(cr, surface);
                    frame_drawn = true;
                    self.fallback_clear();
                    if pick.sequence != self.sequence {
                        if self.sequence != 0 && pick.sequence > self.sequence + 1 {
                            self.dropped += u64::from(pick.sequence - self.sequence - 1);
                        }
                        self.sequence = pick.sequence;
                        self.slot = pick.slot;
                        self.pts = pick.pts;
                        self.presented += 1;
                    }
                    let ring = self.ring().expect("mapped ring");
                    let released = ring.hdr.released.load(Ordering::Acquire);
                    let target = pick.sequence.wrapping_sub(1);
                    if target > released {
                        ring.hdr.released.store(target, Ordering::Release);
                    }
                }
                unsafe { gtk::cairo_surface_destroy(surface) };
            }
        }
        if !frame_drawn && !self.fallback.is_null() {
            self.draw_surface(cr, self.fallback);
            frame_drawn = true;
        }
        unsafe { gtk::cairo_restore(cr) };
        if env_set(c"NS_DBG_COMPOSITE") {
            self.debug_composite(frame_drawn, magic_ok, fw, fh, shown);
        }
    }

    fn debug_composite(
        &self,
        frame_drawn: bool,
        magic_ok: bool,
        fw: u32,
        fh: u32,
        shown: (u32, u32, f64),
    ) {
        COMPOSITE.with(|cell| {
            let (mut last_us, mut drawn, mut black) = cell.get();
            if frame_drawn {
                drawn += 1;
            } else {
                black += 1;
            }
            let now = monotonic_us();
            if now - last_us > 1_000_000 {
                last_us = now;
                let line = format!(
                    "[composite] drawn={}/s black={}/s slot={} magic={} {}x{} seq={} pts={:.3} rect={:.0},{:.0} {:.0}x{:.0}\n",
                    drawn,
                    black,
                    shown.0,
                    if magic_ok { "ok" } else { "BAD" },
                    fw,
                    fh,
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
        let generation = self.ring()?.hdr.generation.load(Ordering::Acquire);
        if generation != self.generation {
            self.generation = generation;
            self.sequence = 0;
            self.pts = 0.0;
        }
        let position = match self.clock_position(&self.token) {
            Some(position) => position,
            None => self.ring()?.clock(self.pts),
        };
        let ring = self.ring()?;
        let published = ring.hdr.published.load(Ordering::Acquire);
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
        if !self.vring.is_mapped() || !self.rect.valid {
            return;
        }
        let r = &self.rect;
        let clip = GrapheneRect::new(
            (r.clip_x / fs) as f32,
            (r.clip_y / fs) as f32,
            (r.clip_w.max(0.0) / fs) as f32,
            (r.clip_h.max(0.0) / fs) as f32,
        );
        let cr = unsafe { gtk::gtk_snapshot_append_cairo(snapshot, &clip) };
        self.draw(cr, fs);
        unsafe { gtk::cairo_destroy(cr) };
    }
}

fn ensure_tick(v: *mut NsProcView, m: &mut Media) {
    if m.tick_id == 0 && m.video_playing() {
        m.tick_id =
            unsafe { gtk::gtk_widget_add_tick_callback(area(v), Some(video_tick), v.cast(), None) };
    }
}

unsafe extern "C" fn video_tick(
    widget: *mut GtkWidget,
    _clock: *mut GdkFrameClock,
    data: *mut c_void,
) -> GBoolean {
    let v = data.cast::<NsProcView>();
    let Some(m) = media_of(v) else {
        return 0;
    };
    if !m.video_playing() {
        m.tick_id = 0;
        return 0;
    }
    unsafe { gtk::gtk_widget_queue_draw(widget) };
    m.tick_count = m.tick_count.wrapping_add(1);
    let count = m.tick_count;
    let audio_in = m.audio_in;
    super::procview_draw::request_tick(v);
    if count % 300 == 0 {
        super::procview_draw::request_render(v);
    }
    if count % 60 == 0 && !audio_in.is_null() {
        write_line(audio_in, b"poll\n");
    }
    1
}

fn read_line_async(stream: *mut GDataInputStream, callback: AsyncReady, data: *mut c_void) {
    unsafe { g_data_input_stream_read_line_async(stream, 0, ptr::null_mut(), callback, data) };
}

unsafe extern "C" fn audio_feedback_line(
    src: *mut c_void,
    result: *mut GAsyncResult,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    let stream = src.cast::<GDataInputStream>();
    let line = unsafe {
        GStr::take(g_data_input_stream_read_line_finish(
            stream,
            result,
            ptr::null_mut(),
            ptr::null_mut(),
        ))
    };
    let Some(line) = line else {
        if let Some(m) = media_of(v) {
            m.clock_unmap();
        }
        unsafe { gtk::g_object_unref(stream.cast()) };
        ns_proc_view_unref(v);
        return;
    };
    let line = line.to_bytes();
    if line.starts_with(b"error ") || debug_audio() {
        let mut out = b"[audio-helper] ".to_vec();
        out.extend_from_slice(line);
        out.push(b'\n');
        err_line(&out);
    }
    if let Some(m) = media_of(v) {
        if let Some(rest) = line.strip_prefix(b"clock ")
            && let Some((name, _)) = media::scan_token(rest)
        {
            m.clock_adopt(name);
        }
        if !m.video_proc.is_null()
            && m.playing
            && !m.token.is_empty()
            && let Some(rest) = line.strip_prefix(b"pos ")
            && let Some((token, rest)) = media::scan_token(rest)
            && let Some((value, _)) = media::scan_token(rest)
            && token == m.token.as_slice()
        {
            let seconds = glib::ascii_strtod(value);
            let now = monotonic_us();
            if seconds >= 0.0 && now - m.resync_us > RESYNC_INTERVAL_US {
                m.resync_us = now;
                let mut cmd = b"resync ".to_vec();
                cmd.extend_from_slice(token);
                cmd.extend_from_slice(format!(" {seconds:.3}").as_bytes());
                video_send(v, m, &cmd);
            }
        }
    }
    read_line_async(stream, audio_feedback_line, v.cast());
}

fn audio_pump(v: *mut NsProcView, m: &mut Media, commands: &[&[u8]]) {
    if m.audio_proc.is_null() {
        let path = audio_helper_path();
        match spawn(&path, true) {
            Ok(process) => m.audio_proc = process,
            Err(message) => {
                let mut line = b"southstar: audio helper ".to_vec();
                line.extend_from_slice(path.to_bytes());
                line.extend_from_slice(b" failed to start: ");
                line.extend_from_slice(&message);
                line.push(b'\n');
                err_line(&line);
                return;
            }
        }
        if debug_audio() {
            let mut line = b"[audio-pump] spawn ".to_vec();
            line.extend_from_slice(path.to_bytes());
            line.extend_from_slice(b" -> ok\n");
            err_line(&line);
        }
        m.audio_in = unsafe { g_subprocess_get_stdin_pipe(m.audio_proc) };
        let feedback = unsafe { g_subprocess_get_stdout_pipe(m.audio_proc) };
        if !feedback.is_null() {
            let stream = unsafe { g_data_input_stream_new(feedback) };
            read_line_async(stream, audio_feedback_line, ns_proc_view_ref(v).cast());
        }
    }
    if m.audio_in.is_null() {
        return;
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
        unsafe {
            g_output_stream_write_all(
                m.audio_in,
                line.as_ptr().cast(),
                line.len(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
    }
    unsafe { g_output_stream_flush(m.audio_in, ptr::null_mut(), ptr::null_mut()) };
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
                m.fallback_clear();
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
                unsafe { gtk::gtk_widget_queue_draw(area(v)) };
            }
        }
        HelperLine::Waiting { token, event } => {
            if token == m.token.as_slice() {
                m.playing = true;
                ensure_tick(v, m);
                unsafe { gtk::gtk_widget_queue_draw(area(v)) };
                let token = CString::new(token).unwrap_or_default();
                let event = CString::new(event).unwrap_or_default();
                ns_proc_view_push_video_event(v, &token, &event);
            }
        }
        HelperLine::Ignored => {}
    }
}

unsafe extern "C" fn video_read_line(
    src: *mut c_void,
    result: *mut GAsyncResult,
    data: *mut c_void,
) {
    let v = data.cast::<NsProcView>();
    let line = unsafe {
        GStr::take(g_data_input_stream_read_line_finish(
            src.cast(),
            result,
            ptr::null_mut(),
            ptr::null_mut(),
        ))
    };
    if let Some(line) = line
        && let Some(m) = media_of(v)
        && !m.video_out.is_null()
    {
        video_handle_line(v, m, line.to_bytes());
        video_read_next(v, m);
    }
    ns_proc_view_unref(v);
}

fn video_read_next(v: *mut NsProcView, m: &Media) {
    if !m.video_out.is_null() {
        read_line_async(m.video_out, video_read_line, ns_proc_view_ref(v).cast());
    }
}

fn video_send(v: *mut NsProcView, m: &mut Media, cmd: &[u8]) {
    if m.video_proc.is_null() {
        let path = video_helper_path();
        let spawned = spawn(&path, false);
        if debug_audio() {
            let (status, detail) = match &spawned {
                Ok(_) => ("ok", b"-".to_vec()),
                Err(message) => ("FAIL", message.clone()),
            };
            let mut line = b"[video-pump] spawn ".to_vec();
            line.extend_from_slice(path.to_bytes());
            line.extend_from_slice(format!(" -> {status} (").as_bytes());
            line.extend_from_slice(&detail);
            line.extend_from_slice(b")\n");
            err_line(&line);
        }
        let Ok(process) = spawned else {
            return;
        };
        m.video_proc = process;
        m.video_in = unsafe { g_subprocess_get_stdin_pipe(process) };
        let out = unsafe { g_subprocess_get_stdout_pipe(process) };
        m.video_out = unsafe { g_data_input_stream_new(out) };
        video_read_next(v, m);
    }
    if m.video_in.is_null() {
        return;
    }
    if debug_audio() {
        let mut line = b"[video-pump] cmd: ".to_vec();
        line.extend_from_slice(cmd);
        line.push(b'\n');
        err_line(&line);
    }
    let mut line = cmd.to_vec();
    line.push(b'\n');
    write_line(m.video_in, &line);
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
                unsafe { gtk::gtk_widget_queue_draw(area(v)) };
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
    let mut hz = unsafe { sysconf(2) };
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

fn stats(m: &Media) -> Vec<u8> {
    let mut out = Vec::new();
    let procs = [
        (
            "audio helper (southstar-audio)",
            subprocess_pid(m.audio_proc),
        ),
        (
            "video helper (southstar-video)",
            subprocess_pid(m.video_proc),
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
        let published = ring.hdr.published.load(Ordering::Acquire);
        let released = ring.hdr.released.load(Ordering::Acquire);
        let line = format!(
            "video queue {}x{} stride {} queued {} frame {} pts {:.2}s shown {} dropped {}\n",
            ring.width(),
            ring.height(),
            ring.stride(),
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
    let mut m = unsafe { Box::from_raw(m) };
    m.audio_shutdown();
    m.video_shutdown();
}

pub(super) fn ns_pv_media_pump(v: *mut NsProcView, commands: *const c_char) {
    let Some(commands) = text(commands).filter(|c| !c.is_empty()) else {
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
    media_of(v).map_or(-1, |m| subprocess_pid(m.audio_proc))
}

pub(super) fn ns_pv_media_video_pid(v: *mut NsProcView) -> c_int {
    media_of(v).map_or(-1, |m| subprocess_pid(m.video_proc))
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

pub(super) fn ns_pv_media_append_stats(v: *mut NsProcView, out: *mut c_void) {
    let Some(m) = media_of(v) else {
        return;
    };
    let text = stats(m);
    unsafe { g_string_append_len(out.cast(), text.as_ptr().cast(), text.len() as isize) };
}

pub(super) fn ns_proc_video_helper_available() -> bool {
    let path = video_helper_path();
    let absolute = unsafe { g_path_is_absolute(path.as_ptr()) } != 0;
    let mut ok = absolute && unsafe { g_file_test(path.as_ptr(), FILE_TEST_IS_EXECUTABLE) } != 0;
    if !ok && !absolute {
        ok = unsafe { GStr::take(g_find_program_in_path(path.as_ptr())) }.is_some();
    }
    ok
}

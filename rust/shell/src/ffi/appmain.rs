//! Southstar — the shell executable's entry point: process bootstrap, headless dispatch and the out-of-process GTK shell.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use southstar_config::{NsConfig, Text};
use southstar_glib::{GBoolean, GError, GStr, g_free, g_path_get_dirname};

use super::app::{ns_procapp_run, ns_procapp_set_window_size};
#[cfg(any(windows, target_os = "macos"))]
use super::gtk::build_filename;
use super::gtk::file_test;
use super::procview_worker::ns_proc_renderer_path;
use southstar_headless::{Dump, Opts as HeadlessOpts};

use crate::appmain::{self, Action, Gsk, Options};

#[repr(C)]
struct GLogField {
    key: *const c_char,
    value: *const c_void,
    length: isize,
}

type GLogWriterFunc = unsafe extern "C" fn(
    log_level: c_uint,
    fields: *const GLogField,
    n_fields: usize,
    user_data: *mut c_void,
) -> c_uint;

const LOG_WRITER_HANDLED: c_uint = 1;
const LOG_LEVEL_ERROR: c_uint = 1 << 2;
const LOG_LEVEL_CRITICAL: c_uint = 1 << 3;
const LOG_LEVEL_WARNING: c_uint = 1 << 4;
const DLOG_INFO: c_int = 0;
const DLOG_WARN: c_int = 1;
const DLOG_ERROR: c_int = 2;
const FILE_TEST_EXISTS: c_uint = 1 << 4;
#[cfg(any(windows, target_os = "macos"))]
const FILE_TEST_IS_DIR: c_uint = 1 << 2;

unsafe extern "C" {
    fn g_setenv(variable: *const c_char, value: *const c_char, overwrite: GBoolean) -> GBoolean;
    fn g_find_program_in_path(program: *const c_char) -> *mut c_char;
    fn g_canonicalize_filename(filename: *const c_char, relative_to: *const c_char) -> *mut c_char;
    fn g_filename_to_uri(
        filename: *const c_char,
        hostname: *const c_char,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_log_set_writer_func(
        func: GLogWriterFunc,
        user_data: *mut c_void,
        user_data_free: *const c_void,
    );
    fn g_log_writer_default(
        log_level: c_uint,
        fields: *const GLogField,
        n_fields: usize,
        user_data: *mut c_void,
    ) -> c_uint;
    fn fflush(stream: *mut c_void) -> c_int;
    fn _exit(status: c_int) -> !;
}

static SELF_EXE: AtomicPtr<c_char> = AtomicPtr::new(ptr::null_mut());

pub(crate) fn self_exe() -> *const c_char {
    SELF_EXE.load(Ordering::Acquire)
}

pub(super) fn self_exe_str() -> Option<&'static CStr> {
    let exe = self_exe();
    // SAFETY: SELF_EXE is null or a NUL-terminated string kept until clear_self_exe at exit.
    (!exe.is_null()).then(|| unsafe { CStr::from_ptr(exe) })
}

fn clear_self_exe() {
    southstar_net::set_self_exe(None);
    // SAFETY: SELF_EXE holds null or the GLib string init_self_exe stored, and the swap hands it over exactly once.
    unsafe { g_free(SELF_EXE.swap(ptr::null_mut(), Ordering::AcqRel).cast()) };
}

fn getenv(name: &CStr) -> Option<Vec<u8>> {
    southstar_glib::env_bytes(name.to_str().ok()?)
}

pub(super) fn setenv(name: &CStr, value: &CStr) {
    // SAFETY: name and value are NUL-terminated; GLib copies both.
    unsafe { g_setenv(name.as_ptr(), value.as_ptr(), 1) };
}

fn setenv_unless_set(name: &CStr, value: &CStr) {
    if getenv(name).is_none() {
        setenv(name, value);
    }
}

pub(super) fn dirname(path: &CStr) -> Option<GStr> {
    // SAFETY: path is NUL-terminated.
    let dir = unsafe { g_path_get_dirname(path.as_ptr()) };
    // SAFETY: g_path_get_dirname returns a newly allocated string, which GStr takes over.
    unsafe { GStr::take(dir) }
}

fn printerr(message: &str) {
    southstar_glib::stderr_write(message.as_bytes());
}

#[cfg(target_os = "linux")]
fn platform_self_exe() -> Option<Vec<u8>> {
    let exe = std::fs::read_link("/proc/self/exe").ok()?;
    Some(southstar_glib::path_bytes(&exe))
}

#[cfg(target_os = "macos")]
fn platform_self_exe() -> Option<Vec<u8>> {
    let exe = std::env::current_exe().ok()?;
    let real = std::fs::canonicalize(&exe).unwrap_or(exe);
    Some(southstar_glib::path_bytes(&real))
}

#[cfg(windows)]
fn platform_self_exe() -> Option<Vec<u8>> {
    let exe = std::env::current_exe().ok()?;
    exe.into_os_string()
        .into_string()
        .ok()
        .map(String::into_bytes)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_self_exe() -> Option<Vec<u8>> {
    None
}

fn is_dir_separator(c: u8) -> bool {
    c == b'/' || (cfg!(windows) && c == b'\\')
}

fn path_is_absolute(path: &[u8]) -> bool {
    match path {
        [c, ..] if is_dir_separator(*c) => true,
        [drive, b':', c, ..] => {
            cfg!(windows) && drive.is_ascii_alphabetic() && is_dir_separator(*c)
        }
        _ => false,
    }
}

fn argv0_exe(argv0: &CStr) -> *mut c_char {
    let found = if path_is_absolute(argv0.to_bytes()) {
        ptr::null_mut()
    } else {
        // SAFETY: argv0 is NUL-terminated; GLib returns null or a newly allocated string.
        unsafe { g_find_program_in_path(argv0.as_ptr()) }
    };
    if found.is_null() {
        southstar_glib::strdup(argv0.to_bytes())
    } else {
        found
    }
}

fn init_self_exe(argv0: Option<&CStr>) {
    let exe = match (platform_self_exe(), argv0) {
        (Some(exe), _) => southstar_glib::strdup(&exe),
        (None, Some(argv0)) => argv0_exe(argv0),
        (None, None) => ptr::null_mut(),
    };
    SELF_EXE.store(exe, Ordering::Release);
    southstar_net::set_self_exe(self_exe_str().map(CStr::to_bytes));
}

const IGNORED_LOG_MESSAGES: [&[u8]; 5] = [
    b"win32 session dbus binary not found",
    b"but sizes must be >= 0",
    b"Baselines must be inside the widget size",
    b"without a current allocation",
    b"No IM module matching GTK_IM_MODULE=",
];

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

unsafe extern "C" fn log_writer(
    log_level: c_uint,
    fields: *const GLogField,
    n_fields: usize,
    user_data: *mut c_void,
) -> c_uint {
    let all = if n_fields == 0 || fields.is_null() {
        &[][..]
    } else {
        // SAFETY: GLib passes n_fields initialized GLogField entries that live for the call.
        unsafe { core::slice::from_raw_parts(fields, n_fields) }
    };
    let mut captured: Option<&CStr> = None;
    let mut domain: Option<&CStr> = None;
    for field in all {
        if field.key.is_null() || field.value.is_null() {
            continue;
        }
        // SAFETY: a field key is a NUL-terminated string alive for the call.
        let key = unsafe { CStr::from_ptr(field.key) }.to_bytes();
        // SAFETY: MESSAGE and GLIB_DOMAIN values, the only ones kept, are NUL-terminated strings alive for the call.
        let value = unsafe { CStr::from_ptr(field.value.cast()) };
        if key == b"MESSAGE" {
            captured = Some(value);
            if IGNORED_LOG_MESSAGES
                .iter()
                .any(|m| contains(value.to_bytes(), m))
            {
                return LOG_WRITER_HANDLED;
            }
        }
        if key == b"GLIB_DOMAIN" {
            domain = Some(value);
        }
    }
    if let Some(message) = captured {
        let level = if log_level & (LOG_LEVEL_ERROR | LOG_LEVEL_CRITICAL) != 0 {
            DLOG_ERROR
        } else if log_level & LOG_LEVEL_WARNING != 0 {
            DLOG_WARN
        } else {
            DLOG_INFO
        };
        let category = domain.unwrap_or(c"glib");
        southstar_debuglog::emit(level, category.to_bytes(), message.to_bytes());
    }
    // SAFETY: the arguments are the ones GLib passed to this writer, forwarded unchanged.
    unsafe { g_log_writer_default(log_level, fields, n_fields, user_data) }
}

#[cfg(windows)]
mod win32 {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;

    use super::{FILE_TEST_EXISTS, FILE_TEST_IS_DIR, build_filename, dirname, file_test, getenv};
    use super::{self_exe_str, setenv, setenv_unless_set};

    unsafe extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
        fn AttachConsole(process_id: u32) -> c_int;
    }

    unsafe extern "C" {
        fn __acrt_iob_func(index: u32) -> *mut c_void;
        fn _get_osfhandle(fd: c_int) -> isize;
        fn _fileno(stream: *mut c_void) -> c_int;
        fn freopen_s(
            fp: *mut *mut c_void,
            path: *const c_char,
            mode: *const c_char,
            stream: *mut c_void,
        ) -> c_int;
        fn setvbuf(stream: *mut c_void, buffer: *mut c_char, mode: c_int, size: usize) -> c_int;
    }

    #[cfg(feature = "fontconfig")]
    unsafe extern "C" {
        fn FcInit() -> c_int;
    }

    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const IONBF: c_int = 0x0004;

    pub fn set_app_id() {
        let id: Vec<u16> = "Southstar.Browser".encode_utf16().chain([0]).collect();
        // SAFETY: id is a NUL-terminated UTF-16 string alive for the call.
        unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
    }

    #[cfg(feature = "fontconfig")]
    fn use_fontconfig_backend(dir: Option<&CStr>) {
        if let Some(dir) = dir
            && getenv(c"FONTCONFIG_FILE").is_none()
        {
            let conf = build_filename(&[dir, c"etc", c"fonts", c"fonts.conf"]);
            if file_test(&conf, FILE_TEST_EXISTS) {
                setenv(c"FONTCONFIG_FILE", &conf);
            }
        }
        if let Some(dir) = dir
            && getenv(c"FONTCONFIG_PATH").is_none()
        {
            let fonts = build_filename(&[dir, c"etc", c"fonts"]);
            if file_test(&fonts, FILE_TEST_IS_DIR) {
                setenv(c"FONTCONFIG_PATH", &fonts);
            }
        }
        setenv_unless_set(c"PANGOCAIRO_BACKEND", c"fc");
        // SAFETY: FcInit has no preconditions and runs before any thread uses fontconfig.
        unsafe { FcInit() };
    }

    #[cfg(not(feature = "fontconfig"))]
    fn use_fontconfig_backend(_dir: Option<&CStr>) {}

    pub fn anchor_gtk_data() {
        let Some(dir) = self_exe_str().and_then(dirname) else {
            use_fontconfig_backend(None);
            return;
        };
        use_fontconfig_backend(Some(&dir));
        let share_dir = build_filename(&[&dir, c"share"]);
        if file_test(&share_dir, FILE_TEST_IS_DIR) {
            setenv_unless_set(c"GTK_DATA_PREFIX", &dir);
            setenv_unless_set(c"GTK_EXE_PREFIX", &dir);
            setenv_unless_set(c"XDG_DATA_DIRS", &share_dir);
        }
        if getenv(c"GDK_PIXBUF_MODULE_FILE").is_none() {
            let loaders =
                build_filename(&[&dir, c"lib", c"gdk-pixbuf-2.0", c"2.10.0", c"loaders.cache"]);
            if file_test(&loaders, FILE_TEST_EXISTS) {
                setenv(c"GDK_PIXBUF_MODULE_FILE", &loaders);
            }
        }
        let ca = build_filename(&[&dir, c"etc", c"ssl", c"certs", c"ca-bundle.crt"]);
        if file_test(&ca, FILE_TEST_EXISTS) {
            setenv_unless_set(c"CURL_CA_BUNDLE", &ca);
            setenv_unless_set(c"SSL_CERT_FILE", &ca);
        }
    }

    fn fd_is_bound(fd: c_int) -> bool {
        // SAFETY: _get_osfhandle validates fd and reports -1 for one that is not open.
        let h = unsafe { _get_osfhandle(fd) };
        h != 0 && h != -1
    }

    fn crt_stream(index: u32) -> *mut c_void {
        // SAFETY: indices 0, 1 and 2 name the C runtime's stdin, stdout and stderr, valid for the process.
        unsafe { __acrt_iob_func(index) }
    }

    fn stream_is_bound(stream: *mut c_void) -> bool {
        // SAFETY: stream is one of the C runtime's standard streams.
        fd_is_bound(unsafe { _fileno(stream) })
    }

    fn reopen(stream: *mut c_void, path: &CStr, mode: &CStr) {
        let mut fp = ptr::null_mut();
        // SAFETY: fp is a writable out pointer, path and mode are NUL-terminated and stream is a standard stream.
        unsafe { freopen_s(&mut fp, path.as_ptr(), mode.as_ptr(), stream) };
    }

    fn unbuffer(stream: *mut c_void) {
        // SAFETY: stream is a standard stream and a null buffer with _IONBF asks for no buffering.
        unsafe { setvbuf(stream, ptr::null_mut(), IONBF, 0) };
    }

    pub fn attach_parent_console() {
        // SAFETY: AttachConsole takes a process id and reports failure as zero.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return;
        }
        let (stdin, stdout, stderr) = (crt_stream(0), crt_stream(1), crt_stream(2));
        if !stream_is_bound(stdout) {
            reopen(stdout, c"CONOUT$", c"w");
        }
        if !stream_is_bound(stderr) {
            reopen(stderr, c"CONOUT$", c"w");
        }
        if !stream_is_bound(stdin) {
            reopen(stdin, c"CONIN$", c"r");
        }
        unbuffer(stdout);
        unbuffer(stderr);
    }
}

#[cfg(target_os = "macos")]
fn macos_anchor_gtk_data() {
    let Some(macos_dir) = self_exe_str().and_then(dirname) else {
        return;
    };
    let Some(contents) = dirname(&macos_dir) else {
        return;
    };
    let res = build_filename(&[&contents, c"Resources"]);
    let schemas = build_filename(&[&res, c"share", c"glib-2.0", c"schemas"]);
    if file_test(&schemas, FILE_TEST_IS_DIR) && getenv(c"GSETTINGS_SCHEMA_DIR").is_none() {
        setenv(c"GSETTINGS_SCHEMA_DIR", &schemas);
    }
    let loaders = build_filename(&[&res, c"lib", c"gdk-pixbuf-2.0", c"2.10.0", c"loaders"]);
    let cache = build_filename(&[&res, c"lib", c"gdk-pixbuf-2.0", c"2.10.0", c"loaders.cache"]);
    if file_test(&cache, FILE_TEST_EXISTS) {
        setenv_unless_set(c"GDK_PIXBUF_MODULE_FILE", &cache);
        setenv_unless_set(c"GDK_PIXBUF_MODULEDIR", &loaders);
    }
    let ca = build_filename(&[&res, c"etc", c"ssl", c"certs", c"ca-bundle.crt"]);
    if file_test(&ca, FILE_TEST_EXISTS) {
        setenv_unless_set(c"CURL_CA_BUNDLE", &ca);
        setenv_unless_set(c"SSL_CERT_FILE", &ca);
    }
}

#[cfg(target_os = "linux")]
fn has_gpu_render_node() -> bool {
    std::fs::read_dir("/dev/dri").is_ok_and(|entries| {
        entries.flatten().any(|e| {
            let name = e.file_name();
            let name = name.as_encoded_bytes();
            name.starts_with(b"renderD") || name.starts_with(b"card")
        })
    })
}

fn apply_gsk_renderer(pref: Option<&CStr>) {
    if getenv(c"GSK_RENDERER").is_some() {
        return;
    }
    match appmain::gsk_renderer(pref.map(CStr::to_bytes)) {
        Gsk::Auto =>
        {
            #[cfg(target_os = "linux")]
            if !has_gpu_render_node() {
                setenv(c"GSK_RENDERER", c"cairo");
                southstar_glib::log(
                    southstar_glib::LogLevel::Message,
                    b"no GPU detected; using the software (cairo) renderer",
                );
            }
        }
        Gsk::Set(renderer) => setenv(c"GSK_RENDERER", renderer),
        Gsk::Unknown => southstar_glib::warning(format!(
            "ignoring unknown gsk_renderer '{}' (expected one of: auto, gl, ngl, vulkan, cairo)",
            String::from_utf8_lossy(pref.map_or(&b""[..], CStr::to_bytes))
        )),
    }
}

fn config() -> Option<&'static NsConfig> {
    southstar_config::get()
}

fn config_text(field: impl FnOnce(&'static NsConfig) -> &'static Text) -> Option<&'static CStr> {
    config().and_then(|c| field(c).c_str())
}

fn create_private_dir(dir: &CStr) {
    let Some(path) = southstar_glib::path_from_bytes(dir.to_bytes()) else {
        return;
    };
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let _ = builder.create(path);
}

fn add_screenshot_writable_dirs(args: &[&CStr]) {
    for path in appmain::screenshot_paths(args) {
        if let Some(dir) = dirname(path)
            && !dir.is_empty()
        {
            create_private_dir(&dir);
            southstar_sandbox::add_writable_dir(dir.to_bytes());
        }
    }
}

fn file_url_for(target: &CStr) -> Option<GStr> {
    if !appmain::is_local_path_candidate(target.to_bytes()) || !file_test(target, FILE_TEST_EXISTS)
    {
        return None;
    }
    // SAFETY: target is NUL-terminated and a null base means the current directory.
    let abs_path = unsafe { g_canonicalize_filename(target.as_ptr(), ptr::null()) };
    // SAFETY: g_canonicalize_filename returns a newly allocated string, which GStr takes over.
    let abs_path = unsafe { GStr::take(abs_path) }?;
    // SAFETY: abs_path is an absolute, NUL-terminated file name; null hostname and error are allowed.
    let uri = unsafe { g_filename_to_uri(abs_path.as_ptr(), ptr::null(), ptr::null_mut()) };
    // SAFETY: g_filename_to_uri returns null or a newly allocated string, which GStr takes over.
    unsafe { GStr::take(uri) }
}

fn print_perf() {
    let southstar_net::PerfSnapshot {
        fetches,
        bytes,
        sum_ms,
        span_ms,
    } = southstar_net::perf_snapshot();
    let (relayouts, layout_ms) = southstar_engine::layout_perf();
    let mut line = format!(
        "[net perf] fetches={fetches} bytes={bytes} net_span={span_ms:.1}ms net_sum={sum_ms:.1}ms | relayouts={relayouts} layout={layout_ms:.1}ms"
    );
    {
        let shape = southstar_text::shape_cache::shape_cache_stats();
        let brk = southstar_text::breaks::break_cache_stats();
        let item = southstar_text::item_cache::item_cache_stats();
        for (name, (hits, misses, skipped, entries)) in [
            ("shape_cache", shape),
            ("break_cache", brk),
            ("item_cache", item),
        ] {
            line.push_str(&format!(
                " | {name} hits={hits} misses={misses} skipped={skipped} entries={entries}"
            ));
        }
    }
    line.push('\n');
    printerr(&line);
}

fn run_headless(opts: &HeadlessOpts<'_>) -> c_int {
    setenv_unless_set(c"NS_WEBGL_ALLOW", c"1");
    let log_net = opts.debug_levels & (1 << appmain::DLOG_NET) != 0;
    southstar_net::init();
    southstar_net::set_allow_file_urls(true);
    if log_net {
        southstar_net::set_log_fetches(true);
    }
    southstar_http_cache::init();
    southstar_history::init();
    southstar_font::init();
    southstar_paint::register_font_oracle();
    southstar_spellcheck::init();
    southstar_bytecode_cache::init();
    let file_url = opts.url.and_then(file_url_for);
    let rc = southstar_headless::run(Some(&HeadlessOpts {
        url: file_url.as_deref().or(opts.url),
        ..*opts
    }));
    if log_net {
        print_perf();
    }
    drop(file_url);
    southstar_bytecode_cache::shutdown();
    southstar_history::shutdown();
    southstar_http_cache::shutdown();
    southstar_net::shutdown();
    southstar_config::shutdown();
    rc
}

fn run_proc_gui(
    url: Option<&CStr>,
    gsk_renderer_override: Option<&CStr>,
    session_path: Option<&CStr>,
    recover: bool,
    private_mode: bool,
) -> c_int {
    apply_gsk_renderer(gsk_renderer_override.or_else(|| config_text(|c| &c.gsk_renderer)));
    let start = match url {
        Some(url) if !url.is_empty() => url,
        _ => config_text(|c| &c.home_url)
            .filter(|h| !h.is_empty())
            .unwrap_or(c"about:start"),
    };
    let file_url = file_url_for(start);
    let start = file_url.as_deref().unwrap_or(start);
    let status = ns_procapp_run(start, session_path, recover, private_mode);
    southstar_renderer_host::single_process_shutdown();
    drop(file_url);
    clear_self_exe();
    southstar_config::shutdown();
    status
}

fn write_stdout(bytes: &[u8]) {
    southstar_glib::stdout_write(bytes);
    southstar_glib::stdout_flush();
}

fn print_config() {
    write_stdout(&southstar_config::dump(
        southstar_netutil::default_accept_language(),
    ));
}

fn headless_opts<'a>(o: &Options<'a>) -> HeadlessOpts<'a> {
    HeadlessOpts {
        url: o.url,
        dump: Dump::from_index(o.dump),
        out_path: o.out_path,
        viewport_width: o.viewport_width,
        viewport_height: o.viewport_height,
        settle_ms: o.settle_ms,
        time_ms: o.time_ms,
        debug_levels: southstar_headless::debug_mask(o.debug),
        actions: o.script,
        eval: o.eval,
        inspect: o.inspect,
        inspect_at: o.inspect_at,
        wpt: o.wpt,
        wpt_timeout_ms: o.wpt_timeout_ms,
    }
}

fn renderer_path() -> Option<GStr> {
    // SAFETY: ns_proc_renderer_path returns a newly allocated string or null, which GStr takes over.
    unsafe { GStr::take(ns_proc_renderer_path()) }
}

fn sandbox_proc_mode(single_process: bool) {
    southstar_sandbox::add_writable_dir(b"/dev/shm");
    if !single_process
        && let Some(rpath) = renderer_path()
        && let Some(rdir) = dirname(&rpath)
    {
        southstar_sandbox::add_exec_dir(rdir.to_bytes());
    }
    southstar_sandbox::allow_gpu_devices();
    southstar_sandbox::sandbox_init(self_exe_str());
}

#[cfg(not(feature = "rust-main"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let args: Vec<&CStr> = (0..usize::try_from(argc).unwrap_or(0))
        .map(|i| argv.wrapping_add(i))
        // SAFETY: the C runtime passes argc readable entries in argv, ending with a null pointer, and take_while stops there.
        .map(|slot| unsafe { *slot })
        .take_while(|a| !a.is_null())
        // SAFETY: each argument is a NUL-terminated string that lives until the process exits.
        .map(|a| unsafe { CStr::from_ptr(a) })
        .collect();
    run(&args)
}

pub fn run(args: &[&CStr]) -> c_int {
    #[cfg(windows)]
    if appmain::need_console(args) {
        win32::attach_parent_console();
    }
    if !southstar_sandbox::refuse_root() {
        return 77;
    }
    init_self_exe(args.first().copied());
    let exe = self_exe_str();
    southstar_i18n::init(exe);
    let watchdog_args: Vec<&[u8]> = args.iter().skip(1).map(|a| a.to_bytes()).collect();
    if appmain::flag_given(args, b"--private") {
        setenv(c"NS_PRIVATE", c"1");
    }
    southstar_config::init();

    let proc_mode = appmain::proc_mode_wanted(args);

    for _ in args
        .iter()
        .skip(1)
        .filter(|a| a.to_bytes() == b"--enable-webgpu")
    {
        if cfg!(feature = "webgpu") {
            setenv(c"NS_WEBGPU_ALLOW", c"1");
        } else {
            printerr("southstar: --enable-webgpu ignored (built without WebGPU support)\n");
        }
    }

    if proc_mode {
        let enabled = config().is_some_and(|c| c.watchdog_enabled != 0);
        if southstar_watchdog::should_supervise(watchdog_args.iter().copied(), enabled) {
            let rc = southstar_watchdog::run_supervisor(
                exe.map_or(&[][..], CStr::to_bytes),
                &watchdog_args,
            );
            clear_self_exe();
            southstar_config::shutdown();
            return rc;
        }
    }

    southstar_sandbox::win32_mitigations_init(proc_mode);
    add_screenshot_writable_dirs(args);

    let single_process = proc_mode
        && (appmain::flag_given(args, b"--single-process")
            || appmain::env_enabled(getenv(c"NS_SINGLE_PROCESS").as_deref()));
    if single_process {
        southstar_renderer_host::single_process_enable();
    }

    if proc_mode {
        sandbox_proc_mode(single_process);
    } else {
        southstar_sandbox::sandbox_init(self_exe_str());
        southstar_sandbox::seccomp_init();
    }
    southstar_threaddump::install_signal(Some(b"southstar"));
    southstar_debuglog::init();
    // SAFETY: log_writer has GLogWriterFunc's signature and needs no user data.
    unsafe { g_log_set_writer_func(log_writer, ptr::null_mut(), ptr::null()) };
    #[cfg(windows)]
    {
        win32::set_app_id();
        win32::anchor_gtk_data();
    }
    #[cfg(target_os = "macos")]
    macos_anchor_gtk_data();

    let options = appmain::parse(args);
    for action in &options.actions {
        match action {
            Action::Proxy(proxy) => {
                southstar_net::set_proxy_override(Some(proxy.to_bytes()));
                if !proxy.is_empty() {
                    setenv(c"NS_HTTP_PROXY", proxy);
                    setenv(c"NS_HTTPS_PROXY", proxy);
                }
            }
            Action::WindowSize(w, h) => ns_procapp_set_window_size(*w, *h),
            Action::PrintConfig => {
                print_config();
                southstar_config::shutdown();
                return 0;
            }
            Action::BadViewport(spec) => {
                let mut message =
                    b"southstar: --viewport wants WIDTH or WIDTHxHEIGHT, got '".to_vec();
                message.extend_from_slice(spec.to_bytes());
                message.extend_from_slice(b"'\n");
                southstar_glib::stderr_write(&message);
                return 2;
            }
        }
    }

    if proc_mode {
        if southstar_watchdog::is_child(watchdog_args.iter().copied()) {
            southstar_watchdog::child_guard_parent_death();
            let budget = if single_process {
                config().map_or(0, |c| c.js_eval_budget_ms)
            } else {
                0
            };
            southstar_watchdog::child_arm_hang_monitor(budget);
        }
        let session = southstar_watchdog::session_arg(args.iter().skip(1).copied());
        let recover = southstar_watchdog::child_is_recovery();
        return run_proc_gui(
            options.url,
            options.gsk_renderer_override,
            session,
            recover,
            options.private_window,
        );
    }

    apply_gsk_renderer(
        options
            .gsk_renderer_override
            .or_else(|| config_text(|c| &c.gsk_renderer)),
    );
    let opts = headless_opts(&options);
    let headless_rc = run_headless(&opts);
    if !southstar_net::idle() {
        // SAFETY: a null stream asks the C runtime to flush every open output stream.
        unsafe { fflush(ptr::null_mut()) };
        // SAFETY: _exit ends the process without running destructors, which is the point while network threads still run.
        unsafe { _exit(headless_rc) };
    }
    headless_rc
}

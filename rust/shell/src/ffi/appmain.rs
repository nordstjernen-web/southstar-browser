//! Southstar — the shell executable's entry point: process bootstrap, headless dispatch and the out-of-process GTK shell.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};
use std::ffi::CString;

use southstar_config::NsConfig;
use southstar_glib::{GBoolean, GError, GStr, g_free, g_getenv, g_path_get_dirname, g_strdup};

use crate::appmain::{self, Action, Gsk, Options};

#[repr(C)]
struct HeadlessOpts {
    url: *const c_char,
    dump: c_uint,
    out_path: *const c_char,
    viewport_width: c_int,
    viewport_height: c_int,
    settle_ms: c_int,
    time_ms: c_int,
    debug_levels: c_uint,
    actions: *const c_char,
    eval: *const c_char,
    inspect: *const c_char,
    inspect_at: *const c_char,
    wpt: GBoolean,
    wpt_timeout_ms: c_int,
}

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
#[cfg(target_os = "linux")]
const LOG_LEVEL_MESSAGE: c_uint = 1 << 5;
const DLOG_INFO: c_uint = 0;
const DLOG_WARN: c_uint = 1;
const DLOG_ERROR: c_uint = 2;
const FILE_TEST_EXISTS: c_uint = 1 << 4;
#[cfg(any(windows, target_os = "macos"))]
const FILE_TEST_IS_DIR: c_uint = 1 << 2;

unsafe extern "C" {
    fn g_setenv(variable: *const c_char, value: *const c_char, overwrite: GBoolean) -> GBoolean;
    fn g_file_test(filename: *const c_char, test: c_uint) -> GBoolean;
    fn g_path_is_absolute(file_name: *const c_char) -> GBoolean;
    fn g_find_program_in_path(program: *const c_char) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_canonicalize_filename(filename: *const c_char, relative_to: *const c_char) -> *mut c_char;
    fn g_filename_to_uri(
        filename: *const c_char,
        hostname: *const c_char,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_log_set_writer_func(
        func: GLogWriterFunc,
        user_data: *mut c_void,
        user_data_free: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn g_log_writer_default(
        log_level: c_uint,
        fields: *const GLogField,
        n_fields: usize,
        user_data: *mut c_void,
    ) -> c_uint;
    fn g_log(log_domain: *const c_char, log_level: c_uint, format: *const c_char, ...);
    fn g_printerr(format: *const c_char, ...);
    fn fflush(stream: *mut c_void) -> c_int;
    fn _exit(status: c_int) -> !;

    fn ns_debug_log_init();
    fn ns_debug_log_emit_take(level: c_uint, category: *const c_char, message: *mut c_char);
    fn ns_security_refuse_root() -> GBoolean;
    fn ns_security_sandbox_init(self_exe: *const c_char);
    fn ns_security_add_writable_dir(dir: *const c_char);
    fn ns_security_add_exec_dir(dir: *const c_char);
    fn ns_security_seccomp_init();
    fn ns_security_win32_mitigations_init(allow_child_processes: GBoolean);
    fn ns_config_init();
    fn ns_config_shutdown();
    fn ns_config_get() -> *const NsConfig;
    fn ns_config_dump() -> *mut c_char;
    fn ns_thread_dump_install_signal(label: *const c_char);
    fn ns_watchdog_should_supervise(
        argc: c_int,
        argv: *mut *mut c_char,
        enabled_by_default: GBoolean,
    ) -> GBoolean;
    fn ns_watchdog_run_supervisor(
        self_exe: *const c_char,
        argc: c_int,
        argv: *mut *mut c_char,
    ) -> c_int;
    fn ns_watchdog_is_child(argc: c_int, argv: *mut *mut c_char) -> GBoolean;
    fn ns_watchdog_child_guard_parent_death();
    fn ns_watchdog_child_arm_hang_monitor(js_budget_ms: c_int);
    fn ns_watchdog_child_session_arg(argc: c_int, argv: *mut *mut c_char) -> *const c_char;
    fn ns_watchdog_child_is_recovery() -> GBoolean;
    fn ns_rproc_single_process_enable();
    fn ns_proc_renderer_path() -> *mut c_char;
    fn ns_procapp_run(
        startup_url: *const c_char,
        session_path: *const c_char,
        recover: GBoolean,
        private_mode: GBoolean,
    ) -> c_int;
    fn ns_procapp_set_window_size(width: c_int, height: c_int);
    fn ns_headless_debug_mask(spec: *const c_char) -> c_uint;
    fn ns_headless_run(opts: *const HeadlessOpts) -> c_int;
    fn ns_net_init();
    fn ns_net_shutdown();
    fn ns_net_idle() -> GBoolean;
    fn ns_net_set_proxy_override(proxy_url: *const c_char);
    fn ns_net_set_allow_file_urls(allow: GBoolean);
    fn ns_net_set_log_fetches(on: GBoolean);
    fn ns_net_perf_snapshot(
        fetches: *mut u64,
        bytes: *mut u64,
        sum_ms: *mut c_double,
        span_ms: *mut c_double,
    );
    fn ns_engine_layout_perf(relayouts: *mut u64, total_ms: *mut c_double);
    fn ns_cache_init();
    fn ns_cache_shutdown();
    fn ns_history_init();
    fn ns_history_shutdown();
    fn ns_font_init();
    fn ns_spell_init();
}

#[cfg(feature = "ns-pango")]
unsafe extern "C" {
    fn ns_pango_cache_get_stats(
        hits: *mut u64,
        misses: *mut u64,
        skipped: *mut u64,
        entries: *mut u64,
    );
    fn ns_pango_break_cache_stats(
        hits: *mut u64,
        misses: *mut u64,
        skipped: *mut u64,
        entries: *mut u64,
    );
    fn ns_pango_item_cache_stats(
        hits: *mut u64,
        misses: *mut u64,
        skipped: *mut u64,
        entries: *mut u64,
    );
}

static SELF_EXE: AtomicPtr<c_char> = AtomicPtr::new(ptr::null_mut());

fn self_exe() -> *const c_char {
    SELF_EXE.load(Ordering::Acquire)
}

fn clear_self_exe() {
    unsafe { g_free(SELF_EXE.swap(ptr::null_mut(), Ordering::AcqRel).cast()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_app_self_exe() -> *const c_char {
    self_exe()
}

fn getenv(name: &CStr) -> Option<&'static CStr> {
    let value = unsafe { g_getenv(name.as_ptr()) };
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) })
}

fn setenv(name: &CStr, value: &CStr) {
    unsafe { g_setenv(name.as_ptr(), value.as_ptr(), 1) };
}

fn setenv_unless_set(name: &CStr, value: &CStr) {
    if getenv(name).is_none() {
        setenv(name, value);
    }
}

fn file_test(path: &CStr, test: c_uint) -> bool {
    unsafe { g_file_test(path.as_ptr(), test) != 0 }
}

#[cfg(any(windows, target_os = "macos"))]
fn build_filename(parts: &[&CStr]) -> GStr {
    let mut argv: Vec<*mut c_char> = parts.iter().map(|p| p.as_ptr().cast_mut()).collect();
    argv.push(ptr::null_mut());
    unsafe { GStr::take(southstar_glib::g_build_filenamev(argv.as_mut_ptr())) }
        .expect("g_build_filenamev")
}

fn dirname(path: &CStr) -> Option<GStr> {
    unsafe { GStr::take(g_path_get_dirname(path.as_ptr())) }
}

fn log(level: c_uint, message: &str) {
    let message = CString::new(message).unwrap_or_default();
    unsafe { g_log(ptr::null(), level, c"%s".as_ptr(), message.as_ptr()) };
}

fn printerr(message: &str) {
    let message = CString::new(message).unwrap_or_default();
    unsafe { g_printerr(c"%s".as_ptr(), message.as_ptr()) };
}

#[cfg(target_os = "linux")]
fn platform_self_exe() -> *mut c_char {
    unsafe { southstar_glib::g_file_read_link(c"/proc/self/exe".as_ptr(), ptr::null_mut()) }
}

#[cfg(target_os = "macos")]
fn platform_self_exe() -> *mut c_char {
    unsafe extern "C" {
        fn _NSGetExecutablePath(buf: *mut c_char, bufsize: *mut u32) -> c_int;
        fn realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
        fn free(p: *mut c_void);
    }
    let mut size: u32 = 0;
    unsafe {
        _NSGetExecutablePath(ptr::null_mut(), &mut size);
        if size == 0 || size > 32768 {
            return ptr::null_mut();
        }
        let raw = southstar_glib::g_malloc(size as usize).cast::<c_char>();
        let mut exe = ptr::null_mut();
        if _NSGetExecutablePath(raw, &mut size) == 0 {
            let real = realpath(raw, ptr::null_mut());
            if real.is_null() {
                exe = g_strdup(raw);
            } else {
                exe = g_strdup(real);
                free(real.cast());
            }
        }
        g_free(raw.cast());
        exe
    }
}

#[cfg(windows)]
fn platform_self_exe() -> *mut c_char {
    unsafe extern "system" {
        fn GetModuleFileNameW(module: *mut c_void, filename: *mut u16, size: u32) -> u32;
    }
    let mut cap: u32 = 260;
    let mut buf = vec![0u16; cap as usize];
    let mut n = unsafe { GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), cap) };
    while n >= cap && cap < 32768 {
        cap *= 2;
        buf.resize(cap as usize, 0);
        n = unsafe { GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), cap) };
    }
    if n > 0 && n < cap {
        unsafe {
            southstar_glib::g_utf16_to_utf8(
                buf.as_ptr(),
                -1,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        }
    } else {
        ptr::null_mut()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_self_exe() -> *mut c_char {
    ptr::null_mut()
}

fn init_self_exe(argv0: Option<&CStr>) {
    let mut exe = platform_self_exe();
    if exe.is_null()
        && let Some(argv0) = argv0
    {
        unsafe {
            exe = if g_path_is_absolute(argv0.as_ptr()) != 0 {
                g_strdup(argv0.as_ptr())
            } else {
                g_find_program_in_path(argv0.as_ptr())
            };
            if exe.is_null() {
                exe = g_strdup(argv0.as_ptr());
            }
        }
    }
    SELF_EXE.store(exe, Ordering::Release);
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
        unsafe { core::slice::from_raw_parts(fields, n_fields) }
    };
    let mut captured: Option<&CStr> = None;
    let mut domain: Option<&CStr> = None;
    for field in all {
        if field.key.is_null() || field.value.is_null() {
            continue;
        }
        let key = unsafe { CStr::from_ptr(field.key) }.to_bytes();
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
        unsafe {
            ns_debug_log_emit_take(level, category.as_ptr(), g_strdup(message.as_ptr()));
        }
    }
    unsafe { g_log_writer_default(log_level, fields, n_fields, user_data) }
}

#[cfg(windows)]
mod win32 {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;

    use super::{FILE_TEST_EXISTS, FILE_TEST_IS_DIR, build_filename, dirname, file_test, getenv};
    use super::{self_exe, setenv, setenv_unless_set};

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
        #[cfg(feature = "ns-pango")]
        setenv_unless_set(c"NS_PANGOCAIRO_BACKEND", c"fc");
        unsafe { FcInit() };
    }

    #[cfg(not(feature = "fontconfig"))]
    fn use_fontconfig_backend(_dir: Option<&CStr>) {}

    pub fn anchor_gtk_data() {
        let exe = self_exe();
        if exe.is_null() {
            use_fontconfig_backend(None);
            return;
        }
        let Some(dir) = dirname(unsafe { CStr::from_ptr(exe) }) else {
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
        let h = unsafe { _get_osfhandle(fd) };
        h != 0 && h != -1
    }

    pub fn attach_parent_console() {
        unsafe {
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return;
            }
            let (stdin, stdout, stderr) =
                (__acrt_iob_func(0), __acrt_iob_func(1), __acrt_iob_func(2));
            let mut fp = ptr::null_mut();
            if !fd_is_bound(_fileno(stdout)) {
                freopen_s(&mut fp, c"CONOUT$".as_ptr(), c"w".as_ptr(), stdout);
            }
            if !fd_is_bound(_fileno(stderr)) {
                freopen_s(&mut fp, c"CONOUT$".as_ptr(), c"w".as_ptr(), stderr);
            }
            if !fd_is_bound(_fileno(stdin)) {
                freopen_s(&mut fp, c"CONIN$".as_ptr(), c"r".as_ptr(), stdin);
            }
            setvbuf(stdout, ptr::null_mut(), IONBF, 0);
            setvbuf(stderr, ptr::null_mut(), IONBF, 0);
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_anchor_gtk_data() {
    let exe = self_exe();
    if exe.is_null() {
        return;
    }
    let Some(macos_dir) = dirname(unsafe { CStr::from_ptr(exe) }) else {
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
                log(
                    LOG_LEVEL_MESSAGE,
                    "no GPU detected; using the software (cairo) renderer",
                );
            }
        }
        Gsk::Set(renderer) => setenv(c"GSK_RENDERER", renderer),
        Gsk::Unknown => log(
            LOG_LEVEL_WARNING,
            &format!(
                "ignoring unknown gsk_renderer '{}' (expected one of: auto, gl, ngl, vulkan, cairo)",
                String::from_utf8_lossy(pref.map_or(&b""[..], CStr::to_bytes))
            ),
        ),
    }
}

fn config() -> Option<&'static NsConfig> {
    unsafe { ns_config_get().as_ref() }
}

fn config_text(field: impl FnOnce(&NsConfig) -> *mut c_char) -> Option<&'static CStr> {
    let value = config().map_or(ptr::null_mut(), field);
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) })
}

fn add_screenshot_writable_dirs(args: &[&CStr]) {
    for path in appmain::screenshot_paths(args) {
        if let Some(dir) = dirname(path)
            && !dir.is_empty()
        {
            unsafe {
                g_mkdir_with_parents(dir.as_ptr(), 0o700);
                ns_security_add_writable_dir(dir.as_ptr());
            }
        }
    }
}

fn file_url_for(target: &CStr) -> Option<GStr> {
    if !appmain::is_local_path_candidate(target.to_bytes()) || !file_test(target, FILE_TEST_EXISTS)
    {
        return None;
    }
    unsafe {
        let abs_path = GStr::take(g_canonicalize_filename(target.as_ptr(), ptr::null()))?;
        GStr::take(g_filename_to_uri(
            abs_path.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
        ))
    }
}

fn opt_ptr(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

fn print_perf() {
    let (mut fetches, mut bytes, mut relayouts) = (0u64, 0u64, 0u64);
    let (mut sum_ms, mut span_ms, mut layout_ms) = (0f64, 0f64, 0f64);
    unsafe {
        ns_net_perf_snapshot(&mut fetches, &mut bytes, &mut sum_ms, &mut span_ms);
        ns_engine_layout_perf(&mut relayouts, &mut layout_ms);
    }
    let mut line = format!(
        "[net perf] fetches={fetches} bytes={bytes} net_span={span_ms:.1}ms net_sum={sum_ms:.1}ms | relayouts={relayouts} layout={layout_ms:.1}ms"
    );
    #[cfg(feature = "ns-pango")]
    {
        let mut shape = [0u64; 4];
        let mut brk = [0u64; 4];
        let mut item = [0u64; 4];
        unsafe {
            ns_pango_cache_get_stats(&mut shape[0], &mut shape[1], &mut shape[2], &mut shape[3]);
            ns_pango_break_cache_stats(&mut brk[0], &mut brk[1], &mut brk[2], &mut brk[3]);
            ns_pango_item_cache_stats(&mut item[0], &mut item[1], &mut item[2], &mut item[3]);
        }
        for (name, s) in [
            ("shape_cache", shape),
            ("break_cache", brk),
            ("item_cache", item),
        ] {
            line.push_str(&format!(
                " | {name} hits={} misses={} skipped={} entries={}",
                s[0], s[1], s[2], s[3]
            ));
        }
    }
    line.push('\n');
    printerr(&line);
}

fn run_headless(opts: &mut HeadlessOpts) -> c_int {
    setenv_unless_set(c"NS_WEBGL_ALLOW", c"1");
    let log_net = opts.debug_levels & (1 << appmain::DLOG_NET) != 0;
    unsafe {
        ns_net_init();
        ns_net_set_allow_file_urls(1);
        if log_net {
            ns_net_set_log_fetches(1);
        }
        ns_cache_init();
        ns_history_init();
        ns_font_init();
        ns_spell_init();
    }
    southstar_bytecode_cache::init();
    let file_url = if opts.url.is_null() {
        None
    } else {
        file_url_for(unsafe { CStr::from_ptr(opts.url) })
    };
    if let Some(url) = &file_url {
        opts.url = url.as_ptr();
    }
    let rc = unsafe { ns_headless_run(opts) };
    if log_net {
        print_perf();
    }
    drop(file_url);
    southstar_bytecode_cache::shutdown();
    unsafe {
        ns_history_shutdown();
        ns_cache_shutdown();
        ns_net_shutdown();
        ns_config_shutdown();
    }
    rc
}

fn run_proc_gui(
    url: Option<&CStr>,
    gsk_renderer_override: Option<&CStr>,
    session_path: *const c_char,
    recover: bool,
    private_mode: bool,
) -> c_int {
    apply_gsk_renderer(gsk_renderer_override.or_else(|| config_text(|c| c.gsk_renderer)));
    let start = match url {
        Some(url) if !url.is_empty() => url,
        _ => config_text(|c| c.home_url)
            .filter(|h| !h.is_empty())
            .unwrap_or(c"about:start"),
    };
    let file_url = file_url_for(start);
    let start = file_url.as_deref().unwrap_or(start);
    let status = unsafe {
        ns_procapp_run(
            start.as_ptr(),
            session_path,
            GBoolean::from(recover),
            GBoolean::from(private_mode),
        )
    };
    drop(file_url);
    clear_self_exe();
    unsafe { ns_config_shutdown() };
    status
}

fn write_stdout(bytes: &[u8]) {
    #[cfg(windows)]
    {
        unsafe extern "C" {
            fn __acrt_iob_func(index: u32) -> *mut c_void;
            fn fwrite(data: *const c_void, size: usize, count: usize, stream: *mut c_void)
            -> usize;
        }
        unsafe { fwrite(bytes.as_ptr().cast(), 1, bytes.len(), __acrt_iob_func(1)) };
    }
    #[cfg(not(windows))]
    {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = out.write_all(bytes);
        let _ = out.flush();
    }
}

fn print_config() {
    unsafe {
        if let Some(dump) = GStr::take(ns_config_dump()) {
            write_stdout(dump.to_bytes());
        }
    }
}

fn headless_opts(o: &Options<'_>) -> HeadlessOpts {
    HeadlessOpts {
        url: opt_ptr(o.url),
        dump: o.dump,
        out_path: opt_ptr(o.out_path),
        viewport_width: o.viewport_width,
        viewport_height: o.viewport_height,
        settle_ms: o.settle_ms,
        time_ms: o.time_ms,
        debug_levels: o
            .debug
            .map_or(0, |spec| unsafe { ns_headless_debug_mask(spec.as_ptr()) }),
        actions: opt_ptr(o.script),
        eval: opt_ptr(o.eval),
        inspect: opt_ptr(o.inspect),
        inspect_at: opt_ptr(o.inspect_at),
        wpt: GBoolean::from(o.wpt),
        wpt_timeout_ms: o.wpt_timeout_ms,
    }
}

fn sandbox_proc_mode(single_process: bool) {
    unsafe {
        ns_security_add_writable_dir(c"/dev/shm".as_ptr());
        if !single_process
            && let Some(rpath) = GStr::take(ns_proc_renderer_path())
            && let Some(rdir) = dirname(&rpath)
        {
            ns_security_add_exec_dir(rdir.as_ptr());
        }
        ns_security_sandbox_init(self_exe());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let args: Vec<&CStr> = (0..usize::try_from(argc).unwrap_or(0))
        .map(|i| unsafe { *argv.add(i) })
        .take_while(|a| !a.is_null())
        .map(|a| unsafe { CStr::from_ptr(a) })
        .collect();
    #[cfg(windows)]
    if appmain::need_console(&args) {
        win32::attach_parent_console();
    }
    if unsafe { ns_security_refuse_root() } == 0 {
        return 77;
    }
    init_self_exe(args.first().copied());
    let exe = self_exe();
    southstar_i18n::init((!exe.is_null()).then(|| unsafe { CStr::from_ptr(exe) }));
    if appmain::flag_given(&args, b"--private") {
        setenv(c"NS_PRIVATE", c"1");
    }
    unsafe {
        ns_config_init();
        ns_thread_dump_install_signal(c"southstar".as_ptr());
    }

    let proc_mode = appmain::proc_mode_wanted(&args);

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
        if unsafe { ns_watchdog_should_supervise(argc, argv, GBoolean::from(enabled)) } != 0 {
            let rc = unsafe { ns_watchdog_run_supervisor(self_exe(), argc, argv) };
            clear_self_exe();
            unsafe { ns_config_shutdown() };
            return rc;
        }
    }

    unsafe { ns_security_win32_mitigations_init(GBoolean::from(proc_mode)) };
    add_screenshot_writable_dirs(&args);

    let single_process = proc_mode
        && (appmain::flag_given(&args, b"--single-process")
            || appmain::env_enabled(getenv(c"NS_SINGLE_PROCESS").map(CStr::to_bytes)));
    if single_process {
        unsafe { ns_rproc_single_process_enable() };
    }

    if proc_mode {
        sandbox_proc_mode(single_process);
    } else {
        unsafe {
            ns_security_sandbox_init(self_exe());
            ns_security_seccomp_init();
        }
    }
    unsafe {
        ns_debug_log_init();
        g_log_set_writer_func(log_writer, ptr::null_mut(), None);
    }
    #[cfg(windows)]
    {
        win32::set_app_id();
        win32::anchor_gtk_data();
    }
    #[cfg(target_os = "macos")]
    macos_anchor_gtk_data();

    let options = appmain::parse(&args);
    for action in &options.actions {
        match action {
            Action::Proxy(proxy) => unsafe {
                ns_net_set_proxy_override(proxy.as_ptr());
                if !proxy.is_empty() {
                    setenv(c"NS_HTTP_PROXY", proxy);
                    setenv(c"NS_HTTPS_PROXY", proxy);
                }
            },
            Action::WindowSize(w, h) => unsafe { ns_procapp_set_window_size(*w, *h) },
            Action::PrintConfig => {
                print_config();
                unsafe { ns_config_shutdown() };
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
        if unsafe { ns_watchdog_is_child(argc, argv) } != 0 {
            unsafe { ns_watchdog_child_guard_parent_death() };
            let budget = if single_process {
                config().map_or(0, |c| c.js_eval_budget_ms)
            } else {
                0
            };
            unsafe { ns_watchdog_child_arm_hang_monitor(budget) };
        }
        let session = unsafe { ns_watchdog_child_session_arg(argc, argv) };
        let recover = unsafe { ns_watchdog_child_is_recovery() } != 0;
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
            .or_else(|| config_text(|c| c.gsk_renderer)),
    );
    let mut opts = headless_opts(&options);
    let headless_rc = run_headless(&mut opts);
    if unsafe { ns_net_idle() } == 0 {
        unsafe {
            fflush(ptr::null_mut());
            _exit(headless_rc);
        }
    }
    headless_rc
}

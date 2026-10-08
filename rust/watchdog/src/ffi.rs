//! Southstar — the C ABI of the watchdog, as declared in src/watchdog.h, over GLib's main loop and child watches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Instant;

use southstar_glib::{self as glib, FALSE, GBoolean, TRUE};

use crate::{Burst, ChildExit, HangMonitor, Restart};

type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

#[cfg(unix)]
type GPid = c_int;
#[cfg(windows)]
type GPid = *mut c_void;

type ChildWatchFn = unsafe extern "C" fn(pid: GPid, status: c_int, data: *mut c_void);

#[repr(C)]
struct GMainLoop {
    _private: [u8; 0],
}

const G_LOG_LEVEL_WARNING: c_int = 1 << 4;
const G_LOG_LEVEL_MESSAGE: c_int = 1 << 5;

unsafe extern "C" {
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    fn g_timeout_add(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_timeout_add_seconds(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_source_remove(tag: c_uint) -> GBoolean;
    fn g_main_loop_new(context: *mut c_void, is_running: GBoolean) -> *mut GMainLoop;
    fn g_main_loop_run(main_loop: *mut GMainLoop);
    fn g_main_loop_quit(main_loop: *mut GMainLoop);
    fn g_main_loop_unref(main_loop: *mut GMainLoop);
    fn g_child_watch_add(pid: GPid, function: ChildWatchFn, data: *mut c_void) -> c_uint;
    fn g_spawn_close_pid(pid: GPid);
    fn g_uuid_string_random() -> *mut c_char;
    fn g_get_user_runtime_dir() -> *const c_char;
    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_unlink(filename: *const c_char) -> c_int;
    fn _exit(status: c_int) -> !;
}

fn log(level: c_int, message: &str) {
    let message = CString::new(message.replace('\0', "")).unwrap_or_default();
    unsafe { g_log(ptr::null(), level, c"%s".as_ptr(), message.as_ptr()) };
}

fn message(text: &str) {
    log(G_LOG_LEVEL_MESSAGE, text);
}

fn warning(text: &str) {
    log(G_LOG_LEVEL_WARNING, text);
}

#[cfg(unix)]
mod os {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;
    use std::ffi::CString;

    use southstar_glib::{self as glib, GBoolean, GError, TRUE};

    use super::GPid;

    pub(super) const NO_PID: GPid = 0;
    const G_SPAWN_DO_NOT_REAP_CHILD: c_int = 1 << 1;
    const SIGKILL: c_int = 9;
    const SIGTERM: c_int = 15;
    const SIGINT: c_int = 2;
    pub(super) const STOP_SIGNALS: [c_int; 2] = [SIGINT, SIGTERM];

    unsafe extern "C" {
        fn g_get_environ() -> *mut *mut c_char;
        fn g_environ_setenv(
            envp: *mut *mut c_char,
            variable: *const c_char,
            value: *const c_char,
            overwrite: GBoolean,
        ) -> *mut *mut c_char;
        fn g_environ_unsetenv(envp: *mut *mut c_char, variable: *const c_char) -> *mut *mut c_char;
        fn g_spawn_async(
            working_directory: *const c_char,
            argv: *mut *mut c_char,
            envp: *mut *mut c_char,
            flags: c_int,
            child_setup: Option<unsafe extern "C" fn(data: *mut c_void)>,
            user_data: *mut c_void,
            child_pid: *mut GPid,
            error: *mut *mut GError,
        ) -> GBoolean;
        pub(super) fn g_unix_signal_add(
            signum: c_int,
            function: super::SourceFn,
            data: *mut c_void,
        ) -> u32;
        fn kill(pid: GPid, signal: c_int) -> c_int;
        fn getppid() -> c_int;
        #[cfg(target_os = "linux")]
        fn prctl(option: c_int, ...) -> c_int;
    }

    pub(super) fn spawn(argv: &[CString], recover: bool) -> Result<GPid, String> {
        let mut pointers: Vec<*mut c_char> =
            argv.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
        pointers.push(ptr::null_mut());
        let mut pid: GPid = 0;
        let mut error: *mut GError = ptr::null_mut();
        let ok = unsafe {
            let mut envp = g_get_environ();
            envp = if recover {
                g_environ_setenv(envp, c"NS_WATCHDOG_RECOVER".as_ptr(), c"1".as_ptr(), TRUE)
            } else {
                g_environ_unsetenv(envp, c"NS_WATCHDOG_RECOVER".as_ptr())
            };
            let ok = g_spawn_async(
                ptr::null(),
                pointers.as_mut_ptr(),
                envp,
                G_SPAWN_DO_NOT_REAP_CHILD,
                None,
                ptr::null_mut(),
                &mut pid,
                &mut error,
            );
            glib::g_strfreev(envp);
            ok
        };
        if ok != 0 {
            return Ok(pid);
        }
        let text = unsafe { error.as_ref() }
            .filter(|e| !e.message.is_null())
            .map_or_else(
                || "unknown error".to_owned(),
                |e| {
                    unsafe { CStr::from_ptr(e.message) }
                        .to_string_lossy()
                        .into_owned()
                },
            );
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        Err(text)
    }

    pub(super) fn kill_child(pid: GPid, force: bool) {
        unsafe { kill(pid, if force { SIGKILL } else { SIGTERM }) };
    }

    pub(super) fn report_failure(_detail: &str) {}

    pub(super) fn guard_parent_death() {
        #[cfg(target_os = "linux")]
        {
            const PR_SET_PDEATHSIG: c_int = 1;
            unsafe { prctl(PR_SET_PDEATHSIG, SIGTERM as core::ffi::c_ulong) };
            if unsafe { getppid() } == 1 {
                unsafe { super::_exit(0) };
            }
        }
    }

    pub(super) fn parent_pid() -> c_int {
        let parent = unsafe { getppid() };
        if parent > 1 { parent } else { 0 }
    }
}

#[cfg(windows)]
mod os {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::{mem, ptr};
    use std::ffi::CString;

    use southstar_glib as glib;

    use super::GPid;

    type Handle = *mut c_void;

    pub(super) const NO_PID: GPid = ptr::null_mut();

    #[repr(C)]
    struct StartupInfo {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count_chars: u32,
        y_count_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
        reserved2_len: u16,
        reserved2: *mut u8,
        std_input: Handle,
        std_output: Handle,
        std_error: Handle,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: Handle,
        thread: Handle,
        process_id: u32,
        thread_id: u32,
    }

    #[repr(C)]
    struct ProcessEntry {
        size: u32,
        usage: u32,
        process_id: u32,
        default_heap_id: usize,
        module_id: u32,
        threads: u32,
        parent_process_id: u32,
        priority_base: i32,
        flags: u32,
        exe_file: [u16; 260],
    }

    const TH32CS_SNAPPROCESS: u32 = 0x2;
    const MB_OK: u32 = 0;
    const MB_ICONERROR: u32 = 0x10;
    const MB_SETFOREGROUND: u32 = 0x1_0000;

    unsafe extern "system" {
        fn CreateProcessW(
            application: *const u16,
            command_line: *mut u16,
            process_attributes: *mut c_void,
            thread_attributes: *mut c_void,
            inherit_handles: i32,
            creation_flags: u32,
            environment: *mut c_void,
            current_directory: *const u16,
            startup_info: *mut StartupInfo,
            process_information: *mut ProcessInformation,
        ) -> i32;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: Handle) -> i32;
        fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> i32;
        fn TerminateProcess(process: Handle, exit_code: u32) -> i32;
        fn GetCurrentProcessId() -> u32;
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
        fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
        fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
        fn MessageBoxW(window: Handle, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    unsafe extern "C" {
        fn g_win32_error_message(error: c_int) -> *mut c_char;
        fn ns_debug_log_file_path() -> *const c_char;
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn spawn(argv: &[CString], recover: bool) -> Result<GPid, String> {
        let args: Vec<Vec<u8>> = argv.iter().map(|arg| arg.as_bytes().to_vec()).collect();
        let app = argv.first().and_then(|exe| exe.to_str().ok()).map(wide);
        let command = crate::windows_command_line(&args);
        let (Some(app), Some(mut command)) = (app, command) else {
            return Err("could not build Windows child command line".to_owned());
        };
        command.push(0);
        let name = wide("NS_WATCHDOG_RECOVER");
        let old = unsafe { glib::bytes(glib::g_getenv(c"NS_WATCHDOG_RECOVER".as_ptr())) }
            .and_then(|old| core::str::from_utf8(old).ok())
            .map(wide);
        let one = wide("1");
        unsafe {
            SetEnvironmentVariableW(
                name.as_ptr(),
                if recover { one.as_ptr() } else { ptr::null() },
            )
        };
        let mut startup: StartupInfo = unsafe { mem::zeroed() };
        startup.cb = mem::size_of::<StartupInfo>() as u32;
        let mut info: ProcessInformation = unsafe { mem::zeroed() };
        let ok = unsafe {
            CreateProcessW(
                app.as_ptr(),
                command.as_mut_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                0,
                ptr::null_mut(),
                ptr::null(),
                &mut startup,
                &mut info,
            )
        };
        let error = if ok != 0 {
            0
        } else {
            unsafe { GetLastError() }
        };
        let restore = old.filter(|old| old.len() > 1);
        unsafe {
            SetEnvironmentVariableW(
                name.as_ptr(),
                restore.as_ref().map_or(ptr::null(), |old| old.as_ptr()),
            )
        };
        if ok == 0 {
            let message = unsafe { g_win32_error_message(error as c_int) };
            let text = unsafe { glib::bytes(message) }.map_or_else(
                || "unknown error".to_owned(),
                |m| String::from_utf8_lossy(m).into_owned(),
            );
            unsafe { glib::g_free(message.cast()) };
            return Err(format!("CreateProcessW failed: {text}"));
        }
        unsafe { CloseHandle(info.thread) };
        Ok(info.process)
    }

    pub(super) fn kill_child(pid: GPid, _force: bool) {
        unsafe { TerminateProcess(pid, 1) };
    }

    pub(super) fn report_failure(detail: &str) {
        let log = unsafe { ns_debug_log_file_path() };
        let log = if log.is_null() {
            "(log file unavailable)".to_owned()
        } else {
            unsafe { CStr::from_ptr(log) }
                .to_string_lossy()
                .into_owned()
        };
        let body = format!(
            "Southstar could not start.\n\n{detail}\n\nA diagnostic log was written to:\n{log}\n\n\
             If this persists, try launching with the cairo renderer by setting the environment \
             variable NS_GSK_RENDERER=cairo before starting."
        );
        let body = wide(&body);
        let caption = wide("Southstar");
        unsafe {
            MessageBoxW(
                ptr::null_mut(),
                body.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
            )
        };
    }

    pub(super) fn guard_parent_death() {}

    pub(super) fn parent_pid() -> c_int {
        let me = unsafe { GetCurrentProcessId() };
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot as isize == -1 {
            return 0;
        }
        let mut entry: ProcessEntry = unsafe { mem::zeroed() };
        entry.size = mem::size_of::<ProcessEntry>() as u32;
        let mut parent = 0;
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            if entry.process_id == me {
                parent = entry.parent_process_id;
                break;
            }
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        unsafe { CloseHandle(snapshot) };
        parent as c_int
    }
}

static BEAT: AtomicI32 = AtomicI32::new(0);
static HANG_SECS: AtomicI32 = AtomicI32::new(0);
static IS_CHILD: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn beat(_data: *mut c_void) -> GBoolean {
    BEAT.fetch_add(1, Ordering::SeqCst);
    TRUE
}

fn watch_for_hangs() {
    let mut monitor = HangMonitor::new(BEAT.load(Ordering::SeqCst), Instant::now());
    loop {
        std::thread::sleep(crate::CHECK_INTERVAL);
        let secs = HANG_SECS.load(Ordering::SeqCst);
        if monitor.hung(BEAT.load(Ordering::SeqCst), Instant::now(), secs) {
            let mut stderr = std::io::stderr();
            let _ = writeln!(
                stderr,
                "ns_watchdog: main loop unresponsive for {secs}s — exiting for restart"
            );
            let _ = stderr.flush();
            unsafe { _exit(crate::HANG_EXIT) };
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_watchdog_child_guard_parent_death() {
    IS_CHILD.store(true, Ordering::SeqCst);
    os::guard_parent_death();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_watchdog_supervisor_pid() -> c_int {
    if IS_CHILD.load(Ordering::SeqCst) {
        os::parent_pid()
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_watchdog_child_arm_hang_monitor(js_budget_ms: c_int) {
    let secs = crate::hang_seconds(js_budget_ms);
    HANG_SECS.store(secs, Ordering::SeqCst);
    message(&format!(
        "ns_watchdog: hang monitor armed, exit after {secs}s unresponsive"
    ));
    unsafe { g_timeout_add_seconds(crate::BEAT_SECS, beat, ptr::null_mut()) };
    let _ = std::thread::Builder::new()
        .name("nd-watchdog".to_owned())
        .spawn(watch_for_hangs);
}

unsafe fn args<'a>(argc: c_int, argv: *mut *mut c_char) -> Vec<&'a [u8]> {
    if argv.is_null() {
        return Vec::new();
    }
    (1..usize::try_from(argc).unwrap_or(0))
        .filter_map(|i| {
            let arg = unsafe { *argv.add(i) };
            (!arg.is_null()).then(|| unsafe { CStr::from_ptr(arg) }.to_bytes())
        })
        .collect()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_watchdog_child_session_arg(
    argc: c_int,
    argv: *mut *mut c_char,
) -> *const c_char {
    crate::session_arg(unsafe { args(argc, argv) })
        .map_or(ptr::null(), |value| value.as_ptr().cast())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_watchdog_is_child(argc: c_int, argv: *mut *mut c_char) -> GBoolean {
    glib::boolean(crate::is_child(unsafe { args(argc, argv) }))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_watchdog_child_is_recovery() -> GBoolean {
    let value = unsafe { glib::bytes(glib::g_getenv(c"NS_WATCHDOG_RECOVER".as_ptr())) };
    glib::boolean(value == Some(b"1"))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_watchdog_should_supervise(
    argc: c_int,
    argv: *mut *mut c_char,
    enabled_by_default: GBoolean,
) -> GBoolean {
    glib::boolean(crate::should_supervise(
        unsafe { args(argc, argv) },
        enabled_by_default != 0,
    ))
}

struct Supervisor {
    child_argv: Vec<CString>,
    main_loop: *mut GMainLoop,
    pid: GPid,
    have_pid: bool,
    stopping: bool,
    watch_id: c_uint,
    respawn_id: c_uint,
    force_kill_id: c_uint,
    burst: Burst,
    exit_status: c_int,
}

impl Supervisor {
    fn quit(&mut self, status: c_int) {
        self.exit_status = status;
        unsafe { g_main_loop_quit(self.main_loop) };
    }

    fn data(&mut self) -> *mut c_void {
        (self as *mut Supervisor).cast()
    }

    fn spawn(&mut self, recover: bool) -> bool {
        match os::spawn(&self.child_argv, recover) {
            Ok(pid) => {
                self.pid = pid;
                self.have_pid = true;
                self.watch_id = unsafe { g_child_watch_add(pid, child_exited, self.data()) };
                true
            }
            Err(error) => {
                warning(&format!("ns_watchdog: failed to launch browser: {error}"));
                os::report_failure(&format!("Failed to launch the browser process: {error}"));
                self.quit(1);
                false
            }
        }
    }

    fn schedule_restart(&mut self) {
        match self.burst.record(Instant::now()) {
            Restart::GiveUp(count) => {
                warning(&format!(
                    "ns_watchdog: child failed {count} times in under {}s — giving up",
                    crate::BURST_SECS
                ));
                os::report_failure("The browser repeatedly exited during startup.");
                self.quit(1);
            }
            Restart::Attempt(count) => {
                message(&format!(
                    "ns_watchdog: restarting browser (attempt {count})"
                ));
                self.respawn_id = unsafe { g_timeout_add(crate::BACKOFF_MS, respawn, self.data()) };
            }
        }
    }

    fn exited(&mut self, status: c_int) {
        if self.stopping {
            unsafe { g_main_loop_quit(self.main_loop) };
            return;
        }
        match crate::classify_exit(status) {
            ChildExit::Clean => {
                self.quit(0);
                return;
            }
            ChildExit::StoppedBySignal(signal) => {
                message(&format!(
                    "ns_watchdog: browser terminated by signal {signal} — stopping"
                ));
                self.quit(0);
                return;
            }
            ChildExit::KilledBySignal(signal) => {
                warning(&format!("ns_watchdog: browser stopped by signal {signal}"));
            }
            ChildExit::Failed(code) if cfg!(windows) => {
                warning(&format!(
                    "ns_watchdog: browser exited abnormally (code {code})"
                ));
            }
            ChildExit::Failed(code) => {
                warning(&format!("ns_watchdog: browser exited with code {code}"));
            }
        }
        self.schedule_restart();
    }

    #[cfg_attr(windows, allow(dead_code))]
    fn stop(&mut self) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        self.exit_status = 0;
        if self.have_pid {
            os::kill_child(self.pid, false);
            self.force_kill_id =
                unsafe { g_timeout_add_seconds(crate::STOP_GRACE_SECS, force_kill, self.data()) };
        } else {
            unsafe { g_main_loop_quit(self.main_loop) };
        }
    }
}

unsafe fn supervisor<'a>(data: *mut c_void) -> &'a mut Supervisor {
    unsafe { &mut *data.cast::<Supervisor>() }
}

unsafe extern "C" fn child_exited(pid: GPid, status: c_int, data: *mut c_void) {
    unsafe { g_spawn_close_pid(pid) };
    let sup = unsafe { supervisor(data) };
    sup.have_pid = false;
    sup.watch_id = 0;
    sup.exited(status);
}

unsafe extern "C" fn respawn(data: *mut c_void) -> GBoolean {
    let sup = unsafe { supervisor(data) };
    sup.respawn_id = 0;
    if !sup.stopping {
        sup.spawn(true);
    }
    FALSE
}

unsafe extern "C" fn force_kill(data: *mut c_void) -> GBoolean {
    let sup = unsafe { supervisor(data) };
    sup.force_kill_id = 0;
    if sup.have_pid {
        os::kill_child(sup.pid, true);
    }
    FALSE
}

#[cfg_attr(windows, allow(dead_code))]
unsafe extern "C" fn stop_signal(data: *mut c_void) -> GBoolean {
    unsafe { supervisor(data) }.stop();
    TRUE
}

fn session_path() -> Vec<u8> {
    unsafe {
        let uuid = g_uuid_string_random();
        let name = CString::new(
            [
                b"southstar-watchdog-",
                CStr::from_ptr(uuid).to_bytes(),
                b".session",
            ]
            .concat(),
        )
        .unwrap_or_default();
        glib::g_free(uuid.cast());
        let path = g_build_filename(
            g_get_user_runtime_dir(),
            name.as_ptr(),
            ptr::null::<c_char>(),
        );
        let bytes = CStr::from_ptr(path).to_bytes().to_vec();
        glib::g_free(path.cast());
        bytes
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_watchdog_run_supervisor(
    self_exe: *const c_char,
    argc: c_int,
    argv: *mut *mut c_char,
) -> c_int {
    let session = session_path();
    let self_exe = unsafe { glib::bytes(self_exe) }.unwrap_or_default();
    let child_argv = crate::child_args(self_exe, unsafe { args(argc, argv) }, &session)
        .into_iter()
        .map(|arg| CString::new(arg).unwrap_or_default())
        .collect();
    let sup = Box::into_raw(Box::new(Supervisor {
        child_argv,
        main_loop: unsafe { g_main_loop_new(ptr::null_mut(), FALSE) },
        pid: os::NO_PID,
        have_pid: false,
        stopping: false,
        watch_id: 0,
        respawn_id: 0,
        force_kill_id: 0,
        burst: Burst::new(Instant::now()),
        exit_status: 0,
    }));

    #[cfg(unix)]
    let signal_sources: Vec<c_uint> = os::STOP_SIGNALS
        .iter()
        .map(|&signal| unsafe { os::g_unix_signal_add(signal, stop_signal, sup.cast()) })
        .collect();

    message("ns_watchdog: supervising browser");

    if unsafe { (*sup).spawn(false) } {
        unsafe { g_main_loop_run((*sup).main_loop) };
    }

    #[cfg(unix)]
    for id in signal_sources {
        unsafe { g_source_remove(id) };
    }
    let sup = unsafe { Box::from_raw(sup) };
    for id in [sup.watch_id, sup.respawn_id, sup.force_kill_id] {
        if id != 0 {
            unsafe { g_source_remove(id) };
        }
    }
    if sup.have_pid {
        os::kill_child(sup.pid, true);
        unsafe { g_spawn_close_pid(sup.pid) };
    }
    unsafe { g_main_loop_unref(sup.main_loop) };
    let session = CString::new(session).unwrap_or_default();
    unsafe { g_unlink(session.as_ptr()) };
    sup.exit_status
}

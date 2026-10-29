//! Southstar — the supervisor loop and the child-side hang monitor of the watchdog, over GLib's main loop and child watches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
#[cfg(windows)]
use core::ffi::c_void;
use core::ffi::{CStr, c_char, c_int};
use core::ops::ControlFlow;
use std::ffi::CString;
use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Instant;

use southstar_glib::{self as glib, LogLevel, SourceId};

use crate::{Burst, ChildExit, HangMonitor, Restart};

#[cfg(unix)]
type GPid = c_int;
#[cfg(windows)]
type GPid = *mut c_void;

unsafe extern "C" {
    fn g_spawn_close_pid(pid: GPid);
    fn g_uuid_string_random() -> *mut c_char;
    fn g_get_user_runtime_dir() -> *const c_char;
    fn _exit(status: c_int) -> !;
}

fn close_pid(pid: GPid) {
    // SAFETY: pid is a child this supervisor spawned, closed once after its watch fired or it was killed.
    unsafe { g_spawn_close_pid(pid) };
}

fn log(level: LogLevel, message: &str) {
    southstar_glib::log(level, message.as_bytes());
}

fn message(text: &str) {
    log(LogLevel::Message, text);
}

fn warning(text: &str) {
    log(LogLevel::Warning, text);
}

#[cfg(unix)]
mod os {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;
    use std::ffi::CString;

    use southstar_glib::{self as glib, GBoolean, GError, TRUE};

    use super::GPid;

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
    use core::ffi::{c_int, c_void};
    use core::{mem, ptr};
    use std::ffi::CString;

    use southstar_glib as glib;

    use super::GPid;

    type Handle = *mut c_void;

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
        let old = glib::env_bytes("NS_WATCHDOG_RECOVER")
            .and_then(|old| String::from_utf8(old).ok())
            .map(|old| wide(&old));
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
            let text = std::io::Error::from_raw_os_error(error as i32).to_string();
            let text = text
                .rsplit_once(" (os error ")
                .map_or(text.as_str(), |(detail, _)| detail);
            return Err(format!("CreateProcessW failed: {text}"));
        }
        unsafe { CloseHandle(info.thread) };
        Ok(info.process)
    }

    pub(super) fn kill_child(pid: GPid, _force: bool) {
        unsafe { TerminateProcess(pid, 1) };
    }

    pub(super) fn report_failure(detail: &str) {
        let log = southstar_debuglog::file_path().map_or_else(
            || "(log file unavailable)".to_owned(),
            |log| log.to_string_lossy().into_owned(),
        );
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

pub fn child_guard_parent_death() {
    IS_CHILD.store(true, Ordering::SeqCst);
    os::guard_parent_death();
}

pub fn supervisor_pid() -> c_int {
    if IS_CHILD.load(Ordering::SeqCst) {
        os::parent_pid()
    } else {
        0
    }
}

pub fn child_arm_hang_monitor(js_budget_ms: c_int) {
    let secs = crate::hang_seconds(js_budget_ms);
    HANG_SECS.store(secs, Ordering::SeqCst);
    message(&format!(
        "ns_watchdog: hang monitor armed, exit after {secs}s unresponsive"
    ));
    glib::timeout_add_seconds_local(crate::BEAT_SECS, || {
        BEAT.fetch_add(1, Ordering::SeqCst);
        ControlFlow::Continue(())
    });
    let _ = std::thread::Builder::new()
        .name("nd-watchdog".to_owned())
        .spawn(watch_for_hangs);
}

pub fn child_is_recovery() -> bool {
    std::env::var_os("NS_WATCHDOG_RECOVER").is_some_and(|value| value == "1")
}

struct Supervisor {
    child_argv: Vec<CString>,
    main_loop: glib::MainLoop,
    child: Option<GPid>,
    stopping: bool,
    watch: Option<SourceId>,
    respawn: Option<SourceId>,
    force_kill: Option<SourceId>,
    burst: Burst,
    exit_status: c_int,
}

type Shared = Rc<RefCell<Supervisor>>;

impl Supervisor {
    fn quit(&mut self, status: c_int) {
        self.exit_status = status;
        self.main_loop.quit();
    }

    fn remove_sources(&mut self) {
        for source in [
            self.watch.take(),
            self.respawn.take(),
            self.force_kill.take(),
        ]
        .into_iter()
        .flatten()
        {
            source.remove();
        }
    }
}

fn spawn(sup: &Shared, recover: bool) -> bool {
    let argv = sup.borrow().child_argv.clone();
    match os::spawn(&argv, recover) {
        Ok(pid) => {
            let watcher = Rc::clone(sup);
            let watch = glib::child_watch_add_local(glib::Pid::from_raw(pid), move |status| {
                child_exited(&watcher, status);
            });
            let mut state = sup.borrow_mut();
            state.child = Some(pid);
            state.watch = Some(watch);
            true
        }
        Err(error) => {
            warning(&format!("ns_watchdog: failed to launch browser: {error}"));
            os::report_failure(&format!("Failed to launch the browser process: {error}"));
            sup.borrow_mut().quit(1);
            false
        }
    }
}

fn schedule_restart(sup: &Shared) {
    let restart = sup.borrow_mut().burst.record(Instant::now());
    match restart {
        Restart::GiveUp(count) => {
            warning(&format!(
                "ns_watchdog: child failed {count} times in under {}s — giving up",
                crate::BURST_SECS
            ));
            os::report_failure("The browser repeatedly exited during startup.");
            sup.borrow_mut().quit(1);
        }
        Restart::Attempt(count) => {
            message(&format!(
                "ns_watchdog: restarting browser (attempt {count})"
            ));
            let target = Rc::clone(sup);
            let respawn = glib::timeout_add_local_once(crate::BACKOFF_MS, move || {
                target.borrow_mut().respawn = None;
                if !target.borrow().stopping {
                    spawn(&target, true);
                }
            });
            sup.borrow_mut().respawn = Some(respawn);
        }
    }
}

fn child_exited(sup: &Shared, status: c_int) {
    let pid = {
        let mut state = sup.borrow_mut();
        state.watch = None;
        state.child.take()
    };
    if let Some(pid) = pid {
        close_pid(pid);
    }
    if sup.borrow().stopping {
        sup.borrow().main_loop.quit();
        return;
    }
    match crate::classify_exit(status) {
        ChildExit::Clean => {
            sup.borrow_mut().quit(0);
            return;
        }
        ChildExit::StoppedBySignal(signal) => {
            message(&format!(
                "ns_watchdog: browser terminated by signal {signal} — stopping"
            ));
            sup.borrow_mut().quit(0);
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
    schedule_restart(sup);
}

#[cfg_attr(windows, allow(dead_code))]
fn stop(sup: &Shared) {
    let mut state = sup.borrow_mut();
    if state.stopping {
        return;
    }
    state.stopping = true;
    state.exit_status = 0;
    let Some(pid) = state.child else {
        state.main_loop.quit();
        return;
    };
    os::kill_child(pid, false);
    let target = Rc::clone(sup);
    state.force_kill = Some(glib::timeout_add_local_once(
        crate::STOP_GRACE_SECS * 1000,
        move || {
            let mut state = target.borrow_mut();
            state.force_kill = None;
            if let Some(pid) = state.child {
                os::kill_child(pid, true);
            }
        },
    ));
}

fn session_path() -> Vec<u8> {
    // SAFETY: g_uuid_string_random has no preconditions and returns a new string the caller owns.
    let uuid = unsafe { g_uuid_string_random() };
    // SAFETY: uuid is null or the NUL-terminated string just allocated by GLib, freed once when it drops.
    let uuid = unsafe { glib::GStr::take(uuid) };
    let name = [
        &b"southstar-watchdog-"[..],
        uuid.as_deref().map_or(&b""[..], CStr::to_bytes),
        b".session",
    ]
    .concat();
    // SAFETY: g_get_user_runtime_dir has no preconditions and returns a string GLib keeps.
    let runtime_dir = unsafe { g_get_user_runtime_dir() };
    // SAFETY: runtime_dir is null or a NUL-terminated string GLib keeps for the process.
    let runtime_dir = unsafe { glib::bytes(runtime_dir) }.unwrap_or_default();
    let Some(dir) = glib::path_from_bytes(runtime_dir) else {
        return name;
    };
    let Some(name) = glib::path_from_bytes(&name) else {
        return Vec::new();
    };
    glib::path_bytes(&dir.join(name))
}

pub fn run_supervisor(self_exe: &[u8], args: &[&[u8]]) -> c_int {
    let session = session_path();
    let child_argv = crate::child_args(self_exe, args.iter().copied(), &session)
        .into_iter()
        .map(|arg| CString::new(arg).unwrap_or_default())
        .collect();
    let main_loop = glib::MainLoop::new();
    let sup: Shared = Rc::new(RefCell::new(Supervisor {
        child_argv,
        main_loop: main_loop.clone(),
        child: None,
        stopping: false,
        watch: None,
        respawn: None,
        force_kill: None,
        burst: Burst::new(Instant::now()),
        exit_status: 0,
    }));

    #[cfg(unix)]
    let signal_sources: Vec<SourceId> = os::STOP_SIGNALS
        .iter()
        .map(|&signal| {
            let target = Rc::clone(&sup);
            glib::unix_signal_add_local(signal, move || {
                stop(&target);
                ControlFlow::Continue(())
            })
        })
        .collect();

    message("ns_watchdog: supervising browser");

    if spawn(&sup, false) {
        main_loop.run();
    }

    #[cfg(unix)]
    for source in signal_sources {
        source.remove();
    }
    let mut state = sup.borrow_mut();
    state.remove_sources();
    if let Some(pid) = state.child.take() {
        os::kill_child(pid, true);
        close_pid(pid);
    }
    if let Some(session) = glib::path_from_bytes(&session) {
        let _ = std::fs::remove_file(session);
    }
    state.exit_status
}

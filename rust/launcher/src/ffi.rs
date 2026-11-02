//! Southstar — the launcher's entry point over Win32 and the CRT: console attach, the child process and its exit code.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;

use crate::launch;

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
    cb_reserved2: u16,
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

unsafe extern "system" {
    safe fn GetCommandLineW() -> *const u16;
    fn CommandLineToArgvW(cmd: *const u16, argc: *mut c_int) -> *mut *mut u16;
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
    safe fn AttachConsole(process_id: u32) -> c_int;
    safe fn SetErrorMode(mode: u32) -> u32;
    fn CreateProcessW(
        application: *const u16,
        command_line: *mut u16,
        process_attributes: *mut c_void,
        thread_attributes: *mut c_void,
        inherit_handles: c_int,
        creation_flags: u32,
        environment: *mut c_void,
        current_directory: *const u16,
        startup_info: *mut StartupInfo,
        process_information: *mut ProcessInformation,
    ) -> c_int;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(process: Handle, exit_code: *mut u32) -> c_int;
    safe fn GetLastError() -> u32;
    fn MessageBoxW(window: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> c_int;
}

unsafe extern "C" {
    safe fn __acrt_iob_func(index: u32) -> *mut c_void;
    safe fn _get_osfhandle(fd: c_int) -> isize;
    fn _fileno(stream: *mut c_void) -> c_int;
    fn freopen_s(
        fp: *mut *mut c_void,
        path: *const c_char,
        mode: *const c_char,
        stream: *mut c_void,
    ) -> c_int;
    fn setvbuf(stream: *mut c_void, buffer: *mut c_char, mode: c_int, size: usize) -> c_int;
    fn fwprintf(stream: *mut c_void, format: *const u16, ...) -> c_int;
    fn fflush(stream: *mut c_void) -> c_int;
}

const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
const IONBF: c_int = 0x0004;
const SEM_FAILCRITICALERRORS: u32 = 0x0001;
const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;
const INFINITE: u32 = u32::MAX;
const MB_ICONERROR: u32 = 0x10;
const MB_SETFOREGROUND: u32 = 0x1_0000;

const STDIN: u32 = 0;
const STDOUT: u32 = 1;
const STDERR: u32 = 2;

fn terminated(mut text: Vec<u16>) -> Vec<u16> {
    text.push(0);
    text
}

fn stream_is_bound(index: u32) -> bool {
    let stream = __acrt_iob_func(index);
    // SAFETY: `stream` is one of the CRT's three standard FILE streams, which live for the
    // whole process.
    let fd = unsafe { _fileno(stream) };
    let h = _get_osfhandle(fd);
    h != 0 && h != -1
}

fn reopen_unbound(index: u32, path: &CStr, mode: &CStr) {
    if stream_is_bound(index) {
        return;
    }
    let mut fp = ptr::null_mut();
    // SAFETY: `path` and `mode` are NUL-terminated, `fp` is a valid out-pointer and the
    // stream is one of the CRT's standard FILE streams.
    unsafe {
        freopen_s(
            &mut fp,
            path.as_ptr(),
            mode.as_ptr(),
            __acrt_iob_func(index),
        )
    };
}

fn unbuffer(index: u32) {
    // SAFETY: the stream is a standard CRT FILE nothing has been written to yet, and a null
    // buffer with a zero size is what _IONBF expects.
    unsafe { setvbuf(__acrt_iob_func(index), ptr::null_mut(), IONBF, 0) };
}

fn attach_parent_console() {
    if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
        return;
    }
    reopen_unbound(STDOUT, c"CONOUT$", c"w");
    reopen_unbound(STDERR, c"CONOUT$", c"w");
    reopen_unbound(STDIN, c"CONIN$", c"r");
    unbuffer(STDOUT);
    unbuffer(STDERR);
}

fn module_path() -> Option<Vec<u16>> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.as_os_str().encode_wide().collect())
}

fn file_exists(path: &[u16]) -> bool {
    Path::new(&OsString::from_wide(path)).is_file()
}

fn report(console: bool, text: &str) {
    let text = terminated(launch::wide(text));
    let format = terminated(launch::wide("%ls\n"));
    let stderr = __acrt_iob_func(STDERR);
    // SAFETY: `format` is the NUL-terminated wide format "%ls\n", whose one conversion is
    // matched by `text`, a NUL-terminated wide string; `stderr` is the CRT's stderr FILE.
    unsafe { fwprintf(stderr, format.as_ptr(), text.as_ptr()) };
    // SAFETY: `stderr` is the CRT's stderr FILE.
    unsafe { fflush(stderr) };
    if !console {
        let caption = terminated(launch::wide("Southstar"));
        // SAFETY: `text` and `caption` are NUL-terminated wide strings that outlive the
        // modal call, and a null owner window is allowed.
        unsafe {
            MessageBoxW(
                ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_ICONERROR | MB_SETFOREGROUND,
            )
        };
    }
}

fn arguments() -> Vec<Vec<u16>> {
    let mut argc: c_int = 0;
    // SAFETY: GetCommandLineW returns the process's NUL-terminated command line, which stays
    // valid for the life of the process, and `argc` is a valid out-pointer.
    let argv = unsafe { CommandLineToArgvW(GetCommandLineW(), &mut argc) };
    if argv.is_null() {
        return Vec::new();
    }
    let count = usize::try_from(argc).unwrap_or(0);
    // SAFETY: CommandLineToArgvW returned an array of `argc` string pointers that stays
    // allocated until the LocalFree below, after which `pointers` is no longer used.
    let pointers = unsafe { core::slice::from_raw_parts(argv.cast_const(), count) };
    let args = pointers.iter().map(|&arg| wide_until_nul(arg)).collect();
    // SAFETY: `argv` is the single LocalAlloc block CommandLineToArgvW returned, freed once.
    unsafe { LocalFree(argv.cast()) };
    args
}

fn wide_until_nul(text: *const u16) -> Vec<u16> {
    let mut out = Vec::new();
    loop {
        // SAFETY: `text` is one of CommandLineToArgvW's NUL-terminated strings and the loop
        // stops at the terminator, so every unit read lies inside it.
        let unit = unsafe { text.wrapping_add(out.len()).read() };
        if unit == 0 {
            return out;
        }
        out.push(unit);
    }
}

fn run(console: bool, args: &[Vec<u16>]) -> c_int {
    let browser = module_path().map(|exe| {
        let app_dir = launch::join_path(&launch::dirname(&exe), launch::APP_DIR);
        let browser = launch::join_path(&app_dir, launch::BROWSER_EXE);
        (app_dir, browser)
    });
    let Some((app_dir, browser)) = browser.filter(|(_, b)| file_exists(b)) else {
        report(console, launch::EXTRACT_NEEDED);
        return launch::EXIT_MISSING_RUNTIME;
    };
    let mut command_line = terminated(launch::command_line(&browser, args));
    let browser = terminated(browser);
    let app_dir = terminated(app_dir);
    let mut si = StartupInfo {
        cb: size_of::<StartupInfo>() as u32,
        reserved: ptr::null_mut(),
        desktop: ptr::null_mut(),
        title: ptr::null_mut(),
        x: 0,
        y: 0,
        x_size: 0,
        y_size: 0,
        x_count_chars: 0,
        y_count_chars: 0,
        fill_attribute: 0,
        flags: 0,
        show_window: 0,
        cb_reserved2: 0,
        reserved2: ptr::null_mut(),
        std_input: ptr::null_mut(),
        std_output: ptr::null_mut(),
        std_error: ptr::null_mut(),
    };
    let mut pi = ProcessInformation {
        process: ptr::null_mut(),
        thread: ptr::null_mut(),
        process_id: 0,
        thread_id: 0,
    };
    // SAFETY: `browser` and `app_dir` are NUL-terminated wide strings, `command_line` is the
    // writable NUL-terminated buffer CreateProcessW requires, `si` is a zero-filled
    // STARTUPINFOW with its size set and `pi` is a valid out-pointer.
    let ok = unsafe {
        CreateProcessW(
            browser.as_ptr(),
            command_line.as_mut_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            1,
            0,
            ptr::null_mut(),
            app_dir.as_ptr(),
            &mut si,
            &mut pi,
        )
    };
    if ok == 0 {
        report(console, &launch::start_error(GetLastError()));
        return 1;
    }
    // SAFETY: CreateProcessW succeeded, so `pi.thread` is a new handle nothing else owns.
    drop(unsafe { OwnedHandle::from_raw_handle(pi.thread) });
    // SAFETY: CreateProcessW succeeded, so `pi.process` is a new handle nothing else owns.
    let process = unsafe { OwnedHandle::from_raw_handle(pi.process) };
    // SAFETY: `process` is an open process handle with full access, kept open by `process`.
    unsafe { WaitForSingleObject(process.as_raw_handle(), INFINITE) };
    let mut exit_code: u32 = 0;
    // SAFETY: `process` is an open process handle and `exit_code` is a valid out-pointer.
    if unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut exit_code) } == 0 {
        exit_code = 1;
    }
    drop(process);
    if exit_code == launch::STATUS_DLL_NOT_FOUND || exit_code == launch::STATUS_ENTRYPOINT_NOT_FOUND
    {
        report(console, launch::RUNTIME_MISSING);
    }
    exit_code as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *mut *mut c_char) -> c_int {
    let args = arguments();
    let console = launch::args_need_console(&args);
    if console {
        attach_parent_console();
    }
    let old_error_mode = SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX);
    let rc = run(console, &args);
    SetErrorMode(old_error_mode);
    rc
}

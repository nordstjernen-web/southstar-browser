//! Southstar — the launcher's entry point over Win32 and the CRT: console attach, the child process and its exit code.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

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
    fn GetCommandLineW() -> *const u16;
    fn CommandLineToArgvW(cmd: *const u16, argc: *mut c_int) -> *mut *mut u16;
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
    fn AttachConsole(process_id: u32) -> c_int;
    fn SetErrorMode(mode: u32) -> u32;
    fn GetModuleFileNameW(module: *mut c_void, filename: *mut u16, size: u32) -> u32;
    fn GetFileAttributesW(path: *const u16) -> u32;
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
    fn CloseHandle(handle: Handle) -> c_int;
    fn GetLastError() -> u32;
    fn MessageBoxW(window: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> c_int;
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
    fn fwprintf(stream: *mut c_void, format: *const u16, ...) -> c_int;
    fn fflush(stream: *mut c_void) -> c_int;
}

const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
const IONBF: c_int = 0x0004;
const SEM_FAILCRITICALERRORS: u32 = 0x0001;
const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;
const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const INFINITE: u32 = u32::MAX;
const MB_ICONERROR: u32 = 0x10;
const MB_SETFOREGROUND: u32 = 0x1_0000;

fn terminated(mut text: Vec<u16>) -> Vec<u16> {
    text.push(0);
    text
}

unsafe fn stream(index: u32) -> *mut c_void {
    unsafe { __acrt_iob_func(index) }
}

fn fd_is_bound(fd: c_int) -> bool {
    let h = unsafe { _get_osfhandle(fd) };
    h != 0 && h != -1
}

fn attach_parent_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let (stdin, stdout, stderr) = (stream(0), stream(1), stream(2));
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

fn module_path() -> Option<Vec<u16>> {
    let mut cap: u32 = 512;
    loop {
        let mut buf = vec![0u16; cap as usize];
        let n = unsafe { GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), cap) };
        if n == 0 {
            return None;
        }
        if n + 1 < cap {
            buf.truncate(n as usize);
            return Some(buf);
        }
        if cap > 32768 {
            return None;
        }
        cap *= 2;
    }
}

fn file_exists(path: &[u16]) -> bool {
    let path = terminated(path.to_vec());
    let attrs = unsafe { GetFileAttributesW(path.as_ptr()) };
    attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_DIRECTORY == 0
}

fn report(console: bool, text: &str) {
    let text = terminated(launch::wide(text));
    let format = terminated(launch::wide("%ls\n"));
    unsafe {
        let stderr = stream(2);
        fwprintf(stderr, format.as_ptr(), text.as_ptr());
        fflush(stderr);
        if !console {
            let caption = terminated(launch::wide("Southstar"));
            MessageBoxW(
                ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_ICONERROR | MB_SETFOREGROUND,
            );
        }
    }
}

fn arguments() -> Vec<Vec<u16>> {
    let mut argc: c_int = 0;
    unsafe {
        let argv = CommandLineToArgvW(GetCommandLineW(), &mut argc);
        if argv.is_null() {
            return Vec::new();
        }
        let args = (0..usize::try_from(argc).unwrap_or(0))
            .map(|i| {
                let p = *argv.add(i);
                let len = (0..).take_while(|&k| *p.add(k) != 0).count();
                core::slice::from_raw_parts(p, len).to_vec()
            })
            .collect();
        LocalFree(argv.cast());
        args
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
    let mut si: StartupInfo = unsafe { core::mem::zeroed() };
    si.cb = size_of::<StartupInfo>() as u32;
    let mut pi: ProcessInformation = unsafe { core::mem::zeroed() };
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
        report(console, &launch::start_error(unsafe { GetLastError() }));
        return 1;
    }
    let mut exit_code: u32 = 0;
    unsafe {
        WaitForSingleObject(pi.process, INFINITE);
        if GetExitCodeProcess(pi.process, &mut exit_code) == 0 {
            exit_code = 1;
        }
        CloseHandle(pi.thread);
        CloseHandle(pi.process);
    }
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
    let old_error_mode = unsafe { SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX) };
    let rc = run(console, &args);
    unsafe { SetErrorMode(old_error_mode) };
    rc
}

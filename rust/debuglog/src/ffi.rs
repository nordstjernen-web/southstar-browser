//! Southstar — the C ABI of the debug log, as declared in src/debuglog.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use southstar_glib as glib;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_get_real_time() -> i64;
    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_path_get_dirname(file_name: *const c_char) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
}

type Listener = Option<unsafe extern "C" fn(entry: *const Entry, user_data: *mut c_void)>;

#[repr(C)]
pub struct Entry {
    monotonic_us: i64,
    level: c_int,
    category: *mut c_char,
    message: *mut c_char,
}

impl Drop for Entry {
    fn drop(&mut self) {
        unsafe {
            glib::g_free(self.category.cast());
            glib::g_free(self.message.cast());
        }
    }
}

#[derive(Clone, Copy)]
struct Subscription {
    id: c_uint,
    callback: Listener,
    user_data: *mut c_void,
}

struct Log {
    entries: VecDeque<Box<Entry>>,
    subscriptions: Vec<Subscription>,
    next_id: c_uint,
    file: Option<File>,
    file_tried: bool,
}

unsafe impl Send for Log {}

struct FilePath(*mut c_char);

unsafe impl Send for FilePath {}

static LOG: Mutex<Log> = Mutex::new(Log {
    entries: VecDeque::new(),
    subscriptions: Vec::new(),
    next_id: 1,
    file: None,
    file_tried: false,
});
static FILE_PATH: Mutex<FilePath> = Mutex::new(FilePath(ptr::null_mut()));
static INIT: OnceLock<()> = OnceLock::new();

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn file_enabled() -> bool {
    let set = |name: &CStr| !unsafe { glib::g_getenv(name.as_ptr()) }.is_null();
    if cfg!(windows) {
        !set(c"NS_NO_LOG_FILE")
    } else {
        set(c"NS_LOG_FILE")
    }
}

fn path_buf(path: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(path))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(path).into_owned())
    }
}

fn open_log_file() -> Option<File> {
    let path = ns_debug_log_file_path();
    let bytes = unsafe { glib::bytes(path) }?;
    unsafe {
        let dir = g_path_get_dirname(path);
        if !dir.is_null() {
            g_mkdir_with_parents(dir, 0o700);
            glib::g_free(dir.cast());
        }
    }
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(path_buf(bytes))
        .ok()
}

fn write_file(log: &mut Log, entry: &Entry) {
    if !file_enabled() {
        return;
    }
    if log.file.is_none() && !log.file_tried {
        log.file_tried = true;
        log.file = open_log_file();
    }
    if let Some(file) = log.file.as_mut() {
        let line = crate::file_line(
            unsafe { g_get_real_time() } / 1000,
            entry.level,
            unsafe { glib::bytes(entry.category) }.unwrap_or_default(),
            unsafe { glib::bytes(entry.message) }.unwrap_or_default(),
        );
        let _ = file.write_all(&line);
        let _ = file.flush();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_debug_log_init() {
    INIT.get_or_init(|| ());
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dlog_level_name(level: c_int) -> *const c_char {
    crate::level_name(level).as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_debug_log_file_path() -> *const c_char {
    let mut path = lock(&FILE_PATH);
    if path.0.is_null() && file_enabled() {
        unsafe {
            let fixed = glib::g_getenv(c"NS_LOG_FILE".as_ptr());
            path.0 = if !fixed.is_null() && *fixed != 0 {
                glib::g_strdup(fixed)
            } else {
                g_build_filename(
                    glib::g_get_user_data_dir(),
                    c"Southstar".as_ptr(),
                    c"southstar-debug.log".as_ptr(),
                    ptr::null::<c_char>(),
                )
            };
        }
    }
    path.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_debug_log_emit_take(
    level: c_int,
    category: *const c_char,
    message: *mut c_char,
) {
    ns_debug_log_init();
    let entry = Box::new(Entry {
        monotonic_us: unsafe { g_get_monotonic_time() },
        level,
        category: glib::strdup(unsafe { glib::bytes(category) }.unwrap_or_default()),
        message: if message.is_null() {
            glib::strdup(b"")
        } else {
            message
        },
    });
    let subscriptions = lock(&LOG).subscriptions.clone();
    for subscription in &subscriptions {
        if let Some(callback) = subscription.callback {
            unsafe { callback(&*entry, subscription.user_data) };
        }
    }
    let mut log = lock(&LOG);
    write_file(&mut log, &entry);
    log.entries.push_back(entry);
    while log.entries.len() > crate::CAPACITY {
        log.entries.pop_front();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_debug_log_subscribe(callback: Listener, user_data: *mut c_void) -> c_uint {
    if callback.is_none() {
        return 0;
    }
    ns_debug_log_init();
    let mut log = lock(&LOG);
    let id = log.next_id;
    log.next_id = log.next_id.wrapping_add(1);
    log.subscriptions.push(Subscription {
        id,
        callback,
        user_data,
    });
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_debug_log_unsubscribe(id: c_uint) {
    if id == 0 || INIT.get().is_none() {
        return;
    }
    let mut log = lock(&LOG);
    if let Some(i) = log.subscriptions.iter().position(|s| s.id == id) {
        log.subscriptions.remove(i);
    }
}

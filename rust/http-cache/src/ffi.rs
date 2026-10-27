//! Southstar — the GLib, file and ACL calls behind the HTTP cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::path::PathBuf;

use southstar_config::util as fs;
use southstar_glib as glib;

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

#[repr(C)]
struct GTimeZone {
    _private: [u8; 0],
}

const G_LOG_LEVEL_WARNING: c_int = 1 << 4;

unsafe extern "C" {
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    fn g_date_time_new_from_iso8601(
        text: *const c_char,
        default_tz: *mut GTimeZone,
    ) -> *mut GDateTime;
    fn g_date_time_new(
        tz: *mut GTimeZone,
        year: c_int,
        month: c_int,
        day: c_int,
        hour: c_int,
        minute: c_int,
        seconds: f64,
    ) -> *mut GDateTime;
    fn g_date_time_to_unix(datetime: *mut GDateTime) -> i64;
    fn g_date_time_unref(datetime: *mut GDateTime);
    fn g_time_zone_new_utc() -> *mut GTimeZone;
    fn g_time_zone_unref(tz: *mut GTimeZone);
    fn sscanf(s: *const c_char, format: *const c_char, ...) -> c_int;
}

pub(crate) fn warn(message: &[u8]) {
    let message = crate::cstring(message);
    unsafe {
        g_log(
            ptr::null(),
            G_LOG_LEVEL_WARNING,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    };
}

pub(crate) fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    fs::build_filename(parts)
}

pub(crate) fn user_cache_dir() -> Vec<u8> {
    unsafe { glib::bytes(glib::g_get_user_cache_dir()) }
        .unwrap_or_default()
        .to_vec()
}

pub(crate) fn mkdir_with_parents(path: &[u8]) -> bool {
    fs::os_path(path).is_some_and(|path| fs::make_dirs(&path, 0o700).is_ok())
}

#[cfg(not(windows))]
pub(crate) fn restrict_to_owner(path: &[u8], is_dir: bool) {
    if let Some(path) = fs::os_path(path) {
        let _ = fs::set_mode(&path, if is_dir { 0o700 } else { 0o600 });
    }
}

#[cfg(windows)]
pub(crate) fn restrict_to_owner(path: &[u8], is_dir: bool) {
    acl::set_owner_only(path, is_dir);
}

#[cfg(windows)]
mod acl {
    use core::ffi::c_void;
    use core::ptr;

    type Handle = *mut c_void;

    #[repr(C)]
    struct Trustee {
        multiple_trustee: *mut c_void,
        multiple_trustee_operation: i32,
        form: i32,
        kind: i32,
        name: *mut c_void,
    }

    #[repr(C)]
    struct ExplicitAccess {
        permissions: u32,
        mode: i32,
        inheritance: u32,
        trustee: Trustee,
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_USER: i32 = 1;
    const GENERIC_ALL: u32 = 0x1000_0000;
    const SET_ACCESS: i32 = 2;
    const OBJECT_INHERIT_ACE: u32 = 0x1;
    const CONTAINER_INHERIT_ACE: u32 = 0x2;
    const TRUSTEE_IS_SID: i32 = 0;
    const TRUSTEE_IS_USER: i32 = 1;
    const SE_FILE_OBJECT: i32 = 1;
    const DACL_SECURITY_INFORMATION: u32 = 0x4;
    const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x8000_0000;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: i32,
            info: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
        fn SetEntriesInAclW(
            count: u32,
            entries: *const ExplicitAccess,
            old_acl: *mut c_void,
            new_acl: *mut *mut c_void,
        ) -> u32;
        fn SetNamedSecurityInfoW(
            name: *const u16,
            object_type: i32,
            info: u32,
            owner: *mut c_void,
            group: *mut c_void,
            dacl: *mut c_void,
            sacl: *mut c_void,
        ) -> u32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    pub(super) fn set_owner_only(path: &[u8], container: bool) -> bool {
        let Ok(path) = std::str::from_utf8(path) else {
            return false;
        };
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let mut token = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return false;
        }
        let mut need = 0u32;
        unsafe { GetTokenInformation(token, TOKEN_USER, ptr::null_mut(), 0, &mut need) };
        if need == 0 {
            unsafe { CloseHandle(token) };
            return false;
        }
        let mut user = vec![0u64; (need as usize).div_ceil(8)];
        let got = unsafe {
            GetTokenInformation(token, TOKEN_USER, user.as_mut_ptr().cast(), need, &mut need)
        };
        unsafe { CloseHandle(token) };
        if got == 0 {
            return false;
        }
        let sid = user[0] as usize as *mut c_void;
        let access = ExplicitAccess {
            permissions: GENERIC_ALL,
            mode: SET_ACCESS,
            inheritance: if container {
                CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE
            } else {
                0
            },
            trustee: Trustee {
                multiple_trustee: ptr::null_mut(),
                multiple_trustee_operation: 0,
                form: TRUSTEE_IS_SID,
                kind: TRUSTEE_IS_USER,
                name: sid,
            },
        };
        let mut dacl = ptr::null_mut();
        if unsafe { SetEntriesInAclW(1, &access, ptr::null_mut(), &mut dacl) } != 0 {
            return false;
        }
        let result = unsafe {
            SetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                dacl,
                ptr::null_mut(),
            )
        };
        unsafe { LocalFree(dacl) };
        result == 0
    }
}

#[cfg(unix)]
fn path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn unlink(file: &[u8]) {
    let _ = std::fs::remove_file(path(file));
}

pub(crate) fn rmrf(dir: &[u8]) {
    remove_tree(&path(dir));
}

fn remove_tree(dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let child = entry.path();
            let is_link =
                std::fs::symlink_metadata(&child).is_ok_and(|m| m.file_type().is_symlink());
            if !is_link && child.is_dir() {
                remove_tree(&child);
            } else {
                let _ = std::fs::remove_file(&child);
            }
        }
    }
    let _ = std::fs::remove_dir(dir);
}

pub(crate) fn file_size(file: &[u8]) -> Option<u64> {
    std::fs::metadata(path(file)).ok().map(|m| m.len())
}

pub(crate) fn read_file(file: &[u8]) -> Option<Vec<u8>> {
    std::fs::read(path(file)).ok()
}

pub(crate) fn write_file_consistent(file: &[u8], body: &[u8]) -> Result<(), Vec<u8>> {
    let Some(target) = fs::os_path(file) else {
        return Err(b"invalid file name".to_vec());
    };
    fs::write_atomic(&target, body, 0o600).map_err(|e| e.to_string().into_bytes())
}

pub(crate) fn now_seconds() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

pub(crate) fn ascii_strtoll(text: &[u8]) -> i64 {
    fs::ascii_strtoll(text).unwrap_or(0)
}

fn month_from_name(m: &[u8]) -> c_int {
    const MONTHS: [&[u8]; 12] = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    MONTHS
        .iter()
        .position(|name| m.len() >= 3 && m[..3].eq_ignore_ascii_case(name))
        .map_or(0, |i| i as c_int + 1)
}

fn unix_from(dt: *mut GDateTime) -> i64 {
    if dt.is_null() {
        return 0;
    }
    unsafe {
        let r = g_date_time_to_unix(dt);
        g_date_time_unref(dt);
        r
    }
}

pub(crate) fn parse_http_date(text: &[u8]) -> i64 {
    if text.is_empty() {
        return 0;
    }
    let s = crate::cstring(text);
    let iso = unsafe { g_date_time_new_from_iso8601(s.as_ptr(), ptr::null_mut()) };
    if !iso.is_null() {
        return unix_from(iso);
    }
    let bytes = s.as_bytes();
    let mut p = bytes
        .iter()
        .position(|&c| c == b',')
        .map_or(0, |comma| comma + 1);
    while bytes.get(p) == Some(&b' ') {
        p += 1;
    }
    let rest = crate::cstring(&bytes[p..]);
    let (mut day, mut year, mut hh, mut mm, mut ss) =
        (0 as c_int, 0 as c_int, 0 as c_int, 0 as c_int, 0 as c_int);
    let mut mon = [0 as c_char; 4];
    let mut sep: c_char = 0;
    let scanned = unsafe {
        sscanf(
            rest.as_ptr(),
            c"%d %3s %d %d:%d:%d".as_ptr(),
            &mut day as *mut c_int,
            mon.as_mut_ptr(),
            &mut year as *mut c_int,
            &mut hh as *mut c_int,
            &mut mm as *mut c_int,
            &mut ss as *mut c_int,
        )
    };
    if scanned != 6 {
        let scanned = unsafe {
            sscanf(
                rest.as_ptr(),
                c"%d%c%3s%c%d %d:%d:%d".as_ptr(),
                &mut day as *mut c_int,
                &mut sep as *mut c_char,
                mon.as_mut_ptr(),
                &mut sep as *mut c_char,
                &mut year as *mut c_int,
                &mut hh as *mut c_int,
                &mut mm as *mut c_int,
                &mut ss as *mut c_int,
            )
        };
        if scanned == 8 {
            if year < 100 {
                year += if year < 70 { 2000 } else { 1900 };
            }
        } else {
            let scanned = unsafe {
                sscanf(
                    rest.as_ptr(),
                    c"%3s %d %d:%d:%d %d".as_ptr(),
                    mon.as_mut_ptr(),
                    &mut day as *mut c_int,
                    &mut hh as *mut c_int,
                    &mut mm as *mut c_int,
                    &mut ss as *mut c_int,
                    &mut year as *mut c_int,
                )
            };
            if scanned != 6 {
                return 0;
            }
        }
    }
    let mon = unsafe { CStr::from_ptr(mon.as_ptr()) }.to_bytes();
    let month = month_from_name(mon);
    if month == 0 {
        return 0;
    }
    if !(1970..=9999).contains(&year)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hh)
        || !(0..=59).contains(&mm)
        || !(0..=60).contains(&ss)
    {
        return 0;
    }
    unsafe {
        let utc = g_time_zone_new_utc();
        let dt = g_date_time_new(utc, year, month, day, hh, mm, f64::from(ss));
        g_time_zone_unref(utc);
        unix_from(dt)
    }
}

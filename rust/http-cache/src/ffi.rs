//! Southstar — the C ABI of the HTTP cache, as declared in src/cache.h, and the GLib, file and ACL calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::path::PathBuf;

use southstar_glib::{self as glib, GBoolean};

use crate::{Entry, Response};

#[repr(C)]
pub struct CacheEntry {
    final_url: *mut c_char,
    status: c_long,
    content_type: *mut c_char,
    cors_allow_origin: *mut c_char,
    etag: *mut c_char,
    last_modified: *mut c_char,
    expires_at: i64,
    fetched_at: i64,
    body: *mut GByteArray,
}

#[repr(C)]
pub struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

#[repr(C)]
struct GTimeZone {
    _private: [u8; 0],
}

#[repr(C)]
struct GError {
    domain: u32,
    code: c_int,
    message: *mut c_char,
}

const G_LOG_LEVEL_WARNING: c_int = 1 << 4;
const G_FILE_SET_CONTENTS_CONSISTENT: c_int = 1;

unsafe extern "C" {
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    fn g_checksum_get_string(checksum: *mut glib::GChecksum) -> *const c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    #[cfg(not(windows))]
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_get_real_time() -> i64;
    fn g_ascii_strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_uint) -> i64;
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
    fn g_file_set_contents_full(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        flags: c_int,
        mode: c_int,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_error_free(error: *mut GError);
    fn g_byte_array_new() -> *mut GByteArray;
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
    fn g_byte_array_unref(array: *mut GByteArray);
    fn sscanf(s: *const c_char, format: *const c_char, ...) -> c_int;
    fn ns_url_is_http_or_https(url: *const c_char) -> GBoolean;
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

pub(crate) fn sha256_hex(parts: &[&[u8]]) -> Vec<u8> {
    unsafe {
        let checksum = glib::g_checksum_new(glib::G_CHECKSUM_SHA256);
        for part in parts {
            glib::g_checksum_update(checksum, part.as_ptr(), part.len() as isize);
        }
        let hex = CStr::from_ptr(g_checksum_get_string(checksum))
            .to_bytes()
            .to_vec();
        glib::g_checksum_free(checksum);
        hex
    }
}

pub(crate) fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    let owned: Vec<CString> = parts.iter().map(|part| crate::cstring(part)).collect();
    let mut pointers: Vec<*mut c_char> = owned.iter().map(|p| p.as_ptr().cast_mut()).collect();
    pointers.push(ptr::null_mut());
    unsafe {
        let joined = glib::g_build_filenamev(pointers.as_mut_ptr());
        let bytes = CStr::from_ptr(joined).to_bytes().to_vec();
        glib::g_free(joined.cast());
        bytes
    }
}

pub(crate) fn user_cache_dir() -> Vec<u8> {
    unsafe { glib::bytes(glib::g_get_user_cache_dir()) }
        .unwrap_or_default()
        .to_vec()
}

pub(crate) fn mkdir_with_parents(path: &[u8]) -> bool {
    let path = crate::cstring(path);
    unsafe { g_mkdir_with_parents(path.as_ptr(), 0o700) == 0 }
}

#[cfg(not(windows))]
pub(crate) fn restrict_to_owner(path: &[u8], is_dir: bool) {
    let path = crate::cstring(path);
    unsafe { g_chmod(path.as_ptr(), if is_dir { 0o700 } else { 0o600 }) };
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
    let name = crate::cstring(file);
    let mut error: *mut GError = ptr::null_mut();
    let contents = if body.is_empty() {
        c"".as_ptr()
    } else {
        body.as_ptr().cast()
    };
    let ok = unsafe {
        g_file_set_contents_full(
            name.as_ptr(),
            contents,
            body.len() as isize,
            G_FILE_SET_CONTENTS_CONSISTENT,
            0o600,
            &mut error,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    let message = unsafe {
        let message = error
            .as_ref()
            .and_then(|e| glib::bytes(e.message))
            .unwrap_or_default()
            .to_vec();
        if !error.is_null() {
            g_error_free(error);
        }
        message
    };
    Err(message)
}

pub(crate) fn now_seconds() -> i64 {
    let micros = unsafe { g_get_real_time() };
    micros / 1_000_000
}

pub(crate) fn ascii_strtoll(text: &[u8]) -> i64 {
    let text = crate::cstring(text);
    unsafe { g_ascii_strtoll(text.as_ptr(), ptr::null_mut(), 10) }
}

pub(crate) fn url_is_http_or_https(url: &[u8]) -> bool {
    let url = crate::cstring(url);
    unsafe { ns_url_is_http_or_https(url.as_ptr()) != 0 }
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

fn widen<T: Into<i64>>(value: T) -> i64 {
    value.into()
}

unsafe fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

unsafe fn headers<'a>(list: *const *const c_char) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    if list.is_null() {
        return out;
    }
    let mut i = 0;
    loop {
        let item = unsafe { *list.add(i) };
        if item.is_null() {
            break;
        }
        out.push(unsafe { CStr::from_ptr(item) }.to_bytes());
        i += 1;
    }
    out
}

fn strdup(bytes: Option<Vec<u8>>) -> *mut c_char {
    bytes.map_or(ptr::null_mut(), |b| glib::strdup(&b))
}

fn into_c(entry: Entry) -> *mut CacheEntry {
    let body = unsafe { g_byte_array_new() };
    if !entry.body.is_empty() {
        unsafe { g_byte_array_append(body, entry.body.as_ptr(), entry.body.len() as c_uint) };
    }
    let out = unsafe { glib::g_malloc0(size_of::<CacheEntry>()) }.cast::<CacheEntry>();
    unsafe {
        out.write(CacheEntry {
            final_url: strdup(entry.final_url),
            status: entry.status as c_long,
            content_type: strdup(entry.content_type),
            cors_allow_origin: strdup(entry.cors_allow_origin),
            etag: strdup(entry.etag),
            last_modified: strdup(entry.last_modified),
            expires_at: entry.expires_at,
            fetched_at: entry.fetched_at,
            body,
        })
    };
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_cache_init() {
    crate::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_cache_shutdown() {
    crate::shutdown();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_cache_clear() {
    crate::clear();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cache_get(
    url: *const c_char,
    partition: *const c_char,
    request_headers: *const *const c_char,
) -> *mut CacheEntry {
    let Some(url) = (unsafe { text(url) }) else {
        return ptr::null_mut();
    };
    let headers = unsafe { headers(request_headers) };
    crate::get(url, unsafe { text(partition) }, &headers).map_or(ptr::null_mut(), into_c)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cache_is_fresh(e: *const CacheEntry) -> GBoolean {
    match unsafe { e.as_ref() } {
        Some(e) => glib::boolean(crate::is_fresh(e.expires_at)),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cache_entry_free(e: *mut CacheEntry) {
    let Some(entry) = (unsafe { e.as_mut() }) else {
        return;
    };
    unsafe {
        for p in [
            entry.final_url,
            entry.content_type,
            entry.cors_allow_origin,
            entry.etag,
            entry.last_modified,
        ] {
            glib::g_free(p.cast());
        }
        if !entry.body.is_null() {
            g_byte_array_unref(entry.body);
        }
        glib::g_free(e.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cache_put(
    url: *const c_char,
    partition: *const c_char,
    final_url: *const c_char,
    status: c_long,
    content_type: *const c_char,
    cors_allow_origin: *const c_char,
    etag: *const c_char,
    last_modified: *const c_char,
    cache_control: *const c_char,
    expires_header: *const c_char,
    vary: *const c_char,
    request_headers: *const *const c_char,
    body: *const c_void,
    body_len: usize,
) {
    let Some(url) = (unsafe { text(url) }) else {
        return;
    };
    let response = unsafe {
        Response {
            final_url: text(final_url),
            status: widen(status),
            content_type: text(content_type),
            cors_allow_origin: text(cors_allow_origin),
            etag: text(etag),
            last_modified: text(last_modified),
            cache_control: text(cache_control),
            expires_header: text(expires_header),
            vary: text(vary),
            body: glib::slice(body.cast(), body_len),
        }
    };
    if body.is_null() && body_len > 0 {
        return;
    }
    let headers = unsafe { headers(request_headers) };
    crate::put(url, unsafe { text(partition) }, &response, &headers);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cache_promote_304(
    url: *const c_char,
    partition: *const c_char,
    request_headers: *const *const c_char,
    cache_control: *const c_char,
    expires_header: *const c_char,
) {
    let Some(url) = (unsafe { text(url) }) else {
        return;
    };
    let headers = unsafe { headers(request_headers) };
    unsafe {
        crate::promote_304(
            url,
            text(partition),
            &headers,
            text(cache_control),
            text(expires_header),
        )
    };
}

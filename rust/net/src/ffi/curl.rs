//! Southstar — the libcurl calls, constants and structs the transport layer uses: easy, multi and share handles, version information, header lists and the TLS options.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};

pub type Code = c_int;

pub const E_FAILED_INIT: Code = 2;
pub const E_ABORTED_BY_CALLBACK: Code = 42;
pub const M_OK: c_int = 0;
pub const MSG_DONE: c_int = 1;

pub const OPT_SSL_VERIFYPEER: c_int = 64;
pub const OPT_SSL_VERIFYHOST: c_int = 81;
pub const OPT_SSL_OPTIONS: c_int = 216;
pub const OPT_CAINFO: c_int = 10065;
pub const OPT_SSL_CIPHER_LIST: c_int = 10083;
pub const OPT_TLS13_CIPHERS: c_int = 10276;
pub const OPT_DOH_URL: c_int = 10279;
pub const OPT_SSL_EC_CURVES: c_int = 10298;
pub const OPT_ECH: c_int = 10325;
pub const SSLOPT_NATIVE_CA: c_long = 16;

pub const MOPT_PIPELINING: c_int = 3;
pub const PIPE_MULTIPLEX: c_long = 2;

pub const SHOPT_SHARE: c_int = 1;
pub const SHOPT_LOCKFUNC: c_int = 3;
pub const SHOPT_UNLOCKFUNC: c_int = 4;
pub const LOCK_DATA_DNS: c_int = 3;
pub const LOCK_DATA_SSL_SESSION: c_int = 4;
pub const LOCK_DATA_CONNECT: c_int = 5;
pub const LOCK_DATA_PSL: c_int = 6;
pub const LOCK_DATA_HSTS: c_int = 7;
pub const LOCK_DATA_SLOTS: usize = 16;

pub const GLOBAL_DEFAULT: c_long = 3;
pub const VERSION_NOW: c_int = 10;
pub const VERSION_LIBZ: c_int = 1 << 3;
pub const VERSION_BROTLI: c_int = 1 << 23;
pub const VERSION_HTTP3: c_int = 1 << 25;
pub const VERSION_ZSTD: c_int = 1 << 26;
pub const ECH_MIN_VERSION: c_uint = 0x0008_0800;

pub const HTTP_VERSION_2TLS: c_long = 4;
pub const HTTP_VERSION_3: c_long = 30;

#[repr(C)]
pub struct VersionInfo {
    pub age: c_int,
    pub version: *const c_char,
    pub version_num: c_uint,
    pub host: *const c_char,
    pub features: c_int,
    pub ssl_version: *const c_char,
    pub ssl_version_num: c_long,
    pub libz_version: *const c_char,
    pub protocols: *const *const c_char,
    pub ares: *const c_char,
    pub ares_num: c_int,
    pub libidn: *const c_char,
    pub iconv_ver_num: c_int,
    pub libssh_version: *const c_char,
    pub brotli_ver_num: c_uint,
    pub brotli_version: *const c_char,
    pub nghttp2_ver_num: c_uint,
    pub nghttp2_version: *const c_char,
    pub quic_version: *const c_char,
    pub cainfo: *const c_char,
    pub capath: *const c_char,
    pub zstd_ver_num: c_uint,
    pub zstd_version: *const c_char,
    pub hyper_version: *const c_char,
    pub gsasl_version: *const c_char,
    pub feature_names: *const *const c_char,
}

#[cfg(all(target_pointer_width = "64", not(windows)))]
const _: () = assert!(
    core::mem::offset_of!(VersionInfo, features) == 32
        && core::mem::offset_of!(VersionInfo, ssl_version) == 40
        && core::mem::offset_of!(VersionInfo, feature_names) == 200
);

#[repr(C)]
pub struct Message {
    pub msg: c_int,
    pub easy_handle: *mut c_void,
    pub result: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::offset_of!(Message, result) == 16);

#[repr(C)]
pub struct Slist {
    pub data: *mut c_char,
    pub next: *mut Slist,
}

pub type LockFn =
    unsafe extern "C" fn(handle: *mut c_void, data: c_int, access: c_int, user: *mut c_void);
pub type UnlockFn = unsafe extern "C" fn(handle: *mut c_void, data: c_int, user: *mut c_void);

unsafe extern "C" {
    pub fn curl_global_init(flags: c_long) -> Code;
    pub fn curl_global_cleanup();
    pub fn curl_version_info(age: c_int) -> *const VersionInfo;
    pub fn curl_easy_setopt(handle: *mut c_void, option: c_int, ...) -> Code;
    pub fn curl_easy_perform(handle: *mut c_void) -> Code;
    pub fn curl_multi_init() -> *mut c_void;
    pub fn curl_multi_cleanup(multi: *mut c_void) -> c_int;
    pub fn curl_multi_setopt(multi: *mut c_void, option: c_int, ...) -> c_int;
    pub fn curl_multi_add_handle(multi: *mut c_void, easy: *mut c_void) -> c_int;
    pub fn curl_multi_remove_handle(multi: *mut c_void, easy: *mut c_void) -> c_int;
    pub fn curl_multi_perform(multi: *mut c_void, running: *mut c_int) -> c_int;
    pub fn curl_multi_info_read(multi: *mut c_void, queued: *mut c_int) -> *mut Message;
    pub fn curl_multi_timeout(multi: *mut c_void, timeout: *mut c_long) -> c_int;
    pub fn curl_multi_poll(
        multi: *mut c_void,
        extra_fds: *mut c_void,
        extra_nfds: c_uint,
        timeout_ms: c_int,
        ret: *mut c_int,
    ) -> c_int;
    pub fn curl_multi_wakeup(multi: *mut c_void) -> c_int;
    pub fn curl_share_init() -> *mut c_void;
    pub fn curl_share_setopt(share: *mut c_void, option: c_int, ...) -> c_int;
    pub fn curl_share_cleanup(share: *mut c_void) -> c_int;
}

pub struct Version {
    pub features: c_int,
    pub version_num: c_uint,
    pub ssl_version: Vec<u8>,
    pub feature_names: Vec<Vec<u8>>,
}

pub fn version() -> Option<Version> {
    let info = unsafe { curl_version_info(VERSION_NOW).as_ref() }?;
    let text = |p: *const c_char| {
        if p.is_null() {
            Vec::new()
        } else {
            unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
        }
    };
    let mut feature_names = Vec::new();
    if info.age >= 10 && !info.feature_names.is_null() {
        let mut p = info.feature_names;
        while !unsafe { *p }.is_null() {
            feature_names.push(text(unsafe { *p }));
            p = unsafe { p.add(1) };
        }
    }
    Some(Version {
        features: info.features,
        version_num: info.version_num,
        ssl_version: text(info.ssl_version),
        feature_names,
    })
}

//! Southstar — the libcurl calls, constants and structs the transport layer uses: easy, multi and share handles, transfer options and information, version information, header lists and the TLS options.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};

pub type Code = c_int;

pub const E_OK: Code = 0;
pub const E_FAILED_INIT: Code = 2;
pub const E_COULDNT_RESOLVE_HOST: Code = 6;
pub const E_COULDNT_CONNECT: Code = 7;
pub const E_OPERATION_TIMEDOUT: Code = 28;
pub const E_ABORTED_BY_CALLBACK: Code = 42;
pub const E_RECV_ERROR: Code = 56;
pub const E_PEER_FAILED_VERIFICATION: Code = 60;
pub const E_FILESIZE_EXCEEDED: Code = 63;
pub const E_SSL_CACERT_BADFILE: Code = 77;
pub const E_SSL_ISSUER_ERROR: Code = 83;
pub const ERROR_SIZE: usize = 256;
pub const M_OK: c_int = 0;
pub const MSG_DONE: c_int = 1;

pub const OPT_WRITEDATA: c_int = 10001;
pub const OPT_URL: c_int = 10002;
pub const OPT_PROXY: c_int = 10004;
pub const OPT_ERRORBUFFER: c_int = 10010;
pub const OPT_WRITEFUNCTION: c_int = 20011;
pub const OPT_TIMEOUT: c_int = 13;
pub const OPT_POSTFIELDS: c_int = 10015;
pub const OPT_REFERER: c_int = 10016;
pub const OPT_USERAGENT: c_int = 10018;
pub const OPT_HTTPHEADER: c_int = 10023;
pub const OPT_HEADERDATA: c_int = 10029;
pub const OPT_COOKIEFILE: c_int = 10031;
pub const OPT_CUSTOMREQUEST: c_int = 10036;
pub const OPT_VERBOSE: c_int = 41;
pub const OPT_NOPROGRESS: c_int = 43;
pub const OPT_NOBODY: c_int = 44;
pub const OPT_POST: c_int = 47;
pub const OPT_FOLLOWLOCATION: c_int = 52;
pub const OPT_XFERINFODATA: c_int = 10057;
pub const OPT_AUTOREFERER: c_int = 58;
pub const OPT_POSTFIELDSIZE: c_int = 60;
pub const OPT_MAXREDIRS: c_int = 68;
pub const OPT_CONNECTTIMEOUT: c_int = 78;
pub const OPT_HEADERFUNCTION: c_int = 20079;
pub const OPT_COOKIEJAR: c_int = 10082;
pub const OPT_HTTP_VERSION: c_int = 84;
pub const OPT_NOSIGNAL: c_int = 99;
pub const OPT_SHARE: c_int = 10100;
pub const OPT_ACCEPT_ENCODING: c_int = 10102;
pub const OPT_UNRESTRICTED_AUTH: c_int = 105;
pub const OPT_MAXFILESIZE_LARGE: c_int = 30117;
pub const OPT_COOKIELIST: c_int = 10135;
pub const OPT_CONNECT_ONLY: c_int = 141;
pub const OPT_NOPROXY: c_int = 10177;
pub const OPT_XFERINFOFUNCTION: c_int = 20219;
pub const OPT_ALTSVC: c_int = 10287;
pub const OPT_HSTS_CTRL: c_int = 299;
pub const OPT_HSTS: c_int = 10300;
pub const OPT_PROTOCOLS_STR: c_int = 10318;
pub const OPT_REDIR_PROTOCOLS_STR: c_int = 10319;
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
pub const HSTS_ENABLE: c_long = 1;

pub const INFO_EFFECTIVE_URL: c_int = 0x10_0001;
pub const INFO_PRIMARY_IP: c_int = 0x10_0020;
pub const INFO_RESPONSE_CODE: c_int = 0x20_0002;
pub const INFO_NUM_CONNECTS: c_int = 0x20_001a;
pub const INFO_HTTP_VERSION: c_int = 0x20_002e;
pub const INFO_TOTAL_TIME_T: c_int = 0x60_0032;
pub const INFO_NAMELOOKUP_TIME_T: c_int = 0x60_0033;
pub const INFO_CONNECT_TIME_T: c_int = 0x60_0034;
pub const INFO_PRETRANSFER_TIME_T: c_int = 0x60_0035;
pub const INFO_STARTTRANSFER_TIME_T: c_int = 0x60_0036;
pub const INFO_APPCONNECT_TIME_T: c_int = 0x60_0038;

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
    pub fn curl_easy_init() -> *mut c_void;
    pub fn curl_easy_cleanup(handle: *mut c_void);
    pub fn curl_easy_setopt(handle: *mut c_void, option: c_int, ...) -> Code;
    pub fn curl_easy_getinfo(handle: *mut c_void, info: c_int, ...) -> Code;
    pub fn curl_easy_perform(handle: *mut c_void) -> Code;
    pub fn curl_easy_strerror(code: Code) -> *const c_char;
    pub fn curl_slist_append(list: *mut Slist, data: *const c_char) -> *mut Slist;
    pub fn curl_slist_free_all(list: *mut Slist);
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

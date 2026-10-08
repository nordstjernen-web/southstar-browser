//! Southstar — the C ABI of startup security, as declared in src/security.h, and the system calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::Mutex;

use southstar_glib::{self as glib, FALSE, GBoolean, TRUE};

use crate::SriDigest;

const G_LOG_LEVEL_WARNING: c_int = 1 << 4;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const G_LOG_LEVEL_INFO: c_int = 1 << 6;

unsafe extern "C" {
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    #[cfg(target_os = "linux")]
    fn g_printerr(format: *const c_char, ...);
    #[cfg(target_os = "linux")]
    fn g_strerror(errnum: c_int) -> *const c_char;
    #[cfg(target_os = "linux")]
    fn g_get_home_dir() -> *const c_char;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn g_get_user_runtime_dir() -> *const c_char;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn g_get_user_special_dir(directory: c_int) -> *const c_char;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
}

fn log(level: c_int, message: &str) {
    let message = CString::new(message.replace('\0', "")).unwrap_or_default();
    unsafe { g_log(ptr::null(), level, c"%s".as_ptr(), message.as_ptr()) };
}

fn warning(message: &str) {
    log(G_LOG_LEVEL_WARNING, message);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn info(message: &str) {
    log(G_LOG_LEVEL_INFO, message);
}

#[cfg(target_os = "linux")]
fn strerror(errnum: c_int) -> String {
    unsafe { CStr::from_ptr(g_strerror(errnum)) }
        .to_string_lossy()
        .into_owned()
}

fn getenv(name: &CStr) -> Option<&'static [u8]> {
    unsafe { glib::bytes(glib::g_getenv(name.as_ptr())) }
}

pub(crate) fn digest_base64(digest: SriDigest, body: &[u8]) -> Vec<u8> {
    let kind = match digest {
        SriDigest::Sha256 => glib::G_CHECKSUM_SHA256,
        SriDigest::Sha384 => glib::G_CHECKSUM_SHA384,
        SriDigest::Sha512 => glib::G_CHECKSUM_SHA512,
    };
    let mut raw = [0u8; 64];
    let mut raw_len = raw.len();
    unsafe {
        let checksum = glib::g_checksum_new(kind);
        glib::g_checksum_update(checksum, body.as_ptr(), body.len() as isize);
        glib::g_checksum_get_digest(checksum, raw.as_mut_ptr(), &mut raw_len);
        glib::g_checksum_free(checksum);
        let encoded = glib::g_base64_encode(raw.as_ptr(), raw_len);
        if encoded.is_null() {
            return Vec::new();
        }
        let text = CStr::from_ptr(encoded).to_bytes().to_vec();
        glib::g_free(encoded.cast());
        text
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_security_sri_check(
    integrity_attr: *const c_char,
    body: *const c_void,
    body_len: usize,
) -> GBoolean {
    let integrity = unsafe { glib::bytes(integrity_attr) }.unwrap_or_default();
    let body = (!body.is_null()).then(|| unsafe { glib::slice(body.cast(), body_len) });
    glib::boolean(crate::sri_check(integrity, body))
}

static WRITABLE_DIRS: Mutex<Vec<CString>> = Mutex::new(Vec::new());
static EXEC_DIRS: Mutex<Vec<CString>> = Mutex::new(Vec::new());

fn remember(list: &Mutex<Vec<CString>>, dir: *const c_char) {
    if let Some(dir) = unsafe { glib::bytes(dir) }.filter(|dir| !dir.is_empty()) {
        if let Ok(dir) = CString::new(dir) {
            list.lock().unwrap_or_else(|e| e.into_inner()).push(dir);
        }
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn remembered(list: &Mutex<Vec<CString>>) -> Vec<CString> {
    list.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_security_add_writable_dir(dir: *const c_char) {
    remember(&WRITABLE_DIRS, dir);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_security_add_exec_dir(dir: *const c_char) {
    remember(&EXEC_DIRS, dir);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn glib_dir(get: unsafe extern "C" fn() -> *const c_char) -> Option<Vec<u8>> {
    unsafe { glib::bytes(get()) }.map(<[u8]>::to_vec)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    let owned: Vec<CString> = parts
        .iter()
        .map(|part| CString::new(*part).unwrap_or_default())
        .collect();
    let mut pointers: Vec<*mut c_char> =
        owned.iter().map(|part| part.as_ptr().cast_mut()).collect();
    pointers.push(ptr::null_mut());
    unsafe {
        let joined = glib::g_build_filenamev(pointers.as_mut_ptr());
        let bytes = CStr::from_ptr(joined).to_bytes().to_vec();
        glib::g_free(joined.cast());
        bytes
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn mkdir_with_parents(path: &[u8]) {
    if let Ok(path) = CString::new(path) {
        unsafe { g_mkdir_with_parents(path.as_ptr(), 0o700) };
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn own_dir(base: &[u8], rest: &[&[u8]]) -> Vec<u8> {
    let mut parts = vec![base];
    parts.extend_from_slice(rest);
    let dir = build_filename(&parts);
    mkdir_with_parents(&dir);
    dir
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn download_dir() -> Option<Vec<u8>> {
    const G_USER_DIRECTORY_DOWNLOAD: c_int = 2;
    unsafe { glib::bytes(g_get_user_special_dir(G_USER_DIRECTORY_DOWNLOAD)) }.map(<[u8]>::to_vec)
}

#[cfg(target_os = "linux")]
fn file_test(path: &[u8], test: glib::GFileTest) -> bool {
    CString::new(path).is_ok_and(|path| unsafe { glib::g_file_test(path.as_ptr(), test) } != 0)
}

#[cfg(target_os = "linux")]
fn dirname(path: &[u8]) -> Vec<u8> {
    let Ok(path) = CString::new(path) else {
        return b".".to_vec();
    };
    unsafe {
        let dir = glib::g_path_get_dirname(path.as_ptr());
        let bytes = CStr::from_ptr(dir).to_bytes().to_vec();
        glib::g_free(dir.cast());
        bytes
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{
        EXEC_DIRS, WRITABLE_DIRS, build_filename, dirname, download_dir, file_test, getenv,
        glib_dir, info, own_dir, remembered, strerror,
    };
    use crate::landlock::{self, Access};
    use core::ffi::{c_char, c_int, c_long, c_void};
    use core::ptr;
    use southstar_glib as glib;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    const SYS_LANDLOCK_CREATE_RULESET: c_long = 444;
    const SYS_LANDLOCK_ADD_RULE: c_long = 445;
    const SYS_LANDLOCK_RESTRICT_SELF: c_long = 446;
    const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
    const LANDLOCK_RULE_PATH_BENEATH: c_int = 1;
    const O_PATH: c_int = 0o10000000;
    pub(super) const PR_SET_NO_NEW_PRIVS: c_int = 38;
    const ENOSYS: c_int = 38;
    const EOPNOTSUPP: c_int = 95;

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
    }

    #[repr(C, packed)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
    }

    unsafe extern "C" {
        fn syscall(number: c_long, ...) -> c_long;
        pub(super) fn prctl(option: c_int, ...) -> c_int;
        fn _exit(status: c_int) -> !;
        fn geteuid() -> u32;
        fn getuid() -> u32;
    }

    pub(super) fn is_root() -> bool {
        unsafe { geteuid() == 0 || getuid() == 0 }
    }

    pub(super) fn errno() -> c_int {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    pub(super) fn require_or_die(what: &str) {
        let required = getenv(c"NS_REQUIRE_SANDBOX");
        if required.is_none_or(|value| value.is_empty() || value == b"0") {
            return;
        }
        let message = format!(
            "southstar: {what}, but NS_REQUIRE_SANDBOX is set \u{2014} refusing to run untrusted content unconfined.\n"
        );
        let message = std::ffi::CString::new(message).unwrap_or_default();
        unsafe {
            super::g_printerr(c"%s".as_ptr(), message.as_ptr());
            _exit(70);
        }
    }

    fn create_ruleset(attr: Option<&RulesetAttr>, flags: u32) -> c_int {
        let (attr, size) = match attr {
            Some(attr) => (
                (attr as *const RulesetAttr).cast::<c_void>(),
                core::mem::size_of::<RulesetAttr>(),
            ),
            None => (ptr::null(), 0),
        };
        unsafe {
            syscall(
                SYS_LANDLOCK_CREATE_RULESET,
                attr,
                size as c_long,
                c_long::from(flags),
            ) as c_int
        }
    }

    struct Ruleset(c_int);

    impl Ruleset {
        fn allow(&self, access: u64, path: &[u8]) {
            let path = std::ffi::OsStr::from_bytes(path);
            let Ok(file) = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(O_PATH)
                .open(path)
            else {
                return;
            };
            let rule = PathBeneathAttr {
                allowed_access: access,
                parent_fd: file.as_raw_fd(),
            };
            unsafe {
                syscall(
                    SYS_LANDLOCK_ADD_RULE,
                    c_long::from(self.0),
                    c_long::from(LANDLOCK_RULE_PATH_BENEATH),
                    (&raw const rule).cast::<c_void>(),
                    0 as c_long,
                )
            };
        }
    }

    pub(super) fn sandbox_init(self_exe: Option<&[u8]>) {
        if getenv(c"NS_NO_SANDBOX").is_some() {
            return;
        }
        let abi = create_ruleset(None, LANDLOCK_CREATE_RULESET_VERSION);
        let access = Access::for_abi(abi);
        let (fs_read, fs_write, fs_exec) = (access.read, access.write, access.exec);
        let fs_rw = access.read_write();
        let attr = RulesetAttr {
            handled_access_fs: access.all(),
        };
        let fd = create_ruleset(Some(&attr), 0);
        if fd < 0 {
            let err = errno();
            if err != ENOSYS && err != EOPNOTSUPP {
                info(&format!(
                    "landlock: create_ruleset failed: {}",
                    strerror(err)
                ));
            }
            require_or_die("Landlock filesystem sandbox is unavailable");
            return;
        }
        let rules = Ruleset(fd);
        if unsafe {
            prctl(
                PR_SET_NO_NEW_PRIVS,
                1 as c_long,
                0 as c_long,
                0 as c_long,
                0 as c_long,
            )
        } != 0
        {
            info(&format!(
                "landlock: PR_SET_NO_NEW_PRIVS failed: {}",
                strerror(errno())
            ));
        }

        rules.allow(landlock::READ_FILE, b"/dev/urandom");
        rules.allow(
            landlock::READ_FILE | landlock::WRITE_FILE | (fs_write & landlock::TRUNCATE),
            b"/dev/null",
        );
        for dir in crate::SYSTEM_EXEC_DIRS {
            rules.allow(fs_read | fs_exec, dir.as_bytes());
        }
        for dir in crate::SYSTEM_READ_DIRS {
            rules.allow(fs_read, dir.as_bytes());
        }
        let fs_dev = landlock::READ_FILE | landlock::WRITE_FILE;
        for index in 0..64 {
            let device = format!("/dev/video{index}");
            if file_test(device.as_bytes(), glib::FILE_TEST_EXISTS) {
                rules.allow(fs_dev, device.as_bytes());
            }
        }
        if file_test(b"/dev/snd", glib::FILE_TEST_IS_DIR) {
            rules.allow(fs_read | landlock::WRITE_FILE, b"/dev/snd");
        }
        if let Some(xauth) = getenv(c"XAUTHORITY").filter(|xauth| !xauth.is_empty()) {
            rules.allow(fs_read, &dirname(xauth));
        }

        let home = glib_dir(super::g_get_home_dir);
        let config = glib_dir(glib::g_get_user_config_dir);
        let data = glib_dir(glib::g_get_user_data_dir);
        let cache = glib_dir(glib::g_get_user_cache_dir);
        for dir in [&config, &data, &cache].into_iter().flatten() {
            rules.allow(fs_read, dir);
        }
        if let Some(runtime) = glib_dir(super::g_get_user_runtime_dir) {
            rules.allow(fs_rw, &runtime);
        }
        for base in [&config, &data, &cache].into_iter().flatten() {
            rules.allow(fs_rw, &own_dir(base, &[b"southstar"]));
        }
        if let Some(cache) = &cache {
            rules.allow(fs_rw, &own_dir(cache, &[b"southstar", b"cache"]));
        }

        let downloads = match download_dir().filter(|dir| !dir.is_empty()) {
            Some(dir) => Some(dir),
            None => home
                .as_deref()
                .map(|home| build_filename(&[home, b"Downloads"])),
        };
        if let (Some(downloads), Some(home)) = (&downloads, &home) {
            if downloads.starts_with(home) {
                super::mkdir_with_parents(downloads);
                if file_test(downloads, glib::FILE_TEST_IS_DIR) {
                    rules.allow(fs_rw, downloads);
                }
            }
        }

        let home_base = home.clone().unwrap_or_default();
        for sub in crate::HOME_READ_SUBDIRS {
            rules.allow(fs_read, &build_filename(&[&home_base, sub.as_bytes()]));
        }

        if let Some(self_exe) = self_exe {
            let exe_dir = dirname(self_exe);
            rules.allow(fs_read | fs_exec, &exe_dir);
            for rel in crate::DEV_DATA_DIRS {
                let path = build_filename(&[&exe_dir, rel.as_bytes()]);
                if file_test(&path, glib::FILE_TEST_IS_DIR) {
                    rules.allow(fs_read, &path);
                }
            }
            for rel in crate::PREFIX_LIB_DIRS {
                let path = build_filename(&[&exe_dir, rel.as_bytes()]);
                if file_test(&path, glib::FILE_TEST_IS_DIR) {
                    rules.allow(fs_read | fs_exec, &path);
                }
            }
            for rel in crate::DEV_ROOT_MARKERS {
                let marker = build_filename(&[&exe_dir, rel.as_bytes()]);
                if file_test(&marker, glib::FILE_TEST_IS_REGULAR) {
                    rules.allow(fs_read, &dirname(&marker));
                    break;
                }
            }
        }

        for dir in remembered(&WRITABLE_DIRS) {
            rules.allow(fs_rw, dir.as_bytes());
        }
        for dir in remembered(&EXEC_DIRS) {
            rules.allow(fs_read | fs_exec, dir.as_bytes());
        }

        if unsafe { syscall(SYS_LANDLOCK_RESTRICT_SELF, c_long::from(fd), 0 as c_long) } != 0 {
            info(&format!(
                "landlock: restrict_self failed: {}",
                strerror(errno())
            ));
            require_or_die("Landlock enforcement (restrict_self) failed");
        }
        unsafe { close(fd) };
    }

    unsafe extern "C" {
        fn close(fd: c_int) -> c_int;
        fn setxattr(
            path: *const c_char,
            name: *const c_char,
            value: *const c_void,
            size: usize,
            flags: c_int,
        ) -> c_int;
    }

    pub(super) fn mark_download_origin(path: &std::ffi::CStr, url: Option<&[u8]>) {
        if let Some(url) = url.filter(|url| !url.is_empty()) {
            unsafe {
                setxattr(
                    path.as_ptr(),
                    c"user.xdg.origin.url".as_ptr(),
                    url.as_ptr().cast(),
                    url.len(),
                    0,
                )
            };
        }
    }
}

#[cfg(all(target_os = "linux", feature = "seccomp"))]
mod seccomp {
    use super::linux::{PR_SET_NO_NEW_PRIVS, errno, prctl, require_or_die};
    use super::{info, strerror, warning};
    use core::ffi::{c_char, c_int, c_long, c_uint, c_void};
    use std::ffi::CString;

    const SCMP_ACT_ALLOW: u32 = 0x7fff_0000;
    const SCMP_ACT_ERRNO_EPERM: u32 = 0x0005_0000 | 1;
    const SCMP_FLTATR_CTL_TSYNC: c_int = 4;
    const SCMP_CMP_MASKED_EQ: c_int = 7;
    const NR_SCMP_ERROR: c_int = -1;
    const TIOCSTI: u64 = 0x5412;
    const TIOCLINUX: u64 = 0x541C;

    #[repr(C)]
    struct ArgCmp {
        arg: c_uint,
        op: c_int,
        datum_a: u64,
        datum_b: u64,
    }

    unsafe extern "C" {
        fn seccomp_init(def_action: u32) -> *mut c_void;
        fn seccomp_attr_set(ctx: *mut c_void, attr: c_int, value: u32) -> c_int;
        fn seccomp_rule_add_array(
            ctx: *mut c_void,
            action: u32,
            syscall: c_int,
            arg_cnt: c_uint,
            arg_array: *const ArgCmp,
        ) -> c_int;
        fn seccomp_syscall_resolve_name(name: *const c_char) -> c_int;
        fn seccomp_load(ctx: *mut c_void) -> c_int;
        fn seccomp_release(ctx: *mut c_void);
    }

    fn resolve(name: &str) -> c_int {
        let name = CString::new(name).unwrap_or_default();
        unsafe { seccomp_syscall_resolve_name(name.as_ptr()) }
    }

    fn deny_tty_injection() -> c_int {
        const ENOMEM: c_int = 12;
        let ctx = unsafe { seccomp_init(SCMP_ACT_ALLOW) };
        if ctx.is_null() {
            return -ENOMEM;
        }
        unsafe { seccomp_attr_set(ctx, SCMP_FLTATR_CTL_TSYNC, 1) };
        let ioctl = resolve("ioctl");
        let mut rc = 0;
        for request in [TIOCSTI, TIOCLINUX] {
            if rc != 0 {
                break;
            }
            let compare = ArgCmp {
                arg: 1,
                op: SCMP_CMP_MASKED_EQ,
                datum_a: 0xFFFF_FFFF,
                datum_b: request,
            };
            rc = unsafe { seccomp_rule_add_array(ctx, SCMP_ACT_ERRNO_EPERM, ioctl, 1, &compare) };
        }
        if rc == 0 {
            rc = unsafe { seccomp_load(ctx) };
        }
        unsafe { seccomp_release(ctx) };
        rc
    }

    pub(super) fn init() {
        if unsafe {
            prctl(
                PR_SET_NO_NEW_PRIVS,
                1 as c_long,
                0 as c_long,
                0 as c_long,
                0 as c_long,
            )
        } != 0
        {
            info(&format!(
                "seccomp: PR_SET_NO_NEW_PRIVS failed: {}",
                strerror(errno())
            ));
            require_or_die("seccomp prerequisite (no_new_privs) failed");
            return;
        }
        let tty_rc = deny_tty_injection();
        if tty_rc != 0 {
            warning(&format!(
                "seccomp: terminal-injection filter failed to load: {}",
                strerror(-tty_rc)
            ));
            require_or_die("seccomp terminal-injection filter failed to load");
        }
        let ctx = unsafe { seccomp_init(SCMP_ACT_ERRNO_EPERM) };
        if ctx.is_null() {
            info("seccomp: seccomp_init failed");
            require_or_die("seccomp syscall filter could not be created");
            return;
        }
        unsafe { seccomp_attr_set(ctx, SCMP_FLTATR_CTL_TSYNC, 1) };
        for name in crate::SECCOMP_ALLOWED {
            let nr = resolve(name);
            if nr == NR_SCMP_ERROR {
                continue;
            }
            unsafe { seccomp_rule_add_array(ctx, SCMP_ACT_ALLOW, nr, 0, core::ptr::null()) };
        }
        let rc = unsafe { seccomp_load(ctx) };
        if rc != 0 {
            warning(&format!(
                "seccomp: load failed, process is NOT syscall-sandboxed: {}",
                strerror(-rc)
            ));
            require_or_die("seccomp syscall filter failed to load");
        }
        unsafe { seccomp_release(ctx) };
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        WRITABLE_DIRS, download_dir, getenv, glib_dir, info, own_dir, remembered, warning,
    };
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;
    use southstar_glib as glib;
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn sandbox_init(profile: *const c_char, flags: u64, errorbuf: *mut *mut c_char) -> c_int;
        fn sandbox_free_error(errorbuf: *mut c_char);
        fn setxattr(
            path: *const c_char,
            name: *const c_char,
            value: *const c_void,
            size: usize,
            position: u32,
            options: c_int,
        ) -> c_int;
        fn geteuid() -> u32;
        fn getuid() -> u32;
    }

    pub(super) fn is_root() -> bool {
        unsafe { geteuid() == 0 || getuid() == 0 }
    }

    fn subpath(profile: &mut Vec<u8>, dir: Option<&[u8]>) {
        let Some(dir) = dir.filter(|dir| !dir.is_empty()) else {
            return;
        };
        let resolved = std::fs::canonicalize(std::ffi::OsStr::from_bytes(dir)).ok();
        let used = resolved
            .as_ref()
            .map_or(dir, |path| path.as_os_str().as_bytes());
        crate::seatbelt_subpath(profile, used);
    }

    pub(super) fn sandbox_init_profile() {
        if getenv(c"NS_NO_SANDBOX").is_some() {
            return;
        }
        let mut profile = crate::SEATBELT_HEAD.to_vec();
        subpath(
            &mut profile,
            glib_dir(super::g_get_user_runtime_dir).as_deref(),
        );
        for get in [
            glib::g_get_user_config_dir as unsafe extern "C" fn() -> *const c_char,
            glib::g_get_user_data_dir,
            glib::g_get_user_cache_dir,
        ] {
            let base = glib_dir(get).unwrap_or_default();
            subpath(&mut profile, Some(&own_dir(&base, &[b"southstar"])));
        }
        subpath(&mut profile, download_dir().as_deref());
        for dir in remembered(&WRITABLE_DIRS) {
            subpath(&mut profile, Some(dir.as_bytes()));
        }
        profile.extend_from_slice(b")\n");
        profile.push(0);
        let mut err = ptr::null_mut();
        let rc = unsafe { sandbox_init(profile.as_ptr().cast(), 0, &mut err) };
        if rc != 0 {
            let reason = if err.is_null() {
                "unknown error".to_owned()
            } else {
                unsafe { CStr::from_ptr(err) }
                    .to_string_lossy()
                    .into_owned()
            };
            warning(&format!(
                "sandbox: macOS Seatbelt init failed, process is NOT filesystem-confined: {reason}"
            ));
            if !err.is_null() {
                unsafe { sandbox_free_error(err) };
            }
        } else {
            info("sandbox: macOS Seatbelt filesystem write-confinement active");
        }
    }

    pub(super) fn mark_download_origin(path: &CStr) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let value = format!("0001;{:08x};Southstar;", now as u32);
        unsafe {
            setxattr(
                path.as_ptr(),
                c"com.apple.quarantine".as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
    }
}

#[cfg(windows)]
mod windows {
    use core::ffi::{c_char, c_int, c_void};
    use core::ptr;

    type Handle = *mut c_void;
    type SetMitigationPolicy = unsafe extern "system" fn(c_int, *mut c_void, usize) -> c_int;

    #[repr(C)]
    struct SidAuthority {
        value: [u8; 6],
    }

    #[repr(C)]
    struct StartupInfo {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        dims: [u32; 8],
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

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn AllocateAndInitializeSid(
            authority: *const SidAuthority,
            count: u8,
            s0: u32,
            s1: u32,
            s2: u32,
            s3: u32,
            s4: u32,
            s5: u32,
            s6: u32,
            s7: u32,
            sid: *mut *mut c_void,
        ) -> c_int;
        fn CheckTokenMembership(token: Handle, sid: *mut c_void, is_member: *mut c_int) -> c_int;
        fn FreeSid(sid: *mut c_void) -> *mut c_void;
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> c_int;
        fn DuplicateTokenEx(
            token: Handle,
            access: u32,
            attributes: *mut c_void,
            level: c_int,
            kind: c_int,
            new_token: *mut Handle,
        ) -> c_int;
        fn GetTokenInformation(
            token: Handle,
            class: c_int,
            info: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> c_int;
        fn CreateProcessWithTokenW(
            token: Handle,
            logon_flags: u32,
            application: *const u16,
            command_line: *mut u16,
            creation_flags: u32,
            environment: *mut c_void,
            current_directory: *const u16,
            startup: *const StartupInfo,
            information: *mut ProcessInformation,
        ) -> c_int;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetShellWindow() -> Handle;
        fn GetWindowThreadProcessId(window: Handle, process_id: *mut u32) -> u32;
        fn MessageBoxA(
            window: Handle,
            text: *const c_char,
            caption: *const c_char,
            kind: u32,
        ) -> c_int;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: c_int, process_id: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> c_int;
        fn GetModuleFileNameW(module: Handle, filename: *mut u16, size: u32) -> u32;
        fn GetCommandLineW() -> *mut u16;
        fn lstrlenW(text: *const u16) -> c_int;
        fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> c_int;
        fn GetModuleHandleW(name: *const u16) -> Handle;
        fn GetProcAddress(module: Handle, name: *const c_char) -> *mut c_void;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn WriteFile(
            file: Handle,
            buffer: *const c_void,
            length: u32,
            written: *mut u32,
            overlapped: *mut c_void,
        ) -> c_int;
    }

    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(algorithm: Handle, buffer: *mut u8, length: u32, flags: u32) -> i32;
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    pub(super) fn csprng_fill(buf: &mut [u8]) -> bool {
        const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 2;
        unsafe {
            BCryptGenRandom(
                ptr::null_mut(),
                buf.as_mut_ptr(),
                buf.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            ) == 0
        }
    }

    pub(super) fn is_elevated() -> bool {
        const SECURITY_BUILTIN_DOMAIN_RID: u32 = 32;
        const DOMAIN_ALIAS_RID_ADMINS: u32 = 544;
        let authority = SidAuthority {
            value: [0, 0, 0, 0, 0, 5],
        };
        let mut sid = ptr::null_mut();
        if unsafe {
            AllocateAndInitializeSid(
                &authority,
                2,
                SECURITY_BUILTIN_DOMAIN_RID,
                DOMAIN_ALIAS_RID_ADMINS,
                0,
                0,
                0,
                0,
                0,
                0,
                &mut sid,
            )
        } == 0
        {
            return false;
        }
        let mut member = 0;
        if unsafe { CheckTokenMembership(ptr::null_mut(), sid, &mut member) } == 0 {
            member = 0;
        }
        unsafe { FreeSid(sid) };
        member != 0
    }

    fn launch_with_token(user_token: Handle) -> bool {
        let mut exe = vec![0u16; 4096];
        let n = unsafe { GetModuleFileNameW(ptr::null_mut(), exe.as_mut_ptr(), exe.len() as u32) };
        let app = if n > 0 && (n as usize) < exe.len() {
            exe.as_ptr()
        } else {
            ptr::null()
        };
        let source = unsafe { GetCommandLineW() };
        let length = unsafe { lstrlenW(source) } as usize + 1;
        let mut command = unsafe { core::slice::from_raw_parts(source, length) }.to_vec();
        unsafe { SetEnvironmentVariableW(wide("NS_DEELEVATED").as_ptr(), wide("1").as_ptr()) };
        let startup = StartupInfo {
            cb: core::mem::size_of::<StartupInfo>() as u32,
            reserved: ptr::null_mut(),
            desktop: ptr::null_mut(),
            title: ptr::null_mut(),
            dims: [0; 8],
            show_window: 0,
            cb_reserved2: 0,
            reserved2: ptr::null_mut(),
            std_input: ptr::null_mut(),
            std_output: ptr::null_mut(),
            std_error: ptr::null_mut(),
        };
        let mut info = ProcessInformation {
            process: ptr::null_mut(),
            thread: ptr::null_mut(),
            process_id: 0,
            thread_id: 0,
        };
        let launched = unsafe {
            CreateProcessWithTokenW(
                user_token,
                0,
                app,
                command.as_mut_ptr(),
                0,
                ptr::null_mut(),
                ptr::null(),
                &startup,
                &mut info,
            )
        } != 0;
        if launched {
            unsafe {
                CloseHandle(info.process);
                CloseHandle(info.thread);
            }
        }
        launched
    }

    pub(super) fn relaunch_deelevated() -> bool {
        const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
        const TOKEN_ASSIGN_PRIMARY: u32 = 0x0001;
        const TOKEN_DUPLICATE: u32 = 0x0002;
        const TOKEN_QUERY: u32 = 0x0008;
        const TOKEN_ADJUST_DEFAULT: u32 = 0x0080;
        const TOKEN_ADJUST_SESSIONID: u32 = 0x0100;
        const SECURITY_IMPERSONATION: c_int = 2;
        const TOKEN_PRIMARY: c_int = 1;
        const TOKEN_ELEVATION: c_int = 20;

        let shell = unsafe { GetShellWindow() };
        if shell.is_null() {
            return false;
        }
        let mut shell_pid = 0;
        unsafe { GetWindowThreadProcessId(shell, &mut shell_pid) };
        if shell_pid == 0 {
            return false;
        }
        let shell_process = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, 0, shell_pid) };
        if shell_process.is_null() {
            return false;
        }
        let mut relaunched = false;
        let mut shell_token = ptr::null_mut();
        if unsafe { OpenProcessToken(shell_process, TOKEN_DUPLICATE, &mut shell_token) } != 0 {
            let mut user_token = ptr::null_mut();
            let access = TOKEN_QUERY
                | TOKEN_ASSIGN_PRIMARY
                | TOKEN_DUPLICATE
                | TOKEN_ADJUST_DEFAULT
                | TOKEN_ADJUST_SESSIONID;
            if unsafe {
                DuplicateTokenEx(
                    shell_token,
                    access,
                    ptr::null_mut(),
                    SECURITY_IMPERSONATION,
                    TOKEN_PRIMARY,
                    &mut user_token,
                )
            } != 0
            {
                let mut elevation = 0u32;
                let mut got = 0;
                let shell_elevated = unsafe {
                    GetTokenInformation(
                        user_token,
                        TOKEN_ELEVATION,
                        (&raw mut elevation).cast(),
                        4,
                        &mut got,
                    )
                } != 0
                    && elevation != 0;
                if !shell_elevated {
                    relaunched = launch_with_token(user_token);
                }
                unsafe { CloseHandle(user_token) };
            }
            unsafe { CloseHandle(shell_token) };
        }
        unsafe { CloseHandle(shell_process) };
        relaunched
    }

    pub(super) fn ask_to_run_elevated(message: &str) -> bool {
        const MB_YESNO: u32 = 0x4;
        const MB_ICONWARNING: u32 = 0x30;
        const MB_DEFBUTTON2: u32 = 0x100;
        const MB_SETFOREGROUND: u32 = 0x10000;
        const MB_TOPMOST: u32 = 0x40000;
        const IDYES: c_int = 6;
        let text = std::ffi::CString::new(message).unwrap_or_default();
        unsafe {
            MessageBoxA(
                ptr::null_mut(),
                text.as_ptr(),
                c"Southstar - running as administrator".as_ptr(),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND | MB_TOPMOST,
            ) == IDYES
        }
    }

    pub(super) fn mitigations_init(allow_child_processes: bool) {
        const PROCESS_ASLR_POLICY: c_int = 1;
        const PROCESS_STRICT_HANDLE_CHECK_POLICY: c_int = 3;
        const PROCESS_EXTENSION_POINT_DISABLE_POLICY: c_int = 6;
        const PROCESS_IMAGE_LOAD_POLICY: c_int = 10;
        const PROCESS_CHILD_PROCESS_POLICY: c_int = 13;
        let kernel32 = unsafe { GetModuleHandleW(wide("kernel32.dll").as_ptr()) };
        if kernel32.is_null() {
            return;
        }
        let address = unsafe { GetProcAddress(kernel32, c"SetProcessMitigationPolicy".as_ptr()) };
        if address.is_null() {
            return;
        }
        let set: SetMitigationPolicy = unsafe { core::mem::transmute(address) };
        let apply = |policy: c_int, flags: u32| {
            let mut value = flags;
            unsafe { set(policy, (&raw mut value).cast(), core::mem::size_of::<u32>()) };
        };
        apply(PROCESS_ASLR_POLICY, 0x02);
        apply(PROCESS_STRICT_HANDLE_CHECK_POLICY, 0x03);
        apply(PROCESS_EXTENSION_POINT_DISABLE_POLICY, 0x01);
        apply(PROCESS_IMAGE_LOAD_POLICY, 0x07);
        if !allow_child_processes {
            apply(PROCESS_CHILD_PROCESS_POLICY, 0x01);
        }
    }

    pub(super) fn mark_download_origin(path: &[u8], url: Option<&[u8]>) {
        const GENERIC_WRITE: u32 = 0x4000_0000;
        const CREATE_ALWAYS: u32 = 2;
        const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
        let mut stream = path.to_vec();
        stream.extend_from_slice(b":Zone.Identifier");
        let Ok(stream) = std::str::from_utf8(&stream) else {
            return;
        };
        let handle = unsafe {
            CreateFileW(
                wide(stream).as_ptr(),
                GENERIC_WRITE,
                0,
                ptr::null_mut(),
                CREATE_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle as isize == -1 {
            return;
        }
        let zone = crate::zone_identifier(url);
        let mut wrote = 0;
        unsafe {
            WriteFile(
                handle,
                zone.as_ptr().cast(),
                zone.len() as u32,
                &mut wrote,
                ptr::null_mut(),
            );
            CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
fn urandom_fill(buf: &mut [u8]) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open("/dev/urandom") else {
        return false;
    };
    let mut got = 0;
    while got < buf.len() {
        match file.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    got == buf.len()
}

#[cfg(target_os = "linux")]
fn os_fill(buf: &mut [u8]) -> bool {
    unsafe extern "C" {
        fn getrandom(buf: *mut c_void, len: usize, flags: u32) -> isize;
    }
    const EINTR: c_int = 4;
    let mut off = 0;
    while off < buf.len() {
        let n = unsafe { getrandom(buf[off..].as_mut_ptr().cast(), buf.len() - off, 0) };
        if n < 0 {
            if linux::errno() == EINTR {
                continue;
            }
            break;
        }
        off += n as usize;
    }
    off == buf.len()
}

#[cfg(any(
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn os_fill(buf: &mut [u8]) -> bool {
    unsafe extern "C" {
        fn getentropy(buf: *mut c_void, len: usize) -> c_int;
    }
    for chunk in buf.chunks_mut(256) {
        if unsafe { getentropy(chunk.as_mut_ptr().cast(), chunk.len()) } != 0 {
            return false;
        }
    }
    true
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "dragonfly"
)))]
fn os_fill(_buf: &mut [u8]) -> bool {
    false
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_security_csprng_fill(buf: *mut c_void, len: usize) -> GBoolean {
    if buf.is_null() || len == 0 {
        return TRUE;
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(buf.cast::<u8>(), len) };
    #[cfg(windows)]
    let filled = windows::csprng_fill(buf);
    #[cfg(not(windows))]
    let filled = os_fill(buf) || urandom_fill(buf);
    glib::boolean(filled)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_security_refuse_root() -> GBoolean {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        #[cfg(target_os = "linux")]
        let root = linux::is_root();
        #[cfg(target_os = "macos")]
        let root = macos::is_root();
        if !root {
            return TRUE;
        }
        if getenv(c"NS_ALLOW_ROOT").is_some() {
            warning("southstar: running as root because NS_ALLOW_ROOT is set");
            return TRUE;
        }
        use std::io::Write;
        let _ = std::io::stderr().write_all(crate::ROOT_MESSAGE.as_bytes());
        FALSE
    }
    #[cfg(windows)]
    {
        if !windows::is_elevated() {
            return TRUE;
        }
        if getenv(c"NS_ALLOW_ROOT").is_some() {
            warning("southstar: running as Administrator because NS_ALLOW_ROOT is set");
            return TRUE;
        }
        if getenv(c"NS_DEELEVATED").is_none() && windows::relaunch_deelevated() {
            warning(
                "southstar: dropped Administrator rights by relaunching as the current desktop user",
            );
            return FALSE;
        }
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "southstar: {}", crate::ELEVATED_MESSAGE);
        if windows::ask_to_run_elevated(crate::ELEVATED_MESSAGE) {
            warning("southstar: running as Administrator at the user's request");
            return TRUE;
        }
        FALSE
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_security_sandbox_init(self_exe: *const c_char) {
    #[cfg(target_os = "linux")]
    linux::sandbox_init(unsafe { glib::bytes(self_exe) });
    #[cfg(target_os = "macos")]
    {
        let _ = self_exe;
        macos::sandbox_init_profile();
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = self_exe;
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_security_seccomp_init() {
    #[cfg(target_os = "linux")]
    {
        let disabled = getenv(c"NS_NO_SANDBOX").is_some() || getenv(c"NS_NO_SECCOMP").is_some();
        #[cfg(feature = "seccomp")]
        if !disabled {
            seccomp::init();
        }
        #[cfg(not(feature = "seccomp"))]
        let _ = disabled;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_security_win32_mitigations_init(allow_child_processes: GBoolean) {
    #[cfg(windows)]
    {
        if getenv(c"NS_NO_WIN32_MITIGATIONS").is_some() {
            return;
        }
        windows::mitigations_init(allow_child_processes != FALSE);
    }
    #[cfg(not(windows))]
    let _ = allow_child_processes;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_security_mark_download_origin(path: *const c_char, url: *const c_char) {
    let Some(path_bytes) = unsafe { glib::bytes(path) }.filter(|path| !path.is_empty()) else {
        return;
    };
    let url = unsafe { glib::bytes(url) };
    #[cfg(target_os = "macos")]
    {
        let _ = (path_bytes, url);
        macos::mark_download_origin(unsafe { CStr::from_ptr(path) });
    }
    #[cfg(target_os = "linux")]
    {
        let _ = path_bytes;
        linux::mark_download_origin(unsafe { CStr::from_ptr(path) }, url);
    }
    #[cfg(windows)]
    windows::mark_download_origin(path_bytes, url);
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let _ = (path_bytes, url);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_security_harden_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn mallopt(param: c_int, value: c_int) -> c_int;
        }
        const M_PERTURB: c_int = -6;
        unsafe { mallopt(M_PERTURB, 0xAA) };
    }
}
